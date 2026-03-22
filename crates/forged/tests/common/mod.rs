use forged::{AppState, Settings};
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use uuid::Uuid;

pub mod fixtures;

pub struct TestContext {
    pub app_state: AppState,
    pub db: Arc<DatabaseConnection>,
    pub test_db_name: String,
    pub test_namespace: String,
}

impl TestContext {
    /// Create isolated test context with unique database
    pub async fn new() -> Self {
        let test_id = Uuid::new_v4().simple().to_string();
        let test_db_name = format!("forged_test_{}", test_id);
        let test_namespace = format!("test_{}", test_id);

        // Create test database
        Self::create_test_database(&test_db_name).await;

        // Create JJ repos directory
        let jj_repos_path = format!("./test_data/{}/jj-repos", test_db_name);
        std::fs::create_dir_all(&jj_repos_path).expect("Failed to create test JJ repos directory");

        let settings = Settings {
            postgres: forged::settings::PostgresConfig {
                url: std::env::var("TEST_DATABASE_URL")
                    .unwrap_or_else(|_| {
                        "postgresql://forged:forged@localhost/forged_test".to_string()
                    })
                    .replace("/forged_test", &format!("/{}", test_db_name)),
                max_connections: 5,
                ..Default::default()
            },
            seaweedfs: forged::settings::SeaweedFsConfig {
                master_url: std::env::var("TEST_SEAWEEDFS_URL")
                    .unwrap_or_else(|_| "http://localhost:9333".to_string()),
                namespace: test_namespace.clone(),
                ..Default::default()
            },
            jj_repos: forged::settings::JjReposConfig {
                root: format!("./test_data/{}/jj-repos", test_db_name),
            },
            ..Default::default()
        };

        let app_state = AppState::new(settings)
            .await
            .expect("Failed to create test AppState");

        let db = app_state.db.clone();

        Self {
            app_state,
            db,
            test_db_name,
            test_namespace,
        }
    }

    async fn create_test_database(db_name: &str) {
        // Connect to postgres database to create test database
        let base_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://forged:forged@localhost/forged_test".to_string());
        let admin_url = base_url.replace("/forged_test", "/postgres");

        let admin_db = sea_orm::Database::connect(&admin_url)
            .await
            .expect("Failed to connect to postgres database");

        // Drop if exists, then create
        use sea_orm::ConnectionTrait;
        let _ = admin_db
            .execute_unprepared(&format!("DROP DATABASE IF EXISTS {}", db_name))
            .await;
        admin_db
            .execute_unprepared(&format!("CREATE DATABASE {}", db_name))
            .await
            .expect("Failed to create test database");
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Cleanup test database and Jujutsu repos
        let db_name = self.test_db_name.clone();
        let jj_path = format!("./test_data/{}", db_name);

        tokio::spawn(async move {
            // Drop database
            let base_url = std::env::var("TEST_DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://forged:forged@localhost/forged_test".to_string());
            let admin_url = base_url.replace("/forged_test", "/postgres");

            if let Ok(admin_db) = sea_orm::Database::connect(&admin_url).await {
                use sea_orm::ConnectionTrait;
                let _ = admin_db
                    .execute_unprepared(&format!("DROP DATABASE IF EXISTS {}", db_name))
                    .await;
            }

            // Remove Jujutsu repos
            if std::path::Path::new(&jj_path).exists() {
                let _ = std::fs::remove_dir_all(&jj_path);
            }
        });
    }
}
