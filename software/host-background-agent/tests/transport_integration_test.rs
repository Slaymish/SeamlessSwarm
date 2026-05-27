use nng::{Socket as NngSocket, Protocol as NngProtocol};
use prost::Message;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use seamless_node::proto;
use seamless_node::transport::NngClient;

#[tokio::test]
async fn test_packet_drop_and_exponential_backoff_recovery() {
    let auth_addr = "tcp://127.0.0.1:5955";

    // Mock auth server that drops the first 2 requests and succeeds on the 3rd
    let server_socket = NngSocket::new(NngProtocol::Rep0).unwrap();
    server_socket.listen(auth_addr).unwrap();

    let drop_counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = drop_counter.clone();

    std::thread::spawn(move || {
        loop {
            if let Ok(msg) = server_socket.recv() {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    // Intentionally ignore — client hits 2 s RecvTimeout and backs off
                    continue;
                }

                let slice = msg.as_slice();
                let node_id = String::from_utf8_lossy(&slice[5..]).to_string();

                let challenge = proto::HandshakeChallenge {
                    high_entropy_token: vec![1, 2, 3, 4],
                    timestamp: 1000,
                };
                let mut buf = Vec::new();
                challenge.encode(&mut buf).unwrap();
                let mut frame = Vec::new();
                frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                frame.push(2);
                frame.extend_from_slice(&buf);
                server_socket.send(&frame).unwrap();

                let _resp = server_socket.recv().unwrap();

                let result = proto::HandshakeResult {
                    authenticated: true,
                    session_token: "session-abc-123".to_string(),
                    message: format!("Welcome {}!", node_id),
                };
                let mut res_buf = Vec::new();
                result.encode(&mut res_buf).unwrap();
                let mut res_frame = Vec::new();
                res_frame.extend_from_slice(&((res_buf.len() as u32).to_be_bytes()));
                res_frame.push(4);
                res_frame.extend_from_slice(&res_buf);
                server_socket.send(&res_frame).unwrap();
                break;
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Stub listeners so the client can dial heartbeat/task/progress endpoints
    let heartbeat = NngSocket::new(NngProtocol::Pull0).unwrap();
    heartbeat.listen("tcp://127.0.0.1:5957").unwrap();
    let task = NngSocket::new(NngProtocol::Push0).unwrap();
    task.listen("tcp://127.0.0.1:5956").unwrap();
    let progress = NngSocket::new(NngProtocol::Pull0).unwrap();
    progress.listen("tcp://127.0.0.1:5958").unwrap();

    let client = NngClient::new(
        auth_addr,
        "tcp://127.0.0.1:5956",
        "tcp://127.0.0.1:5957",
        "tcp://127.0.0.1:5958",
        "node-backoff-tester",
    );

    let start = std::time::Instant::now();
    let result = client.run_demo_lifecycle(vec![]).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Should connect after recovering from packet drops");
    assert_eq!(drop_counter.load(Ordering::SeqCst), 3, "Should succeed on 3rd attempt");
    assert!(
        elapsed.as_secs_f64() >= 4.0,
        "Must have experienced at least two 2-second timeouts"
    );
}
