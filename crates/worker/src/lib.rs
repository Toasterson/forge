use axum::{response::IntoResponse, routing::get, Json, Router};
use clap::Parser;
use component::{Component, PackageMeta, SourceNode};
use component::Recipe;
use config::{Environment, File};
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::options::QueueBindOptions;
use deadpool_lapin::lapin::options::QueueDeclareOptions;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
};
use deadpool_lapin::lapin::protocol::basic::AMQPProperties;
use deadpool_lapin::lapin::{types::FieldTable, Channel};
use forge::{CommitRef, Scheme, Job, JobReport, JobReportData, PatchFile};
use futures::{join, StreamExt};
use github::GitHubError;
use integration::{read_forge_manifest, ForgeIntegrationManifest};
use itertools::Itertools;
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, remove_dir_all};
use std::future::IntoFuture;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use base64::Engine;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::trace;
use tracing::{debug, error, event, info, instrument, Level};
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

    #[error(transparent)]
    #[diagnostic(transparent)]
    ComponentError(#[from] component::ComponentError),

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

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct AppState {
    amqp: deadpool_lapin::Pool,
    inbox: String,
    job_inbox: String,
    base_url: Url,
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
        base_url: format!("{}://{}", Scheme::from(cfg.scheme), cfg.domain).parse()?,
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
        "Defining JOB inbox: {} queue from channel id {}",
        &state.job_inbox,
        channel.id()
    );
    channel
        .exchange_declare(
            &state.job_inbox,
            deadpool_lapin::lapin::ExchangeKind::Direct,
            deadpool_lapin::lapin::options::ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .queue_declare(
            &state.job_inbox,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .queue_bind(
            &state.job_inbox,
            &state.job_inbox,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let app = Router::new()
        .route("/healthz", get(health_check))
        //.layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state.clone());
    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3000
    let listener = TcpListener::bind(&cfg.listen).await?;
    let _ = join!(
        rabbitmq_listen(state,),
        axum::serve(listener, app.into_make_service()).into_future(),
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
                    &state.worker_dir,
                )
                .await
                {
                    Ok(_) => {
                        debug!("handled message");
                        channel.basic_ack(tag, BasicAckOptions::default()).await?;
                    }
                    Err(e) => {
                        error!(error = ?e, "failed to handle message");
                        channel.basic_nack(tag, BasicNackOptions::default()).await?;
                    }
                }
            }
            Err(err) => return Err(Error::String(err.to_string())),
        }
    }

    Ok(())
}

#[instrument(skip_all)]
async fn handle_message(
    delivery: Delivery,
    channel: &Channel,
    inbox_name: &str,
    worker_dir: &str,
) -> Result<()> {
    let body = delivery.data;
    let job: Job = serde_json::from_slice(&body)?;
    let job_report: JobReport = match job {
        Job::GetRecipes { cr_id, gate_id, cr } => {
            info!("getting recipes for change_request {}", cr.id);
            let build_dir = get_repo_path(worker_dir, &cr.git_url, &cr.head.sha);
            debug!("cleaning workspace {}", &build_dir.display());
            clean_ws(&build_dir)?;
            debug!("cloning repo {}", &cr.git_url);
            let manifest = clone_repo(&build_dir, &cr.git_url, &cr.head, None)?;
            let component_list = get_component_list_in_repo(&build_dir, &manifest)?;
            let changed_files = get_changed_files(&build_dir, &cr.base)?;
            let changed_components = get_changed_components(component_list, changed_files);
            create_gen_meatdata_script(&build_dir, &manifest)?;
            let mut recipes: Vec<(String, Recipe, Option<PackageMeta>, Vec<PatchFile>)> = vec![];
            for component in changed_components {
                let (recipe, package_meta) = get_component_metadata(
                    &build_dir,
                    &component,
                    manifest.change_to_component_dir,
                    &manifest.component_metadata_filename,
                )?;
                let patches = get_component_patches(&build_dir, &component, &recipe)?;
                recipes.push((component, recipe, package_meta, patches));
            }
            debug!("Fetched recipes successfully");

            JobReport::Success(JobReportData::GetRecipes {
                gate_id,
                change_request_id: cr_id.to_string(),
                recipes,
            })
        }
    };

    event!(Level::DEBUG, report = ?job_report, "Sent this Report to forged");

    let msg = serde_json::to_vec(&job_report)?;

    channel
        .basic_publish(
            inbox_name,
            "forged.jobreport",
            BasicPublishOptions::default(),
            &msg,
            AMQPProperties::default(),
        )
        .await?;
    event!(Level::INFO, "Job finished");
    Ok(())
}

