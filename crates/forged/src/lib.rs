use axum::extract::multipart::MultipartError;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use config::Environment;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueDeclareOptions,
};
use deadpool_lapin::lapin::types::FieldTable;
use deadpool_lapin::Pool;
use forge::FileKindError;
use futures::{join, StreamExt};
use message_queue::handle_message;
use miette::Diagnostic;
use opendal::Operator;
use prisma::PrismaClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use std::future::IntoFuture;
use utoipa::{openapi::security::{ApiKey, ApiKeyValue, SecurityScheme}, Modify, OpenApi, ToSchema};
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

mod api;
mod message_queue;
#[allow(warnings, unused)]
mod prisma;

#[derive(Parser, Debug)]
pub struct Args {
    pub rabbitmq_url: Option<String>,
}

#[derive(Debug, Diagnostic, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    AddrParse(#[from] std::net::AddrParseError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

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
    NewClient(#[from] prisma_client_rust::NewClientError),

    #[error(transparent)]
    QueryError(#[from] prisma_client_rust::QueryError),

    #[error(transparent)]
    OpenDal(#[from] opendal::Error),

    #[error("no version found in component with name: {0}")]
    NoVersionFoundInRecipe(String),

    #[error("no revision found in component with name: {0}")]
    NoRevisionFoundInRecipe(String),

    #[error("no project URL found in component with name: {0}")]
    NoProjectUrlFoundInRecipe(String),

    #[error(transparent)]
    MultipartError(#[from] MultipartError),

    #[error("no component found")]
    NoComponentFound,

    #[error("no id found in gate object: {0}")]
    NoIdFoundINGate(String),

    #[error("{0}")]
    String(String),

    #[error("entity not found {0}")]
    NotFound(String),

    #[error("neither url nor file provided in upload")]
    NoFileOrUrl,

    #[error("no domain found")]
    NoDomainFound,

    #[error("invalid multipart request ")]
    InvalidMultipartRequest,

    #[error("unauthorized")]
    Unauthorized,

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FileKindError(#[from] FileKindError),

    #[error("user is unauthorized to claim this existing handle")]
    UnauthorizedToClaimHandle,
    
    #[error(transparent)]
    PasetoGenericBuilderError(#[from] rusty_paseto::prelude::GenericBuilderError),

    #[error(transparent)]
    PasetoClaimError(#[from] rusty_paseto::prelude::PasetoClaimError),
    
    #[error(transparent)]
    OctoRustClientError(#[from] octorust::ClientError),
}

pub type Result<T> = miette::Result<T, Error>;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub enum ApiError {
    // Bad input Request
    BadRequest(String),
    // Not Authorized
    Unauthorized,
    // Entity not found
    NotFound(String),
    // Internal server error
    ServerError(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NoRevisionFoundInRecipe(msg) => (StatusCode::BAD_REQUEST, Json(ApiError::BadRequest(msg))).into_response(),
            Error::NoProjectUrlFoundInRecipe(msg) => (StatusCode::BAD_REQUEST, Json(ApiError::BadRequest(msg))).into_response(),
            Error::NoDomainFound => (StatusCode::NOT_FOUND, Json(ApiError::NotFound("no domain found".to_string()))).into_response(),
            Error::NoComponentFound => (StatusCode::NOT_FOUND, Json(ApiError::NotFound("no component found".to_string()))).into_response(),
            Error::NoIdFoundINGate(msg) => (StatusCode::NOT_FOUND, Json(ApiError::NotFound(msg))).into_response(),
            Error::NotFound(msg) => (StatusCode::NOT_FOUND, Json(ApiError::NotFound(msg))).into_response(),
            err => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::ServerError(err.to_string()))).into_response(),
        }
    }
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("apikey"))),
            )
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub amqp: deadpool_lapin::Config,
    pub listen: String,
    pub job_inbox: String,
    pub inbox: String,
    pub connection_string: String,
    pub opendal: OpenDalConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenDalConfig {
    pub service: String,
    pub endpoint: String,
    pub bucket: String,
    pub key_id: String,
    pub secret_key: String,
}

pub fn load_config(args: Args) -> Result<Config> {
    let cfg = config::Config::builder()
        .add_source(config::File::with_name("/etc/forge/forged").required(false))
        .add_source(config::File::with_name("forged").required(false))
        .add_source(
            Environment::with_prefix("APP")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3100")?
        .set_default("job_inbox", "JOB_INBOX")?
        .set_default("inbox", "INBOX")?
        .set_default("opendal.endpoint", "http://localhost:9000")?
        .set_default("opendal.bucket", "forge")?
        .set_default("opendal.service", "s3")?
        .set_default("opendal.key_id", "")?
        .set_default("opendal.secret_key", "")?
        .set_default(
            "connection_string",
            "postgres://forge:forge@localhost/forge",
        )?
        .set_default("amqp.url", "amqp://dev:dev@localhost:5672/master")?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;
    Ok(cfg.try_deserialize()?)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        api::v1::actor::actor_connect,
    ),
    components(
      schemas(api::v1::actor::ActorConnectRequest, api::v1::actor::ActorSSHKeyFingerprint, api::v1::actor::ActorConnectResponse, ApiError)
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "forge", description = "Forge your packages")
    )
)]
struct ApiDoc;

