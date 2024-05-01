use axum::extract::multipart::MultipartError;
use axum::extract::{Host, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use clap::{Parser, Subcommand};
use config::Environment;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueDeclareOptions,
};
use deadpool_lapin::lapin::types::FieldTable;
use deadpool_lapin::Pool;
use forge::{AuthConfig, FileKindError, OpenIdConfig};
use futures::{join, StreamExt};
use message_queue::handle_message;
use miette::Diagnostic;
use opendal::Operator;
use pasetors::claims::ClaimsValidationRules;
use pasetors::keys::{AsymmetricKeyPair, AsymmetricPublicKey, Generate};
use pasetors::paserk::FormatAsPaserk;
use pasetors::token::UntrustedToken;
use pasetors::{version4::V4, Public};
use prisma::PrismaClient;
use serde::{Deserialize, Serialize};
use std::future::IntoFuture;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi, ToSchema,
};
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

mod api;
mod message_queue;
#[allow(warnings, unused)]
mod prisma;

#[derive(Parser, Debug)]
pub struct Args {
    pub rabbitmq_url: Option<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand, Default)]
pub enum Commands {
    #[default]
    Start,
    GenDomain {
        name: String,
        #[arg(long)]
        gh_client_id: Option<String>,
        #[arg(long)]
        gl_client_id: Option<String>,
    },
    SetDomain {
        name: String,
        #[arg(long)]
        gh_client_id: Option<String>,
        #[arg(long)]
        gl_client_id: Option<String>,
    },
}

