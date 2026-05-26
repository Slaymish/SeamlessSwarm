use serde::{Serialize, Deserialize};
use std::process::Command;
use std::thread;

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
        let mut caps = Vec::new();

        // 1. CPU Cores
        let cores = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8);
        caps.push(DiscoveredCapability {
            name: "cpu_cores".to_string(),
            val_type: "integer".to_string(),
            value: cores.to_string(),
        });

        // 2. OS Platform
        caps.push(DiscoveredCapability {
            name: "os_platform".to_string(),
            val_type: "string".to_string(),
            value: std::env::consts::OS.to_string(),
        });

        // 3. System Memory (RAM)
        let mem_bytes = self.get_total_memory();
        let mem_gb = (mem_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        caps.push(DiscoveredCapability {
            name: "total_memory_gb".to_string(),
            val_type: "float".to_string(),
            value: format!("{:.2}", mem_gb),
        });

        // 4. GPU Accelerators
        let has_cuda = self.check_cuda();
        caps.push(DiscoveredCapability {
            name: "has_cuda".to_string(),
            val_type: "boolean".to_string(),
            value: has_cuda.to_string(),
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

    fn get_total_memory(&self) -> u64 {
        if cfg!(target_os = "macos") {
            if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
                if let Ok(s) = String::from_utf8(output.stdout) {
                    if let Ok(val) = s.trim().parse::<u64>() {
                        return val;
                    }
                }
            }
        } else if cfg!(target_os = "linux") {
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(val_kb) = parts[1].parse::<u64>() {
                                return val_kb * 1024; // Convert KiB to Bytes
                            }
                        }
                    }
                }
            }
        }
        // Fallback default: 16 GB in bytes
        16 * 1024 * 1024 * 1024
    }

    fn check_cuda(&self) -> bool {
        if cfg!(target_os = "linux") {
            if std::path::Path::new("/dev/nvidia0").exists() {
                return true;
            }
            if let Ok(output) = Command::new("which").arg("nvidia-smi").output() {
                if output.status.success() {
                    return true;
                }
            }
        }
        false
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
}