#[instrument(skip_all)]
fn get_changed_components(component_list: Vec<String>, changed_files: Vec<String>) -> Vec<String> {
    let mut changed_components: Vec<String> = vec![];
    'outer: for file_path in changed_files.iter() {
        trace!("checking if file: {} is a changed component", &file_path);
        for component in component_list.iter() {
            if file_path.contains(component) {
                trace!("detected changed component {}", component);
                changed_components.push(component.clone());
                continue 'outer;
            }
        }
    }
    let changed_components = changed_components
        .into_iter()
        .unique()
        .filter(|c| c != "")
        .collect();
    debug!(
        "the following components changed: {:?}",
        &changed_components
    );
    changed_components
}

#[instrument]
fn get_changed_files<P: AsRef<Path> + std::fmt::Debug>(
    ws: P,
    target_branch_ref: &CommitRef,
) -> Result<Vec<String>> {
    debug!("Detecting changed files");
    let target_sha = format!("{}..", &target_branch_ref.sha);

    let mut diff_cmd = Command::new("git");
    diff_cmd.arg("show");
    diff_cmd.arg("--name-only");
    diff_cmd.arg("--format=tformat:");
    diff_cmd.arg(target_sha.as_str());
    diff_cmd.current_dir(ws.as_ref());
    let out = diff_cmd.output()?;
    if !out.status.success() {
        let out_string = String::from_utf8(out.stderr)?;
        return Err(Error::GitError("diff".into(), out_string));
    }
    let result = String::from_utf8(out.stdout)?;
    Ok(result.split("\n").map(|s| s.to_owned()).collect())
}

#[instrument]
fn get_component_list_in_repo<P: AsRef<Path> + std::fmt::Debug>(
    ws: P,
    manifest: &ForgeIntegrationManifest,
) -> Result<Vec<String>> {
    debug!("creating list_component script");
    let list_script_path = ws.as_ref().join(".forge_script_list_components.sh");
    let mut list_script = std::fs::File::create(&list_script_path)?;
    list_script.write_all(manifest.component_list_script.join("\n").as_bytes())?;
    debug!("running list_component script");
    let mut script_cmd = Command::new("bash");
    script_cmd.arg("-ex");
    script_cmd.current_dir(ws.as_ref());
    script_cmd.arg("./.forge_script_list_components.sh");
    let out = script_cmd.output()?;
    if !out.status.success() {
        let out_string = String::from_utf8(out.stderr)?;
        return Err(Error::ScriptError("list_components.sh".into(), out_string));
    }
    let result = String::from_utf8(out.stdout)?;
    Ok(result.split("\n").map(|s| s.to_owned()).collect())
}

#[instrument]
fn create_gen_meatdata_script<P: AsRef<Path> + std::fmt::Debug>(
    ws: P,
    manifest: &ForgeIntegrationManifest,
) -> Result<()> {
    debug!("creating gen metadata hepler script");
    let list_script_path = ws.as_ref().join(".forge_script_components_gen_metadata.sh");
    let mut list_script = std::fs::File::create(&list_script_path)?;
    list_script.write_all(manifest.component_metadata_gen_script.join("\n").as_bytes())?;
    Ok(())
}

