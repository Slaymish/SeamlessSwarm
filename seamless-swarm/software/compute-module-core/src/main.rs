use compute_module_core::registry::{EphemeralRegistry, NodeProfile, Capability};
use compute_module_core::scheduler::{ProfileScheduler, Task, TaskType, ExecutionResult};

#[tokio::main]
async fn main() {
    let registry = EphemeralRegistry::new();
    let scheduler = ProfileScheduler::new(registry.clone());

    let node = NodeProfile {
        node_id: "studio-node-1".to_string(),
        os_platform: "macOS".to_string(),
        capabilities: vec![
            Capability {
                name: "GPU".to_string(),
                val_type: "boolean".to_string(),
                value: "true".to_string(),
            }
        ],
        last_seen: 1200,
    };

    registry.register_node(node);

    let stateless_task = Task {
        task_id: "task-001".to_string(),
        task_type: TaskType::StatelessIdempotent,
        payload: vec![10, 20, 30],
        max_retries: 3,
    };

    let result = scheduler.schedule_task(&stateless_task, "studio-node-1");
    match result {
        ExecutionResult::Success => println!("Stateless task scheduled successfully"),
        _ => println!("Stateless task schedule failed"),
    }
}