#[derive(Debug, Diagnostic, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    AddrParse(#[from] std::net::AddrParseError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    CreatePool(#[from] deadpool_lapin::CreatePoolError),

    #[error(transparent)]
    Lapin(#[from] deadpool_lapin::lapin::Error),

    #[error(transparent)]
    Pool(#[from] deadpool_lapin::PoolError),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    ParseError(#[from] url::ParseError),

    #[error(transparent)]
    NewClient(#[from] prisma_client_rust::NewClientError),

    #[error(transparent)]
    QueryError(#[from] prisma_client_rust::QueryError),

    #[error(transparent)]
    OpenDal(#[from] opendal::Error),

    #[error("no version found in component with name: {0}")]
    NoVersionFoundInRecipe(String),

    #[error("no revision found in component with name: {0}")]
    NoRevisionFoundInRecipe(String),

    #[error("no project URL found in component with name: {0}")]
    NoProjectUrlFoundInRecipe(String),

    #[error(transparent)]
    MultipartError(#[from] MultipartError),

    #[error("no component found")]
    NoComponentFound,

    #[error("no id found in gate object: {0}")]
    NoIdFoundINGate(String),

    #[error("{0}")]
    String(String),

    #[error("entity not found {0}")]
    NotFound(String),

    #[error("neither url nor file provided in upload")]
    NoFileOrUrl,

    #[error("no domain found")]
    NoDomainFound,

    #[error("invalid multipart request ")]
    InvalidMultipartRequest,

    #[error("unauthorized")]
    Unauthorized,

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FileKindError(#[from] FileKindError),

    #[error("user is unauthorized to claim this existing handle")]
    UnauthorizedToClaimHandle,

    #[error(transparent)]
    OctoRustClientError(#[from] octorust::ClientError),

    #[error(transparent)]
    OSSH(#[from] openssh_keys::errors::OpenSSHKeyError),

    #[error(transparent)]
    Pasetors(#[from] pasetors::errors::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
}

pub type Result<T> = miette::Result<T, Error>;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub enum ApiError {
    // Bad input Request
    BadRequest(String),
    // Not Authorized
    Unauthorized,
    // Entity not found
    NotFound(String),
    // Internal server error
    ServerError(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NoRevisionFoundInRecipe(msg) => {
                (StatusCode::BAD_REQUEST, Json(ApiError::BadRequest(msg))).into_response()
            }
            Error::NoProjectUrlFoundInRecipe(msg) => {
                (StatusCode::BAD_REQUEST, Json(ApiError::BadRequest(msg))).into_response()
            }
            Error::NoDomainFound => (
                StatusCode::NOT_FOUND,
                Json(ApiError::NotFound("no domain found".to_string())),
            )
                .into_response(),
            Error::NoComponentFound => (
                StatusCode::NOT_FOUND,
                Json(ApiError::NotFound("no component found".to_string())),
            )
                .into_response(),
            Error::NoIdFoundINGate(msg) => {
                (StatusCode::NOT_FOUND, Json(ApiError::NotFound(msg))).into_response()
            }
            Error::NotFound(msg) => {
                (StatusCode::NOT_FOUND, Json(ApiError::NotFound(msg))).into_response()
            }
            err => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::ServerError(err.to_string())),
            )
                .into_response(),
        }
    }
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("apikey"))),
            )
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub amqp: deadpool_lapin::Config,
    pub listen: String,
    pub job_inbox: String,
    pub inbox: String,
    pub connection_string: String,
    pub opendal: OpenDalConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenDalConfig {
    pub service: String,
    pub endpoint: String,
    pub bucket: String,
    pub key_id: String,
    pub secret_key: String,
}

pub fn load_config(args: &Args) -> Result<Config> {
    let cfg = config::Config::builder()
        .add_source(config::File::with_name("/etc/forge/forged").required(false))
        .add_source(config::File::with_name("forged").required(false))
        .add_source(
            Environment::with_prefix("APP")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3100")?
        .set_default("job_inbox", "JOB_INBOX")?
        .set_default("inbox", "INBOX")?
        .set_default("opendal.endpoint", "http://localhost:9000")?
        .set_default("opendal.bucket", "forge")?
        .set_default("opendal.service", "s3")?
        .set_default("opendal.key_id", "")?
        .set_default("opendal.secret_key", "")?
        .set_default(
            "connection_string",
            "postgres://forge:forge@localhost/forge",
        )?
        .set_default("amqp.url", "amqp://dev:dev@localhost:5672/master")?
        .set_override_option("amqp.url", args.rabbitmq_url.clone())?
        .build()?;
    Ok(cfg.try_deserialize()?)
}

#[derive(OpenApi)]
#[openapi(
    info(
        description = "Manage your interactions with a distribution community",
        version = "v1",
        title = "Package forge API",
        license(name= "MPL-2.0", url = "https://www.mozilla.org/en-US/MPL/2.0/"),
        contact(
            name = "Till Wegmüller",
            email = "toasterson@gmail.com"
        )
    ),
    paths(
        api::v1::actor::actor_connect,
        api::v1::gate::get_gate,
        api::v1::gate::list_gates,
        api::v1::gate::create_gate,
        api::v1::gate::update_gate,
        api::v1::component::get_component,
        api::v1::component::list_components,
        api::v1::component::create_component,
        api::v1::component::import_component,
        api::v1::publisher::create_publisher,
        api::v1::publisher::list_publishers,
        api::v1::auth::login_info,
    ),
    components(
      schemas(
        api::v1::actor::ActorConnectRequest, 
        api::v1::actor::ActorSSHKeyFingerprint, 
        api::v1::actor::ActorConnectResponse,
        api::v1::gate::GateSearchRequest,
        api::v1::gate::Gate,
        api::v1::gate::GateListRequest,
        api::v1::gate::CreateGateInput,
        api::v1::gate::UpdateGateInput,
        api::v1::component::GetComponentRequest,
        api::v1::component::Component,
        api::v1::component::ListComponentRequest,
        api::v1::component::ComponentInput,
        api::v1::component::ComponentIdentifier,
        api::v1::publisher::Publisher,
        api::v1::publisher::CreatePublisherInput,
        api::v1::auth::AuthConfig,
        api::v1::auth::OpenIdConfig,
        api::v1::PaginationInput,
        component::PackageMeta,
        component::ComponentMetadataItem,
        component::ComponentMetadata,
        component::Recipe,
        component::Dependency,
        component::DependencyKind,
        component::SourceSection,
        component::SourceNode,
        component::ArchiveSource,
        component::GitSource,
        component::FileSource,
        component::DirectorySource,
        component::PatchSource,
        component::OverlaySource,
        component::BuildSection,
        component::ConfigureBuildSection,
        component::ScriptBuildSection,
        component::InstallDirectiveNode,
        component::ScriptNode,
        component::BuildFlagNode,
        component::BuildOptionNode,
        component::FileNode,
        ApiError,
      )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "forge", description = "Forge your packages")
    )
)]
struct ApiDoc;

pub async fn gen_domain(cfg: Config, name: String, gh_client_id: Option<String>) -> Result<()> {
    debug!("Opening Database Connection");
    let db_conn = prisma::PrismaClient::_builder()
        .with_url(cfg.connection_string.clone())
        .build()
        .await?;

    let auth_conf = serde_json::to_value(&AuthConfig {
        github: gh_client_id.map(|id| OpenIdConfig { client_id: id }),
        gitlab: None,
    })?;

    let kp = AsymmetricKeyPair::<V4>::generate()?;
    let mut secret_key_str = String::new();
    let mut public_key_str = String::new();

    kp.secret.fmt(&mut secret_key_str)?;
    kp.public.fmt(&mut public_key_str)?;

    db_conn
        .domain()
        .create(name, auth_conf, secret_key_str, public_key_str, vec![])
        .exec()
        .await?;

    Ok(())
}

pub async fn set_domain(cfg: Config, name: String, gh_client_id: Option<String>) -> Result<()> {
    debug!("Opening Database Connection");
    let db_conn = prisma::PrismaClient::_builder()
        .with_url(cfg.connection_string.clone())
        .build()
        .await?;

    let db_domain = db_conn
        .domain()
        .find_unique(prisma::domain::UniqueWhereParam::DnsNameEquals(
            name.clone(),
        ))
        .exec()
        .await?;

    if let Some(db_domain) = db_domain {
        let mut auth_conf: AuthConfig = serde_json::from_value(db_domain.authconf)?;

        auth_conf.github = gh_client_id.map(|cid| OpenIdConfig { client_id: cid });

        db_conn
            .domain()
            .update(
                prisma::domain::UniqueWhereParam::DnsNameEquals(name),
                vec![prisma::domain::SetParam::SetAuthconf(serde_json::to_value(
                    &auth_conf,
                )?)],
            )
            .exec()
            .await?;
    } else {
        error!("Domain does not exist");
    }

    Ok(())
}

async fn authorize_token_middleware(
    State(state): State<SharedState>,
    // you can add more extractors here but the last
    // extractor must implement `FromRequest` which
    // `Request` does
    Host(host): Host,
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    mut request: Request,
    next: Next,
) -> Response {
    let domain = match state
        .lock()
        .await
        .prisma
        .domain()
        .find_unique(prisma::domain::UniqueWhereParam::DnsNameEquals(host))
        .exec()
        .await
    {
        Ok(d) => d,
        Err(err) => {
            error!("Could not find keys in database for domain: {}", err);
            return (StatusCode::UNAUTHORIZED, Json(ApiError::Unauthorized)).into_response();
        }
    };

    let response: Response = if let Some(domain) = domain {
        let public_key = match AsymmetricPublicKey::<V4>::try_from(domain.public_key.as_str()).ok()
        {
            None => {
                error!(
                    "could not deserialize public key from database for domain: {}",
                    &domain.dns_name
                );
                return (StatusCode::UNAUTHORIZED, Json(ApiError::Unauthorized)).into_response();
            }
            Some(k) => k,
        };

        let untrusted_token = match UntrustedToken::<Public, V4>::try_from(authorization.token()) {
            Ok(token) => token,
            Err(err) => {
                error!("could not get token from request: {}", err);
                return (StatusCode::UNAUTHORIZED, Json(ApiError::Unauthorized)).into_response();
            }
        };
        let validation_rules = ClaimsValidationRules::new();
        let trusted_token = match pasetors::public::verify(
            &public_key,
            &untrusted_token,
            &validation_rules,
            None,
            None,
        )
        .ok()
        {
            Some(token) => token,
            None => {
                return (StatusCode::UNAUTHORIZED, Json(ApiError::Unauthorized)).into_response();
            }
        };

        request.extensions_mut().insert(trusted_token);

        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, Json(ApiError::Unauthorized)).into_response()
    };

    response
}

#[derive(Debug)]
struct AppState {
    amqp: Pool,
    prisma: PrismaClient,
    fs_operator: Operator,
    job_inbox: String,
    inbox: String,
}

type SharedState = Arc<Mutex<AppState>>;

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening Database Connection");
    let db_conn = prisma::PrismaClient::_builder()
        .with_url(cfg.connection_string.clone())
        .build()
        .await?;

    debug!("Setting up Filesystem Operator");
    let mut op_builder = opendal::services::S3::default();
    op_builder.endpoint(&cfg.opendal.endpoint);
    op_builder.region("default");
    op_builder.bucket(&cfg.opendal.bucket);
    op_builder.access_key_id(&cfg.opendal.key_id);
    op_builder.secret_access_key(&cfg.opendal.secret_key);

    let fs_operator = Operator::new(op_builder)?
        .layer(opendal::layers::LoggingLayer::default())
        .finish();

    debug!("Checking if operator is setup correctly");
    fs_operator.check().await?;

    debug!("Opening RabbitMQ Connection");
    let state = Arc::new(Mutex::new(AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        prisma: db_conn,
        fs_operator,
        job_inbox: cfg.job_inbox.clone(),
        inbox: cfg.inbox.clone(),
    }));
    let conn = state.lock().await.amqp.get().await?;
    let job_inbox = cfg.job_inbox.clone();
    let inbox = cfg.inbox.clone();
    debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    debug!(
        "Defining JOB inbox: {} queue from channel id {}",
        job_inbox,
        channel.id()
    );
    channel
        .queue_declare(
            job_inbox.as_str(),
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    debug!(
        "Defining inbox: {} queue from channel id {}",
        inbox,
        channel.id()
    );
    channel
        .queue_declare(
            inbox.as_str(),
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let amqp_consume_pool = state.lock().await.amqp.clone();
    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .merge(RapiDoc::new("/api-docs/openapi.json").path("/rapidoc"))
        .route("/healthz", get(health_check))
        .nest(
            "/api",
            api::get_api_router().layer(middleware::from_fn_with_state(
                state.clone(),
                authorize_token_middleware,
            )),
        )
        .route(
            "/api/v1/actors/connect",
            post(api::v1::actor::actor_connect),
        )
        .route("/api/v1/auth/login_info", get(api::v1::auth::login_info))
        .with_state(state);

    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3100
    let listener = TcpListener::bind(&cfg.listen).await?;
    let _ = join!(
        rabbitmq_listen(
            amqp_consume_pool,
            cfg.connection_string.clone(),
            inbox.as_str(),
            job_inbox.as_str()
        ),
        axum::serve(listener, app.into_make_service()).into_future(),
    );
    Ok(())
}

async fn rabbitmq_listen(
    pool: Pool,
    connection_string: String,
    inbox_name: &str,
    job_inbox_name: &str,
) -> Result<()> {
    let mut retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let database = PrismaClient::_builder()
        .with_url(connection_string)
        .build()
        .await?;
    loop {
        retry_interval.tick().await;
        info!("connecting amqp consumer...");
        match handle_rabbitmq(pool.clone(), &database, inbox_name, job_inbox_name).await {
            Ok(_) => info!("rmq listen returned"),
            Err(e) => error!(error = e.to_string(), "rmq listen had an error"),
        };
    }
}

async fn handle_rabbitmq(
    pool: Pool,
    database: &PrismaClient,
    inbox_name: &str,
    _job_inbox_name: &str,
) -> Result<()> {
    let rmq_con = pool.get().await.map_err(|e| Error::String(e.to_string()))?;
    let channel = rmq_con.create_channel().await?;

    let mut consumer = channel
        .basic_consume(
            inbox_name,
            "forged.consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    info!("amqp consumer connected, waiting for messages");
    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let tag = delivery.delivery_tag;
                match handle_message(delivery, database, &channel).await {
                    Ok(_) => {
                        debug!("handled message");
                        channel.basic_ack(tag, BasicAckOptions::default()).await?;
                    }
                    Err(e) => {
                        error!(error = e.to_string(), "failed to handle message");
                        channel.basic_nack(tag, BasicNackOptions::default()).await?;
                    }
                }
            }
            Err(err) => return Err(Error::String(err.to_string())),
        }
    }

    Ok(())
}

#[derive(Serialize, Default)]
struct HealthResponse {}

async fn health_check(State(state): State<SharedState>) -> Result<Json<HealthResponse>> {
    let conn = state.lock().await.amqp.get().await?;
    if !matches!(
        conn.status().state(),
        deadpool_lapin::lapin::ConnectionState::Connected
    ) {
        return Err(Error::Lapin(
            deadpool_lapin::lapin::Error::InvalidConnectionState(conn.status().state()),
        ));
    }
    Ok(Json(HealthResponse::default()))
}
