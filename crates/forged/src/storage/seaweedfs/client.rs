use crate::types::ContentHash;
use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SeaweedFsConfig {
    pub master_url: String,
    pub namespace: String,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for SeaweedFsConfig {
    fn default() -> Self {
        Self {
            master_url: "http://localhost:9333".to_string(),
            namespace: "default".to_string(),
            connect_timeout_secs: 5,
            request_timeout_secs: 30,
            max_retries: 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SeaweedFsClient {
    master_url: String,
    http_client: reqwest::Client,
    /// Namespace prefix for blob keys to support multi-tenancy
    #[allow(dead_code)]
    namespace: String,
    max_retries: u32,
}

/// Retry a fallible async operation with exponential backoff.
///
/// Retries only on HTTP 5xx responses or connection/transport errors.
/// Starts at 100ms delay with a backoff factor of 2.
async fn retry_with_backoff<F, Fut, T>(f: F, max_retries: u32) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0u32;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(err) if attempt < max_retries && is_retryable(&err) => {
                attempt += 1;
                let delay = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                tracing::warn!(
                    attempt,
                    max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %err,
                    "SeaweedFS request failed, retrying"
                );
                tokio::time::sleep(delay).await;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Determine whether an error is retryable (5xx or connection failure).
fn is_retryable(err: &miette::Report) -> bool {
    let msg = format!("{err:?}");
    // Connection/transport errors
    if msg.contains("connection")
        || msg.contains("Connection")
        || msg.contains("timed out")
        || msg.contains("dns error")
        || msg.contains("broken pipe")
    {
        return true;
    }
    // HTTP 5xx status codes
    if msg.contains("500 Internal Server Error")
        || msg.contains("502 Bad Gateway")
        || msg.contains("503 Service Unavailable")
        || msg.contains("504 Gateway Timeout")
    {
        return true;
    }
    false
}

impl SeaweedFsClient {
    pub fn new(config: SeaweedFsConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .expect("failed to build reqwest client for SeaweedFS");

        Self {
            master_url: config.master_url,
            http_client,
            namespace: config.namespace,
            max_retries: config.max_retries,
        }
    }

    /// Write blob with content-addressed key
    pub async fn write_blob(&self, key: &BlobKey, data: &[u8]) -> Result<BlobMetadata> {
        let data = data.to_vec();
        let key = key.clone();

        retry_with_backoff(
            || {
                let data = data.clone();
                let key = key.clone();
                async move { self.write_blob_inner(&key, &data).await }
            },
            self.max_retries,
        )
        .await
    }

    async fn write_blob_inner(&self, key: &BlobKey, data: &[u8]) -> Result<BlobMetadata> {
        // 1. Request file assignment from master
        let assign_resp = self
            .assign_file_id()
            .await
            .wrap_err("failed to assign file id from SeaweedFS master")?;

        // 2. Upload to assigned volume server
        let upload_url = format!("http://{}/{}", assign_resp.public_url, assign_resp.fid);

        self.http_client
            .post(&upload_url)
            .body(data.to_vec())
            .send()
            .await
            .into_diagnostic()
            .wrap_err("failed to upload blob to SeaweedFS volume server")?
            .error_for_status()
            .into_diagnostic()
            .wrap_err("SeaweedFS volume server returned error")?;

        // 3. Return metadata
        let metadata = BlobMetadata {
            key: key.clone(),
            fid: assign_resp.fid,
            size: data.len() as u64,
            created_at: chrono::Utc::now(),
        };

        Ok(metadata)
    }

    /// Read blob by content-addressed key
    /// Note: This requires looking up the fid from the key, which is done via PostgreSQL
    /// The actual implementation will need access to the database connection
    pub async fn read_blob_by_fid(&self, fid: &str) -> Result<Vec<u8>> {
        let fid = fid.to_string();
        retry_with_backoff(
            || {
                let fid = fid.clone();
                async move { self.read_blob_by_fid_inner(&fid).await }
            },
            self.max_retries,
        )
        .await
    }

    async fn read_blob_by_fid_inner(&self, fid: &str) -> Result<Vec<u8>> {
        // Look up which volume server has this fid
        let lookup_url = format!("{}/dir/lookup?volumeId={}", self.master_url, fid);

        let lookup_resp: LookupResponse = self
            .http_client
            .get(&lookup_url)
            .send()
            .await
            .into_diagnostic()
            .wrap_err("failed to lookup volume for fid")?
            .json()
            .await
            .into_diagnostic()
            .wrap_err("failed to parse lookup response")?;

        // Fetch from volume server
        let fetch_url = if let Some(public_url) = lookup_resp.locations.first() {
            format!("http://{}/{}", public_url.public_url, fid)
        } else {
            return Err(miette::miette!(
                "no volume locations found for fid: {}",
                fid
            ));
        };

        let bytes = self
            .http_client
            .get(&fetch_url)
            .send()
            .await
            .into_diagnostic()
            .wrap_err("failed to fetch blob from volume server")?
            .bytes()
            .await
            .into_diagnostic()
            .wrap_err("failed to read blob bytes")?;

        Ok(bytes.to_vec())
    }

    async fn assign_file_id(&self) -> Result<AssignResponse> {
        let url = format!("{}/dir/assign", self.master_url);
        let resp: AssignResponse = self
            .http_client
            .get(&url)
            .send()
            .await
            .into_diagnostic()
            .wrap_err("failed to request file assignment")?
            .json()
            .await
            .into_diagnostic()
            .wrap_err("failed to parse assign response")?;

        Ok(resp)
    }

    /// Check whether the SeaweedFS master is reachable and healthy.
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/cluster/status", self.master_url);

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to connect to SeaweedFS master at {}.\n\
                     Ensure the SeaweedFS master is running and reachable.",
                    self.master_url
                )
            })?;

        if !resp.status().is_success() {
            return Err(miette::miette!(
                "SeaweedFS master health check returned HTTP {}.\n\
                 The master at {} may be degraded or misconfigured.",
                resp.status(),
                self.master_url
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobKey {
    /// Content hash (SHA256)
    pub hash: ContentHash,
    /// Blob type for namespacing
    pub blob_type: BlobType,
}

impl BlobKey {
    pub fn new(hash: ContentHash, blob_type: BlobType) -> Self {
        Self { hash, blob_type }
    }

    pub fn to_path(&self) -> String {
        let hex = self.hash.hex();
        format!(
            "{}/{}/{}",
            self.blob_type.as_str(),
            &hex[0..2], // sharding
            &hex[2..]
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlobType {
    Commit,
    Tree,
    File,
    Symlink,
    Conflict,
}

impl BlobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlobType::Commit => "commit",
            BlobType::Tree => "tree",
            BlobType::File => "file",
            BlobType::Symlink => "symlink",
            BlobType::Conflict => "conflict",
        }
    }
}

impl std::fmt::Display for BlobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct BlobMetadata {
    pub key: BlobKey,
    pub fid: String,
    pub size: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct AssignResponse {
    fid: String,
    #[allow(dead_code)]
    url: String,
    #[serde(rename = "publicUrl")]
    public_url: String,
    #[allow(dead_code)]
    count: u32,
}

#[derive(Debug, Deserialize)]
struct LookupResponse {
    #[serde(rename = "volumeId")]
    #[allow(dead_code)]
    volume_id: String,
    locations: Vec<LocationInfo>,
}

#[derive(Debug, Deserialize)]
struct LocationInfo {
    #[allow(dead_code)]
    url: String,
    #[serde(rename = "publicUrl")]
    public_url: String,
}
