use crate::api::forged::api::v1::{
    auth_service_server::AuthServiceServer, component_service_server::ComponentServiceServer,
    gate_service_server::GateServiceServer,
};
use crate::services::{AuthServiceImpl, ComponentServiceImpl, GateServiceImpl};
use crate::settings::Settings;
use crate::storage::json::JsonStore;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use miette::{Context, IntoDiagnostic};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::transport::Server;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerSettings {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    private_key_ssh: String,
    public_key_ssh: String,
    created_at: u64,
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_server_settings() -> miette::Result<ServerSettings> {
    let mut rng = ssh_key::rand_core::OsRng;
    let priv_key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519)
        .into_diagnostic()
        .wrap_err("generate ssh key")?;
    let public = priv_key.public_key();
    let private_key_ssh = priv_key
        .to_openssh(Default::default())
        .into_diagnostic()
        .wrap_err("encode openssh private")?
        .to_string();
    let public_key_ssh = public
        .to_openssh()
        .into_diagnostic()
        .wrap_err("encode openssh public")?;
    Ok(ServerSettings {
        id: Some("server_keys".into()),
        private_key_ssh,
        public_key_ssh,
        created_at: now_sec(),
    })
}

async fn load_or_init_server_settings_mongo(
    client: &mongodb::Client,
    db_name: &str,
) -> miette::Result<ServerSettings> {
    let coll = client
        .database(db_name)
        .collection::<ServerSettings>("settings");
    if let Some(found) = coll
        .find_one(doc! {"_id": "server_keys"})
        .await
        .into_diagnostic()
        .wrap_err("mongo find settings")?
    {
        return Ok(found);
    }
    let settings = generate_server_settings()?;
    let filter = doc! {"_id": "server_keys"};
    coll.replace_one(filter, &settings)
        .upsert(true)
        .await
        .into_diagnostic()
        .wrap_err("mongo upsert settings")?;
    Ok(settings)
}

fn load_or_init_server_settings_file() -> miette::Result<ServerSettings> {
    let store = JsonStore::new(".")?;
    if let Some(s) = store.get::<ServerSettings>("server_keys")? {
        return Ok(s);
    }
    let settings = generate_server_settings()?;
    store.put("server_keys", &settings)?;
    Ok(settings)
}

pub async fn start_grpc_server(addr: SocketAddr) -> miette::Result<()> {
    use tonic_health::server::health_reporter;

    let (_health_reporter, health_service) = health_reporter();

    // Load Settings (config file + env overrides). On error, continue with defaults.
    let settings = Settings::load().unwrap_or_default();

    info!(%addr, "starting gRPC server");

    // Build optional SMTP mailer from settings
    let (mailer_opt, mail_from_opt) = if let Some(smtp) = &settings.smtp {
        if let (Some(host), Some(from)) = (smtp.host.clone(), smtp.from.clone()) {
            let starttls = smtp.starttls.unwrap_or(true);
            let builder = if starttls {
                match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host) {
                    Ok(b) => b,
                    Err(_) => {
                        AsyncSmtpTransport::<Tokio1Executor>::relay(&host).expect("relay builder")
                    }
                }
            } else {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&host).expect("relay builder")
            };
            let builder = if let Some(port) = smtp.port {
                builder.port(port)
            } else {
                builder
            };
            let builder =
                if let (Some(user), Some(pass)) = (smtp.username.clone(), smtp.password.clone()) {
                    builder.credentials(Credentials::new(user, pass))
                } else {
                    builder
                };
            let mailer = builder.build();
            (Some(mailer), Some(from))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Select backend and load/generate server SSH keys
    let mongo_uri = settings
        .mongodb
        .uri
        .clone()
        .or_else(|| std::env::var("FORGED_MONGO_URI").ok());
    let db_name = settings
        .mongodb
        .db
        .clone()
        .or_else(|| std::env::var("FORGED_MONGO_DB").ok())
        .unwrap_or_else(|| "forged".to_string());

    let auth_impl = if let Some(uri) = mongo_uri {
        info!("using MongoDB for pending registrations and settings");
        let client = crate::storage::connect(&uri)
            .await
            .wrap_err("connect mongodb")?;
        let keys = load_or_init_server_settings_mongo(&client, &db_name).await?;
        AuthServiceImpl::with_mongo_pending_keys_and_mailer(
            client,
            &db_name,
            keys.private_key_ssh,
            keys.public_key_ssh,
            mailer_opt,
            mail_from_opt,
        )
    } else {
        let keys = load_or_init_server_settings_file()?;
        AuthServiceImpl::with_keys_and_mailer(
            keys.private_key_ssh,
            keys.public_key_ssh,
            mailer_opt,
            mail_from_opt,
        )
    };

    Server::builder()
        .add_service(health_service)
        .add_service(GateServiceServer::new(GateServiceImpl::default()))
        .add_service(ComponentServiceServer::new(ComponentServiceImpl::default()))
        .add_service(AuthServiceServer::new(auth_impl))
        .serve_with_shutdown(addr, shutdown_signal())
        .await
        .into_diagnostic()
        .wrap_err("serve grpc")?;

    Ok(())
}

fn shutdown_signal() -> impl std::future::Future<Output = ()> {
    async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received");
    }
}

#[cfg(feature = "quic")]
pub mod quic {
    use miette::{Context, IntoDiagnostic};
    use quiche::{Config, Connection, ConnectionId, Header, RecvInfo, SendInfo};
    use rand::{rngs::OsRng, RngCore};
    use std::collections::HashMap;
    use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
    use std::time::Duration;
    use tokio::net::UdpSocket;
    use tokio::time::MissedTickBehavior;
    use tracing::{debug, error, info, warn};

