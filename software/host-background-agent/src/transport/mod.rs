use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;
use std::process::Command;
use socket2::{Socket, Domain, Type, Protocol};
use nng::{Socket as NngSocket, Protocol as NngProtocol};
use nng::options::Options;
use prost::Message;
use rand::Rng;
use log::{info, warn, error};
use crate::proto;
use crate::secure_element::SimulatedSecureElement;
use crate::election::ElectionState;

// ── mDNS peer/leader discovery ────────────────────────────────────────────────

const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

/// Returns the local LAN IP by routing toward an external address without sending data.
pub fn get_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

/// Creates a UDP socket suitable for mDNS multicast send/receive.
fn make_mdns_socket() -> Result<UdpSocket, String> {
    let bind_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), MDNS_PORT);

    let raw = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| e.to_string())?;
    raw.set_reuse_address(true).map_err(|e| e.to_string())?;

    let std_sock: UdpSocket = match raw.bind(&bind_addr.into()) {
        Ok(_) => {
            let _ = raw.join_multicast_v4(&MDNS_GROUP, &Ipv4Addr::UNSPECIFIED);
            raw.into()
        }
        Err(_) => {
            // Fall back to ephemeral port — sending still works, but we won't receive
            // standard mDNS on port 5353 from other nodes.
            let fallback = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
                .map_err(|e| e.to_string())?;
            fallback.set_reuse_address(true).ok();
            let _ = fallback.bind(&SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0).into());
            let _ = fallback.join_multicast_v4(&MDNS_GROUP, &Ipv4Addr::UNSPECIFIED);
            fallback.into()
        }
    };

    Ok(std_sock)
}

fn parse_mdns_announcement(text: &str, election: &Arc<ElectionState>) {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 3 || parts[0] != "SEAMLESS-SWARM" {
        return;
    }
    match parts[1] {
        "PEER" => election.observe_peer(parts[2]),
        "LEADER" if parts.len() >= 4 => election.observe_leader(parts[2], parts[3]),
        _ => {}
    }
}

