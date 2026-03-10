use common::config::NetworkConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = NetworkConfig::from_file("config/network.toml")?;
    relay_node::run(cfg).await?;
    Ok(())
}
