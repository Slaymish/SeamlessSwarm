use compute_module_core::registry::{EphemeralRegistry, NodeProfile, Capability};
use compute_module_core::scheduler::{ProfileScheduler, Task, TaskType, TaskState, ExecutionResult};
use compute_module_core::proto;
use prost::Message;

fn make_test_node(id: &str, last_seen: u64) -> NodeProfile {
    NodeProfile {
        node_id: id.to_string(),
        os_platform: "macOS".to_string(),
        capabilities: vec![
            Capability {
                name: "metal".to_string(),
                val_type: "bool".to_string(),
                value: "true".to_string(),
            }
        ],
        last_seen,
        public_key: "".to_string(),
    }
}

#[test]
fn test_e2e_protobuf_serialization_and_registration_lifecycle() {
    let raw_cap_gpu = proto::DeviceCapability {
        resource_name: "GPU".to_string(),
        value_type: "boolean".to_string(),
        resource_value: Some(proto::device_capability::ResourceValue::BoolVal(true)),
    };
    
    let raw_cap_cpu = proto::DeviceCapability {
        resource_name: "cpu_cores".to_string(),
        value_type: "integer".to_string(),
        resource_value: Some(proto::device_capability::ResourceValue::IntVal(8)),
    };

    let proto_profile = proto::CapabilityProfile {
        node_id: "proto-node-99".to_string(),
        os_platform: "Linux".to_string(),
        capabilities: vec![raw_cap_gpu, raw_cap_cpu],
        updated_timestamp: 1716670000,
    };

    let mut buf = Vec::new();
    proto_profile.encode(&mut buf).unwrap();

    let decoded_profile = proto::CapabilityProfile::decode(&buf[..]).unwrap();
    assert_eq!(decoded_profile.node_id, "proto-node-99");
    assert_eq!(decoded_profile.os_platform, "Linux");
    assert_eq!(decoded_profile.capabilities.len(), 2);

    let mapped_capabilities: Vec<Capability> = decoded_profile.capabilities.iter().map(|c| {
        let val_str = match &c.resource_value {
            Some(proto::device_capability::ResourceValue::BoolVal(b)) => b.to_string(),
            Some(proto::device_capability::ResourceValue::IntVal(i)) => i.to_string(),
            _ => "".to_string(),
        };
        Capability {
            name: c.resource_name.clone(),
            val_type: c.value_type.clone(),
            value: val_str,
        }
    }).collect();

    let node_profile = NodeProfile {
        node_id: decoded_profile.node_id.clone(),
        os_platform: decoded_profile.os_platform.clone(),
        capabilities: mapped_capabilities,
        last_seen: 1200,
        public_key: "".to_string(),
    };

    let registry = EphemeralRegistry::new();
    registry.register_node(node_profile);

    let retrieved = registry.get_node("proto-node-99").unwrap();
    assert_eq!(retrieved.capabilities.len(), 2);
    assert_eq!(retrieved.capabilities[0].name, "GPU");
    assert_eq!(retrieved.capabilities[0].value, "true");
    assert_eq!(retrieved.capabilities[1].name, "cpu_cores");
    assert_eq!(retrieved.capabilities[1].value, "8");
}

