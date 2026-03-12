use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A peer endpoint advertised by supernodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerEndpoint {
    pub username: String,
    pub tcp_addr: String,
    pub udp_addr: String,
    pub supernode_addr: String,
    pub relay_required: bool,
    pub status: String,
}

/// Peer/login request variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Login {
        username: String,
        password: String,
    },
    Register {
        username: String,
        password: String,
    },
    Heartbeat {
        username: String,
        token: String,
    },
    PresenceUpdate {
        username: String,
        token: String,
        status: String,
    },
    AnnouncePeer {
        username: String,
        token: String,
        tcp_addr: String,
        udp_addr: String,
        relay_required: bool,
        status: String,
    },
    ConnectRequest {
        username: String,
        token: String,
        target_user: String,
    },
    PeerLookup {
        username: String,
        token: String,
        target_user: String,
    },
    SignalForward {
        username: String,
        token: String,
        target_user: String,
        payload: serde_json::Value,
    },
    SupernodeDirectoryPush {
        peers: Vec<PeerEndpoint>,
    },
}

/// Generic service response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    LoginOk {
        token: Uuid,
        assigned_supernode: String,
    },
    RegisterOk,
    Ack {
        message: String,
    },
    PeerFound {
        endpoint: PeerEndpoint,
    },
    RelayRequired {
        relay_addr: String,
    },
    IncomingSignal {
        from_user: String,
        payload: serde_json::Value,
    },
    SignalDelivered,
    Error {
        message: String,
    },
}
