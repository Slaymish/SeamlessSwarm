use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiscoveredCapability {
    pub name: String,
    pub val_type: String,
    pub value: String,
}

pub struct ScoutEngine;

impl ScoutEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn discover_capabilities(&self) -> Vec<DiscoveredCapability> {
        let mut caps = vec![
            DiscoveredCapability {
                name: "cpu_cores".to_string(),
                val_type: "integer".to_string(),
                value: "8".to_string(),
            },
            DiscoveredCapability {
                name: "has_cuda".to_string(),
                val_type: "boolean".to_string(),
                value: "false".to_string(),
            },
        ];

        if cfg!(target_os = "macos") {
            caps.push(DiscoveredCapability {
                name: "metal_support".to_string(),
                val_type: "boolean".to_string(),
                value: "true".to_string(),
            });
        }

        caps
    }
}

impl Default for ScoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_capabilities_not_empty() {
        let scout = ScoutEngine::new();
        let caps = scout.discover_capabilities();
        assert!(!caps.is_empty());
        assert!(caps.iter().any(|c| c.name == "cpu_cores"));
        assert!(caps.iter().any(|c| c.name == "has_cuda"));
    }

    #[test]
    fn test_discover_capabilities_platform_specific() {
        let scout = ScoutEngine::new();
        let caps = scout.discover_capabilities();
        if cfg!(target_os = "macos") {
            assert!(caps.iter().any(|c| c.name == "metal_support"));
        } else {
            assert!(!caps.iter().any(|c| c.name == "metal_support"));
        }
    }
}
