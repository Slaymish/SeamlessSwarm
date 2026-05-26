use compute_module_core::registry::EphemeralRegistry;
use compute_module_core::scheduler::ProfileScheduler;
use compute_module_core::server::SwarmHubServer;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    println!("=== Seamless Swarm: Central Hub / ARM Appliance (Simulation Mode) ===");
    
    let registry = EphemeralRegistry::new();
    let scheduler = ProfileScheduler::new(registry.clone());

    let hub_server = Arc::new(SwarmHubServer::new(registry, scheduler));
    hub_server.start_servers();

    println!("[Hub] Services started. Standing by for workstation agents to authenticate...");
    println!("[Hub] Press Ctrl+C to stop.");

    match tokio::signal::ctrl_c().await {
        Ok(_) => println!("[Hub] Shutting down central hub."),
        Err(err) => eprintln!("[Hub] Error waiting for Ctrl+C: {}", err),
    }
}
