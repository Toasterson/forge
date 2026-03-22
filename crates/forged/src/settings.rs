use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String, // e.g. "0.0.0.0:50051"
    /// Maximum upload size in bytes (default: 2 GiB)
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: u64,
    /// Default page size for list operations
    #[serde(default = "default_page_size")]
    pub default_page_size: u32,
    /// Maximum page size for list operations
    #[serde(default = "default_max_page_size")]
    pub max_page_size: u32,
    /// Maximum number of requests allowed per rate limit window
    #[serde(default = "default_rate_limit_requests")]
    pub rate_limit_requests: u64,
    /// Duration of the rate limit window in seconds
    #[serde(default = "default_rate_limit_per_secs")]
    pub rate_limit_per_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            max_upload_size: default_max_upload_size(),
            default_page_size: default_page_size(),
            max_page_size: default_max_page_size(),
            rate_limit_requests: default_rate_limit_requests(),
            rate_limit_per_secs: default_rate_limit_per_secs(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:50051".to_string()
}

fn default_rate_limit_requests() -> u64 {
    100
}

fn default_rate_limit_per_secs() -> u64 {
    1
}

fn default_max_upload_size() -> u64 {
    2 * 1024 * 1024 * 1024 // 2 GiB
}

fn default_page_size() -> u32 {
    50
}

fn default_max_page_size() -> u32 {
    1000
}

/// TLS mode
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TlsMode {
    #[default]
    None,
    Manual,
    Acme,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManualTlsConfig {
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub client_ca_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeConfig {
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub contact: Vec<String>,
    #[serde(default = "default_acme_cache_dir")]
    pub cache_dir: String,
    #[serde(default = "default_acme_directory_url")]
    pub directory_url: String,
    #[serde(default = "default_acme_challenge_type")]
    pub challenge_type: String,
    #[serde(default = "default_acme_http_listen_addr")]
    pub http_listen_addr: String,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            domains: Vec::new(),
            contact: Vec::new(),
            cache_dir: default_acme_cache_dir(),
            directory_url: default_acme_directory_url(),
            challenge_type: default_acme_challenge_type(),
            http_listen_addr: default_acme_http_listen_addr(),
        }
    }
}

fn default_acme_cache_dir() -> String { "./data/acme".to_string() }
fn default_acme_directory_url() -> String { "https://acme-v02.api.letsencrypt.org/directory".to_string() }
fn default_acme_challenge_type() -> String { "http-01".to_string() }
fn default_acme_http_listen_addr() -> String { "0.0.0.0:80".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsConfig {
    #[serde(default)]
    pub mode: TlsMode,
    #[serde(default)]
    pub manual: ManualTlsConfig,
    #[serde(default)]
    pub acme: AcmeConfig,
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
    /// Timeout for establishing a connection to SeaweedFS (seconds)
    #[serde(default = "default_seaweedfs_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Timeout for the entire HTTP request to SeaweedFS (seconds)
    #[serde(default = "default_seaweedfs_request_timeout")]
    pub request_timeout_secs: u64,
    /// Maximum number of retry attempts for failed SeaweedFS operations
    #[serde(default = "default_seaweedfs_max_retries")]
    pub max_retries: u32,
}

impl Default for SeaweedFsConfig {
    fn default() -> Self {
        Self {
            master_url: default_seaweedfs_master_url(),
            namespace: default_seaweedfs_namespace(),
            connect_timeout_secs: default_seaweedfs_connect_timeout(),
            request_timeout_secs: default_seaweedfs_request_timeout(),
            max_retries: default_seaweedfs_max_retries(),
        }
    }
}

fn default_seaweedfs_master_url() -> String {
    "http://localhost:9333".to_string()
}

fn default_seaweedfs_namespace() -> String {
    "default".to_string()
}

fn default_seaweedfs_connect_timeout() -> u64 {
    5
}

fn default_seaweedfs_request_timeout() -> u64 {
    30
}

fn default_seaweedfs_max_retries() -> u32 {
    3
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// Sender email address for outgoing emails (e.g. "forge@example.com")
    #[serde(default)]
    pub from: String,
    /// SMTP relay URL (e.g. "smtp.example.com"). If empty, email sending is disabled.
    #[serde(default)]
    pub url: Option<String>,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            from: String::new(),
            url: None,
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
    #[serde(default)]
    pub smtp: SmtpConfig,
    #[serde(default)]
    pub amqp: crate::services::AmqpConfig,
    #[serde(default)]
    pub tls: TlsConfig,
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

        match self.tls.mode {
            TlsMode::Manual => {
                if self.tls.manual.cert_file.is_empty() || self.tls.manual.key_file.is_empty() {
                    return Err(miette::miette!(
                        "TLS mode is 'manual' but cert_file or key_file is missing."
                    ));
                }
            }
            TlsMode::Acme => {
                if self.tls.acme.domains.is_empty() {
                    return Err(miette::miette!(
                        "TLS mode is 'acme' but no domains are configured."
                    ));
                }
                if self.tls.acme.contact.is_empty() {
                    return Err(miette::miette!(
                        "TLS mode is 'acme' but no contact addresses are configured."
                    ));
                }
            }
            TlsMode::None => {}
        }

        Ok(())
    }
}
