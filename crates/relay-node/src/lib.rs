//! UDP relay node for NAT fallback traffic.

use common::{config::NetworkConfig, error::AppError};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::UdpSocket;

#[derive(Debug, Serialize, Deserialize)]
struct RelayPacket {
    from_user: String,
    to_user: String,
    payload: Vec<u8>,
}

/// Starts a UDP relay that forwards packets between known peer addresses.
pub async fn run(cfg: NetworkConfig) -> Result<(), AppError> {
    let bind = format!("{}:{}", cfg.relay_host, cfg.relay_port);
    let socket = UdpSocket::bind(bind).await?;
    let peer_addrs: Arc<DashMap<String, SocketAddr>> = Arc::new(DashMap::new());
    let mut buf = vec![0u8; 2048];

    loop {
        let (n, src) = socket.recv_from(&mut buf).await?;
        let packet = match serde_json::from_slice::<RelayPacket>(&buf[..n]) {
            Ok(p) => p,
            Err(_) => continue,
        };

        peer_addrs.insert(packet.from_user.clone(), src);
        if let Some(target) = peer_addrs.get(&packet.to_user) {
            let out = serde_json::to_vec(&packet)?;
            socket.send_to(&out, *target).await?;
        }
    }
}
