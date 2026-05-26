use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use socket2::{Socket, Domain, Type, Protocol};
use nng::{Socket as NngSocket, Protocol as NngProtocol};
use prost::Message;
use crate::proto;
use crate::secure_element::SimulatedSecureElement;

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
    node_id: String,
    secure_element: SimulatedSecureElement,
}

impl NngClient {
    pub fn new(auth_endpoint: &str, task_endpoint: &str, heartbeat_endpoint: &str, node_id: &str) -> Self {
        Self {
            auth_endpoint: auth_endpoint.to_string(),
            task_endpoint: task_endpoint.to_string(),
            heartbeat_endpoint: heartbeat_endpoint.to_string(),
            node_id: node_id.to_string(),
            secure_element: SimulatedSecureElement::new(),
        }
    }

    pub async fn run_demo_lifecycle(&self, capabilities: Vec<crate::scout::DiscoveredCapability>) -> Result<(), String> {
        println!("\n[Agent] === Beginning Swarm Handshake Lifecycle ===");
        println!("[Agent] Simulated Node Hardware ID: {}", self.node_id);
        
        let pk_bytes = self.secure_element.get_public_key();
        let thumbprint = self.secure_element.get_static_thumbprint();
        println!("[Agent] Simulated Node Key Public Key: {}", hex::encode(&pk_bytes));
        println!("[Agent] Simulated Node Key Thumbprint: {}", hex::encode(&thumbprint));

        // 1. Establish Req/Rep socket for handshake
        let req_socket = NngSocket::new(NngProtocol::Req0).map_err(|e| e.to_string())?;
        req_socket.dial(&self.auth_endpoint).map_err(|e| e.to_string())?;

        // Send Join initiation request (fixed frame header: Length (4B) + MsgType (1B) + Payload)
        // MsgType: 1 = Join Handshake Request
        let init_payload = self.node_id.as_bytes();
        let mut frame = Vec::new();
        frame.extend_from_slice(&((init_payload.len() as u32).to_be_bytes()));
        frame.push(1); // MsgType 1
        frame.extend_from_slice(init_payload);

        println!("[Agent] Sending Hello/Join frame to Appliance...");
        req_socket.send(&frame).map_err(|(_, e)| e.to_string())?;

        // 2. Receive HandshakeChallenge
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

        println!("[Agent] Received high-entropy cryptographic challenge: {}", hex::encode(&challenge.high_entropy_token));

        // 3. Sign the challenge using simulated secure element
        let sig = self.secure_element.sign_challenge(&challenge.high_entropy_token);
        println!("[Agent] Generated secure signature on workstation: {}", hex::encode(&sig));

        // 4. Construct HandshakeResponse
        let response = proto::HandshakeResponse {
            signature: sig,
            identity: Some(proto::HardwareIdentity {
                node_uuid: self.node_id.clone(),
                ecdsa_public_key: pk_bytes,
                static_thumbprint: thumbprint,
            }),
        };

        let mut encoded_response = Vec::new();
        response.encode(&mut encoded_response).unwrap();

        let mut resp_frame = Vec::new();
        resp_frame.extend_from_slice(&((encoded_response.len() as u32).to_be_bytes()));
        resp_frame.push(3); // MsgType 3 = HandshakeResponse
        resp_frame.extend_from_slice(&encoded_response);

        println!("[Agent] Sending challenge response signature and HardwareIdentity to Appliance...");
        req_socket.send(&resp_frame).map_err(|(_, e)| e.to_string())?;

        // 5. Receive HandshakeResult
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

        println!("[Agent] SUCCESS: Swarm Authentication Granted! Session Token: {}", result.session_token);
        println!("[Agent] Greeting message: {}", result.message);

        // Clean up handshake socket
        drop(req_socket);

        // 6. Push Capability Profile to Heartbeat socket (Push/Pull)
        let push_socket = NngSocket::new(NngProtocol::Push0).map_err(|e| e.to_string())?;
        push_socket.dial(&self.heartbeat_endpoint).map_err(|e| e.to_string())?;

        let mut proto_caps = Vec::new();
        for cap in capabilities {
            let resource_value = if cap.val_type == "boolean" {
                Some(proto::device_capability::ResourceValue::BoolVal(cap.value == "true"))
            } else if cap.val_type == "integer" {
                Some(proto::device_capability::ResourceValue::IntVal(cap.value.parse().unwrap_or(0)))
            } else if cap.val_type == "float" {
                Some(proto::device_capability::ResourceValue::DoubleVal(cap.value.parse().unwrap_or(0.0)))
            } else {
                Some(proto::device_capability::ResourceValue::StringVal(cap.value))
            };

            proto_caps.push(proto::DeviceCapability {
                resource_name: cap.name,
                value_type: cap.val_type,
                resource_value,
            });
        }

        let profile = proto::CapabilityProfile {
            node_id: self.node_id.clone(),
            os_platform: std::env::consts::OS.to_string(),
            capabilities: proto_caps,
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
        tokio::spawn(async move {
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
                                    println!("[Agent] Target: {}, Category: {:?}, Payload Size: {}B", task.module_target, task.category, task.payload.len());
                                    println!("[Agent] Simulating execution...");
                                    
                                    // Simulate processing
                                    tokio::time::sleep(Duration::from_millis(500)).await;
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
}
