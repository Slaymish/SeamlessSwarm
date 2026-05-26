mod scout;
mod transport;
mod secure_element;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/seamless_swarm.rs"));
}

use scout::ScoutEngine;
use transport::{MdnsResponder, NngClient};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Seamless Swarm: Host Background Agent (Simulation Mode) ===");
    
    // Start mDNS Responder
    let responder = MdnsResponder::new("swarm-host-agent", 5353)?;
    responder.start_broadcast().await?;
    println!("[Agent] mDNS Peer Discovery Responder started on UDP 5353.");

    // Run capability discovery
    let scout = ScoutEngine::new();
    let capabilities = scout.discover_capabilities();
    println!("[Agent] Discovered Local Machine Capabilities:");
    for cap in &capabilities {
        println!("  - {}: {} ({})", cap.name, cap.value, cap.val_type);
    }

    // Generate/Reuse simulated unique node id
    let node_uuid = format!("node-{}", &Uuid::new_v4().to_string()[0..8]);

    // Connect and run handshake / execution loop
    let client = NngClient::new(
        "tcp://127.0.0.1:5555", // Authentication Port (Req/Rep)
        "tcp://127.0.0.1:5556", // Task Distribution Port (Pull)
        "tcp://127.0.0.1:5557", // Heartbeat / Profile Port (Push)
        &node_uuid,
    );

    // Run the full demo lifecycle (Handshake, registration, task listening)
    client.run_demo_lifecycle(capabilities).await?;

    println!("[Agent] Swarm connection active. Press Ctrl+C to terminate.");
    tokio::signal::ctrl_c().await?;
    println!("[Agent] Shutting down agent.");

    Ok(())
}
