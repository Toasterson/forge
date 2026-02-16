use crate::types::ContentHash;
use miette::{Context, IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SeaweedFsConfig {
    pub master_url: String,
    pub namespace: String,
}

#[derive(Clone, Debug)]
pub struct SeaweedFsClient {
    master_url: String,
    http_client: reqwest::Client,
    /// Namespace prefix for blob keys to support multi-tenancy
    #[allow(dead_code)]
    namespace: String,
}

impl SeaweedFsClient {
    pub fn new(config: SeaweedFsConfig) -> Self {
        Self {
            master_url: config.master_url,
            http_client: reqwest::Client::new(),
            namespace: config.namespace,
        }
    }

    /// Write blob with content-addressed key
    pub async fn write_blob(&self, key: &BlobKey, data: &[u8]) -> Result<BlobMetadata> {
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
    url: String,
    #[serde(rename = "publicUrl")]
    public_url: String,
    count: u32,
}

#[derive(Debug, Deserialize)]
struct LookupResponse {
    #[serde(rename = "volumeId")]
    volume_id: String,
    locations: Vec<LocationInfo>,
}

#[derive(Debug, Deserialize)]
struct LocationInfo {
    url: String,
    #[serde(rename = "publicUrl")]
    public_url: String,
}
