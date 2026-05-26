use nng::{Socket, Protocol};
use prost::Message;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::registry::{EphemeralRegistry, NodeProfile, Capability};
use crate::proto;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    HandshakeChallenge = 1,
    HandshakeResponse = 2,
    HandshakeResult = 3,
    CapabilityProfile = 4,
    TaskDefinition = 5,
    TaskProgress = 6,
}

pub struct NngServer {
    endpoint: String,
    registry: EphemeralRegistry,
}

impl NngServer {
    pub fn new(endpoint: &str, registry: EphemeralRegistry) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            registry,
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let socket = Socket::new(Protocol::Pull0).map_err(|e| e.to_string())?;
        socket.listen(&self.endpoint).map_err(|e| e.to_string())?;
        println!("NNG Server listening on {}...", self.endpoint);

        let registry = self.registry.clone();
        
        std::thread::spawn(move || {
            loop {
                match socket.recv() {
                    Ok(msg) => {
                        let bytes = msg.as_slice();
                        if bytes.len() < 5 {
                            eprintln!("Received message too small to contain header");
                            continue;
                        }

                        let len_bytes: [u8; 4] = bytes[0..4].try_into().unwrap();
                        let payload_len = u32::from_be_bytes(len_bytes) as usize;
                        let msg_type_raw = bytes[4];

                        if bytes.len() < 5 + payload_len {
                            eprintln!("Received truncated message: expected {} bytes, got {}", 5 + payload_len, bytes.len());
                            continue;
                        }

                        let payload_bytes = &bytes[5..5 + payload_len];

                        if msg_type_raw == MessageType::CapabilityProfile as u8 {
                            match proto::CapabilityProfile::decode(payload_bytes) {
                                Ok(proto_profile) => {
                                    let now = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();

                                    let capabilities: Vec<Capability> = proto_profile.capabilities.iter().map(|c| {
                                        let val_str = match &c.resource_value {
                                            Some(proto::device_capability::ResourceValue::BoolVal(b)) => b.to_string(),
                                            Some(proto::device_capability::ResourceValue::IntVal(i)) => i.to_string(),
                                            Some(proto::device_capability::ResourceValue::DoubleVal(d)) => d.to_string(),
                                            Some(proto::device_capability::ResourceValue::StringVal(s)) => s.clone(),
                                            None => "".to_string(),
                                        };
                                        Capability {
                                            name: c.resource_name.clone(),
                                            val_type: c.value_type.clone(),
                                            value: val_str,
                                        }
                                    }).collect();

                                    let profile = NodeProfile {
                                        node_id: proto_profile.node_id.clone(),
                                        os_platform: proto_profile.os_platform.clone(),
                                        capabilities,
                                        last_seen: now,
                                        public_key: "".to_string(),
                                    };

                                    println!("Received heartbeat profile from node: {}", profile.node_id);
                                    registry.register_node(profile);
                                }
                                Err(e) => {
                                    eprintln!("Failed to decode CapabilityProfile: {}", e);
                                }
                            }
                        } else {
                            eprintln!("Received unsupported NNG message type: {}", msg_type_raw);
                        }
                    }
                    Err(e) => {
                        eprintln!("NNG socket recv error: {}", e);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });

        Ok(())
    }
}
