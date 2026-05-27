use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use std::process::Command;
use socket2::{Socket, Domain, Type, Protocol};
use nng::{Socket as NngSocket, Protocol as NngProtocol};
use nng::options::Options;
use prost::Message;
use rand::Rng;
use crate::proto;
use crate::secure_element::SimulatedSecureElement;

struct CapabilityExecutor;

impl CapabilityExecutor {
    fn probe(capability: &str) -> (bool, String) {
        match capability {
            "ffmpeg_execution" => Self::run("ffmpeg", &["-version"]),
            "blender_execution" => {
                let (ok, out) = Self::run("blender", &["--version"]);
                if ok {
                    return (ok, out);
                }
                Self::run("/Applications/Blender.app/Contents/MacOS/Blender", &["--version"])
            }
            "handbrake_execution" => {
                let (ok, out) = Self::run("HandBrakeCLI", &["--version"]);
                if ok {
                    return (ok, out);
                }
                let exists = std::path::Path::new("/Applications/HandBrake.app").exists();
                (exists, if exists { "HandBrake.app bundle present".to_string() } else { "not found".to_string() })
            }
            "inkscape_execution" => Self::run("inkscape", &["--version"]),
            "claude_execution" => Self::run("claude", &["--version"]),
            cap => {
                // For GUI-only apps, verify the .app bundle is present
                let app_name = cap.strip_suffix("_execution").unwrap_or(cap);
                let title_case: String = app_name.split('_')
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let app_path = format!("/Applications/{}.app", title_case);
                let exists = std::path::Path::new(&app_path).exists();
                (exists, if exists {
                    format!("{}.app bundle present", title_case)
                } else {
                    format!("no executor registered for '{}'", cap)
                })
            }
        }
    }

    fn run(cmd: &str, args: &[&str]) -> (bool, String) {
        match Command::new(cmd).args(args).output() {
            Ok(output) => {
                let success = output.status.success();
                let text = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                (success, if success { text } else { format!("exit {}", output.status) })
            }
            Err(e) => (false, format!("not found: {}", e)),
        }
    }
}

pub struct MdnsResponder {
    service_name: String,
    port: u16,
    socket: UdpSocket,
}

impl MdnsResponder {
    pub fn new(service_name: &str, port: u16) -> Result<Self, String> {
        let ip = Ipv4Addr::new(224, 0, 0, 251);
        let bind_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 5353);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| e.to_string())?;

        socket.set_reuse_address(true).map_err(|e| e.to_string())?;

        let actual_socket = match socket.bind(&bind_addr.into()) {
            Ok(_) => {
                let _ = socket.join_multicast_v4(&ip, &Ipv4Addr::UNSPECIFIED);
                socket.into()
            }
            Err(_) => {
                let ephemeral_bind = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);
                let fallback = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
                    .map_err(|e| e.to_string())?;
                fallback.set_reuse_address(true).map_err(|e| e.to_string())?;
                let _ = fallback.bind(&ephemeral_bind.into());
                let _ = fallback.join_multicast_v4(&ip, &Ipv4Addr::UNSPECIFIED);
                fallback.into()
            }
        };

        Ok(Self {
            service_name: service_name.to_string(),
            port,
            socket: actual_socket,
        })
    }

    pub async fn start_broadcast(&self) -> Result<(), String> {
        let dest = SocketAddr::new(Ipv4Addr::new(224, 0, 0, 251).into(), 5353);
        let payload = format!("SEAMLESS-SWARM:REGISTER:{}:{}", self.service_name, self.port);
        let bytes = payload.into_bytes();
        let socket_clone = self.socket.try_clone().map_err(|e| e.to_string())?;

        tokio::spawn(async move {
            loop {
                let _ = socket_clone.send_to(&bytes, &dest);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });

        Ok(())
    }
}

pub struct NngClient {
    auth_endpoint: String,
    task_endpoint: String,
    heartbeat_endpoint: String,
    progress_endpoint: String,
    node_id: String,
    secure_element: SimulatedSecureElement,
}