#[allow(dead_code)]
#[derive(Debug)]
struct AppState {
    amqp: Pool,
    job_inbox: String,
    inbox: String,
    prisma: PrismaClient,
    fs_operator: Operator,
}

type SharedState = Arc<Mutex<AppState>>;

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening Database Connection");
    let db_conn = prisma::PrismaClient::_builder()
        .with_url(cfg.connection_string.clone())
        .build()
        .await?;

    debug!("Setting up Filesystem Operator");
    let mut op_builder = opendal::services::S3::default();
    op_builder.endpoint(&cfg.opendal.endpoint);
    op_builder.region("default");
    op_builder.bucket(&cfg.opendal.bucket);
    op_builder.access_key_id(&cfg.opendal.key_id);
    op_builder.secret_access_key(&cfg.opendal.secret_key);

    let fs_operator = Operator::new(op_builder)?
        .layer(opendal::layers::LoggingLayer::default())
        .finish();

    debug!("Checking if operator is setup correctly");
    fs_operator.check().await?;

    debug!("Opening RabbitMQ Connection");
    let state = Arc::new(Mutex::new(AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        job_inbox: cfg.job_inbox,
        inbox: cfg.inbox,
        prisma: db_conn,
        fs_operator,
    }));
    let conn = state.lock().await.amqp.get().await?;
    let job_inbox = state.lock().await.job_inbox.clone();
    let inbox = state.lock().await.inbox.clone();
    debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    debug!(
        "Defining JOB inbox: {} queue from channel id {}",
        job_inbox,
        channel.id()
    );
    channel
        .queue_declare(
            job_inbox.as_str(),
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    debug!(
        "Defining inbox: {} queue from channel id {}",
        inbox,
        channel.id()
    );
    channel
        .queue_declare(
            inbox.as_str(),
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let amqp_consume_pool = state.lock().await.amqp.clone();
    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .merge(RapiDoc::new("/api-docs/openapi.json").path("/rapidoc"))
        .route("/healthz", get(health_check))
        .nest("/api", api::get_api_router())
        .with_state(state);
    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3100
    let listener = TcpListener::bind(&cfg.listen).await?;
    let _ = join!(
        rabbitmq_listen(
            amqp_consume_pool,
            cfg.connection_string.clone(),
            inbox.as_str(),
            job_inbox.as_str()
        ),
        axum::serve(listener, app.into_make_service()).into_future(),
    );
    Ok(())
}

async fn rabbitmq_listen(
    pool: Pool,
    connection_string: String,
    inbox_name: &str,
    job_inbox_name: &str,
) -> Result<()> {
    let mut retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let database = PrismaClient::_builder()
        .with_url(connection_string)
        .build()
        .await?;
    loop {
        retry_interval.tick().await;
        info!("connecting amqp consumer...");
        match handle_rabbitmq(pool.clone(), &database, inbox_name, job_inbox_name).await {
            Ok(_) => info!("rmq listen returned"),
            Err(e) => error!(error = e.to_string(), "rmq listen had an error"),
        };
    }
}

async fn handle_rabbitmq(
    pool: Pool,
    database: &PrismaClient,
    inbox_name: &str,
    _job_inbox_name: &str,
) -> Result<()> {
    let rmq_con = pool.get().await.map_err(|e| Error::String(e.to_string()))?;
    let channel = rmq_con.create_channel().await?;

    let mut consumer = channel
        .basic_consume(
            inbox_name,
            "forged.consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    info!("amqp consumer connected, waiting for messages");
    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let tag = delivery.delivery_tag;
                match handle_message(delivery, database, &channel).await {
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

#[derive(Serialize, Default)]
struct HealthResponse {}

async fn health_check(State(state): State<SharedState>) -> Result<Json<HealthResponse>> {
    let conn = state.lock().await.amqp.get().await?;
    if !matches!(
        conn.status().state(),
        deadpool_lapin::lapin::ConnectionState::Connected
    ) {
        return Err(Error::Lapin(
            deadpool_lapin::lapin::Error::InvalidConnectionState(conn.status().state()),
        ));
    }
    Ok(Json(HealthResponse::default()))
}
