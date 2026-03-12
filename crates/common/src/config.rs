use serde::Deserialize;
use std::{fs, path::Path};

use crate::error::AppError;

/// Runtime configuration shared by all node roles.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    pub login_host: String,
    pub login_port: u16,
    pub supernode_host: String,
    pub supernode_port: u16,
    pub relay_host: String,
    pub relay_port: u16,
    pub supernode_cache_size: usize,
    pub peer_cluster_limit: usize,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default)]
    pub known_supernodes: Vec<String>,
}

fn default_database_url() -> String {
    "sqlite://./data/skype-rs.db".to_string()
}

impl NetworkConfig {
    /// Reads a TOML file from disk and deserializes it into [`NetworkConfig`].
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let raw = fs::read_to_string(path)?;
        let cfg = toml::from_str::<Self>(&raw)?;
        Ok(cfg)
    }
}
