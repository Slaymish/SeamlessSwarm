use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct Capability {
    pub name: String,
    pub val_type: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct NodeProfile {
    pub node_id: String,
    pub os_platform: String,
    pub capabilities: Vec<Capability>,
    pub last_seen: u64,
}

#[derive(Clone)]
pub struct EphemeralRegistry {
    nodes: Arc<RwLock<HashMap<String, NodeProfile>>>,
}

impl EphemeralRegistry {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_node(&self, profile: NodeProfile) {
        if let Ok(mut lock) = self.nodes.write() {
            lock.insert(profile.node_id.clone(), profile);
        }
    }

    pub fn unregister_node(&self, node_id: &str) {
        if let Ok(mut lock) = self.nodes.write() {
            lock.remove(node_id);
        }
    }

    pub fn get_node(&self, node_id: &str) -> Option<NodeProfile> {
        self.nodes.read().ok()?.get(node_id).cloned()
    }

    pub fn list_nodes(&self) -> Vec<NodeProfile> {
        match self.nodes.read() {
            Ok(lock) => lock.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for EphemeralRegistry {
    fn default() -> Self {
        Self::new()
    }
}
