use compute_module_core::registry::{EphemeralRegistry, NodeProfile, Capability};
use compute_module_core::scheduler::{ProfileScheduler, Task, TaskType, ExecutionResult};
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

    let stateless_task = Task {
        task_id: "stateless-01".to_string(),
        task_type: TaskType::StatelessIdempotent,
        payload: vec![1, 2, 3],
        max_retries: 3,
    };

    let stateful_task = Task {
        task_id: "stateful-01".to_string(),
        task_type: TaskType::StatefulLongRunning,
        payload: vec![4, 5, 6],
        max_retries: 3,
    };

    let interactive_task = Task {
        task_id: "interactive-01".to_string(),
        task_type: TaskType::InteractiveLowLatency,
        payload: vec![7, 8, 9],
        max_retries: 1,
    };

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
    let server_addr = "tcp://127.0.0.1:5689";
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