#[test]
fn test_e2e_swarm_registration_and_scheduling_lifecycle() {
    let registry = EphemeralRegistry::new();
    let scheduler = ProfileScheduler::new(registry.clone());

    let n1 = make_test_node("studio-node-1", 1200);
    let n2 = make_test_node("studio-node-2", 300);

    registry.register_node(n1);
    registry.register_node(n2);

    let stateless_task = Task::new(
        "stateless-01".to_string(),
        TaskType::StatelessIdempotent,
        vec![1, 2, 3],
        3,
    );

    let stateful_task = Task::new(
        "stateful-01".to_string(),
        TaskType::StatefulLongRunning,
        vec![4, 5, 6],
        3,
    );

    let interactive_task = Task::new(
        "interactive-01".to_string(),
        TaskType::InteractiveLowLatency,
        vec![7, 8, 9],
        1,
    );

    assert!(matches!(
        scheduler.schedule_task(&stateless_task, "studio-node-1"),
        ExecutionResult::Success
    ));

    assert!(matches!(
        scheduler.schedule_task(&stateful_task, "studio-node-1"),
        ExecutionResult::CheckpointSaved(_)
    ));

    assert!(matches!(
        scheduler.schedule_task(&interactive_task, "studio-node-1"),
        ExecutionResult::Success
    ));

    assert!(matches!(
        scheduler.schedule_task(&stateless_task, "studio-node-2"),
        ExecutionResult::RetryNeeded(_)
    ));

    assert!(matches!(
        scheduler.schedule_task(&interactive_task, "studio-node-2"),
        ExecutionResult::ImmediateFailure(_)
    ));

    registry.unregister_node("studio-node-1");

    assert!(matches!(
        scheduler.schedule_task(&stateless_task, "studio-node-1"),
        ExecutionResult::RetryNeeded(_)
    ));

    assert!(matches!(
        scheduler.schedule_task(&interactive_task, "studio-node-1"),
        ExecutionResult::ImmediateFailure(_)
    ));
}