/// Broadcasts `SEAMLESS-SWARM:PEER:<id>` every 2 s, plus a `LEADER` message when leading.
pub async fn run_mdns_broadcaster(node_id: String, election: Arc<ElectionState>) {
    let dest = SocketAddr::new(MDNS_GROUP.into(), MDNS_PORT);
    let socket = match make_mdns_socket() {
        Ok(s) => s,
        Err(e) => {
            warn!("[mDNS] Broadcaster failed to init: {}", e);
            return;
        }
    };

    loop {
        let peer_msg = format!("SEAMLESS-SWARM:PEER:{}", node_id);
        let _ = socket.send_to(peer_msg.as_bytes(), dest);

        if election.am_i_leader() {
            let ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
            let leader_msg = format!("SEAMLESS-SWARM:LEADER:{}:{}", node_id, ip);
            let _ = socket.send_to(leader_msg.as_bytes(), dest);
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Listens for mDNS announcements from other nodes and feeds the `ElectionState`.
pub async fn run_mdns_listener(election: Arc<ElectionState>) {
    let std_sock = match make_mdns_socket() {
        Ok(s) => s,
        Err(e) => {
            warn!("[mDNS] Listener failed to init: {}", e);
            return;
        }
    };

    if let Err(e) = std_sock.set_nonblocking(true) {
        warn!("[mDNS] set_nonblocking failed: {}", e);
        return;
    }

    let async_sock = match tokio::net::UdpSocket::from_std(std_sock) {
        Ok(s) => s,
        Err(e) => {
            warn!("[mDNS] Async socket conversion failed: {}", e);
            return;
        }
    };

    let mut buf = [0u8; 512];
    loop {
        match async_sock.recv_from(&mut buf).await {
            Ok((n, _src)) => {
                if let Ok(msg) = std::str::from_utf8(&buf[..n]) {
                    parse_mdns_announcement(msg, &election);
                }
            }
            Err(e) => {
                warn!("[mDNS] Receive error: {}", e);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

// ── Runtime capability probing ────────────────────────────────────────────────

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
        let base_path = std::env::var("PATH").unwrap_or_default();
        let enriched = format!(
            "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:{}",
            base_path
        );
        match Command::new(cmd).args(args).env("PATH", &enriched).output() {
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

// ── NNG worker client ─────────────────────────────────────────────────────────

pub struct NngClient {
    auth_endpoint: String,
    task_endpoint: String,
    heartbeat_endpoint: String,
    progress_endpoint: String,
    node_id: String,
    secure_element: SimulatedSecureElement,
}

impl NngClient {
    pub fn new(
        auth_endpoint: &str,
        task_endpoint: &str,
        heartbeat_endpoint: &str,
        progress_endpoint: &str,
        node_id: &str,
    ) -> Self {
        Self {
            auth_endpoint: auth_endpoint.to_string(),
            task_endpoint: task_endpoint.to_string(),
            heartbeat_endpoint: heartbeat_endpoint.to_string(),
            progress_endpoint: progress_endpoint.to_string(),
            node_id: node_id.to_string(),
            secure_element: SimulatedSecureElement::new(),
        }
    }

    pub async fn run_demo_lifecycle(
        &self,
        capabilities: Vec<proto::DeviceCapability>,
    ) -> Result<(), String> {
        info!("\n[Worker {}] === Beginning Swarm Handshake ===", self.node_id);

        let pk_bytes = self.secure_element.get_public_key();
        let thumbprint = self.secure_element.get_static_thumbprint();

        let mut attempt = 0;
        let mut backoff = 1.0f64;

        let (req_socket, result) = loop {
            attempt += 1;
            info!("[Worker {}] Handshake attempt {}...", self.node_id, attempt);
            match self.execute_handshake_attempt(&pk_bytes, &thumbprint).await {
                Ok(pair) => break pair,
                Err(e) => {
                    warn!("[Worker {}] Handshake attempt {} failed: {}.", self.node_id, attempt, e);
                    let jitter: f64 = rand::thread_rng().gen_range(0.0..1.0);
                    let sleep_secs = f64::min(30.0, backoff * 1.5) + jitter;
                    backoff = sleep_secs;
                    info!("[Worker {}] Backing off {:.2}s...", self.node_id, sleep_secs);
                    tokio::time::sleep(Duration::from_secs_f64(sleep_secs)).await;
                }
            }
        };

        info!("[Worker {}] Authenticated. Session: {}", self.node_id, result.session_token);
        drop(req_socket);

        // Push capability profile
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

        let mut encoded = Vec::new();
        profile.encode(&mut encoded).unwrap();
        let mut frame = Vec::new();
        frame.extend_from_slice(&((encoded.len() as u32).to_be_bytes()));
        frame.push(5); // MsgType 5 = CapabilityProfile
        frame.extend_from_slice(&encoded);
        push_socket.send(&frame).map_err(|(_, e)| e.to_string())?;

        // Start task pull loop in background
        let pull_socket = NngSocket::new(NngProtocol::Pull0).map_err(|e| e.to_string())?;
        pull_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(500)))
            .map_err(|e: nng::Error| e.to_string())?;
        pull_socket.dial(&self.task_endpoint).map_err(|e| e.to_string())?;

        let progress_ep = self.progress_endpoint.clone();
        let node_id = self.node_id.clone();
        tokio::spawn(async move {
            let progress_socket = NngSocket::new(NngProtocol::Push0)
                .expect("Failed to create progress socket");
            if let Err(e) = progress_socket.dial(&progress_ep) {
                error!("[Worker {}] Failed to dial progress endpoint: {}", node_id, e);
                return;
            }

            loop {
                match pull_socket.recv() {
                    Ok(task_msg) => {
                        let s = task_msg.as_slice();
                        if s.len() >= 5 {
                            let task_len = u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize;
                            if s[4] == 6 {
                                if let Ok(task) = proto::TaskDefinition::decode(&s[5..5 + task_len]) {
                                    execute_task(task, &node_id, &progress_socket).await;
                                }
                            }
                        }
                    }
                    Err(nng::Error::TimedOut) => continue,
                    Err(e) => {
                        error!("[Worker {}] Task socket error: {}", node_id, e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn execute_handshake_attempt(
        &self,
        pk_bytes: &[u8],
        thumbprint: &[u8],
    ) -> Result<(NngSocket, proto::HandshakeResult), String> {
        let req = NngSocket::new(NngProtocol::Req0).map_err(|e| e.to_string())?;
        req.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(2000)))
            .map_err(|e: nng::Error| e.to_string())?;
        req.set_opt::<nng::options::SendTimeout>(Some(Duration::from_millis(2000)))
            .map_err(|e: nng::Error| e.to_string())?;
        req.dial(&self.auth_endpoint).map_err(|e| e.to_string())?;

        let init = self.node_id.as_bytes();
        let mut frame = Vec::new();
        frame.extend_from_slice(&((init.len() as u32).to_be_bytes()));
        frame.push(1);
        frame.extend_from_slice(init);
        req.send(&frame).map_err(|(_, e)| e.to_string())?;

        let reply = req.recv().map_err(|e| e.to_string())?;
        let r = reply.as_slice();
        if r.len() < 5 { return Err("Invalid frame".to_string()); }
        let plen = u32::from_be_bytes([r[0], r[1], r[2], r[3]]) as usize;
        if r[4] != 2 { return Err(format!("Expected challenge (2), got {}", r[4])); }

        let challenge = proto::HandshakeChallenge::decode(&r[5..5 + plen])
            .map_err(|e| format!("Decode challenge: {}", e))?;

        let sig = self.secure_element.sign_challenge(&challenge.high_entropy_token);
        let response = proto::HandshakeResponse {
            signature: sig,
            identity: Some(proto::HardwareIdentity {
                node_uuid: self.node_id.clone(),
                ecdsa_public_key: pk_bytes.to_vec(),
                static_thumbprint: thumbprint.to_vec(),
            }),
        };

        let mut encoded = Vec::new();
        response.encode(&mut encoded).unwrap();
        let mut resp_frame = Vec::new();
        resp_frame.extend_from_slice(&((encoded.len() as u32).to_be_bytes()));
        resp_frame.push(3);
        resp_frame.extend_from_slice(&encoded);
        req.send(&resp_frame).map_err(|(_, e)| e.to_string())?;

        let result_msg = req.recv().map_err(|e| e.to_string())?;
        let rs = result_msg.as_slice();
        if rs.len() < 5 { return Err("Invalid result frame".to_string()); }
        let rlen = u32::from_be_bytes([rs[0], rs[1], rs[2], rs[3]]) as usize;
        if rs[4] != 4 { return Err(format!("Expected result (4), got {}", rs[4])); }

        let result = proto::HandshakeResult::decode(&rs[5..5 + rlen])
            .map_err(|e| format!("Decode result: {}", e))?;

        if !result.authenticated {
            return Err(format!("Auth failed: {}", result.message));
        }
        Ok((req, result))
    }
}

async fn execute_task(task: proto::TaskDefinition, node_id: &str, progress_socket: &NngSocket) {
    info!("\n[Worker {}] >>> TASK: {}", node_id, task.task_id);

    let mut all_capable = true;
    for cap in &task.required_capabilities {
        let (ok, detail) = CapabilityExecutor::probe(cap);
        if ok {
            info!("[Worker {}] Capability [{}]: OK — {}", node_id, cap, detail);
        } else {
            warn!("[Worker {}] Capability [{}]: FAIL — {}", node_id, cap, detail);
            all_capable = false;
        }
    }

    if !all_capable {
        warn!("[Worker {}] Task {} REJECTED — capability unmet.", node_id, task.task_id);
        send_progress(progress_socket, &task.task_id, proto::TaskStatus::Failed, 0.0, vec![],
            "capability requirements unmet on this node".to_string());
        return;
    }

    // Stage 1 — 33%
    tokio::time::sleep(Duration::from_millis(150)).await;
    let checkpoint = if task.category == proto::TaskCategory::StatefulLongRunning as i32 {
        vec![88, 99, 111, 222]
    } else {
        vec![]
    };
    send_progress(progress_socket, &task.task_id, proto::TaskStatus::Running, 33.3, checkpoint, String::new());

    // Stage 2 — 66%
    tokio::time::sleep(Duration::from_millis(150)).await;
    send_progress(progress_socket, &task.task_id, proto::TaskStatus::Running, 66.6, vec![], String::new());

    // Stage 3 — 100%
    tokio::time::sleep(Duration::from_millis(150)).await;
    send_progress(progress_socket, &task.task_id, proto::TaskStatus::Completed, 100.0, vec![], String::new());

    info!("[Worker {}] Task {} completed.", node_id, task.task_id);
}

fn send_progress(
    socket: &NngSocket,
    task_id: &str,
    status: proto::TaskStatus,
    pct: f32,
    checkpoint: Vec<u8>,
    err: String,
) {
    let msg = proto::TaskProgress {
        task_id: task_id.to_string(),
        status: status as i32,
        progress_percentage: pct,
        checkpoint_data: checkpoint,
        error_message: err,
    };
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();
    let mut frame = Vec::new();
    frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
    frame.push(7);
    frame.extend_from_slice(&buf);
    let _ = socket.send(&frame);
}
