use std::io::{self, Write};
use std::time::{Duration, Instant};
use nng::{Socket as NngSocket, Protocol as NngProtocol};
use nng::options::Options;
use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/seamless_swarm.rs"));
}

const HUB_MEDIUM_ADDR: &str = "tcp://127.0.0.1:5559";
const HUB_PROGRESS_ADDR: &str = "tcp://127.0.0.1:5560";

fn main() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           SEAMLESS SWARM — MEDIUM INTERFACE                 ║");
    println!("║     Your access point to the distributed compute swarm      ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // --- Access key prompt ---
    print!("  Access key (try DEMO-2024): ");
    io::stdout().flush().unwrap();
    let mut access_key = String::new();
    io::stdin().read_line(&mut access_key).unwrap();
    let access_key = access_key.trim().to_string();

    if access_key.is_empty() {
        eprintln!("  No access key provided. Exiting.");
        std::process::exit(1);
    }

    println!();
    println!("  Connecting to swarm hub at {} ...", HUB_MEDIUM_ADDR);

    // --- Connect to hub medium server ---
    let req_socket = NngSocket::new(NngProtocol::Req0).unwrap();
    req_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_secs(5))).unwrap();
    req_socket.set_opt::<nng::options::SendTimeout>(Some(Duration::from_secs(5))).unwrap();

    if let Err(e) = req_socket.dial(HUB_MEDIUM_ADDR) {
        eprintln!("  Failed to connect: {}.", e);
        eprintln!("  Is the hub running? Try: make hub");
        std::process::exit(1);
    }

    // --- Send SwarmStatusRequest (MsgType 10) ---
    let status_req = proto::SwarmStatusRequest { access_key: access_key.clone() };
    if let Err(msg) = send_framed(&req_socket, 10, &status_req) {
        eprintln!("  Failed to send status request: {}", msg);
        std::process::exit(1);
    }

    let (msg_type, payload) = match recv_framed(&req_socket) {
        Ok(v) => v,
        Err(e) => { eprintln!("  No response from hub: {}", e); std::process::exit(1); }
    };

    if msg_type != 11 {
        eprintln!("  Unexpected response type {} from hub.", msg_type);
        std::process::exit(1);
    }

    let status = proto::SwarmStatusResponse::decode(payload.as_slice()).unwrap();

    if !status.authorized {
        eprintln!("  Access denied: {}", status.message);
        std::process::exit(1);
    }

    println!("  {} {}", checkmark(), status.message);
    println!();
    println!("  ┌─ Swarm Status ──────────────────────────────────────────────┐");
    println!("  │  Nodes online   : {}", status.node_count);
    if status.available_capabilities.is_empty() {
        println!("  │  Capabilities   : (none yet — waiting for agents to connect)");
    } else {
        // Wrap long capability lists
        let caps = status.available_capabilities.join(", ");
        println!("  │  Capabilities   : {}", caps);
    }
    println!("  └─────────────────────────────────────────────────────────────┘");
    println!();

    if status.node_count == 0 {
        println!("  No nodes are online yet. Start an agent with: make agent");
        println!("  Tasks you submit will be queued and dispatched when a node joins.");
        println!();
    }

    // --- Subscribe to progress broadcast (Pub/Sub, MsgType 7 frames) ---
    let sub_socket = NngSocket::new(NngProtocol::Sub0).unwrap();
    sub_socket.set_opt::<nng::options::protocol::pubsub::Subscribe>(vec![]).unwrap(); // all topics
    sub_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_millis(100))).unwrap();
    if let Err(e) = sub_socket.dial(HUB_PROGRESS_ADDR) {
        eprintln!("  Warning: could not connect to progress broadcast: {}", e);
    }

    // --- Main task submission loop ---
    loop {
        println!("  ┌─ Submit Task ───────────────────────────────────────────────┐");
        println!("  │  [1] Stateless   — ffmpeg video transcode                  │");
        println!("  │  [2] Stateful    — blender scene render                    │");
        println!("  │  [3] Interactive — claude AI task                          │");
        println!("  │  [s] Swarm status                                          │");
        println!("  │  [q] Quit                                                  │");
        println!("  └─────────────────────────────────────────────────────────────┘");
        print!("  Choice: ");
        io::stdout().flush().unwrap();

        let mut choice = String::new();
        io::stdin().read_line(&mut choice).unwrap();
        let choice = choice.trim();

        match choice {
            "q" | "Q" => {
                println!("  Disconnecting from swarm. Goodbye.");
                break;
            }
            "s" | "S" => {
                refresh_status(&req_socket, &access_key);
                println!();
                continue;
            }
            "1" | "2" | "3" => {}
            _ => {
                println!("  Invalid choice — enter 1, 2, 3, s, or q.");
                println!();
                continue;
            }
        }

        let (category, cap, label) = match choice {
            "1" => (0i32, "ffmpeg_execution",  "ffmpeg video transcode"),
            "2" => (1i32, "blender_execution", "blender scene render"),
            "3" => (0i32, "claude_execution",  "claude AI task"),
            _ => unreachable!(),
        };

        let task_name = format!("{}-{}", label.replace(' ', "-"), &uuid::Uuid::new_v4().to_string()[..6]);
        println!();
        println!("  Submitting '{}' to swarm...", label);

        // --- Send TaskSubmitRequest (MsgType 12) ---
        let submit_req = proto::TaskSubmitRequest {
            access_key: access_key.clone(),
            task_name: task_name.clone(),
            category,
            required_capabilities: vec![cap.to_string()],
            payload: b"demo-payload".to_vec(),
        };

        if let Err(e) = send_framed(&req_socket, 12, &submit_req) {
            eprintln!("  Failed to send task: {}", e);
            continue;
        }

        let (resp_type, resp_payload) = match recv_framed(&req_socket) {
            Ok(v) => v,
            Err(e) => { eprintln!("  No response: {}", e); continue; }
        };

        if resp_type != 13 {
            eprintln!("  Unexpected response type {}.", resp_type);
            continue;
        }

        let submit_resp = proto::TaskSubmitResponse::decode(resp_payload.as_slice()).unwrap();

        if !submit_resp.accepted {
            println!("  {} Task rejected: {}", cross(), submit_resp.message);
            println!();
            continue;
        }

        let task_id = submit_resp.task_id.clone();
        println!("  {} Accepted  →  task ID: {}", checkmark(), task_id);
        println!();

        // --- Watch progress for this specific task ---
        watch_progress(&sub_socket, &task_id, Duration::from_secs(30));
        println!();
    }
}

