//! Forge client library
//!
//! This crate provides a small, ergonomic Rust client for the Forge gRPC API
//! exposed by the `forged` server. It targets ease-of-use with sensible
//! defaults and high-level helpers for the current Smart Git Wire endpoints.
//!
//! Quick start
//! - Add `forged-client` to your Cargo.toml
//! - Connect to your server and call high-level helpers
//!
//! Example: connect, create a repo, push a pack, and fetch the latest pack
//!
//! ```no_run
//! use forged_client::{ForgedClient, AuthKind};
//! use tokio::io::AsyncReadExt;
//!
//! # #[tokio::main]
//! # async fn main() -> miette::Result<()> {
//! let mut client = ForgedClient::connect("http://127.0.0.1:50051").await?;
//!
//! // Create a repository for a component id
//! client.create_repo("com.example.zlib").await?;
//!
//! // Push a prebuilt pack file using an ed25519 key (32-byte seed) and arbitrary proof message
//! let mut pack = tokio::io::empty(); // e.g., replace with tokio::fs::File::open("./zlib.pack").await
//! let private_key_seed = [0u8; 32]; // load from secure storage
//! let proof_msg = b"forge-push-proof-v1";
//! client.push_pack_ed25519(
//!     "com.example.zlib",
//!     "alice@example.com",
//!     AuthKind::User,
//!     "k1",
//!     &private_key_seed,
//!     proof_msg,
//!     &mut pack,
//! ).await?;
//!
//! // Fetch the latest pack
//! let mut buf = Vec::new();
//! client.fetch_latest_pack_bytes("com.example.zlib").await.map(|bytes| buf = bytes)?;
//! # Ok(())
//! # }
//! ```

use miette::{Context, IntoDiagnostic};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::Request;
use tracing::{debug, info, warn};

pub mod api {
    pub mod forged {
        pub mod api {
            pub mod v1 {
                tonic::include_proto!("forged.api.v1");
            }
            pub mod v2 {
                tonic::include_proto!("forged.api.v2");
            }
        }
    }
}

use api::forged::api::v1 as pb;
use pb::git_service_client::GitServiceClient;
use pb::{
    auth_service_client::AuthServiceClient, component_service_client::ComponentServiceClient,
    gate_service_client::GateServiceClient,
};

#[derive(Debug, Clone, Copy)]
pub enum AuthKind {
    User,
    Service,
}

impl From<AuthKind> for pb::ActorKind {
    fn from(v: AuthKind) -> Self {
        match v {
            AuthKind::User => pb::ActorKind::User,
            AuthKind::Service => pb::ActorKind::Service,
        }
    }
}

/// High-level client wrapping tonic-generated service clients.
#[derive(Clone)]
pub struct ForgedClient {
    #[allow(dead_code)]
    channel: Channel,
    pub gate: GateServiceClient<Channel>,
    pub component: ComponentServiceClient<Channel>,
    pub auth: AuthServiceClient<Channel>,
    pub git: GitServiceClient<Channel>,
}