    const MAX_DATAGRAM_SIZE: usize = 1350;

    pub async fn start_quic_endpoint(addr: SocketAddr) -> miette::Result<()> {
        let cert_path = std::env::var("FORGED_TLS_CERT")
            .into_diagnostic()
            .wrap_err("FORGED_TLS_CERT must be set for QUIC")?;
        let key_path = std::env::var("FORGED_TLS_KEY")
            .into_diagnostic()
            .wrap_err("FORGED_TLS_KEY must be set for QUIC")?;

        // Configure QUIC
        let mut config = Config::new(quiche::PROTOCOL_VERSION)
            .into_diagnostic()
            .wrap_err("create quiche config")?;
        config.verify_peer(false);
        config
            .load_cert_chain_from_pem_file(&cert_path)
            .into_diagnostic()
            .wrap_err("load cert chain")?;
        config
            .load_priv_key_from_pem_file(&key_path)
            .into_diagnostic()
            .wrap_err("load private key")?;
        // ALPN for future HTTP/3 or custom proto
        config
            .set_application_protos(&[b"forge", b"h3"]) // advertise custom proto and h3
            .into_diagnostic()
            .wrap_err("set application protos")?;
        config.set_max_idle_timeout(5000);
        config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
        config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
        config.set_initial_max_data(10_000_000);
        config.set_initial_max_stream_data_bidi_local(1_000_000);
        config.set_initial_max_stream_data_bidi_remote(1_000_000);
        config.set_initial_max_streams_bidi(100);
        config.set_initial_max_streams_uni(100);

        // Bind UDP socket
        let std_sock = StdUdpSocket::bind(addr)
            .into_diagnostic()
            .wrap_err("bind quic udp socket")?;
        std_sock.set_nonblocking(true).ok();
        let socket = UdpSocket::from_std(std_sock)
            .into_diagnostic()
            .wrap_err("wrap tokio udp")?;

        info!(%addr, "QUIC endpoint listening");

        // Connection state
        let mut conns: HashMap<ConnectionId<'static>, Connection> = HashMap::new();
        let mut out = vec![0u8; MAX_DATAGRAM_SIZE];
        let mut buf = vec![0u8; 65_535];

        // Tick timer for driving timeouts
        let mut ticker = tokio::time::interval(Duration::from_millis(10));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = ticker.tick() => {
                    // Drive timeouts and send pending packets
                    let mut finished = vec![];
                    for (cid, conn) in conns.iter_mut() {
                        conn.on_timeout();
                        loop {
                            match conn.send(&mut out) {
                                Ok((len, SendInfo{to, ..})) => {
                                    if let Err(e) = socket.send_to(&out[..len], to).await { error!(error=?e, "udp send error"); break; }
                                }
                                Err(quiche::Error::Done) => break,
                                Err(e) => { warn!(?e, "send failed"); finished.push(cid.clone()); break; }
                            }
                        }
                        if conn.is_closed() { finished.push(cid.clone()); }
                    }
                    for cid in finished { conns.remove(&cid); }
                }
                recv = socket.recv_from(&mut buf) => {
                    let (len, from) = match recv { Ok(v) => v, Err(e)=>{ warn!(error=?e, "udp recv error"); continue; } };
                    let pkt = &mut buf[..len];
                    let hdr = match Header::from_slice(pkt, quiche::MAX_CONN_ID_LEN) { Ok(h) => h, Err(e) => { warn!(?e, "packet parse failed"); continue; } };

                    let mut key: ConnectionId<'static> = hdr.dcid.clone().into_owned();

                    // New connection?
                    if !conns.contains_key(&key) {
                        if hdr.ty != quiche::Type::Initial { continue; }

                        // Generate a fresh scid for this connection
                        let mut scid_bytes = [0u8; quiche::MAX_CONN_ID_LEN];
                        OsRng.fill_bytes(&mut scid_bytes);
                        let scid = ConnectionId::from_ref(&scid_bytes);

                        let conn = match quiche::accept(&scid, Some(&hdr.dcid), addr, from, &mut config) {
                            Ok(c) => c,
                            Err(e) => { warn!(?e, "accept failed"); continue; }
                        };
                        let scid_owned = scid.into_owned();
                        key = scid_owned.clone();
                        conns.insert(scid_owned, conn);
                    }

                    // Process the packet
                    if let Some(conn) = conns.get_mut(&key) {
                        let recv_info = RecvInfo { from, to: addr };
                        if let Err(e) = conn.recv(pkt, recv_info) {
                            match e { quiche::Error::Done => {}, _ => { warn!(?e, "conn recv error"); }
                            }
                        }

                        // If handshake complete, we can accept simple streams and echo a small response.
                        if conn.is_established() {
                            let mut stream_buf = [0u8; 1024];
                            for s in conn.readable() {
                                loop {
                                    match conn.stream_recv(s, &mut stream_buf) {
                                        Ok((read, fin)) => {
                                            debug!(stream_id=s, read, fin, "received data");
                                            let _ = conn.stream_send(s, b"OK", true);
                                        }
                                        Err(quiche::Error::Done) => break,
                                        Err(e) => { warn!(?e, "stream recv failed"); break; }
                                    }
                                }
                                break;
                            }
                        }

                        // Send any pending data
                        loop {
                            match conn.send(&mut out) {
                                Ok((len, SendInfo{to, ..})) => {
                                    if let Err(e) = socket.send_to(&out[..len], to).await { error!(error=?e, "udp send error"); break; }
                                }
                                Err(quiche::Error::Done) => break,
                                Err(e) => { warn!(?e, "send failed"); break; }
                            }
                        }
                    }
                }
            }
        }
    }
}
