use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use miette::{Context, IntoDiagnostic};
use serde::{Deserialize, Serialize};
use surrealdb::engine::any::{connect, Any};
use surrealdb::Surreal;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "xtask")]
#[command(about = "Workspace automation tasks for forge", long_about = None)]
pub struct Xtask {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Start a forged server suitable for e2e tests and print JSON with connection details
    SetupTestEnv {
        /// Optional directory to create under target/ for this env (default: random)
        #[arg(long)]
        name: Option<String>,
        /// Timeout to wait for server to accept connections (seconds)
        #[arg(long, default_value_t = 15)]
        timeout: u64,
    },
    /// Fetch the Base64-URL (no padding) registration envelope for a pending actor
    PendingEnvelope {
        /// Path to the embedded SurrealDB directory (the same value used by setup-test-env)
        #[arg(long)]
        surreal_path: PathBuf,
        /// Actor id
        #[arg(long)]
        actor_id: String,
        /// Actor kind: user or service
        #[arg(long, value_parser = parse_actor_kind)]
        kind: i32,
    },
}

fn parse_actor_kind(s: &str) -> Result<i32, String> {
    match s {
        "user" => Ok(0),
        "service" => Ok(1),
        _ => Err("must be 'user' or 'service'".to_string()),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestEnvInfo {
    pub addr: String,
    pub surreal_path: String,
    pub pid: u32,
    pub dir: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let xt = Xtask::parse();
    match xt.cmd {
        Cmd::SetupTestEnv { name, timeout } => {
            let info = setup_test_env(name.as_deref(), Duration::from_secs(timeout))
                .await
                .wrap_err("setup test env failed")?;
            println!("{}", serde_json::to_string_pretty(&info).into_diagnostic()?);
            Ok(())
        }
        Cmd::PendingEnvelope {
            surreal_path,
            actor_id,
            kind,
        } => {
            let env = pending_envelope(&surreal_path, &actor_id, kind)
                .await
                .wrap_err("fetch pending envelope failed")?;
            println!("{}", env);
            Ok(())
        }
    }
}

async fn setup_test_env(name: Option<&str>, timeout: Duration) -> miette::Result<TestEnvInfo> {
    // Directories under target
    let target_dir = PathBuf::from("target");
    fs::create_dir_all(&target_dir).into_diagnostic()?;
    let dir_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("forged-test-{}", uuid::Uuid::new_v4()));
    let env_dir = target_dir.join(dir_name);
    let surreal_path = env_dir.join("surreal");
    fs::create_dir_all(&surreal_path).into_diagnostic()?;

    // Pick a free port
    let port = portpicker::pick_unused_port().unwrap_or(50051);
    let addr = format!("127.0.0.1:{}", port);

    // Spawn `cargo run -p forged`
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("forged")
        .arg("--quiet")
        .env("FORGED_ADDR", &addr)
        .env("FORGED__SURREAL__MODE", "embedded")
        .env("FORGED__SURREAL__PATH", surreal_path.as_os_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(project_root());

    let child = cmd.spawn().into_diagnostic().wrap_err("spawn forged")?;

    wait_for_server(&addr, timeout)?;

    let info = TestEnvInfo {
        addr: addr.clone(),
        surreal_path: surreal_path.display().to_string(),
        pid: child.id(),
        dir: env_dir.display().to_string(),
    };

    // Write a small metadata file for convenience
    let meta_path = env_dir.join("env.json");
    fs::create_dir_all(&env_dir).into_diagnostic()?;
    fs::write(
        &meta_path,
        serde_json::to_vec_pretty(&info).into_diagnostic()?,
    )
    .into_diagnostic()
    .wrap_err_with(|| format!("write {}", meta_path.display()))?;

    // Detach child; The caller is responsible for killing the process after tests.
    std::mem::forget(child);

    Ok(info)
}

fn project_root() -> PathBuf {
    // Assume xtask runs in workspace root; use CARGO_MANIFEST_DIR to derive root/crates/xtask -> root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn wait_for_server(addr: &str, timeout: Duration) -> miette::Result<()> {
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(miette::miette!(
                "server did not start listening at {} within {:?}",
                addr,
                timeout
            ));
        }
        match addr.parse::<SocketAddr>() {
            Ok(sa) => {
                if TcpStream::connect_timeout(&sa, Duration::from_millis(200)).is_ok() {
                    return Ok(());
                }
            }
            Err(_) => return Err(miette::miette!("invalid addr: {}", addr)),
        }
        thread::sleep(Duration::from_millis(200));
    }
}

#[derive(Debug, serde::Deserialize)]
struct PendingRegistrationRec {
    actor_id: String,
    actor_kind: i32,
    expires_at: u64,
    envelope: Vec<u8>,
}

async fn pending_envelope(
    surreal_path: &Path,
    actor_id: &str,
    kind: i32,
) -> miette::Result<String> {
    let uri = format!("rocksdb:{}", surreal_path.display());
    let db: Surreal<Any> = connect(uri.as_str())
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("connect surrealdb at {}", uri))?;
    db.use_ns("forged")
        .use_db("default")
        .await
        .into_diagnostic()
        .wrap_err("select surreal ns/db")?;

    let key = format!("pending_registrations:{}:{}", actor_id, kind);
    let thing: surrealdb::sql::Thing = key.parse().unwrap();
    let res: Option<PendingRegistrationRec> = db
        .select(thing)
        .await
        .into_diagnostic()
        .wrap_err("select pending registration")?;
    let rec = res
        .ok_or_else(|| miette::miette!("no pending registration for {} kind {}", actor_id, kind))?;

    // Encode envelope bytes as Base64-URL (no padding) to match CLI expectations
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&rec.envelope);
    Ok(b64)
}