impl ForgedClient {
    /// Connect to a Forge gRPC endpoint. Use a full URI, e.g. `http://127.0.0.1:50051`.
    pub async fn connect(endpoint: &str) -> miette::Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())
            .into_diagnostic()
            .wrap_err_with(|| format!("invalid endpoint URI: {endpoint}"))?
            .connect()
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("connect to {endpoint}"))?;
        Ok(Self::with_channel(channel))
    }

    /// Construct a client from an existing tonic channel.
    pub fn with_channel(channel: Channel) -> Self {
        let gate = GateServiceClient::new(channel.clone());
        let component = ComponentServiceClient::new(channel.clone());
        let auth = AuthServiceClient::new(channel.clone());
        let git = GitServiceClient::new(channel.clone());
        Self {
            channel,
            gate,
            component,
            auth,
            git,
        }
    }

    /// Create a component repository (idempotent). Returns whether it was created (true) or already existed (false).
    pub async fn create_repo(&mut self, component_id: &str) -> miette::Result<bool> {
        let req = pb::CreateRepoRequest {
            component_id: component_id.to_string(),
        };
        let resp = self
            .git
            .create_repo(Request::new(req))
            .await
            .into_diagnostic()
            .wrap_err("create_repo RPC")?;
        Ok(resp.into_inner().created)
    }

    /// Store a new version's package.kdl contents and get the commit id.
    pub async fn put_version(
        &mut self,
        component_id: &str,
        version: &str,
        package_kdl: impl AsRef<[u8]>,
    ) -> miette::Result<String> {
        let req = pb::PutVersionRequest {
            component_id: component_id.to_string(),
            version: version.to_string(),
            package_kdl: package_kdl.as_ref().to_vec(),
        };
        let resp = self
            .git
            .put_version(Request::new(req))
            .await
            .into_diagnostic()
            .wrap_err("put_version RPC")?;
        Ok(resp.into_inner().commit_id)
    }

    /// Convenience: create a SignedMessage using an ed25519 32-byte seed private key.
    pub fn make_ed25519_proof(
        message: &[u8],
        private_key_seed32: &[u8],
    ) -> miette::Result<pb::SignedMessage> {
        use ed25519_dalek::{Signer, SigningKey};
        if private_key_seed32.len() != 32 {
            return Err(miette::miette!("ed25519 private key seed must be 32 bytes"));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(private_key_seed32);
        let sk = SigningKey::from_bytes(&seed);
        let sig = sk.sign(message);
        Ok(pb::SignedMessage {
            algorithm: "ed25519".to_string(),
            message: message.to_vec(),
            signature: sig.to_bytes().to_vec(),
            key_id: String::new(),
        })
    }

    /// Push a packfile to the server using ed25519 proof for authentication.
    /// - `actor_id`: your identity as stored on the server
    /// - `actor_kind`: user or service
    /// - `key_id`: which stored key to use for verification
    /// - `private_key_seed32`: 32-byte seed used to sign the `proof_message`
    /// - `proof_message`: arbitrary message that the server verifies
    /// - `reader`: async reader that yields raw packfile bytes
    #[allow(clippy::too_many_arguments)]
    pub async fn push_pack_ed25519<R: tokio::io::AsyncRead + Unpin>(
        &mut self,
        component_id: &str,
        actor_id: &str,
        actor_kind: AuthKind,
        key_id: &str,
        private_key_seed32: &[u8],
        proof_message: &[u8],
        reader: &mut R,
    ) -> miette::Result<()> {
        use tokio::io::AsyncReadExt;
        use tokio::sync::mpsc;

        let proof = Self::make_ed25519_proof(proof_message, private_key_seed32)?;

        // Prepare client stream
        let (tx, rx) = mpsc::channel::<pb::SmartPushRequest>(8);
        let outbound = ReceiverStream::new(rx);
        let mut stream = self
            .git
            .smart_push(outbound)
            .await
            .into_diagnostic()
            .wrap_err("smart_push start")?
            .into_inner();

        // Send open
        let open = pb::PushOpen {
            component_id: component_id.to_string(),
            actor_id: actor_id.to_string(),
            actor_kind: pb::ActorKind::from(actor_kind) as i32,
            key_id: key_id.to_string(),
            algorithm: "ed25519".to_string(),
            proof: Some(proof),
        };
        tx.send(pb::SmartPushRequest {
            payload: Some(pb::smart_push_request::Payload::Open(open)),
        })
        .await
        .into_diagnostic()
        .wrap_err("send push open")?;

        // Spawn a task to read server responses for progress
        tokio::spawn(async move {
            while let Some(item) = stream.message().await.ok().and_then(|m| m) {
                match item.payload {
                    Some(pb::smart_push_response::Payload::Progress(msg)) => {
                        info!(target = "forged_client", progress = %msg, "push progress");
                    }
                    Some(pb::smart_push_response::Payload::Accepted(_)) => {
                        info!(target = "forged_client", "push accepted");
                    }
                    Some(pb::smart_push_response::Payload::Error(e)) => {
                        warn!(target = "forged_client", error = %e, "push error from server");
                    }
                    None => {}
                }
            }
        });

        // Stream pack chunks
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader
                .read(&mut buf)
                .await
                .into_diagnostic()
                .wrap_err("read pack chunk")?;
            if n == 0 {
                break;
            }
            tx.send(pb::SmartPushRequest {
                payload: Some(pb::smart_push_request::Payload::PackfileChunk(
                    buf[..n].to_vec(),
                )),
            })
            .await
            .into_diagnostic()
            .wrap_err("send packfile chunk")?;
        }
        // Send done and drop sender to finish stream
        tx.send(pb::SmartPushRequest {
            payload: Some(pb::smart_push_request::Payload::Done(true)),
        })
        .await
        .ok();
        drop(tx);

        Ok(())
    }

    /// Fetch the latest stored pack and write it to the given writer.
    pub async fn fetch_latest_pack_to_writer<W: tokio::io::AsyncWrite + Unpin>(
        &mut self,
        component_id: &str,
        mut writer: W,
    ) -> miette::Result<()> {
        use tokio::io::AsyncWriteExt;

        let req = pb::SmartFetchRequest {
            component_id: component_id.to_string(),
        };
        let mut stream = self
            .git
            .smart_fetch(Request::new(req))
            .await
            .into_diagnostic()
            .wrap_err("smart_fetch start")?
            .into_inner();

        while let Some(msg) = stream
            .message()
            .await
            .into_diagnostic()
            .wrap_err("read fetch msg")?
        {
            match msg.payload {
                Some(pb::smart_fetch_response::Payload::PackfileChunk(bytes)) => {
                    writer
                        .write_all(&bytes)
                        .await
                        .into_diagnostic()
                        .wrap_err("write pack chunk")?;
                }
                Some(pb::smart_fetch_response::Payload::Progress(p)) => {
                    debug!(target = "forged_client", progress = %p, "fetch progress");
                }
                Some(pb::smart_fetch_response::Payload::Done(_)) => break,
                Some(pb::smart_fetch_response::Payload::Error(e)) => {
                    return Err(miette::miette!("server reported error: {e}"));
                }
                None => {}
            }
        }
        writer
            .flush()
            .await
            .into_diagnostic()
            .wrap_err("flush writer")?;
        Ok(())
    }

    /// Fetch the latest stored pack into memory. For large packs, prefer `fetch_latest_pack_to_writer`.
    pub async fn fetch_latest_pack_bytes(&mut self, component_id: &str) -> miette::Result<Vec<u8>> {
        use tokio::io::AsyncWriteExt;
        let mut buf = Vec::new();
        let mut sink = tokio::io::BufWriter::new(&mut buf);
        self.fetch_latest_pack_to_writer(component_id, &mut sink)
            .await?;
        sink.flush().await.into_diagnostic().ok();
        Ok(buf)
    }
}
