use std::sync::{Arc, Mutex};
use std::time::Duration;
use nng::{Socket as NngSocket, Protocol as NngProtocol};
use nng::options::Options;
use prost::Message;
use sha2::Digest;
use log::{info, error, debug};
use crate::registry::{EphemeralRegistry, NodeProfile, Capability};
use crate::scheduler::{ProfileScheduler, Task as SchedulerTask, TaskType, TaskState, ExecutionResult};
use crate::proto;

pub struct SwarmHubServer {
    registry: EphemeralRegistry,
    scheduler: ProfileScheduler,
    trusted_thumbprints: Arc<Mutex<Vec<String>>>,
    // Shared push socket — task sender loop + medium server both use this
    task_push: Arc<Mutex<Option<NngSocket>>>,
    // Pub socket — broadcast progress to all medium CLI subscribers
    progress_pub: Arc<Mutex<Option<NngSocket>>>,
    bind_ip: String,
}

impl SwarmHubServer {
    pub fn new(registry: EphemeralRegistry, scheduler: ProfileScheduler, bind_ip: String) -> Self {
        Self {
            registry,
            scheduler,
            trusted_thumbprints: Arc::new(Mutex::new(Vec::new())),
            task_push: Arc::new(Mutex::new(None)),
            progress_pub: Arc::new(Mutex::new(None)),
            bind_ip,
        }
    }