#[tokio::test]
async fn test_e2e_nng_transport_and_heartbeat_lifecycle() {
    use compute_module_core::transport::{NngServer, MessageType};
    use nng::{Socket as NngSocket, Protocol as NngProtocol};
    use std::time::{SystemTime, UNIX_EPOCH};

    let registry = EphemeralRegistry::new();
    
    // 1. Start NngServer listening on an ephemeral/test port
    let server_addr = "tcp://127.0.0.1:5921";
    let server = NngServer::new(server_addr, registry.clone());
    server.start().await.unwrap();

    // 2. Setup standard Nng Client to dial the server
    let client_socket = NngSocket::new(NngProtocol::Push0).unwrap();
    client_socket.dial(server_addr).unwrap();

    // 3. Build CapabilityProfile
    let cap_cpu = proto::DeviceCapability {
        resource_name: "cpu_cores".to_string(),
        value_type: "integer".to_string(),
        resource_value: Some(proto::device_capability::ResourceValue::IntVal(16)),
    };
    let cap_mem = proto::DeviceCapability {
        resource_name: "total_memory_gb".to_string(),
        value_type: "float".to_string(),
        resource_value: Some(proto::device_capability::ResourceValue::DoubleVal(32.0)),
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let profile = proto::CapabilityProfile {
        node_id: "e2e-simulator-node-abc".to_string(),
        os_platform: "linux".to_string(),
        capabilities: vec![cap_cpu, cap_mem],
        updated_timestamp: timestamp,
    };

    // 4. Encode & frame message
    let mut buf = Vec::new();
    profile.encode(&mut buf).unwrap();

    let mut framed = Vec::with_capacity(5 + buf.len());
    let length = buf.len() as u32;
    framed.extend_from_slice(&length.to_be_bytes());
    framed.push(MessageType::CapabilityProfile as u8);
    framed.extend_from_slice(&buf);

    // 5. Send payload over NNG
    client_socket.send(&framed).unwrap();

    // 6. Give NNG a brief moment to deliver/process the message asynchronously
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 7. Verify the node is registered with the correct capability profile
    let node = registry.get_node("e2e-simulator-node-abc").expect("Node should be registered via heartbeat");
    assert_eq!(node.os_platform, "linux");
    assert_eq!(node.capabilities.len(), 2);
    
    let cpu_cap = node.capabilities.iter().find(|c| c.name == "cpu_cores").unwrap();
    assert_eq!(cpu_cap.value, "16");
    assert_eq!(cpu_cap.val_type, "integer");

    let mem_cap = node.capabilities.iter().find(|c| c.name == "total_memory_gb").unwrap();
    assert_eq!(mem_cap.value, "32");
    assert_eq!(mem_cap.val_type, "float");

    // 8. Test Eviction - if we check at current timestamp + 10s, it must be evicted since threshold is 5s
    registry.evict_offline_nodes(timestamp + 10, 5);
    assert!(registry.get_node("e2e-simulator-node-abc").is_none(), "Node should be evicted after timeout threshold");
}

#[test]
fn test_e2e_fault_injection_and_taxonomy_recovery_lifecycle() {
    let registry = EphemeralRegistry::new();
    let scheduler = ProfileScheduler::new(registry.clone());

    // 1. Register two nodes: node-primary and node-backup
    let node_primary = make_test_node("node-primary", 1000);
    let node_backup = make_test_node("node-backup", 1000);
    registry.register_node(node_primary);
    registry.register_node(node_backup);

    // 2. Submit Stateless, Stateful, and Interactive tasks
    let t_stateless = Task::new("task-stateless".to_string(), TaskType::StatelessIdempotent, vec![1, 2, 3], 2);
    let t_stateful = Task::new("task-stateful".to_string(), TaskType::StatefulLongRunning, vec![10, 20], 2);
    let t_interactive = Task::new("task-interactive".to_string(), TaskType::InteractiveLowLatency, vec![100], 2);

    scheduler.submit_task(t_stateless);
    scheduler.submit_task(t_stateful);
    scheduler.submit_task(t_interactive);

    // 3. Dispatch tasks (they will be balanced/assigned to registered nodes)
    let dispatches = scheduler.dispatch_pending_tasks(1000);
    assert_eq!(dispatches.len(), 3);

    // Filter which tasks are running on node-primary
    let mut primary_tasks = Vec::new();
    for task_id in &["task-stateless", "task-stateful", "task-interactive"] {
        if let Some(task) = scheduler.get_task(task_id) {
            if let TaskState::Running { node_id, .. } = task.state {
                if node_id == "node-primary" {
                    primary_tasks.push(task_id.to_string());
                }
            }
        }
    }

    println!("[Test] Tasks running on primary node: {:?}", primary_tasks);

    // 4. Save a checkpoint on the Stateful task to simulate intermediate progress
    scheduler.update_task_progress("task-stateful", ExecutionResult::CheckpointSaved(vec![10, 20, 30, 40]));
    assert_eq!(scheduler.get_task("task-stateful").unwrap().payload, vec![10, 20, 30, 40]);

    // 5. INJECT FAULT: node-primary goes offline!
    registry.unregister_node("node-primary");
    scheduler.handle_node_failure("node-primary");

    // 6. Verify Recovery Transitions
    for task_id in primary_tasks {
        let task = scheduler.get_task(&task_id).unwrap();
        match task.task_type {
            TaskType::StatelessIdempotent => {
                // Should be rolled back to Pending with incremented retry count
                assert_eq!(task.state, TaskState::Pending);
                assert_eq!(task.current_retry, 1);
            }
            TaskType::StatefulLongRunning => {
                // Should be rolled back to Pending with preserved checkpoint payload and incremented retry count
                assert_eq!(task.state, TaskState::Pending);
                assert_eq!(task.current_retry, 1);
                assert_eq!(task.payload, vec![10, 20, 30, 40]);
            }
            TaskType::InteractiveLowLatency => {
                // Should immediately fail to trigger local host fallback
                assert!(matches!(task.state, TaskState::Failed { .. }));
                assert_eq!(task.current_retry, 0); // bypassed retries
            }
        }
    }

    // 7. Dispatch pending tasks again (remaining healthy backup node should pick them up!)
    let redispatches = scheduler.dispatch_pending_tasks(1001);
    for (task_id, node_id) in redispatches {
        assert_eq!(node_id, "node-backup");
        let task = scheduler.get_task(&task_id).unwrap();
        assert!(matches!(task.state, TaskState::Running { node_id: running_node, .. } if running_node == "node-backup"));
        println!("[Test] Successfully recovered and rescheduled task {} to backup node!", task_id);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_live_progress_and_checkpoint_streaming() {
    use compute_module_core::server::SwarmHubServer;
    use nng::{Socket as NngSocket, Protocol as NngProtocol};
    use std::sync::Arc;

    let registry = EphemeralRegistry::new();
    let scheduler = ProfileScheduler::new(registry.clone());

    // 1. Submit a Stateful task to the scheduler queue
    let stateful_task = Task::new(
        "task-live-checkpoint".to_string(),
        TaskType::StatefulLongRunning,
        vec![1, 2, 3], // Initial payload
        3,
    );
    scheduler.submit_task(stateful_task);

    // Register a node so we can dispatch
    let node = make_test_node("node-test-streaming", 1000);
    registry.register_node(node);

    // Dispatch task
    let current_time = 1000;
    let dispatches = scheduler.dispatch_pending_tasks(current_time);
    assert_eq!(dispatches.len(), 1);

    // 2. Start the SwarmHubServer (contains the run_progress_receiver_server on 5568 for testing)
    // We bind to ephemeral test ports to avoid collisions
    let server = Arc::new(SwarmHubServer::new(registry.clone(), scheduler.clone()));
    
    let progress_endpoint = "tcp://127.0.0.1:5890";
    let progress_server = server.clone();
    std::thread::spawn(move || {
        if let Err(e) = progress_server.run_progress_receiver_server(progress_endpoint) {
            eprintln!("[Test] Progress receiver failed to start: {}", e);
        }
    });

    // Give the server socket a brief moment to bind/listen
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 3. Setup client Nng Push socket simulating the agent's progress channel
    let client_progress_socket = NngSocket::new(NngProtocol::Push0).unwrap();
    client_progress_socket.dial(progress_endpoint).unwrap();

    // 4. Send a simulated live Progress Update with high-watermark checkpoint data
    let checkpoint_payload = vec![99, 100, 101, 102, 103];
    let progress_msg = proto::TaskProgress {
        task_id: "task-live-checkpoint".to_string(),
        status: proto::TaskStatus::Running as i32,
        progress_percentage: 50.0,
        checkpoint_data: checkpoint_payload.clone(),
        error_message: "".to_string(),
    };

    let mut buf = Vec::new();
    progress_msg.encode(&mut buf).unwrap();

    let mut frame = Vec::new();
    let length = buf.len() as u32;
    frame.extend_from_slice(&length.to_be_bytes());
    frame.push(7); // MsgType 7 = TaskProgress
    frame.extend_from_slice(&buf);

    // Send payload over NNG Progress socket
    client_progress_socket.send(&frame).unwrap();

    // 5. Give the async receiver a brief moment to process the message
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 6. VERIFY: The task's checkpoint payload in the Scheduler has updated in real-time!
    let task = scheduler.get_task("task-live-checkpoint").expect("Task must exist in scheduler");
    assert_eq!(task.payload, checkpoint_payload, "Stateful task payload must be updated in real-time to streaming checkpoint data!");
}

#[tokio::test]
async fn test_e2e_packet_drop_and_exponential_backoff_recovery() {
    use nng::{Socket as NngSocket, Protocol as NngProtocol};
    use prost::Message;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let auth_addr = "tcp://127.0.0.1:5955";
    
    // 1. Start a mock Auth Server that simulates dropping/ignoring the first 2 requests
    let server_socket = NngSocket::new(NngProtocol::Rep0).unwrap();
    server_socket.listen(auth_addr).unwrap();
    
    let drop_counter = Arc::new(AtomicUsize::new(0));
    let drop_counter_clone = drop_counter.clone();

    // Start server receiver thread
    std::thread::spawn(move || {
        loop {
            // Receive Join
            if let Ok(msg) = server_socket.recv() {
                let count = drop_counter_clone.fetch_add(1, Ordering::SeqCst);
                
                if count < 2 {
                    // SIMULATE DROP: We simply ignore the request and do NOT reply!
                    // This forces the client to hit its 2-second RecvTimeout and trigger backoff.
                    println!("[Test Server] Intentionally dropping/ignoring handshake request #{}", count + 1);
                    continue;
                }

                // Attempt #3: We successfully reply to complete the handshake!
                let slice = msg.as_slice();
                let node_id = String::from_utf8_lossy(&slice[5..]).to_string();
                println!("[Test Server] Handshake attempt #{} - successfully replying to {}", count + 1, node_id);

                // Generate high-entropy challenge
                let challenge = proto::HandshakeChallenge {
                    high_entropy_token: vec![1, 2, 3, 4],
                    timestamp: 1000,
                };
                let mut buf = Vec::new();
                challenge.encode(&mut buf).unwrap();

                let mut frame = Vec::new();
                frame.extend_from_slice(&((buf.len() as u32).to_be_bytes()));
                frame.push(2); // HandshakeChallenge
                frame.extend_from_slice(&buf);
                server_socket.send(&frame).unwrap();

                // Receive HandshakeResponse
                let _resp_msg = server_socket.recv().unwrap();

                // Reply HandshakeResult success
                let result = proto::HandshakeResult {
                    authenticated: true,
                    session_token: "session-abc-123".to_string(),
                    message: "Welcome to packet drop simulation!".to_string(),
                };
                let mut res_buf = Vec::new();
                result.encode(&mut res_buf).unwrap();

                let mut res_frame = Vec::new();
                res_frame.extend_from_slice(&((res_buf.len() as u32).to_be_bytes()));
                res_frame.push(4); // HandshakeResult
                res_frame.extend_from_slice(&res_buf);
                server_socket.send(&res_frame).unwrap();
                break; // Handshake completed, exit mock loop
            }
        }
    });

    // Give server time to bind
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Bind dummy listeners on heartbeat, task, and progress endpoints to let client dial successfully
    let heartbeat_listener = NngSocket::new(NngProtocol::Pull0).unwrap();
    heartbeat_listener.listen("tcp://127.0.0.1:5957").unwrap();

    let task_listener = NngSocket::new(NngProtocol::Push0).unwrap();
    task_listener.listen("tcp://127.0.0.1:5956").unwrap();

    let progress_listener = NngSocket::new(NngProtocol::Pull0).unwrap();
    progress_listener.listen("tcp://127.0.0.1:5958").unwrap();

    // 2. Start the client (which has our randomized exponential backoff Handshake retry logic!)
    // We import NngClient from host_background_agent crate!
    use host_background_agent::transport::NngClient;

    let client = NngClient::new(
        auth_addr,
        "tcp://127.0.0.1:5956",
        "tcp://127.0.0.1:5957",
        "tcp://127.0.0.1:5958",
        "node-backoff-tester",
    );

    // Run the lifecycle. It should experience 2 timeouts, back off twice,
    // and then successfully complete on the 3rd attempt!
    let start = std::time::Instant::now();
    let res: Result<(), String> = client.run_demo_lifecycle(vec![]).await;
    let elapsed = start.elapsed();

    // 3. Verify SUCCESS: Handshake completed, and backoff occurred
    assert!(res.is_ok(), "Client should successfully connect after recovering from packet drops!");
    assert_eq!(drop_counter.load(Ordering::SeqCst), 3, "Handshake should have completed on the 3rd attempt!");
    assert!(elapsed.as_secs_f64() >= 4.0, "Handshake must have experienced at least two 2-second timeout durations!");
    println!("[Test] Packet-drop validation passed successfully in {:.2}s!", elapsed.as_secs_f64());
}
