use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub listen_addr: Option<String>, // e.g. "127.0.0.1:50051"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SurrealConfig {
    /// Mode: "clustered" (remote) or "embedded" (rocksdb)
    pub mode: Option<String>,
    /// Remote endpoint, e.g. ws://127.0.0.1:8000
    pub endpoint: Option<String>,
    /// Credentials for remote mode
    pub username: Option<String>,
    pub password: Option<String>,
    /// Namespace and database
    pub namespace: Option<String>,
    pub database: Option<String>,
    /// Filesystem path for embedded RocksDB, e.g. ./data/surreal
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmtpConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: Option<String>,
    pub starttls: Option<bool>, // default true when host present
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitS3Config {
    pub bucket: Option<String>,
    pub prefix: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitStorageConfig {
    /// One of: "fs" or "s3". Default: "fs".
    pub mode: Option<String>,
    /// Root path on filesystem when mode == "fs". Default: ./data/repos
    pub root: Option<String>,
    /// S3 settings when mode == "s3"
    pub s3: Option<GitS3Config>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub surreal: SurrealConfig,
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
    #[serde(default)]
    pub repos: GitStorageConfig,
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
                    "config file not found at path specified by FORGED_CONFIG: {explicit_path}"
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
        Ok(settings)
    }
}
