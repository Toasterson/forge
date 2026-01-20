use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String, // e.g. "0.0.0.0:50051"
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:50051".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    #[serde(default = "default_postgres_url")]
    pub url: String, // e.g. "postgresql://forged:forged@localhost/forged"
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            url: default_postgres_url(),
            max_connections: default_max_connections(),
        }
    }
}

fn default_postgres_url() -> String {
    "postgresql://forged:forged@localhost/forged".to_string()
}

fn default_max_connections() -> u32 {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeaweedFsConfig {
    #[serde(default = "default_seaweedfs_master_url")]
    pub master_url: String, // e.g. "http://localhost:9333"
    #[serde(default = "default_seaweedfs_namespace")]
    pub namespace: String, // Default: "default"
}

impl Default for SeaweedFsConfig {
    fn default() -> Self {
        Self {
            master_url: default_seaweedfs_master_url(),
            namespace: default_seaweedfs_namespace(),
        }
    }
}

fn default_seaweedfs_master_url() -> String {
    "http://localhost:9333".to_string()
}

fn default_seaweedfs_namespace() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JjReposConfig {
    #[serde(default = "default_jj_repos_root")]
    pub root: String, // e.g. "/var/lib/forged/jj-repos"
}

impl Default for JjReposConfig {
    fn default() -> Self {
        Self {
            root: default_jj_repos_root(),
        }
    }
}

fn default_jj_repos_root() -> String {
    "./data/jj-repos".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    #[serde(default)]
    pub issuer_url: String, // e.g. "https://auth.example.com"
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub audience: String, // Expected audience claim in tokens
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuer_url: String::new(),
            client_id: String::new(),
            audience: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub postgres: PostgresConfig,
    #[serde(default)]
    pub seaweedfs: SeaweedFsConfig,
    #[serde(default)]
    pub jj_repos: JjReposConfig,
    #[serde(default)]
    pub oidc: OidcConfig,
}

impl Settings {
    pub fn load() -> miette::Result<Self> {
        use miette::{Context, IntoDiagnostic};
        let mut builder = config::Config::builder();

        // Determine config source
        let mut source_desc = String::from("env only");

        // Config file selection:
        // - If FORGED_CONFIG is set, it must exist; otherwise it's an error.
        // - If not set, use ./forged.toml if it exists; otherwise proceed with env-only defaults.
        if let Ok(explicit_path) = std::env::var("FORGED_CONFIG") {
            if !std::path::Path::new(&explicit_path).exists() {
                return Err(miette::miette!(
                    "Config file not found at path specified by FORGED_CONFIG: {}\n\
                     Either create the file or unset the FORGED_CONFIG environment variable.",
                    explicit_path
                ));
            }
            builder = builder.add_source(config::File::with_name(&explicit_path));
            source_desc = format!("file: {}", explicit_path);
        } else {
            let default_path = "forged.toml";
            if std::path::Path::new(default_path).exists() {
                builder = builder.add_source(config::File::with_name(default_path));
                source_desc = format!("file: {}", default_path);
            }
        }

        // Environment overrides: FORGED__SERVER__LISTEN_ADDR etc.
        builder = builder.add_source(config::Environment::with_prefix("FORGED").separator("__"));

        let cfg = builder
            .build()
            .into_diagnostic()
            .wrap_err_with(|| format!("load config ({})", source_desc))?;
        let settings: Settings = cfg
            .try_deserialize()
            .into_diagnostic()
            .wrap_err("deserialize config into Settings")?;

        // Validate required fields
        settings.validate()?;

        Ok(settings)
    }

    fn validate(&self) -> miette::Result<()> {
        if self.postgres.url.is_empty() {
            return Err(miette::miette!(
                "PostgreSQL URL is required. \n\
                 Set FORGED__POSTGRES__URL environment variable or add to forged.toml:\n\
                 [postgres]\n\
                 url = \"postgresql://user:password@host/database\""
            ));
        }

        if self.seaweedfs.master_url.is_empty() {
            return Err(miette::miette!(
                "SeaweedFS master URL is required. \n\
                 Set FORGED__SEAWEEDFS__MASTER_URL environment variable or add to forged.toml:\n\
                 [seaweedfs]\n\
                 master_url = \"http://localhost:9333\""
            ));
        }

        // OIDC is optional for MVP (stub implementation)
        // In production, these would be required

        Ok(())
    }
}
