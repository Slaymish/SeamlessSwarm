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
