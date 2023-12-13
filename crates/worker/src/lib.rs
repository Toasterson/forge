use axum::{response::IntoResponse, routing::get, Json, Router};
use clap::Parser;
use config::{Environment, File};
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
};
use deadpool_lapin::lapin::protocol::basic::AMQPProperties;
use deadpool_lapin::lapin::{options::QueueDeclareOptions, types::FieldTable, Channel};
use forge::{ActivityEnvelope, CommitRef, Event, Job, Scheme};
use futures::{join, StreamExt};
use github::GitHubError;
use integration::{read_forge_manifest, ForgeIntegrationManifest};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, remove_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use tracing::{debug, error, info};
use url::Url;

#[derive(Error, Diagnostic, Debug)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    AddrParse(#[from] std::net::AddrParseError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    GitHubError(#[from] GitHubError),

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
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    FromUTF8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    IntegrationError(#[from] integration::IntegrationError),

    #[error("{0}")]
    String(String),

    #[error("git {0} failed output: {1}")]
    GitError(String, String),

    #[error("{0} failed output: {1}")]
    ScriptError(String, String),

    #[error("no .forge folder found in repo")]
    NoForgeConfigFolder,

    #[error("weird error with the files under .forge: no basename found. Check filesystem")]
    ForgeFilesNoBasename,

    #[error("no .forge/manifest.toml,yaml,json file found")]
    NoForgeManifest,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::GitHubError(err) => err.into_response(),
            err => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                err.to_string(),
            )
                .into_response(),
        }
    }
}

type Result<T> = miette::Result<T, Error>;

#[derive(Deserialize, Serialize)]
pub struct Config {
    amqp: deadpool_lapin::Config,
    listen: String,
    job_inbox: String,
    inbox: String,
    domain: String,
    scheme: String,
    directory: String,
}

#[derive(Parser)]
pub struct Args {
    rabbitmq_url: Option<String>,
}

pub fn load_config(args: Args) -> Result<Config> {
    let cfg = config::Config::builder()
        .add_source(File::with_name("/etc/forge/worker").required(false))
        .add_source(
            Environment::with_prefix("WORKER")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3101")?
        .set_default("job_inbox", "JOB_INBOX")?
        .set_default("inbox", "INBOX")?
        .set_default("scheme", Scheme::HTTPS.to_string())?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;

    Ok(cfg.try_deserialize()?)
}

#[derive(Debug, Clone)]
struct AppState {
    amqp: deadpool_lapin::Pool,
    inbox: String,
    job_inbox: String,
    base_url: String,
    worker_dir: String,
}

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening RabbitMQ Connection");
    let state = AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        inbox: cfg.inbox,
        job_inbox: cfg.job_inbox,
        base_url: format!("{}://{}", Scheme::from(cfg.scheme), cfg.domain),
        worker_dir: cfg.directory,
    };
    let conn = state.amqp.get().await?;
    debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    debug!(
        "Defining inbox: {} queue from channel id {}",
        &state.inbox,
        channel.id()
    );
    channel
        .queue_declare(
            &state.inbox,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let app = Router::new()
        .route("/healthz", get(health_check))
        //.layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state.clone());
    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3000
    let _ = join!(
        rabbitmq_listen(state,),
        axum::Server::bind(&cfg.listen.parse()?).serve(app.into_make_service()),
    );
    Ok(())
}

#[derive(Serialize, Default)]
struct HealthResponse {
    amqp_error: Option<String>,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}

async fn rabbitmq_listen(state: AppState) -> Result<()> {
    let mut retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    loop {
        retry_interval.tick().await;
        info!("connecting amqp consumer...");
        match handle_rabbitmq(state.clone()).await {
            Ok(_) => info!("rmq listen returned"),
            Err(e) => error!(error = e.to_string(), "rmq listen had an error"),
        };
    }
}

