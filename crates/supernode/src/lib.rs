//! Supernode service: peer directory, routing, and signaling bridge.

use common::{config::NetworkConfig, error::AppError};
use dashmap::DashMap;
use database::Database;
use protocol::{
    codec::{read_json, write_json},
    messages::{PeerEndpoint, Request, Response},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time,
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
    channels: Arc<DashMap<String, mpsc::UnboundedSender<Response>>>,
    known_supernodes: Arc<DashMap<String, ()>>,
    cluster_limit: usize,
    self_addr: String,
}

impl SupernodeState {
    fn new(cfg: &NetworkConfig) -> Self {
        let known = DashMap::new();
        for s in cfg.known_supernodes.iter().take(cfg.supernode_cache_size) {
            known.insert(s.clone(), ());
        }

        Self {
            peers: Arc::new(DashMap::new()),
            channels: Arc::new(DashMap::new()),
            known_supernodes: Arc::new(known),
            cluster_limit: cfg.peer_cluster_limit,
            self_addr: format!("{}:{}", cfg.supernode_host, cfg.supernode_port),
        }
    }
}

pub async fn run(cfg: NetworkConfig, db_url: &str) -> Result<(), AppError> {
    let bind_addr = format!("{}:{}", cfg.supernode_host, cfg.supernode_port);
    let listener = TcpListener::bind(&bind_addr).await?;
    let db = Database::connect(db_url).await?;
    let state = SupernodeState::new(&cfg);

    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                let stale: Vec<String> = state
                    .peers
                    .iter()
                    .filter_map(|entry| {
                        (entry.value().last_heartbeat.elapsed() > Duration::from_secs(90))
                            .then(|| entry.key().clone())
                    })
                    .collect();
                for user in stale {
                    state.peers.remove(&user);
                    state.channels.remove(&user);
                }
            }
        });
    }

    loop {
        let (stream, _) = listener.accept().await?;
        let db = db.clone();
        let state = state.clone();
        let relay_addr = format!("{}:{}", cfg.relay_host, cfg.relay_port);
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, db, state, relay_addr).await {
                eprintln!("supernode client error: {err}");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    db: Database,
    state: SupernodeState,
    relay_addr: String,
) -> Result<(), AppError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let (tx, mut rx) = mpsc::unbounded_channel::<Response>();
    let mut bound_user: Option<String> = None;

    loop {
        tokio::select! {
            biased;

            Some(outgoing) = rx.recv() => {
                write_json(&mut write_half, &outgoing).await?;
            }

            req = read_json::<Request,_>(&mut reader) => {
                let req = match req {
                    Ok(r) => r,
                    Err(AppError::Protocol(msg)) if msg == "connection closed" => break,
                    Err(err) => {
                        write_json(&mut write_half, &Response::Error { message: format!("invalid request: {err}") }).await?;
                        continue;
                    }
                };

                let response = match req {
                    Request::Heartbeat { username, token } => {
                        authorize(&db, &username, &token).await?;
                        if let Some(mut p) = state.peers.get_mut(&username) {
                            p.last_heartbeat = Instant::now();
                            Response::Ack { message: "heartbeat accepted".into() }
                        } else {
                            Response::Error { message: "peer not announced".into() }
                        }
                    }
                    Request::AnnouncePeer { username, token, tcp_addr, udp_addr, relay_required, status } => {
                        authorize(&db, &username, &token).await?;
                        if state.peers.len() >= state.cluster_limit && !state.peers.contains_key(&username) {
                            Response::Error { message: "supernode cluster full".into() }
                        } else {
                            let endpoint = PeerEndpoint {
                                username: username.clone(),
                                tcp_addr,
                                udp_addr,
                                supernode_addr: state.self_addr.clone(),
                                relay_required,
                                status,
                            };
                            state.peers.insert(username.clone(), PeerState { endpoint, last_heartbeat: Instant::now() });
                            state.channels.insert(username.clone(), tx.clone());
                            bound_user = Some(username);
                            Response::Ack { message: "peer announced".into() }
                        }
                    }
                    Request::PresenceUpdate { username, token, status } => {
                        authorize(&db, &username, &token).await?;
                        if let Some(mut p) = state.peers.get_mut(&username) {
                            p.endpoint.status = status;
                            p.last_heartbeat = Instant::now();
                            Response::Ack { message: "presence updated".into() }
                        } else {
                            Response::Error { message: "peer not announced".into() }
                        }
                    }
                    Request::PeerLookup { username, token, target_user }
                    | Request::ConnectRequest { username, token, target_user } => {
                        authorize(&db, &username, &token).await?;
                        if let Some(peer) = state.peers.get(&target_user) {
                            if peer.endpoint.relay_required {
                                Response::RelayRequired { relay_addr: relay_addr.clone() }
                            } else {
                                Response::PeerFound { endpoint: peer.endpoint.clone() }
                            }
                        } else {
                            Response::Error { message: "peer not present in this cluster".into() }
                        }
                    }
                    Request::SignalForward { username, token, target_user, payload } => {
                        authorize(&db, &username, &token).await?;
                        if let Some(ch) = state.channels.get(&target_user) {
                            let _ = ch.send(Response::IncomingSignal { from_user: username, payload });
                            Response::SignalDelivered
                        } else {
                            Response::Error { message: "target unavailable".into() }
                        }
                    }
                    Request::SupernodeDirectoryPush { peers } => {
                        for peer in peers {
                            state.peers.insert(peer.username.clone(), PeerState { endpoint: peer, last_heartbeat: Instant::now() });
                        }
                        Response::Ack {
                            message: format!("directory merged, known supernodes: {}", state.known_supernodes.len()),
                        }
                    }
                    _ => Response::Error { message: "unsupported request on supernode".into() },
                };

                write_json(&mut write_half, &response).await?;
            }
        }
    }

    if let Some(user) = bound_user {
        state.channels.remove(&user);
    }

    Ok(())
}

async fn authorize(db: &Database, username: &str, token: &str) -> Result<(), AppError> {
    match db.session_owner(token).await? {
        Some(owner) if owner == username => Ok(()),
        _ => Err(AppError::Protocol("unauthorized session".into())),
    }
}
