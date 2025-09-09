use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub listen_addr: Option<String>, // e.g. "127.0.0.1:50051"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MongoConfig {
    pub uri: Option<String>,
    pub db: Option<String>,
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
pub struct Settings {
    pub server: ServerConfig,
    pub mongodb: MongoConfig,
    pub smtp: Option<SmtpConfig>,
}

impl Settings {
    pub fn load() -> miette::Result<Self> {
        use config::Config;
        let mut builder = config::Config::builder();

        // Default file: ./forged.toml if exists
        let path = std::env::var("FORGED_CONFIG")
            .ok()
            .unwrap_or_else(|| "forged.toml".to_string());
        if std::path::Path::new(&path).exists() {
            builder = builder.add_source(config::File::with_name(&path));
        }

        // Environment overrides: FORGED__SERVER__LISTEN_ADDR etc.
        builder = builder.add_source(config::Environment::with_prefix("FORGED").separator("__"));

        let cfg = builder
            .build()
            .map_err(|e| miette::miette!("load config: {e}"))?;
        let settings: Settings = cfg
            .try_deserialize()
            .map_err(|e| miette::miette!("deserialize config: {e}"))?;
        Ok(settings)
    }
}
