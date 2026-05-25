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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get_node() {
        let registry = EphemeralRegistry::new();
        let profile = NodeProfile {
            node_id: "test-node".to_string(),
            os_platform: "Linux".to_string(),
            capabilities: vec![
                Capability {
                    name: "cores".to_string(),
                    val_type: "int".to_string(),
                    value: "4".to_string(),
                }
            ],
            last_seen: 100,
        };

        registry.register_node(profile.clone());
        let retrieved = registry.get_node("test-node").unwrap();
        assert_eq!(retrieved.node_id, "test-node");
        assert_eq!(retrieved.os_platform, "Linux");
        assert_eq!(retrieved.capabilities.len(), 1);
        assert_eq!(retrieved.capabilities[0].name, "cores");
    }

    #[test]
    fn test_unregister_node() {
        let registry = EphemeralRegistry::new();
        let profile = NodeProfile {
            node_id: "test-node".to_string(),
            os_platform: "Linux".to_string(),
            capabilities: vec![],
            last_seen: 100,
        };

        registry.register_node(profile);
        assert!(registry.get_node("test-node").is_some());

        registry.unregister_node("test-node");
        assert!(registry.get_node("test-node").is_none());
    }

    #[test]
    fn test_list_nodes() {
        let registry = EphemeralRegistry::new();
        assert_eq!(registry.list_nodes().len(), 0);

        let p1 = NodeProfile {
            node_id: "node-1".to_string(),
            os_platform: "macOS".to_string(),
            capabilities: vec![],
            last_seen: 100,
        };
        let p2 = NodeProfile {
            node_id: "node-2".to_string(),
            os_platform: "Windows".to_string(),
            capabilities: vec![],
            last_seen: 200,
        };

        registry.register_node(p1);
        registry.register_node(p2);

        let list = registry.list_nodes();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|n| n.node_id == "node-1"));
        assert!(list.iter().any(|n| n.node_id == "node-2"));
    }
}