impl NngClient {
    pub fn new(auth_endpoint: &str, task_endpoint: &str, heartbeat_endpoint: &str, progress_endpoint: &str, node_id: &str) -> Self {
        Self {
            auth_endpoint: auth_endpoint.to_string(),
            task_endpoint: task_endpoint.to_string(),
            heartbeat_endpoint: heartbeat_endpoint.to_string(),
            progress_endpoint: progress_endpoint.to_string(),
            node_id: node_id.to_string(),
            secure_element: SimulatedSecureElement::new(),
        }
    }

    pub async fn run_demo_lifecycle(&self, capabilities: Vec<crate::proto::DeviceCapability>) -> Result<(), String> {
        println!("\n[Agent] === Beginning Swarm Handshake Lifecycle ===");
        println!("[Agent] Simulated Node Hardware ID: {}", self.node_id);
        
        let pk_bytes = self.secure_element.get_public_key();
        let thumbprint = self.secure_element.get_static_thumbprint();
        println!("[Agent] Simulated Node Key Public Key: {}", hex::encode(&pk_bytes));
        println!("[Agent] Simulated Node Key Thumbprint: {}", hex::encode(&thumbprint));

        // Establish Req/Rep socket for handshake with randomized exponential backoff retries!
        let mut attempt = 0;
        let mut backoff = 1.0f64; // seconds

        let (req_socket, result) = loop {
            attempt += 1;
            println!("[Agent] Connecting/Handshaking attempt {}...", attempt);

            match self.execute_handshake_attempt(&pk_bytes, &thumbprint).await {
                Ok((socket, res)) => {
                    break (socket, res);
                }
                Err(e) => {
                    eprintln!("[Agent] Handshake attempt {} failed: {}. Retrying...", attempt, e);
                    
                    // Generate randomized jitter backoff capped at 30 seconds
                    // backoff = min(30, backoff * 1.5) + random jitter
                    let jitter: f64 = rand::thread_rng().gen_range(0.0..1.0);
                    let sleep_secs = f64::min(30.0, backoff * 1.5) + jitter;
                    backoff = sleep_secs;

                    println!("[Agent] Backing off for {:.2} seconds before re-discovery...", sleep_secs);
                    tokio::time::sleep(Duration::from_secs_f64(sleep_secs)).await;
                }
            }
        };

        println!("[Agent] SUCCESS: Swarm Authentication Granted! Session Token: {}", result.session_token);
        println!("[Agent] Greeting message: {}", result.message);

        // Clean up handshake socket
        drop(req_socket);

        // 6. Push Capability Profile to Heartbeat socket (Push/Pull)
        let push_socket = NngSocket::new(NngProtocol::Push0).map_err(|e| e.to_string())?;
        push_socket.dial(&self.heartbeat_endpoint).map_err(|e| e.to_string())?;

        let profile = proto::CapabilityProfile {
            node_id: self.node_id.clone(),
            os_platform: std::env::consts::OS.to_string(),
            capabilities,
            updated_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        let mut encoded_profile = Vec::new();
        profile.encode(&mut encoded_profile).unwrap();

        let mut profile_frame = Vec::new();
        profile_frame.extend_from_slice(&((encoded_profile.len() as u32).to_be_bytes()));
        profile_frame.push(5); // MsgType 5 = CapabilityProfile
        profile_frame.extend_from_slice(&encoded_profile);

        println!("[Agent] Registering capability profile via NNG Push socket to {}", self.heartbeat_endpoint);
        push_socket.send(&profile_frame).map_err(|(_, e)| e.to_string())?;

        // 7. Start listening for incoming tasks pushed by the Core Appliance (using Pull socket)
        let pull_socket = NngSocket::new(NngProtocol::Pull0).map_err(|e| e.to_string())?;
        pull_socket.dial(&self.task_endpoint).map_err(|e| e.to_string())?;

        println!("[Agent] Listening for scheduled swarm tasks on {}...", self.task_endpoint);

        let _node_id_clone = self.node_id.clone();
        let progress_endpoint_clone = self.progress_endpoint.clone();
        tokio::spawn(async move {
            // Setup Progress Socket
            let progress_socket = NngSocket::new(NngProtocol::Push0).expect("Failed to create progress socket");
            if let Err(e) = progress_socket.dial(&progress_endpoint_clone) {
                eprintln!("[Agent] Failed to dial progress endpoint: {}", e);
                return;
            }

            loop {
                match pull_socket.recv() {
                    Ok(task_msg) => {
                        let task_slice = task_msg.as_slice();
                        if task_slice.len() >= 5 {
                            let task_len = u32::from_be_bytes([task_slice[0], task_slice[1], task_slice[2], task_slice[3]]) as usize;
                            let msg_type = task_slice[4];
                            if msg_type == 6 {
                                if let Ok(task) = proto::TaskDefinition::decode(&task_slice[5..5+task_len]) {
                                    println!("\n[Agent] >>> RECEIVED TASK FROM SWARM: {}", task.task_id);
                                    println!("[Agent] Target: {}, Category: {:?}, Payload Size: {}B",
                                        task.module_target, task.category, task.payload.len());

                                    // Probe required capabilities before starting work
                                    let mut all_capable = true;
                                    if task.required_capabilities.is_empty() {
                                        println!("[Agent] No capability requirements — proceeding.");
                                    } else {
                                        for cap in &task.required_capabilities {
                                            let (ok, detail) = CapabilityExecutor::probe(cap);
                                            if ok {
                                                println!("[Agent] Capability probe [{}]: OK — {}", cap, detail);
                                            } else {
                                                println!("[Agent] Capability probe [{}]: FAIL — {}", cap, detail);
                                                all_capable = false;
                                            }
                                        }
                                    }

                                    if !all_capable {
                                        println!("[Agent] Task {} REJECTED — capability requirements unmet.", task.task_id);
                                        let progress = proto::TaskProgress {
                                            task_id: task.task_id.clone(),
                                            status: proto::TaskStatus::Failed as i32,
                                            progress_percentage: 0.0,
                                            checkpoint_data: vec![],
                                            error_message: "capability requirements unmet on this node".to_string(),
                                        };
                                        let mut buf = Vec::new();
                                        progress.encode(&mut buf).unwrap();
                                        let mut frame = Vec::new();
                                        frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                                        frame.push(7);
                                        frame.extend_from_slice(&buf);
                                        let _ = progress_socket.send(&frame);
                                        continue;
                                    }

                                    println!("[Agent] Executing task {}...", task.task_id);

                                    // Stage 1 — 33% + checkpoint for stateful tasks
                                    tokio::time::sleep(Duration::from_millis(150)).await;
                                    {
                                        let checkpoint_data = if task.category == proto::TaskCategory::StatefulLongRunning as i32 {
                                            vec![88, 99, 111, 222]
                                        } else {
                                            vec![]
                                        };
                                        let progress = proto::TaskProgress {
                                            task_id: task.task_id.clone(),
                                            status: proto::TaskStatus::Running as i32,
                                            progress_percentage: 33.3,
                                            checkpoint_data,
                                            error_message: "".to_string(),
                                        };
                                        let mut buf = Vec::new();
                                        progress.encode(&mut buf).unwrap();
                                        let mut frame = Vec::new();
                                        frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                                        frame.push(7);
                                        frame.extend_from_slice(&buf);
                                        let _ = progress_socket.send(&frame);
                                    }

                                    // Stage 2 — 66%
                                    tokio::time::sleep(Duration::from_millis(150)).await;
                                    {
                                        let progress = proto::TaskProgress {
                                            task_id: task.task_id.clone(),
                                            status: proto::TaskStatus::Running as i32,
                                            progress_percentage: 66.6,
                                            checkpoint_data: vec![],
                                            error_message: "".to_string(),
                                        };
                                        let mut buf = Vec::new();
                                        progress.encode(&mut buf).unwrap();
                                        let mut frame = Vec::new();
                                        frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                                        frame.push(7);
                                        frame.extend_from_slice(&buf);
                                        let _ = progress_socket.send(&frame);
                                    }

                                    // Stage 3 — 100% complete
                                    tokio::time::sleep(Duration::from_millis(150)).await;
                                    {
                                        let progress = proto::TaskProgress {
                                            task_id: task.task_id.clone(),
                                            status: proto::TaskStatus::Completed as i32,
                                            progress_percentage: 100.0,
                                            checkpoint_data: vec![],
                                            error_message: "".to_string(),
                                        };
                                        let mut buf = Vec::new();
                                        progress.encode(&mut buf).unwrap();
                                        let mut frame = Vec::new();
                                        frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                                        frame.push(7);
                                        frame.extend_from_slice(&buf);
                                        let _ = progress_socket.send(&frame);
                                    }

                                    println!("[Agent] Task {} completed successfully!", task.task_id);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[Agent] Task pull socket error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn execute_handshake_attempt(&self, pk_bytes: &[u8], thumbprint: &[u8]) -> Result<(NngSocket, proto::HandshakeResult), String> {
        let req_socket = NngSocket::new(NngProtocol::Req0).map_err(|e| e.to_string())?;
        
        // Set short timeouts of 2 seconds to fail quickly under heavy packet drops!
        req_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(2000))).map_err(|e: nng::Error| e.to_string())?;
        req_socket.set_opt::<nng::options::SendTimeout>(Some(Duration::from_millis(2000))).map_err(|e: nng::Error| e.to_string())?;

        req_socket.dial(&self.auth_endpoint).map_err(|e| e.to_string())?;

        let init_payload = self.node_id.as_bytes();
        let mut frame = Vec::new();
        frame.extend_from_slice(&((init_payload.len() as u32).to_be_bytes()));
        frame.push(1); // MsgType 1
        frame.extend_from_slice(init_payload);

        req_socket.send(&frame).map_err(|(_, e)| e.to_string())?;

        let reply_msg = req_socket.recv().map_err(|e| e.to_string())?;
        let reply_slice = reply_msg.as_slice();
        if reply_slice.len() < 5 {
            return Err("Invalid protocol frame received from server".to_string());
        }
        let payload_len = u32::from_be_bytes([reply_slice[0], reply_slice[1], reply_slice[2], reply_slice[3]]) as usize;
        let msg_type = reply_slice[4];
        if msg_type != 2 {
            return Err(format!("Expected challenge frame type (2), got {}", msg_type));
        }

        let challenge = proto::HandshakeChallenge::decode(&reply_slice[5..5 + payload_len])
            .map_err(|e| format!("Failed to decode challenge: {}", e))?;

        let sig = self.secure_element.sign_challenge(&challenge.high_entropy_token);

        let response = proto::HandshakeResponse {
            signature: sig,
            identity: Some(proto::HardwareIdentity {
                node_uuid: self.node_id.clone(),
                ecdsa_public_key: pk_bytes.to_vec(),
                static_thumbprint: thumbprint.to_vec(),
            }),
        };

        let mut encoded_response = Vec::new();
        response.encode(&mut encoded_response).unwrap();

        let mut resp_frame = Vec::new();
        resp_frame.extend_from_slice(&((encoded_response.len() as u32).to_be_bytes()));
        resp_frame.push(3); // MsgType 3 = HandshakeResponse
        resp_frame.extend_from_slice(&encoded_response);

        req_socket.send(&resp_frame).map_err(|(_, e)| e.to_string())?;

        let result_msg = req_socket.recv().map_err(|e| e.to_string())?;
        let result_slice = result_msg.as_slice();
        if result_slice.len() < 5 {
            return Err("Invalid result frame".to_string());
        }
        let res_len = u32::from_be_bytes([result_slice[0], result_slice[1], result_slice[2], result_slice[3]]) as usize;
        let res_msg_type = result_slice[4];
        if res_msg_type != 4 {
            return Err(format!("Expected result frame type (4), got {}", res_msg_type));
        }

        let result = proto::HandshakeResult::decode(&result_slice[5..5 + res_len])
            .map_err(|e| format!("Failed to decode handshake result: {}", e))?;

        if !result.authenticated {
            return Err(format!("Authentication failed: {}", result.message));
        }

        Ok((req_socket, result))
    }
}
