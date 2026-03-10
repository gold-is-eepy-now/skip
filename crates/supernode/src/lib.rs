//! Supernode service: peer directory, routing, and signaling bridge.

use common::{config::NetworkConfig, error::AppError};
use dashmap::DashMap;
use database::Database;
use protocol::messages::{PeerEndpoint, Request, Response};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

#[derive(Clone)]
struct PeerState {
    endpoint: PeerEndpoint,
    last_heartbeat: Instant,
}

/// Shared in-memory supernode state.
#[derive(Clone)]
pub struct SupernodeState {
    peers: Arc<DashMap<String, PeerState>>,
    known_supernodes: Arc<DashMap<String, ()>>,
    cluster_limit: usize,
}

impl SupernodeState {
    fn new(cfg: &NetworkConfig) -> Self {
        let known = DashMap::new();
        for s in cfg.known_supernodes.iter().take(cfg.supernode_cache_size) {
            known.insert(s.clone(), ());
        }
        Self {
            peers: Arc::new(DashMap::new()),
            known_supernodes: Arc::new(known),
            cluster_limit: cfg.peer_cluster_limit,
        }
    }
}

pub async fn run(cfg: NetworkConfig, db_url: &str) -> Result<(), AppError> {
    let listener =
        TcpListener::bind(format!("{}:{}", cfg.supernode_host, cfg.supernode_port)).await?;
    let db = Database::connect(db_url).await?;
    let state = SupernodeState::new(&cfg);

    loop {
        let (stream, addr) = listener.accept().await?;
        let db = db.clone();
        let state = state.clone();
        let relay_addr = format!("{}:{}", cfg.relay_host, cfg.relay_port);
        tokio::spawn(async move {
            if let Err(err) = handle(stream, addr.to_string(), db, state, relay_addr).await {
                eprintln!("supernode client error: {err}");
            }
        });
    }
}

async fn handle(
    mut stream: TcpStream,
    remote: String,
    db: Database,
    state: SupernodeState,
    relay_addr: String,
) -> Result<(), AppError> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut line).await?;
    }
    let req: Request = serde_json::from_str(line.trim())?;

    let response = match req {
        Request::Heartbeat { username, token } => {
            authorize(&db, &username, &token).await?;
            if state.peers.len() >= state.cluster_limit {
                Response::Error {
                    message: "supernode cluster full".into(),
                }
            } else {
                let endpoint = PeerEndpoint {
                    username: username.clone(),
                    tcp_addr: remote.clone(),
                    udp_addr: remote,
                    supernode_addr: "self".into(),
                    relay_required: false,
                };
                state.peers.insert(
                    username,
                    PeerState {
                        endpoint,
                        last_heartbeat: Instant::now(),
                    },
                );
                Response::Ack {
                    message: "heartbeat accepted".into(),
                }
            }
        }
        Request::PeerLookup {
            username,
            token,
            target_user,
        }
        | Request::ConnectRequest {
            username,
            token,
            target_user,
        } => {
            authorize(&db, &username, &token).await?;
            if let Some(peer) = state.peers.get(&target_user) {
                if peer.last_heartbeat.elapsed() > Duration::from_secs(90) {
                    state.peers.remove(&target_user);
                    Response::Error {
                        message: "peer stale/offline".into(),
                    }
                } else {
                    let mut endpoint = peer.endpoint.clone();
                    endpoint.relay_required = endpoint.udp_addr.ends_with(":0");
                    if endpoint.relay_required {
                        Response::RelayRequired { relay_addr }
                    } else {
                        Response::PeerFound { endpoint }
                    }
                }
            } else {
                Response::Error {
                    message: "peer not present in this cluster".into(),
                }
            }
        }
        Request::SignalForward {
            username,
            token,
            target_user,
            payload,
        } => {
            authorize(&db, &username, &token).await?;
            if state.peers.contains_key(&target_user) {
                let _ = payload;
                Response::SignalDelivered
            } else {
                Response::Error {
                    message: "target unavailable".into(),
                }
            }
        }
        Request::SupernodeDirectoryPush { peers } => {
            for peer in peers {
                state.peers.insert(
                    peer.username.clone(),
                    PeerState {
                        endpoint: peer,
                        last_heartbeat: Instant::now(),
                    },
                );
            }
            Response::Ack {
                message: format!(
                    "directory merged, known supernodes: {}",
                    state.known_supernodes.len()
                ),
            }
        }
        _ => Response::Error {
            message: "unsupported request on supernode".into(),
        },
    };

    let data = serde_json::to_vec(&response)?;
    stream.write_all(&data).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

async fn authorize(db: &Database, username: &str, token: &str) -> Result<(), AppError> {
    match db.session_owner(token).await? {
        Some(owner) if owner == username => Ok(()),
        _ => Err(AppError::Protocol("unauthorized session".into())),
    }
}
