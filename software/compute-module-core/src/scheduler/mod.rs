use crate::registry::EphemeralRegistry;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq)]
pub enum TaskType {
    StatelessIdempotent,
    StatefulLongRunning,
    InteractiveLowLatency,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskState {
    Pending,
    Running { node_id: String, started_at: u64 },
    Completed { result: Vec<u8> },
    Failed { reason: String },
}

#[derive(Clone, Debug)]
pub struct Task {
    pub task_id: String,
    pub task_type: TaskType,
    pub payload: Vec<u8>,
    pub max_retries: usize,
    pub state: TaskState,
    pub current_retry: usize,
}

impl Task {
    pub fn new(task_id: String, task_type: TaskType, payload: Vec<u8>, max_retries: usize) -> Self {
        Self {
            task_id,
            task_type,
            payload,
            max_retries,
            state: TaskState::Pending,
            current_retry: 0,
        }
    }
}

pub enum ExecutionResult {
    Success,
    RetryNeeded(String),
    CheckpointSaved(Vec<u8>),
    ImmediateFailure(String),
}

#[derive(Clone)]
pub struct ProfileScheduler {
    registry: EphemeralRegistry,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
}

impl ProfileScheduler {
    pub fn new(registry: EphemeralRegistry) -> Self {
        Self {
            registry,
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn submit_task(&self, task: Task) {
        if let Ok(mut lock) = self.tasks.write() {
            lock.insert(task.task_id.clone(), task);
        }
    }

    pub fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.read().ok()?.get(task_id).cloned()
    }

    pub fn list_tasks(&self) -> Vec<Task> {
        match self.tasks.read() {
            Ok(lock) => lock.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn update_task_progress(&self, task_id: &str, result: ExecutionResult) {
        if let Ok(mut lock) = self.tasks.write() {
            if let Some(task) = lock.get_mut(task_id) {
                match result {
                    ExecutionResult::Success => {
                        task.state = TaskState::Completed { result: vec![] };
                    }
                    ExecutionResult::CheckpointSaved(checkpoint) => {
                        task.payload = checkpoint;
                        // Remains in Running state or update as appropriate
                    }
                    ExecutionResult::RetryNeeded(reason) => {
                        if task.current_retry < task.max_retries {
                            task.current_retry += 1;
                            task.state = TaskState::Pending;
                        } else {
                            task.state = TaskState::Failed {
                                reason: format!("Max retries exceeded: {}", reason),
                            };
                        }
                    }
                    ExecutionResult::ImmediateFailure(reason) => {
                        task.state = TaskState::Failed { reason };
                    }
                }
            }
        }
    }

    pub fn handle_node_failure(&self, node_id: &str) {
        if let Ok(mut lock) = self.tasks.write() {
            for task in lock.values_mut() {
                if let TaskState::Running { node_id: running_node, .. } = &task.state {
                    if running_node == node_id {
                        match task.task_type {
                            TaskType::StatelessIdempotent => {
                                if task.current_retry < task.max_retries {
                                    task.current_retry += 1;
                                    task.state = TaskState::Pending;
                                } else {
                                    task.state = TaskState::Failed {
                                        reason: "Stateless task failed: node lost and max retries exceeded".to_string(),
                                    };
                                }
                            }
                            TaskType::StatefulLongRunning => {
                                if task.current_retry < task.max_retries {
                                    task.current_retry += 1;
                                    task.state = TaskState::Pending;
                                    // Stateful task keeps its updated payload (checkpoint) so it resumes from there!
                                } else {
                                    task.state = TaskState::Failed {
                                        reason: "Stateful task failed: node lost and max retries exceeded".to_string(),
                                    };
                                }
                            }
                            TaskType::InteractiveLowLatency => {
                                // Interactive tasks bypass retries and fail immediately
                                task.state = TaskState::Failed {
                                    reason: "Interactive node departed. Dropping execution.".to_string(),
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn dispatch_pending_tasks(&self, current_time: u64) -> Vec<(String, String)> {
        let mut dispatches = Vec::new();
        let nodes = self.registry.list_nodes();
        if nodes.is_empty() {
            return dispatches;
        }

        if let Ok(mut lock) = self.tasks.write() {
            let mut node_idx = 0;
            for task in lock.values_mut() {
                if task.state == TaskState::Pending {
                    // Match with an active node
                    let target_node = &nodes[node_idx % nodes.len()];
                    task.state = TaskState::Running {
                        node_id: target_node.node_id.clone(),
                        started_at: current_time,
                    };
                    dispatches.push((task.task_id.clone(), target_node.node_id.clone()));
                    node_idx += 1;
                }
            }
        }
        dispatches
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
            state: TaskState::Pending,
            current_retry: 0,
        };
        let t2 = Task {
            task_id: "t2".to_string(),
            task_type: TaskType::StatefulLongRunning,
            payload: vec![],
            max_retries: 2,
            state: TaskState::Pending,
            current_retry: 0,
        };
        let t3 = Task {
            task_id: "t3".to_string(),
            task_type: TaskType::InteractiveLowLatency,
            payload: vec![],
            max_retries: 2,
            state: TaskState::Pending,
            current_retry: 0,
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
            state: TaskState::Pending,
            current_retry: 0,
        };

        let active_node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 1200,
            public_key: "".to_string(),
        };
        registry.register_node(active_node);
        assert!(matches!(scheduler.schedule_task(&t1, "n1"), ExecutionResult::Success));

        let laggy_node = NodeProfile {
            node_id: "n2".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 800,
            public_key: "".to_string(),
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
            state: TaskState::Pending,
            current_retry: 0,
        };

        let node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 1200,
            public_key: "".to_string(),
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
            state: TaskState::Pending,
            current_retry: 0,
        };

        let normal_node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 600,
            public_key: "".to_string(),
        };
        registry.register_node(normal_node);
        assert!(matches!(scheduler.schedule_task(&t, "n1"), ExecutionResult::Success));

        let high_latency_node = NodeProfile {
            node_id: "n2".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 300,
            public_key: "".to_string(),
        };
        registry.register_node(high_latency_node);
        assert!(matches!(scheduler.schedule_task(&t, "n2"), ExecutionResult::ImmediateFailure(_)));
    }

    #[test]
    fn test_task_taxonomy_recovery_logic() {
        let registry = EphemeralRegistry::new();
        let scheduler = ProfileScheduler::new(registry.clone());

        let t_stateless = Task::new("t-stateless".to_string(), TaskType::StatelessIdempotent, vec![1, 2], 2);
        let t_stateful = Task::new("t-stateful".to_string(), TaskType::StatefulLongRunning, vec![3, 4], 2);
        let t_interactive = Task::new("t-interactive".to_string(), TaskType::InteractiveLowLatency, vec![5, 6], 2);

        scheduler.submit_task(t_stateless);
        scheduler.submit_task(t_stateful);
        scheduler.submit_task(t_interactive);

        let node = NodeProfile {
            node_id: "n1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 1200,
            public_key: "".to_string(),
        };
        registry.register_node(node);

        // Dispatch tasks
        let dispatches = scheduler.dispatch_pending_tasks(1000);
        assert_eq!(dispatches.len(), 3);

        // Check running tasks state
        assert!(matches!(scheduler.get_task("t-stateless").unwrap().state, TaskState::Running { .. }));
        assert!(matches!(scheduler.get_task("t-stateful").unwrap().state, TaskState::Running { .. }));
        assert!(matches!(scheduler.get_task("t-interactive").unwrap().state, TaskState::Running { .. }));

        // Now save checkpoint for stateful task
        scheduler.update_task_progress("t-stateful", ExecutionResult::CheckpointSaved(vec![9, 9, 9]));
        assert_eq!(scheduler.get_task("t-stateful").unwrap().payload, vec![9, 9, 9]);

        // Fail the node!
        scheduler.handle_node_failure("n1");

        // Verify recovery transitions
        let recovered_stateless = scheduler.get_task("t-stateless").unwrap();
        assert_eq!(recovered_stateless.state, TaskState::Pending);
        assert_eq!(recovered_stateless.current_retry, 1);

        let recovered_stateful = scheduler.get_task("t-stateful").unwrap();
        assert_eq!(recovered_stateful.state, TaskState::Pending);
        assert_eq!(recovered_stateful.current_retry, 1);
        assert_eq!(recovered_stateful.payload, vec![9, 9, 9]); // check that checkpoint is preserved!

        let recovered_interactive = scheduler.get_task("t-interactive").unwrap();
        assert!(matches!(recovered_interactive.state, TaskState::Failed { .. }));
        assert_eq!(recovered_interactive.current_retry, 0); // bypassed retries!
    }
}
