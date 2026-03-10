//! Dev helper to print commands for launching a mini cluster.

fn main() {
    println!("Run these in separate terminals:");
    println!("cargo run -p login-server --bin login-server");
    println!("cargo run -p supernode --bin supernode");
    println!("cargo run -p relay-node --bin relay-node");
    println!("\nThen connect a custom peer that speaks crates/protocol JSON frames.");
}