#[instrument]
fn get_component_patches<P: AsRef<Path> + std::fmt::Debug>(
    ws: P,
    component: &str,
    recipe: &Recipe,
) -> Result<Vec<PatchFile>> {
    let patch_base_path = ws
        .as_ref()
        .join("components")
        .join(component)
        .join("patches");
    let mut files = vec![];
    for src in &recipe.sources {
        for s in &src.sources {
            match s {
                SourceNode::Patch(patch) => {
                    let patch_path = patch.get_bundle_path(&patch_base_path);
                    let Ok(mut f) = std::fs::File::open(patch_path) else {
                        error!("Open file error in patch {patch} skipping");
                        continue;
                    };
                    let mut buf = vec![];
                    if f.read_to_end(&mut buf).is_err() {
                        error!("Read error in patch {patch} skipping");
                        continue;
                    }
                    let pf = PatchFile {
                        name: patch.to_string(),
                        content: base64::engine::general_purpose::STANDARD.encode(buf),
                    };
                    files.push(pf);
                }
                _ => {}
            }
        }
    }

    Ok(files)
}

#[instrument]
fn get_component_metadata<P: AsRef<Path> + std::fmt::Debug>(
    ws: P,
    component: &str,
    change_to_component_dir: bool,
    metadata_file_name: &str,
) -> Result<(Recipe, Option<PackageMeta>)> {
    debug!("running create_metadata script");
    let mut script_cmd = Command::new("bash");
    script_cmd.arg("-ex");
    script_cmd.arg(
        ws.as_ref()
            .join(".forge_script_components_gen_metadata.sh")
            .as_os_str(),
    );
    if change_to_component_dir {
        script_cmd.current_dir(ws.as_ref().join("components").join(component));
    } else {
        script_cmd.current_dir(ws.as_ref());
        script_cmd.arg(component);
    }
    let out = script_cmd.output()?;
    if !out.status.success() {
        let out_string = String::from_utf8(out.stderr)?;
        return Err(Error::ScriptError("gen_metadata.sh".into(), out_string));
    }
    let metadata_file_path = ws
        .as_ref()
        .join("components")
        .join(component)
        .join(metadata_file_name);
    let c = Component::open_local(metadata_file_path)?;
    Ok((c.recipe, c.package_meta))
}

#[instrument]
fn get_repo_path(base_dir: &str, repo: &str, sha: &str) -> PathBuf {
    let repo = repo
        .replace(":", "")
        .replace("//", "")
        .replace("/", "_")
        .replace("@", "_");
    Path::new(base_dir).join(repo).join(sha).to_path_buf()
}

#[instrument]
fn clean_ws<P: AsRef<Path> + std::fmt::Debug>(dir: P) -> Result<()> {
    let path = dir.as_ref();
    if path.exists() {
        remove_dir_all(path)?;
    }
    create_dir_all(path)?;
    Ok(())
}

#[instrument]
fn clone_repo<P: AsRef<Path> + std::fmt::Debug>(
    ws: P,
    repository: &str,
    checkout_ref: &CommitRef,
    conf_ref: Option<String>,
) -> Result<ForgeIntegrationManifest> {
    debug!(
        "running git clone {} {}",
        &repository,
        ws.as_ref().display()
    );
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
        debug!("resetting repo to {}, to get config", &conf_ref);
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
        debug!("resetting git repo to {}", &checkout_ref.sha);
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
        debug!("resetting git repo to {}", &checkout_ref.sha);
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

#[instrument]
fn read_manifest<P: AsRef<Path> + std::fmt::Debug>(ws: P) -> Result<ForgeIntegrationManifest> {
    let conf_dir = ws.as_ref().join(".forge");
    if !conf_dir.exists() {
        return Err(Error::NoForgeConfigFolder);
    }

    for file in conf_dir.read_dir()? {
        let file = file?;
        trace!(
            "checking file {:?} if it is a manifest",
            file.path().file_name()
        );
        if file
            .path()
            .file_name()
            .ok_or(Error::ForgeFilesNoBasename)?
            .to_str()
            .ok_or(Error::ForgeFilesNoBasename)?
            .starts_with("manifest")
        {
            debug!("found manifest file {}", &file.path().display());
            return Ok(read_forge_manifest(&file.path())?);
        }
    }

    Err(Error::NoForgeManifest)
}
