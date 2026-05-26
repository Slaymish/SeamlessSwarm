use std::sync::{Arc, Mutex};
use std::time::Duration;
use nng::{Socket as NngSocket, Protocol as NngProtocol};
use prost::Message;
use sha2::Digest;
use crate::registry::{EphemeralRegistry, NodeProfile, Capability};
use crate::scheduler::{ProfileScheduler, Task as SchedulerTask, TaskType, ExecutionResult};
use crate::proto;

pub struct SwarmHubServer {
    registry: EphemeralRegistry,
    scheduler: ProfileScheduler,
    trusted_thumbprints: Arc<Mutex<Vec<String>>>,
}

impl SwarmHubServer {
    pub fn new(registry: EphemeralRegistry, scheduler: ProfileScheduler) -> Self {
        Self {
            registry,
            scheduler,
            trusted_thumbprints: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start_servers(self: Arc<Self>) {
        let self_auth = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_auth.run_auth_server("tcp://127.0.0.1:5555").await {
                eprintln!("[Hub] Handshake auth server error: {}", e);
            }
        });

        let self_heartbeat = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_heartbeat.run_heartbeat_server("tcp://127.0.0.1:5557").await {
                eprintln!("[Hub] Heartbeat receiver server error: {}", e);
            }
        });

        let self_tasks = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_tasks.run_task_sender_loop("tcp://127.0.0.1:5556").await {
                eprintln!("[Hub] Task sender error: {}", e);
            }
        });
    }

    // 1. Handshake Authentication Server (Req/Rep)
    async fn run_auth_server(&self, endpoint: &str) -> Result<(), String> {
        let server_socket = NngSocket::new(NngProtocol::Rep0).map_err(|e| e.to_string())?;
        server_socket.listen(endpoint).map_err(|e| e.to_string())?;
        println!("[Hub] Secure Cryptographic Handshake server listening on {}", endpoint);

        loop {
            // Receive Join request
            let msg = server_socket.recv().map_err(|e| e.to_string())?;
            let slice = msg.as_slice();
            if slice.len() < 5 {
                continue;
            }
            let _payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            let msg_type = slice[4];
            if msg_type != 1 {
                eprintln!("[Hub] Authentication Error: Expected Join initiation (1), received {}", msg_type);
                continue;
            }

            let node_id = String::from_utf8_lossy(&slice[5..]);
            println!("\n[Hub] <<< Handshake Request initiated by Workstation: {}", node_id);

            // Generate high-entropy 32-byte challenge token
            let mut challenge_token = vec![0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut challenge_token);
            println!("[Hub] Generated high-entropy challenge: {}", hex::encode(&challenge_token));

            let challenge = proto::HandshakeChallenge {
                high_entropy_token: challenge_token.clone(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            let mut encoded_challenge = Vec::new();
            challenge.encode(&mut encoded_challenge).unwrap();

            // Frame and send challenge
            let mut challenge_frame = Vec::new();
            challenge_frame.extend_from_slice(&((encoded_challenge.len() as u32).to_be_bytes()));
            challenge_frame.push(2); // MsgType 2 = HandshakeChallenge
            challenge_frame.extend_from_slice(&encoded_challenge);

            println!("[Hub] Sending challenge frame to node...");
            server_socket.send(&challenge_frame).map_err(|(_, e)| e.to_string())?;

            // Receive HandshakeResponse
            let resp_msg = server_socket.recv().map_err(|e| e.to_string())?;
            let resp_slice = resp_msg.as_slice();
            if resp_slice.len() < 5 {
                continue;
            }
            let resp_len = u32::from_be_bytes([resp_slice[0], resp_slice[1], resp_slice[2], resp_slice[3]]) as usize;
            let resp_msg_type = resp_slice[4];
            if resp_msg_type != 3 {
                eprintln!("[Hub] Authentication Error: Expected Response frame (3), got {}", resp_msg_type);
                continue;
            }

            let response = match proto::HandshakeResponse::decode(&resp_slice[5..5 + resp_len]) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[Hub] Failed to decode response payload: {}", e);
                    continue;
                }
            };

            // Verify Challenge
            let mut authenticated = false;
            let mut fail_reason = String::new();
            let mut verified_node_id = String::new();

            if let Some(identity) = response.identity {
                verified_node_id = identity.node_uuid.clone();
                let pk_hex = hex::encode(&identity.ecdsa_public_key);
                let sig_hex = hex::encode(&response.signature);

                println!("[Hub] Verifying cryptographic ECDSA signature from node: {}", verified_node_id);
                println!("[Agent PubKey] {}", pk_hex);
                println!("[Agent Signature] {}", sig_hex);

                // Derive thumbprint
                let mut hasher = sha2::Sha256::new();
                hasher.update(&identity.ecdsa_public_key);
                let derived_thumbprint = hex::encode(hasher.finalize());
                println!("[Hub] Derived Thumbprint from secure element: {}", derived_thumbprint);

                // Auto-authorize dynamic simulated thumbprints for ease of demo!
                {
                    let mut lock = self.trusted_thumbprints.lock().unwrap();
                    if !lock.contains(&derived_thumbprint) {
                        println!("[Hub] Dynamically authorizing new Secure Element thumbprint: {}", derived_thumbprint);
                        lock.push(derived_thumbprint.clone());
                    }
                }

                // Verify ECDSA Signature
                if crate::auth::verify_challenge_response(&pk_hex, &challenge_token, &sig_hex) {
                    authenticated = true;
                    println!("[Hub] SUCCESS: Cryptographic ECDSA signature validated!");
                } else {
                    fail_reason = "Cryptographic ECDSA verification failed".to_string();
                }
            } else {
                fail_reason = "Missing identity block".to_string();
            }

            // Construct and send HandshakeResult
            let result = proto::HandshakeResult {
                authenticated,
                session_token: if authenticated { uuid::Uuid::new_v4().to_string() } else { "".to_string() },
                message: if authenticated {
                    format!("Welcome {}, swarm authentication granted!", verified_node_id)
                } else {
                    format!("Authentication failed: {}", fail_reason)
                },
            };

            let mut encoded_result = Vec::new();
            result.encode(&mut encoded_result).unwrap();

            let mut result_frame = Vec::new();
            result_frame.extend_from_slice(&((encoded_result.len() as u32).to_be_bytes()));
            result_frame.push(4); // MsgType 4 = HandshakeResult
            result_frame.extend_from_slice(&encoded_result);

            println!("[Hub] Sending HandshakeResult frame...");
            server_socket.send(&result_frame).map_err(|(_, e)| e.to_string())?;
        }
    }

    // 2. Heartbeat Receiver Server (Pull)
    async fn run_heartbeat_server(&self, endpoint: &str) -> Result<(), String> {
        let server_socket = NngSocket::new(NngProtocol::Pull0).map_err(|e| e.to_string())?;
        server_socket.listen(endpoint).map_err(|e| e.to_string())?;
        println!("[Hub] Heartbeat Capability Receiver listening on {}", endpoint);

        loop {
            let msg = server_socket.recv().map_err(|e| e.to_string())?;
            let slice = msg.as_slice();
            if slice.len() < 5 {
                continue;
            }
            let payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            let msg_type = slice[4];
            if msg_type != 5 {
                continue;
            }

            let profile = match proto::CapabilityProfile::decode(&slice[5..5+payload_len]) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[Hub] Failed to decode CapabilityProfile: {}", e);
                    continue;
                }
            };

            println!("\n[Hub] <<< RECEIVED CAPABILITY PROFILE from {}", profile.node_id);
            println!("[Hub] Node OS: {}", profile.os_platform);
            
            let mut core_caps = Vec::new();
            for cap in &profile.capabilities {
                let val_str = match &cap.resource_value {
                    Some(proto::device_capability::ResourceValue::BoolVal(b)) => b.to_string(),
                    Some(proto::device_capability::ResourceValue::IntVal(i)) => i.to_string(),
                    Some(proto::device_capability::ResourceValue::DoubleVal(d)) => d.to_string(),
                    Some(proto::device_capability::ResourceValue::StringVal(s)) => s.clone(),
                    None => "".to_string(),
                };
                println!("  - {}: {} ({})", cap.resource_name, val_str, cap.value_type);

                core_caps.push(Capability {
                    name: cap.resource_name.clone(),
                    val_type: cap.value_type.clone(),
                    value: val_str,
                });
            }

            // Register into our Ephemeral Registry
            let node = NodeProfile {
                node_id: profile.node_id.clone(),
                os_platform: profile.os_platform.clone(),
                capabilities: core_caps,
                last_seen: 1200, // Represent high quality node heartbeat
                public_key: "".to_string(),
            };
            self.registry.register_node(node);
            println!("[Hub] Node {} successfully registered in ephemeral registry database.", profile.node_id);
        }
    }

    // 3. Task Distribution Loop (Push)
    async fn run_task_sender_loop(&self, endpoint: &str) -> Result<(), String> {
        let task_socket = NngSocket::new(NngProtocol::Push0).map_err(|e| e.to_string())?;
        task_socket.listen(endpoint).map_err(|e| e.to_string())?;
        println!("[Hub] Task Distribution server listening on {}", endpoint);

        // Keep pushing tasks periodically for testing / demo purposes!
        let mut task_counter = 1;
        loop {
            tokio::time::sleep(Duration::from_secs(8)).await;

            let nodes = self.registry.list_nodes();
            if nodes.is_empty() {
                continue;
            }

            let target_node = &nodes[0];
            println!("\n[Hub] === SCHEDULER DISPATCH INITIATED ===");
            println!("[Hub] Running profile-driven task matching...");

            // Create a stateless simulated task
            let task_id = format!("task-{:03}", task_counter);
            let scheduler_task = SchedulerTask {
                task_id: task_id.clone(),
                task_type: TaskType::StatelessIdempotent,
                payload: vec![100, 101, 102],
                max_retries: 3,
            };

            let schedule_res = self.scheduler.schedule_task(&scheduler_task, &target_node.node_id);
            match schedule_res {
                ExecutionResult::Success => {
                    println!("[Hub] Task {} matched with node {}. Dispatching via NNG control frame!", task_id, target_node.node_id);
                    
                    // Dispatch task via NNG
                    let task_def = proto::TaskDefinition {
                        task_id,
                        category: proto::TaskCategory::StatelessIdempotent as i32,
                        module_target: target_node.node_id.clone(),
                        payload: vec![12, 34, 56],
                        max_retries: 3,
                        timeout_ms: 10000,
                    };

                    let mut encoded_task = Vec::new();
                    task_def.encode(&mut encoded_task).unwrap();

                    let mut task_frame = Vec::new();
                    task_frame.extend_from_slice(&((encoded_task.len() as u32).to_be_bytes()));
                    task_frame.push(6); // MsgType 6 = TaskDefinition
                    task_frame.extend_from_slice(&encoded_task);

                    if let Err(e) = task_socket.send(&task_frame) {
                        eprintln!("[Hub] Failed to push task definition: {:?}", e);
                    } else {
                        println!("[Hub] Task definition sent successfully!");
                        task_counter += 1;
                    }
                }
                _ => {
                    println!("[Hub] Task schedule failed: Node resources not ready");
                }
            }
        }
    }
}