async fn handle_rabbitmq(state: AppState) -> Result<()> {
    let rmq_con = state
        .amqp
        .get()
        .await
        .map_err(|e| Error::String(e.to_string()))?;
    let channel = rmq_con.create_channel().await?;

    let mut consumer = channel
        .basic_consume(
            &state.job_inbox,
            "worker.consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    info!("amqp consumer connected, waiting for messages");
    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let tag = delivery.delivery_tag;
                match handle_message(
                    delivery,
                    &channel,
                    &state.inbox,
                    &state.base_url,
                    &state.worker_dir,
                )
                .await
                {
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

async fn handle_message(
    deliver: Delivery,
    channel: &Channel,
    inbox_name: &str,
    base_url: &str,
    worker_dir: &str,
) -> Result<()> {
    let body = deliver.data;
    let job: Job = serde_json::from_slice(&body)?;
    let worker_actor: Url = format!("{}/actors/worker", base_url).parse()?;
    let forge_actor: Url = format!("{}/actors/forge", base_url).parse()?;
    if let Some(know_job) = job.job_type {
        match know_job {
            forge::KnownJobs::CheckForChangedComponents => {
                let build_dir =
                    get_repo_path(worker_dir, &job.repository, &job.merge_request_ref.sha);
                clean_ws(&build_dir)?;
                let manifest = clone_repo(
                    &build_dir,
                    &job.repository,
                    &job.merge_request_ref,
                    job.conf_ref.clone(),
                )?;
                let component_list = get_component_list_in_repo(&build_dir, &manifest)?;
                let changed_files = get_changed_files(&build_dir, &job.target_ref)?;
                let changed_components = get_changed_components(component_list, changed_files);
                for component in changed_components {
                    let event = Event::Create(ActivityEnvelope {
                        actor: worker_actor.clone(),
                        to: vec![forge_actor.clone()],
                        cc: vec![],
                        object: forge::ActivityObject::SoftwareComponent(
                            forge::SoftwareComponent {
                                name: component,
                                recipe_file: None,
                                merge_request: job.merge_request_id,
                            },
                        ),
                    });
                    let payload = serde_json::to_vec(&event)?;
                    channel
                        .basic_publish(
                            "",
                            inbox_name,
                            BasicPublishOptions::default(),
                            &payload,
                            AMQPProperties::default(),
                        )
                        .await?;
                }
            }
        }
    } else {
    }
    Ok(())
}

fn get_changed_components(component_list: Vec<String>, changed_files: Vec<String>) -> Vec<String> {
    let mut changed_components: Vec<String> = vec![];
    'outer: for component in component_list {
        for file in changed_files.iter() {
            if file.contains(&component) {
                changed_components.push(component.clone());
                continue 'outer;
            }
        }
    }
    changed_components
}

fn get_changed_files<P: AsRef<Path>>(ws: P, target_branch_ref: &CommitRef) -> Result<Vec<String>> {
    let mut diff_cmd = Command::new("git");
    diff_cmd.arg("diff");
    diff_cmd.arg("--name-only");
    diff_cmd.arg(target_branch_ref.sha.as_str());
    diff_cmd.current_dir(ws.as_ref());
    let out = diff_cmd.output()?;
    if !out.status.success() {
        let out_string = String::from_utf8(out.stderr)?;
        return Err(Error::GitError("diff".into(), out_string));
    }
    let result = String::from_utf8(out.stdout)?;
    Ok(result.split("\n").map(|s| s.to_owned()).collect())
}

fn get_component_list_in_repo<P: AsRef<Path>>(
    ws: P,
    manifest: &ForgeIntegrationManifest,
) -> Result<Vec<String>> {
    let list_script_path = ws.as_ref().join(".forge_script_list_components.sh");
    let mut list_script = std::fs::File::create(&list_script_path)?;
    list_script.write_all(manifest.component_list_script.join("\n").as_bytes())?;
    let mut script_cmd = Command::new("bash");
    script_cmd.arg("-ex");
    script_cmd.arg(list_script_path.as_os_str());
    let out = script_cmd.output()?;
    if !out.status.success() {
        let out_string = String::from_utf8(out.stderr)?;
        return Err(Error::ScriptError("list_components.sh".into(), out_string));
    }
    let result = String::from_utf8(out.stdout)?;
    Ok(result.split("\n").map(|s| s.to_owned()).collect())
}

fn get_repo_path(base_dir: &str, repo: &str, sha: &str) -> PathBuf {
    let repo = repo
        .replace(":", "")
        .replace("//", "")
        .replace("/", "_")
        .replace("@", "_");
    Path::new(base_dir).join(repo).join(sha).to_path_buf()
}

fn clean_ws<P: AsRef<Path>>(dir: P) -> Result<()> {
    let path = dir.as_ref();
    if path.exists() {
        remove_dir_all(path)?;
    }
    create_dir_all(path)?;
    Ok(())
}

fn clone_repo<P: AsRef<Path>>(
    ws: P,
    repository: &str,
    checkout_ref: &CommitRef,
    conf_ref: Option<String>,
) -> Result<ForgeIntegrationManifest> {
    let mut git_cmd = Command::new("git");
    git_cmd.arg("clone");
    git_cmd.arg(&repository);
    git_cmd.arg(ws.as_ref().as_os_str());
    let out = git_cmd.output()?;
    if !out.status.success() {
        let out_string = String::from_utf8(out.stderr)?;
        return Err(Error::GitError("clone".into(), out_string));
    }
    struct ProcessInfo(bool, ForgeIntegrationManifest);
    let info: ProcessInfo = if let Some(conf_ref) = conf_ref {
        let mut git_cmd = Command::new("git");
        git_cmd.arg("reset");
        git_cmd.arg("--hard");
        git_cmd.arg(&conf_ref);
        git_cmd.current_dir(ws.as_ref());

        let out = git_cmd.output()?;
        if !out.status.success() {
            let out_string = String::from_utf8(out.stderr)?;
            return Err(Error::GitError("reset".into(), out_string));
        }
        let manifest = read_manifest(ws.as_ref())?;
        ProcessInfo(false, manifest)
    } else {
        let mut git_cmd = Command::new("git");
        git_cmd.arg("reset");
        git_cmd.arg("--hard");
        git_cmd.arg(&checkout_ref.sha);
        git_cmd.current_dir(ws.as_ref());

        let out = git_cmd.output()?;
        if !out.status.success() {
            let out_string = String::from_utf8(out.stderr)?;
            return Err(Error::GitError("reset".into(), out_string));
        }
        let manifest = read_manifest(ws.as_ref())?;
        ProcessInfo(true, manifest)
    };
    if !info.0 {
        let mut git_cmd = Command::new("git");
        git_cmd.arg("reset");
        git_cmd.arg("--hard");
        git_cmd.arg(&checkout_ref.sha);
        git_cmd.current_dir(ws.as_ref());

        let out = git_cmd.output()?;
        if !out.status.success() {
            let out_string = String::from_utf8(out.stderr)?;
            return Err(Error::GitError("reset".into(), out_string));
        }
    }

    Ok(info.1)
}

fn read_manifest<P: AsRef<Path>>(ws: P) -> Result<ForgeIntegrationManifest> {
    let conf_dir = ws.as_ref().join(".forge");
    if !conf_dir.exists() {
        return Err(Error::NoForgeConfigFolder);
    }

    for file in conf_dir.read_dir()? {
        let file = file?;
        if file
            .path()
            .file_name()
            .ok_or(Error::ForgeFilesNoBasename)?
            .to_str()
            == Some("manifest")
        {
            return Ok(read_forge_manifest(&file.path())?);
        }
    }

    Err(Error::NoForgeManifest)
}
