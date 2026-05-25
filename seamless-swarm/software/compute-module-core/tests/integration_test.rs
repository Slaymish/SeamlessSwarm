use compute_module_core::registry::{EphemeralRegistry, NodeProfile, Capability};
use compute_module_core::scheduler::{ProfileScheduler, Task, TaskType, ExecutionResult};

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
    }
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
