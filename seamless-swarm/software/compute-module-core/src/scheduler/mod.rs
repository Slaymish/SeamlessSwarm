use crate::registry::EphemeralRegistry;

pub enum TaskType {
    StatelessIdempotent,
    StatefulLongRunning,
    InteractiveLowLatency,
}

pub struct Task {
    pub task_id: String,
    pub task_type: TaskType,
    pub payload: Vec<u8>,
    pub max_retries: usize,
}

pub enum ExecutionResult {
    Success,
    RetryNeeded(String),
    CheckpointSaved(Vec<u8>),
    ImmediateFailure(String),
}

pub struct ProfileScheduler {
    registry: EphemeralRegistry,
}

impl ProfileScheduler {
    pub fn new(registry: EphemeralRegistry) -> Self {
        Self { registry }
    }

    pub fn schedule_task(&self, task: &Task, node_id: &str) -> ExecutionResult {
        let node = match self.registry.get_node(node_id) {
            Some(n) => n,
            None => {
                return match task.task_type {
                    TaskType::StatelessIdempotent => {
                        ExecutionResult::RetryNeeded("Node not registered, needs reallocation".to_string())
                    }
                    TaskType::StatefulLongRunning => {
                        ExecutionResult::RetryNeeded("Node lost, scheduling from last checkpoint".to_string())
                    }
                    TaskType::InteractiveLowLatency => {
                        ExecutionResult::ImmediateFailure("Interactive node departed. Dropping execution.".to_string())
                    }
                };
            }
        };

        match task.task_type {
            TaskType::StatelessIdempotent => {
                if node.last_seen < 1000 {
                    ExecutionResult::RetryNeeded("Node communication laggy. Retrying on another node.".to_string())
                } else {
                    ExecutionResult::Success
                }
            }
            TaskType::StatefulLongRunning => {
                let checkpoint = vec![1, 2, 3, 4];
                ExecutionResult::CheckpointSaved(checkpoint)
            }
            TaskType::InteractiveLowLatency => {
                if node.last_seen < 500 {
                    ExecutionResult::ImmediateFailure("Latency threshold violated. Immediate host fallback.".to_string())
                } else {
                    ExecutionResult::Success
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::NodeProfile;

    #[test]
    fn test_schedule_missing_node() {
        let registry = EphemeralRegistry::new();
        let scheduler = ProfileScheduler::new(registry);

        let t1 = Task {
            task_id: "t1".to_string(),
            task_type: TaskType::StatelessIdempotent,
            payload: vec![],
            max_retries: 2,
        };
        let t2 = Task {
            task_id: "t2".to_string(),
            task_type: TaskType::StatefulLongRunning,
            payload: vec![],
            max_retries: 2,
        };
        let t3 = Task {
            task_id: "t3".to_string(),
            task_type: TaskType::InteractiveLowLatency,
            payload: vec![],
            max_retries: 2,
        };

        assert!(matches!(scheduler.schedule_task(&t1, "none"), ExecutionResult::RetryNeeded(_)));
        assert!(matches!(scheduler.schedule_task(&t2, "none"), ExecutionResult::RetryNeeded(_)));
        assert!(matches!(scheduler.schedule_task(&t3, "none"), ExecutionResult::ImmediateFailure(_)));
    }

    #[test]
    fn test_schedule_stateless_node() {
        let registry = EphemeralRegistry::new();
        let scheduler = ProfileScheduler::new(registry.clone());
        let t1 = Task {
            task_id: "t1".to_string(),
            task_type: TaskType::StatelessIdempotent,
            payload: vec![],
            max_retries: 2,
        };

        let active_node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 1200,
        };
        registry.register_node(active_node);
        assert!(matches!(scheduler.schedule_task(&t1, "n1"), ExecutionResult::Success));

        let laggy_node = NodeProfile {
            node_id: "n2".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 800,
        };
        registry.register_node(laggy_node);
        assert!(matches!(scheduler.schedule_task(&t1, "n2"), ExecutionResult::RetryNeeded(_)));
    }

    #[test]
    fn test_schedule_stateful_node() {
        let registry = EphemeralRegistry::new();
        let scheduler = ProfileScheduler::new(registry.clone());
        let t = Task {
            task_id: "t".to_string(),
            task_type: TaskType::StatefulLongRunning,
            payload: vec![],
            max_retries: 2,
        };

        let node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 1200,
        };
        registry.register_node(node);
        assert!(matches!(scheduler.schedule_task(&t, "n1"), ExecutionResult::CheckpointSaved(_)));
    }

    #[test]
    fn test_schedule_interactive_node() {
        let registry = EphemeralRegistry::new();
        let scheduler = ProfileScheduler::new(registry.clone());
        let t = Task {
            task_id: "t".to_string(),
            task_type: TaskType::InteractiveLowLatency,
            payload: vec![],
            max_retries: 2,
        };

        let normal_node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 600,
        };
        registry.register_node(normal_node);
        assert!(matches!(scheduler.schedule_task(&t, "n1"), ExecutionResult::Success));

        let high_latency_node = NodeProfile {
            node_id: "n2".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 300,
        };
        registry.register_node(high_latency_node);
        assert!(matches!(scheduler.schedule_task(&t, "n2"), ExecutionResult::ImmediateFailure(_)));
    }
}
