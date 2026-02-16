use crate::repositories::{
    ActorRepository, BlobRepository, BuildJobRepository, ComponentRepository, GateRepository,
    SourceArchiveRepository,
};
use crate::services::{
    AuthService, BlobService, BuildDispatchService, ComponentManager, GateManager, OidcService,
    RbacService,
};
use crate::settings::Settings;
use crate::storage::{
    jj_repos::manager::JjRepoManager,
    seaweedfs::client::{SeaweedFsClient, SeaweedFsConfig as ClientConfig},
};
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use std::sync::Arc;

/// Application state with all dependencies wired together
#[derive(Clone)]
pub struct AppState {
    // Infrastructure
    pub db: Arc<DatabaseConnection>,
    pub seaweedfs: Arc<SeaweedFsClient>,
    pub jj_manager: Arc<JjRepoManager>,
    pub amqp_pool: deadpool_lapin::Pool,
    pub settings: Settings,

    // Repositories
    pub blob_repo: Arc<BlobRepository>,
    pub gate_repo: Arc<GateRepository>,
    pub component_repo: Arc<ComponentRepository>,
    pub source_archive_repo: Arc<SourceArchiveRepository>,
    pub actor_repo: Arc<ActorRepository>,
    pub build_job_repo: Arc<BuildJobRepository>,

    // Services
    pub oidc: Arc<OidcService>,
    pub auth: Arc<AuthService>,
    pub rbac: Arc<RbacService>,
    pub gate_manager: Arc<GateManager>,
    pub component_manager: Arc<ComponentManager>,
    pub blob_service: Arc<BlobService>,
    pub build_dispatch: Arc<BuildDispatchService>,
}

impl AppState {
    /// Create a new AppState with all dependencies initialized
    pub async fn new(settings: Settings) -> Result<Self> {
        tracing::info!("Initializing application state");

        // 1. Connect to PostgreSQL
        tracing::info!(url = %settings.postgres.url, "Connecting to PostgreSQL");
        let db = Database::connect(&settings.postgres.url)
            .await
            .into_diagnostic()
            .wrap_err(
                "Failed to connect to PostgreSQL. \n\
                      Ensure PostgreSQL is running and the connection URL is correct. \n\
                      Example: postgresql://user:password@localhost/database",
            )?;
        let db = Arc::new(db);

        // 2. Run migrations
        tracing::info!("Running database migrations");
        migration::Migrator::up(&*db, None)
            .await
            .into_diagnostic()
            .wrap_err(
                "Failed to run database migrations. \n\
                      Check database permissions and schema compatibility.",
            )?;

        // 3. Initialize SeaweedFS client
        tracing::info!(master_url = %settings.seaweedfs.master_url, "Initializing SeaweedFS client");
        let seaweedfs_config = ClientConfig {
            master_url: settings.seaweedfs.master_url.clone(),
            namespace: settings.seaweedfs.namespace.clone(),
        };
        let seaweedfs = Arc::new(SeaweedFsClient::new(seaweedfs_config));

        // 4. Initialize JjRepoManager
        tracing::info!(root = %settings.jj_repos.root, "Initializing Jujutsu repository manager");
        let jj_seaweedfs_config = crate::storage::seaweedfs::SeaweedFsConfig {
            master_url: settings.seaweedfs.master_url.clone(),
            namespace: settings.seaweedfs.namespace.clone(),
        };
        let jj_manager = Arc::new(
            JjRepoManager::new(
                std::path::PathBuf::from(&settings.jj_repos.root),
                jj_seaweedfs_config,
            )
            .wrap_err(
                "Failed to initialize Jujutsu repository manager. \n\
                 Ensure the jj_repos.root directory exists and is writable.",
            )?,
        );

        // 5. Create repositories
        tracing::debug!("Creating repository layer");
        let blob_repo = Arc::new(BlobRepository::new(
            db.clone(),
            seaweedfs.clone(),
            settings.seaweedfs.namespace.clone(),
        ));

        let source_archive_repo =
            Arc::new(SourceArchiveRepository::new(db.clone(), blob_repo.clone()));

        let gate_repo = Arc::new(GateRepository::new(db.clone(), jj_manager.clone()));

        let component_repo = Arc::new(ComponentRepository::new(
            db.clone(),
            blob_repo.clone(),
            jj_manager.clone(),
        ));

        let actor_repo = Arc::new(ActorRepository::new(db.clone()));

        // 6. Create services
        tracing::debug!("Creating service layer");
        let oidc = Arc::new(OidcService::new(
            settings.oidc.issuer_url.clone(),
            settings.oidc.client_id.clone(),
            settings.oidc.audience.clone(),
        ));

        let auth = Arc::new(AuthService::new(
            actor_repo.clone(),
            oidc.clone(),
            settings.smtp.from.clone(),
            settings.smtp.url.clone(),
        ));

        let rbac = Arc::new(RbacService::new(gate_repo.clone(), component_repo.clone()));

        let gate_manager = Arc::new(GateManager::new(
            gate_repo.clone(),
            component_repo.clone(),
            rbac.clone(),
        ));

        let component_manager = Arc::new(ComponentManager::new(
            component_repo.clone(),
            source_archive_repo.clone(),
            rbac.clone(),
        ));

        let blob_service = Arc::new(BlobService::new(
            db.clone(),
            blob_repo.clone(),
            rbac.clone(),
        ));

        // 7. Initialize AMQP connection pool
        tracing::info!(url = %settings.amqp.url, "Initializing AMQP connection pool");
        let amqp_cfg = deadpool_lapin::Config {
            url: Some(settings.amqp.url.clone()),
            ..Default::default()
        };
        let amqp_pool = amqp_cfg
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))
            .into_diagnostic()
            .wrap_err(
                "Failed to create AMQP connection pool.\n\
                 Ensure RabbitMQ is running and the AMQP URL is correct.\n\
                 Default: amqp://dev:dev@localhost:5672/master",
            )?;

        // 8. Build dispatch service
        let build_job_repo = Arc::new(BuildJobRepository::new((*db).clone()));
        let build_dispatch = Arc::new(BuildDispatchService::new(
            build_job_repo.clone(),
            rbac.clone(),
            amqp_pool.clone(),
            settings.amqp.clone(),
        ));

        tracing::info!("Application state initialized successfully");

        Ok(Self {
            db,
            seaweedfs,
            jj_manager,
            amqp_pool,
            settings,
            blob_repo,
            gate_repo,
            component_repo,
            source_archive_repo,
            actor_repo,
            build_job_repo,
            oidc,
            auth,
            rbac,
            gate_manager,
            component_manager,
            blob_service,
            build_dispatch,
        })
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down application");
        // Database connection pool closes automatically when all Arc references are dropped.
        tracing::info!("Shutdown complete");
        Ok(())
    }
}