    pub fn start_servers(self: Arc<Self>) {
        let task_addr = format!("tcp://{}:5556", self.bind_ip);
        let pub_addr  = format!("tcp://{}:5560", self.bind_ip);
        let auth_addr = format!("tcp://{}:5555", self.bind_ip);
        let hb_addr   = format!("tcp://{}:5557", self.bind_ip);
        let med_addr  = format!("tcp://{}:5559", self.bind_ip);
        let prg_addr  = format!("tcp://{}:5558", self.bind_ip);

        // Create task distribution push socket (agents pull from here)
        let task_socket = NngSocket::new(NngProtocol::Push0)
            .expect("[Hub] Failed to create task push socket");
        task_socket.listen(&task_addr)
            .expect("[Hub] Failed to listen on task port 5556");
        *self.task_push.lock().unwrap() = Some(task_socket);
        info!("[Hub] Task Distribution socket listening on {}", task_addr);

        // Create progress broadcast pub socket (medium CLI subscribes here)
        let pub_socket = NngSocket::new(NngProtocol::Pub0)
            .expect("[Hub] Failed to create progress pub socket");
        pub_socket.listen(&pub_addr)
            .expect("[Hub] Failed to listen on progress broadcast port 5560");
        *self.progress_pub.lock().unwrap() = Some(pub_socket);
        info!("[Hub] Progress broadcast (Pub) socket listening on {}", pub_addr);

        let self_auth = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_auth.run_auth_server(&auth_addr).await {
                eprintln!("[Hub] Auth server error: {}", e);
            }
        });

        let self_heartbeat = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_heartbeat.run_heartbeat_server(&hb_addr).await {
                eprintln!("[Hub] Heartbeat receiver error: {}", e);
            }
        });

        let self_tasks = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_tasks.run_task_sender_loop().await {
                eprintln!("[Hub] Task sender error: {}", e);
            }
        });

        let self_medium = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_medium.run_medium_server(&med_addr).await {
                eprintln!("[Hub] Medium interface server error: {}", e);
            }
        });

        let self_progress = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = self_progress.run_progress_receiver_server(&prg_addr) {
                eprintln!("[Hub] Progress receiver error: {}", e);
            }
        });
    }

    // Encode a task and push it to all connected agents via the shared push socket.
    fn push_task_frame(&self, task: &SchedulerTask, task_id: &str, node_id: &str) -> bool {
        let category = match task.task_type {
            TaskType::StatelessIdempotent => proto::TaskCategory::StatelessIdempotent as i32,
            TaskType::StatefulLongRunning => proto::TaskCategory::StatefulLongRunning as i32,
            TaskType::InteractiveLowLatency => proto::TaskCategory::InteractiveLowLatency as i32,
        };

        let task_def = proto::TaskDefinition {
            task_id: task_id.to_string(),
            category,
            module_target: node_id.to_string(),
            payload: task.payload.clone(),
            max_retries: task.max_retries as u32,
            timeout_ms: 10000,
            required_capabilities: task.required_capabilities.clone(),
        };

        let mut encoded = Vec::new();
        task_def.encode(&mut encoded).unwrap();

        let mut frame = Vec::new();
        frame.extend_from_slice(&((encoded.len() as u32).to_be_bytes()));
        frame.push(6); // MsgType 6 = TaskDefinition
        frame.extend_from_slice(&encoded);

        let lock = self.task_push.lock().unwrap();
        if let Some(ref socket) = *lock {
            socket.send(&frame).is_ok()
        } else {
            false
        }
    }

    // Run dispatch for all pending tasks against the current node registry.
    fn dispatch_now(&self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let dispatches = self.scheduler.dispatch_pending_tasks(current_time);
        for (task_id, node_id) in dispatches {
            if let Some(task) = self.scheduler.get_task(&task_id) {
                let caps = task.required_capabilities.join(", ");
                if caps.is_empty() {
                    info!("[Hub] Dispatching {} → node {}", task_id, node_id);
                } else {
                    info!("[Hub] Dispatching {} → node {} (requires: {})", task_id, node_id, caps);
                }
                if self.push_task_frame(&task, &task_id, &node_id) {
                    info!("[Hub] Task {} sent successfully.", task_id);
                } else {
                    error!("[Hub] Failed to push task {}.", task_id);
                }
            }
        }
    }

    // 1. Handshake Authentication Server (Req/Rep)
    async fn run_auth_server(&self, endpoint: &str) -> Result<(), String> {
        let server_socket = NngSocket::new(NngProtocol::Rep0).map_err(|e| e.to_string())?;
        server_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(500)))
            .map_err(|e: nng::Error| e.to_string())?;
        server_socket.listen(endpoint).map_err(|e| e.to_string())?;
        info!("[Hub] Auth server listening on {}", endpoint);

        loop {
            let msg = match server_socket.recv() {
                Ok(m) => m,
                Err(nng::Error::TimedOut) => continue,
                Err(e) => return Err(e.to_string()),
            };

            let slice = msg.as_slice();
            if slice.len() < 5 { continue; }
            let _payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            let msg_type = slice[4];
            if msg_type != 1 {
                error!("[Hub] Auth: expected Join (1), got {}", msg_type);
                continue;
            }

            let node_id = String::from_utf8_lossy(&slice[5..]);
            info!("[Hub] Handshake initiated by: {}", node_id);

            let mut challenge_token = vec![0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut challenge_token);
            debug!("[Hub] Challenge token: {}", hex::encode(&challenge_token));

            let challenge = proto::HandshakeChallenge {
                high_entropy_token: challenge_token.clone(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            let mut encoded_challenge = Vec::new();
            challenge.encode(&mut encoded_challenge).unwrap();
            let mut challenge_frame = Vec::new();
            challenge_frame.extend_from_slice(&((encoded_challenge.len() as u32).to_be_bytes()));
            challenge_frame.push(2); // MsgType 2 = HandshakeChallenge
            challenge_frame.extend_from_slice(&encoded_challenge);
            server_socket.send(&challenge_frame).map_err(|(_, e)| e.to_string())?;

            let resp_msg = server_socket.recv().map_err(|e| e.to_string())?;
            let resp_slice = resp_msg.as_slice();
            if resp_slice.len() < 5 { continue; }
            let resp_len = u32::from_be_bytes([resp_slice[0], resp_slice[1], resp_slice[2], resp_slice[3]]) as usize;
            if resp_slice[4] != 3 {
                error!("[Hub] Auth: expected Response (3), got {}", resp_slice[4]);
                continue;
            }

            let response = match proto::HandshakeResponse::decode(&resp_slice[5..5 + resp_len]) {
                Ok(r) => r,
                Err(e) => { error!("[Hub] Failed to decode HandshakeResponse: {}", e); continue; }
            };

            let mut authenticated = false;
            let mut fail_reason = String::new();
            let mut verified_node_id = String::new();

            if let Some(identity) = response.identity {
                verified_node_id = identity.node_uuid.clone();
                let pk_hex = hex::encode(&identity.ecdsa_public_key);
                let sig_hex = hex::encode(&response.signature);
                info!("[Hub] Verifying ECDSA signature from: {}", verified_node_id);
                debug!("[PubKey] {}", pk_hex);
                debug!("[Signature] {}", sig_hex);

                let mut hasher = sha2::Sha256::new();
                hasher.update(&identity.ecdsa_public_key);
                let derived_thumbprint = hex::encode(hasher.finalize());

                {
                    let mut lock = self.trusted_thumbprints.lock().unwrap();
                    if !lock.contains(&derived_thumbprint) {
                        info!("[Hub] Authorizing new thumbprint: {}", derived_thumbprint);
                        lock.push(derived_thumbprint.clone());
                    }
                }

                if crate::auth::verify_challenge_response(&pk_hex, &challenge_token, &sig_hex) {
                    authenticated = true;
                    info!("[Hub] ECDSA verification passed.");
                } else {
                    fail_reason = "ECDSA verification failed".to_string();
                }
            } else {
                fail_reason = "Missing identity block".to_string();
            }

            let result = proto::HandshakeResult {
                authenticated,
                session_token: if authenticated { uuid::Uuid::new_v4().to_string() } else { String::new() },
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
            server_socket.send(&result_frame).map_err(|(_, e)| e.to_string())?;
        }
    }

    // 2. Heartbeat / Capability Receiver (Pull)
    async fn run_heartbeat_server(&self, endpoint: &str) -> Result<(), String> {
        let server_socket = NngSocket::new(NngProtocol::Pull0).map_err(|e| e.to_string())?;
        server_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(500)))
            .map_err(|e: nng::Error| e.to_string())?;
        server_socket.listen(endpoint).map_err(|e| e.to_string())?;
        info!("[Hub] Heartbeat receiver listening on {}", endpoint);

        loop {
            let msg = match server_socket.recv() {
                Ok(m) => m,
                Err(nng::Error::TimedOut) => continue,
                Err(e) => return Err(e.to_string()),
            };

            let slice = msg.as_slice();
            if slice.len() < 5 { continue; }
            let payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            if slice[4] != 5 { continue; } // MsgType 5 = CapabilityProfile

            let profile = match proto::CapabilityProfile::decode(&slice[5..5 + payload_len]) {
                Ok(p) => p,
                Err(e) => { eprintln!("[Hub] Failed to decode CapabilityProfile: {}", e); continue; }
            };

            info!("\n[Hub] <<< CAPABILITY PROFILE from {}", profile.node_id);
            info!("[Hub] OS: {}", profile.os_platform);

            let mut core_caps = Vec::new();
            for cap in &profile.capabilities {
                let val_str = match &cap.resource_value {
                    Some(proto::device_capability::ResourceValue::BoolVal(b)) => b.to_string(),
                    Some(proto::device_capability::ResourceValue::IntVal(i)) => i.to_string(),
                    Some(proto::device_capability::ResourceValue::DoubleVal(d)) => d.to_string(),
                    Some(proto::device_capability::ResourceValue::StringVal(s)) => s.clone(),
                    None => String::new(),
                };
                info!("  - {}: {} ({})", cap.resource_name, val_str, cap.value_type);
                core_caps.push(Capability { name: cap.resource_name.clone(), val_type: cap.value_type.clone(), value: val_str });
            }

            self.registry.register_node(NodeProfile {
                node_id: profile.node_id.clone(),
                os_platform: profile.os_platform.clone(),
                capabilities: core_caps,
                last_seen: 1200,
                public_key: String::new(),
            });
            info!("[Hub] Node {} registered.", profile.node_id);
        }
    }

    // 3. Task Dispatch Loop — uses shared push socket
    async fn run_task_sender_loop(&self) -> Result<(), String> {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;

            if self.registry.list_nodes().is_empty() {
                continue;
            }

            let pending_count = self.scheduler.list_tasks().iter()
                .filter(|t| t.state == TaskState::Pending)
                .count();

            if pending_count > 0 {
                info!("\n[Hub] === SCHEDULER DISPATCH ===");
                self.dispatch_now();
            }
        }
    }

    // 4. Medium Interface Server (Req/Rep) — NEW port 5559
    async fn run_medium_server(&self, endpoint: &str) -> Result<(), String> {
        let server_socket = NngSocket::new(NngProtocol::Rep0).map_err(|e| e.to_string())?;
        server_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(500)))
            .map_err(|e: nng::Error| e.to_string())?;
        server_socket.listen(endpoint).map_err(|e| e.to_string())?;
        info!("[Hub] Medium interface server listening on {}", endpoint);

        loop {
            let msg = match server_socket.recv() {
                Ok(m) => m,
                Err(nng::Error::TimedOut) => continue,
                Err(e) => return Err(e.to_string()),
            };

            let slice = msg.as_slice();
            if slice.len() < 5 {
                let _ = server_socket.send(&[0u8; 5][..]);
                continue;
            }

            let payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            let msg_type = slice[4];

            match msg_type {
                10 => {
                    // SwarmStatusRequest
                    let req = match proto::SwarmStatusRequest::decode(&slice[5..5 + payload_len]) {
                        Ok(r) => r,
                        Err(_) => { let _ = server_socket.send(&[0u8; 5][..]); continue; }
                    };

                    let authorized = !req.access_key.is_empty();
                    let nodes = self.registry.list_nodes();
                    let mut caps: Vec<String> = nodes.iter()
                        .flat_map(|n| n.capabilities.iter().map(|c| c.name.clone()))
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    caps.sort();

                    info!("[Hub] Medium status request — access_key='{}', nodes={}, authorized={}",
                        req.access_key, nodes.len(), authorized);

                    let response = proto::SwarmStatusResponse {
                        authorized,
                        node_count: nodes.len() as i32,
                        available_capabilities: caps,
                        message: if authorized { "Access granted".to_string() } else { "Invalid access key".to_string() },
                    };
                    let mut buf = Vec::new();
                    response.encode(&mut buf).unwrap();
                    let mut frame = Vec::new();
                    frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                    frame.push(11); // SwarmStatusResponse
                    frame.extend_from_slice(&buf);
                    let _ = server_socket.send(&frame);
                }
                12 => {
                    // TaskSubmitRequest
                    let req = match proto::TaskSubmitRequest::decode(&slice[5..5 + payload_len]) {
                        Ok(r) => r,
                        Err(_) => { let _ = server_socket.send(&[0u8; 5][..]); continue; }
                    };

                    if req.access_key.is_empty() {
                        let response = proto::TaskSubmitResponse {
                            accepted: false,
                            task_id: String::new(),
                            message: "Invalid access key".to_string(),
                        };
                        let mut buf = Vec::new();
                        response.encode(&mut buf).unwrap();
                        let mut frame = Vec::new();
                        frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                        frame.push(13);
                        frame.extend_from_slice(&buf);
                        let _ = server_socket.send(&frame);
                        continue;
                    }

                    let task_id = format!("medium-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                    info!("[Hub] Medium CLI submitted task: {} (\"{}\")", task_id, req.task_name);

                    let task = SchedulerTask::new(
                        task_id.clone(),
                        match req.category {
                            1 => TaskType::StatefulLongRunning,
                            2 => TaskType::InteractiveLowLatency,
                            _ => TaskType::StatelessIdempotent,
                        },
                        req.payload,
                        3,
                    ).with_capabilities(req.required_capabilities);

                    self.scheduler.submit_task(task);
                    // Immediately try to dispatch — don't wait for the 2s loop
                    self.dispatch_now();

                    let response = proto::TaskSubmitResponse {
                        accepted: true,
                        task_id: task_id.clone(),
                        message: "Task queued for dispatch to swarm".to_string(),
                    };
                    let mut buf = Vec::new();
                    response.encode(&mut buf).unwrap();
                    let mut frame = Vec::new();
                    frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                    frame.push(13); // TaskSubmitResponse
                    frame.extend_from_slice(&buf);
                    let _ = server_socket.send(&frame);
                }
                _ => {
                    // Unknown — send empty ack so the client's Req socket isn't stuck
                    let _ = server_socket.send(&[0u8; 5][..]);
                }
            }
        }
    }

    // 5. Task Progress Receiver (Pull) — also broadcasts on progress_pub
    pub fn run_progress_receiver_server(&self, endpoint: &str) -> Result<(), String> {
        let server_socket = NngSocket::new(NngProtocol::Pull0).map_err(|e| e.to_string())?;
        server_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(500)))
            .map_err(|e: nng::Error| e.to_string())?;
        server_socket.listen(endpoint).map_err(|e| e.to_string())?;
        info!("[Hub] Task progress receiver listening on {}", endpoint);

        loop {
            let msg = match server_socket.recv() {
                Ok(m) => m,
                Err(nng::Error::TimedOut) => continue,
                Err(e) => return Err(e.to_string()),
            };

            let slice = msg.as_slice();
            if slice.len() < 5 { continue; }
            let payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            if slice[4] != 7 { continue; } // MsgType 7 = TaskProgress

            // Save raw frame bytes for broadcasting before we borrow for decode
            let frame_bytes = slice.to_vec();

            let progress = match proto::TaskProgress::decode(&slice[5..5 + payload_len]) {
                Ok(p) => p,
                Err(e) => { error!("[Hub] Failed to decode TaskProgress: {}", e); continue; }
            };

            info!("\n[Hub] <<< TASK PROGRESS: {} — status={:?} {:.1}%",
                progress.task_id, progress.status, progress.progress_percentage);

            // Broadcast raw frame to all medium CLI subscribers
            {
                let lock = self.progress_pub.lock().unwrap();
                if let Some(ref pub_socket) = *lock {
                    let _ = pub_socket.send(&frame_bytes);
                }
            }

            // Update scheduler state
            let result = if progress.status == proto::TaskStatus::Completed as i32 {
                ExecutionResult::Success
            } else if !progress.checkpoint_data.is_empty() {
                info!("[Hub] Checkpoint saved for {}: {}B", progress.task_id, progress.checkpoint_data.len());
                ExecutionResult::CheckpointSaved(progress.checkpoint_data)
            } else {
                continue;
            };

            self.scheduler.update_task_progress(&progress.task_id, result);
        }
    }
}
