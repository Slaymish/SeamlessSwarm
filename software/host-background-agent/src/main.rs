use std::sync::Arc;
use std::time::Duration;
use log::{info, warn, error};
use uuid::Uuid;

use compute_module_core::registry::{EphemeralRegistry, NodeProfile, Capability};
use compute_module_core::scheduler::ProfileScheduler;
use compute_module_core::server::SwarmHubServer;

use seamless_node::election::ElectionState;
use seamless_node::scout::ScoutEngine;
use seamless_node::transport::{self, NngClient};
use seamless_node::proto;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("=== Seamless System Node ===");

    let node_id = format!("node-{}", &Uuid::new_v4().to_string()[..8]);
    info!("[Node {}] Starting up", node_id);

    // Discover local capabilities
    let scout = ScoutEngine::new();
    let capabilities = scout.discover_capabilities_async().await;
    info!("[Node {}] Discovered {} capabilities", node_id, capabilities.len());
    for cap in &capabilities {
        let val = match &cap.resource_value {
            Some(proto::device_capability::ResourceValue::BoolVal(b))   => b.to_string(),
            Some(proto::device_capability::ResourceValue::IntVal(i))     => i.to_string(),
            Some(proto::device_capability::ResourceValue::DoubleVal(d))  => format!("{:.2}", d),
            Some(proto::device_capability::ResourceValue::StringVal(s))  => s.clone(),
            None => String::new(),
        };
        info!("  - {}: {} ({})", cap.resource_name, val, cap.value_type);
    }

    // Leader election state — shared across broadcast, listener, and role loop
    let election = Arc::new(ElectionState::new(node_id.clone()));

    // Start mDNS broadcaster (sends PEER and, when leading, LEADER announcements)
    let (election_b, node_id_b) = (election.clone(), node_id.clone());
    tokio::spawn(async move {
        transport::run_mdns_broadcaster(node_id_b, election_b).await;
    });

    // Start mDNS listener (feeds peer/leader info into election state)
    let election_l = election.clone();
    tokio::spawn(async move {
        transport::run_mdns_listener(election_l).await;
    });

    // Give existing peers time to announce before deciding on a role
    info!("[Node {}] Listening for peers for 5 s...", node_id);
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Main role loop — re-evaluates whenever a role function returns
    loop {
        if let Some((leader_id, leader_tcp_base)) = election.current_leader() {
            info!("[Node {}] Joining swarm under leader {} at {}", node_id, leader_id, leader_tcp_base);
            run_as_follower(&node_id, &leader_tcp_base, capabilities.clone(), election.clone()).await;
            info!("[Node {}] Disconnected from leader. Re-evaluating...", node_id);
            election.clear_leader_flags();
            tokio::time::sleep(Duration::from_secs(2)).await;

        } else if election.i_should_be_leader() {
            info!("[Node {}] No leader found — assuming leadership (min ID in swarm).", node_id);
            run_as_leader(&node_id, capabilities.clone(), election.clone()).await;
            info!("[Node {}] Stepped down from leadership.", node_id);

        } else {
            info!("[Node {}] Waiting for leader with smaller ID to announce...", node_id);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}

/// Starts the hub server, registers own capabilities, also runs as a local worker.
/// Returns when a node with a smaller ID supersedes this node as leader.
async fn run_as_leader(
    node_id: &str,
    capabilities: Vec<proto::DeviceCapability>,
    election: Arc<ElectionState>,
) {
    let local_ip = transport::get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    info!("[Leader {}] Starting hub on 0.0.0.0:5555-5560 (advertising {})", node_id, local_ip);

    let registry = EphemeralRegistry::new();
    let scheduler = ProfileScheduler::new(registry.clone());

    // Self-register so the leader's own capabilities are visible to the scheduler
    let own_caps: Vec<Capability> = capabilities.iter().map(|c| {
        let value = match &c.resource_value {
            Some(proto::device_capability::ResourceValue::StringVal(s)) => s.clone(),
            Some(proto::device_capability::ResourceValue::BoolVal(b))   => b.to_string(),
            Some(proto::device_capability::ResourceValue::IntVal(i))     => i.to_string(),
            Some(proto::device_capability::ResourceValue::DoubleVal(d))  => format!("{:.2}", d),
            None => String::new(),
        };
        Capability { name: c.resource_name.clone(), val_type: c.value_type.clone(), value }
    }).collect();

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    registry.register_node(NodeProfile {
        node_id: node_id.to_string(),
        os_platform: std::env::consts::OS.to_string(),
        capabilities: own_caps,
        last_seen: now_ts,
        public_key: String::new(),
    });

    // Start hub server (bind on all interfaces)
    let hub = Arc::new(SwarmHubServer::new(registry, scheduler, "0.0.0.0".to_string()));
    hub.clone().start_servers();
    info!("[Leader {}] Hub servers started.", node_id);

    // Announce leadership via the election state (broadcaster will include LEADER message)
    election.set_i_am_leader(true);

    // Also participate as a worker via loopback
    let election_w = election.clone();
    let caps_w = capabilities.clone();
    let nid_w = node_id.to_string();
    tokio::spawn(async move {
        // Brief delay so the hub sockets are ready before we dial them
        tokio::time::sleep(Duration::from_millis(500)).await;
        run_worker_on_loopback(&nid_w, caps_w, election_w).await;
    });

    // Monitor: stay leader until a smaller-ID node appears
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if !election.i_should_be_leader() {
            warn!("[Leader {}] Smaller-ID node detected — stepping down.", node_id);
            election.set_i_am_leader(false);
            return; // hub Arc dropped here → sockets close
        }
    }
}

/// Connects to the hub running on loopback (used by the leader to also contribute as a worker).
async fn run_worker_on_loopback(
    node_id: &str,
    capabilities: Vec<proto::DeviceCapability>,
    election: Arc<ElectionState>,
) {
    let client = NngClient::new(
        "tcp://127.0.0.1:5555",
        "tcp://127.0.0.1:5556",
        "tcp://127.0.0.1:5557",
        "tcp://127.0.0.1:5558",
        node_id,
    );

    match client.run_demo_lifecycle(capabilities).await {
        Ok(()) => {
            loop {
                if !election.am_i_leader() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        Err(e) => error!("[Worker-local {}] Self-connect failed: {}", node_id, e),
    }
}

/// Connects to a remote leader and processes tasks until the leader goes offline.
async fn run_as_follower(
    node_id: &str,
    leader_tcp_base: &str,
    capabilities: Vec<proto::DeviceCapability>,
    election: Arc<ElectionState>,
) {
    let client = NngClient::new(
        &format!("tcp://{}:5555", leader_tcp_base),
        &format!("tcp://{}:5556", leader_tcp_base),
        &format!("tcp://{}:5557", leader_tcp_base),
        &format!("tcp://{}:5558", leader_tcp_base),
        node_id,
    );

    match client.run_demo_lifecycle(capabilities).await {
        Ok(()) => {
            // Worker task loop is running in the background; poll for leader health
            loop {
                if election.current_leader().is_none() {
                    warn!("[Follower {}] Leader went offline (mDNS timeout).", node_id);
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        Err(e) => error!("[Follower {}] Failed to connect to leader: {}", node_id, e),
    }
}