// Poll the progress pub socket until the given task_id completes, fails, or times out.
fn watch_progress(sub_socket: &NngSocket, task_id: &str, timeout: Duration) {
    println!("  Waiting for swarm to pick up and execute the task...");
    let deadline = Instant::now() + timeout;
    let mut last_pct = -1.0f32;

    while Instant::now() < deadline {
        match sub_socket.recv() {
            Ok(msg) => {
                let slice = msg.as_slice();
                if slice.len() < 5 { continue; }
                let payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
                if slice[4] != 7 { continue; } // MsgType 7 = TaskProgress
                if slice.len() < 5 + payload_len { continue; }

                let progress = match proto::TaskProgress::decode(&slice[5..5 + payload_len]) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                if progress.task_id != task_id { continue; }

                // Only print if percentage changed (or status changed)
                if (progress.progress_percentage - last_pct).abs() > 0.1 || progress.status != 1 {
                    last_pct = progress.progress_percentage;
                    let status_str = match progress.status {
                        0 => "Pending",
                        1 => "Running",
                        2 => "Completed",
                        3 => "Failed",
                        _ => "Unknown",
                    };
                    println!("  [{}]  {}  {:.0}%", task_id, status_str, progress.progress_percentage);
                }

                match progress.status {
                    2 => { // Completed
                        println!("  {} Task completed successfully!", checkmark());
                        return;
                    }
                    3 => { // Failed
                        println!("  {} Task failed: {}", cross(), progress.error_message);
                        return;
                    }
                    _ => {}
                }
            }
            Err(nng::Error::TimedOut) => {} // keep polling
            Err(e) => {
                eprintln!("  Progress socket error: {}", e);
                return;
            }
        }
    }

    println!("  Warning: timed out waiting for progress. Task may still be running in the swarm.");
}

fn refresh_status(req_socket: &NngSocket, access_key: &str) {
    let req = proto::SwarmStatusRequest { access_key: access_key.to_string() };
    if send_framed(req_socket, 10, &req).is_err() { return; }
    if let Ok((11, payload)) = recv_framed(req_socket) {
        if let Ok(s) = proto::SwarmStatusResponse::decode(payload.as_slice()) {
            println!();
            println!("  ┌─ Swarm Status ──────────────────────────────────────────────┐");
            println!("  │  Nodes online   : {}", s.node_count);
            if s.available_capabilities.is_empty() {
                println!("  │  Capabilities   : (none)");
            } else {
                println!("  │  Capabilities   : {}", s.available_capabilities.join(", "));
            }
            println!("  └─────────────────────────────────────────────────────────────┘");
        }
    }
}

// Encode a proto message and send it with a [4-byte len][1-byte type][payload] frame.
fn send_framed<M: Message>(socket: &NngSocket, msg_type: u8, msg: &M) -> Result<(), String> {
    let mut buf = Vec::new();
    msg.encode(&mut buf).map_err(|e| e.to_string())?;
    let mut frame = Vec::new();
    frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
    frame.push(msg_type);
    frame.extend_from_slice(&buf);
    socket.send(&frame).map_err(|(_, e)| e.to_string())
}

// Receive a framed message and return (msg_type, payload_bytes).
fn recv_framed(socket: &NngSocket) -> Result<(u8, Vec<u8>), String> {
    let msg = socket.recv().map_err(|e| e.to_string())?;
    let slice = msg.as_slice();
    if slice.len() < 5 {
        return Err("Frame too short".to_string());
    }
    let payload_len = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
    let msg_type = slice[4];
    if slice.len() < 5 + payload_len {
        return Err("Truncated frame".to_string());
    }
    Ok((msg_type, slice[5..5 + payload_len].to_vec()))
}

fn checkmark() -> &'static str { "✓" }
fn cross() -> &'static str { "✗" }
