use common::config::NetworkConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = NetworkConfig::from_file("config/network.toml")?;
    supernode::run(cfg, "sqlite://skype-rs.db").await?;
    Ok(())
}
