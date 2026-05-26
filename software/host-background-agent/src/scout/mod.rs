pub mod profilers;

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

    pub async fn discover_capabilities_async(&self) -> Vec<crate::proto::DeviceCapability> {
        use profilers::{CpuProfiler, GpuProfiler, MemoryProfiler, NetworkProfiler, SoftwareProfiler, Profiler, adapt_to_medium_profile};

        let mut raw_caps = Vec::new();

        match CpuProfiler.profile().await {
            Ok(mut caps) => raw_caps.append(&mut caps),
            Err(e) => eprintln!("[Scout] CPU Profiler failed: {}", e),
        }

        match GpuProfiler.profile().await {
            Ok(mut caps) => raw_caps.append(&mut caps),
            Err(e) => eprintln!("[Scout] GPU Profiler failed: {}", e),
        }

        match MemoryProfiler.profile().await {
            Ok(mut caps) => raw_caps.append(&mut caps),
            Err(e) => eprintln!("[Scout] Memory Profiler failed: {}", e),
        }

        match NetworkProfiler.profile().await {
            Ok(mut caps) => raw_caps.append(&mut caps),
            Err(e) => eprintln!("[Scout] Network Profiler failed: {}", e),
        }

        match SoftwareProfiler.profile().await {
            Ok(mut caps) => raw_caps.append(&mut caps),
            Err(e) => eprintln!("[Scout] Software Profiler failed: {}", e),
        }

        // Apply Medium Profiles adaptation layer!
        let mut adapted = adapt_to_medium_profile(&raw_caps);
        raw_caps.append(&mut adapted);
        raw_caps
    }

    pub fn discover_capabilities(&self) -> Vec<DiscoveredCapability> {
        use std::thread;
        let mut caps = Vec::new();

        // CPU Cores
        let cores = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8);
        caps.push(DiscoveredCapability {
            name: "cpu_cores".to_string(),
            val_type: "integer".to_string(),
            value: cores.to_string(),
        });

        // OS Platform
        caps.push(DiscoveredCapability {
            name: "os_platform".to_string(),
            val_type: "string".to_string(),
            value: std::env::consts::OS.to_string(),
        });

        // System Memory (RAM)
        let mem_gb = if cfg!(target_os = "macos") {
            16.0
        } else {
            16.0
        };
        caps.push(DiscoveredCapability {
            name: "total_memory_gb".to_string(),
            val_type: "float".to_string(),
            value: format!("{:.2}", mem_gb),
        });

        // GPU Accelerators
        caps.push(DiscoveredCapability {
            name: "has_cuda".to_string(),
            val_type: "boolean".to_string(),
            value: "false".to_string(),
        });

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
        assert!(caps.iter().any(|c| c.name == "os_platform"));
        assert!(caps.iter().any(|c| c.name == "total_memory_gb"));
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

    #[tokio::test]
    async fn test_scout_model_asynchronous_profilers() {
        let scout = ScoutEngine::new();
        let caps = scout.discover_capabilities_async().await;
        assert!(!caps.is_empty());

        // Raw metrics
        assert!(caps.iter().any(|c| c.resource_name == "cpu_cores"));
        assert!(caps.iter().any(|c| c.resource_name == "total_memory_gb"));

        // Medium profile adaptation normalization
        assert!(caps.iter().any(|c| c.resource_name == "cpu_class"));
        assert!(caps.iter().any(|c| c.resource_name == "accelerator_class"));
        assert!(caps.iter().any(|c| c.resource_name == "memory_tier"));
        assert!(caps.iter().any(|c| c.resource_name == "low_latency_ready"));
        assert!(caps.iter().any(|c| c.resource_name == "creative_capability_blender"));
        assert!(caps.iter().any(|c| c.resource_name == "creative_capability_inkscape"));
    }
}
