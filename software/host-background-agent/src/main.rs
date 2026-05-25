mod scout;
mod transport;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/seamless_swarm.rs"));
}

use scout::ScoutEngine;
use transport::{MdnsResponder, NngClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let responder = MdnsResponder::new("swarm-host-agent", 5353)?;
    responder.start_broadcast().await?;

    let scout = ScoutEngine::new();
    let capabilities = scout.discover_capabilities();
    println!("Discovered Capabilities:");
    for cap in &capabilities {
        println!(" - {}: {} ({})", cap.name, cap.value, cap.val_type);
    }

    let payload = serde_json::to_vec(&capabilities)?;

    let mut client = NngClient::new("tcp://127.0.0.1:5555");
    client.connect().await?;
    client.send_payload(&payload).await?;

    Ok(())
}
