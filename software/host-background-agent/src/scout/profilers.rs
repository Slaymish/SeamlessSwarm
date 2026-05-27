use std::process::Command;
use std::thread;
use crate::proto::DeviceCapability;
use crate::proto::device_capability::ResourceValue;

#[allow(async_fn_in_trait)]
pub trait Profiler: Send + Sync {
    async fn profile(&self) -> Result<Vec<DeviceCapability>, String>;
}

// 1. CPU Profiler
pub struct CpuProfiler;

impl Profiler for CpuProfiler {
    async fn profile(&self) -> Result<Vec<DeviceCapability>, String> {
        let cores = thread::available_parallelism()
            .map(|n| n.get() as i64)
            .unwrap_or(8);

        let has_neon = cfg!(target_arch = "aarch64");
        let has_avx2 = cfg!(target_feature = "avx2");

        let mut caps = vec![
            DeviceCapability {
                resource_name: "cpu_cores".to_string(),
                value_type: "integer".to_string(),
                resource_value: Some(ResourceValue::IntVal(cores)),
            },
            DeviceCapability {
                resource_name: "has_neon".to_string(),
                value_type: "boolean".to_string(),
                resource_value: Some(ResourceValue::BoolVal(has_neon)),
            },
            DeviceCapability {
                resource_name: "has_avx2".to_string(),
                value_type: "boolean".to_string(),
                resource_value: Some(ResourceValue::BoolVal(has_avx2)),
            },
        ];

        // Core clock frequency simulation
        caps.push(DeviceCapability {
            resource_name: "cpu_clock_mhz".to_string(),
            value_type: "integer".to_string(),
            resource_value: Some(ResourceValue::IntVal(3200)),
        });

        Ok(caps)
    }
}

// 2. GPU Profiler
pub struct GpuProfiler;

impl Profiler for GpuProfiler {
    async fn profile(&self) -> Result<Vec<DeviceCapability>, String> {
        // Offload matrix multiplication FLOPS benchmark to spawn_blocking
        let gflops = tokio::task::spawn_blocking(|| {
            // Simple synthetic matrix-multiplication benchmark
            let size = 128;
            let a = vec![1.0f32; size * size];
            let b = vec![2.0f32; size * size];
            let mut c = vec![0.0f32; size * size];

            let start = std::time::Instant::now();
            for i in 0..size {
                for j in 0..size {
                    let mut sum = 0.0;
                    for k in 0..size {
                        sum += a[i * size + k] * b[k * size + j];
                    }
                    c[i * size + j] = sum;
                }
            }
            let elapsed = start.elapsed().as_secs_f64();
            let ops = (2 * size * size * size) as f64;
            let flops = ops / elapsed;
            flops / 1e9 // GFLOPS
        })
        .await
        .unwrap_or(0.5);

        let is_macos = cfg!(target_os = "macos");
        let is_linux = cfg!(target_os = "linux");

        let has_cuda = if is_linux {
            std::path::Path::new("/dev/nvidia0").exists()
        } else {
            false
        };

        let has_metal = is_macos;

        let mut caps = vec![
            DeviceCapability {
                resource_name: "has_cuda".to_string(),
                value_type: "boolean".to_string(),
                resource_value: Some(ResourceValue::BoolVal(has_cuda)),
            },
            DeviceCapability {
                resource_name: "has_metal".to_string(),
                value_type: "boolean".to_string(),
                resource_value: Some(ResourceValue::BoolVal(has_metal)),
            },
            DeviceCapability {
                resource_name: "gpu_gflops_benchmark".to_string(),
                value_type: "float".to_string(),
                resource_value: Some(ResourceValue::DoubleVal(gflops)),
            },
        ];

        if is_macos {
            caps.push(DeviceCapability {
                resource_name: "metal_vram_gb".to_string(),
                value_type: "integer".to_string(),
                resource_value: Some(ResourceValue::IntVal(16)), // Mock/Apple Unified memory standard
            });
        }

        Ok(caps)
    }
}

// 3. Memory & High-Speed I/O Profiler
pub struct MemoryProfiler;

impl Profiler for MemoryProfiler {
    async fn profile(&self) -> Result<Vec<DeviceCapability>, String> {
        let mem_gb = tokio::task::spawn_blocking(|| {
            if cfg!(target_os = "macos") {
                if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
                    if let Ok(s) = String::from_utf8(output.stdout) {
                        if let Ok(val) = s.trim().parse::<u64>() {
                            return (val as f64) / (1024.0 * 1024.0 * 1024.0);
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
                                    return (val_kb as f64) / (1024.0 * 1024.0);
                                }
                            }
                        }
                    }
                }
            }
            16.0 // Fallback
        })
        .await
        .unwrap_or(16.0);

        // Simulated NVMe Write Speed benchmark
        let nvme_speed_mbs = tokio::task::spawn_blocking(|| {
            let start = std::time::Instant::now();
            let data = vec![0u8; 10 * 1024 * 1024]; // 10MB write test
            let path = std::env::temp_dir().join("seamless_speed_test.bin");
            if std::fs::write(&path, &data).is_ok() {
                let elapsed = start.elapsed().as_secs_f64();
                let _ = std::fs::remove_file(path);
                if elapsed > 0.0 {
                    return 10.0 / elapsed; // MB/s
                }
            }
            500.0 // Default fallback
        })
        .await
        .unwrap_or(500.0);

        Ok(vec![
            DeviceCapability {
                resource_name: "total_memory_gb".to_string(),
                value_type: "float".to_string(),
                resource_value: Some(ResourceValue::DoubleVal(mem_gb)),
            },
            DeviceCapability {
                resource_name: "nvme_write_mbps".to_string(),
                value_type: "float".to_string(),
                resource_value: Some(ResourceValue::DoubleVal(nvme_speed_mbs)),
            },
        ])
    }
}

// 4. Network Interface Profiler
pub struct NetworkProfiler;

impl Profiler for NetworkProfiler {
    async fn profile(&self) -> Result<Vec<DeviceCapability>, String> {
        // Measure mock link-local latency
        let latency_ms = 1.25f64; // Low-latency local network mock

        Ok(vec![
            DeviceCapability {
                resource_name: "link_latency_ms".to_string(),
                value_type: "float".to_string(),
                resource_value: Some(ResourceValue::DoubleVal(latency_ms)),
            },
            DeviceCapability {
                resource_name: "link_bandwidth_mbps".to_string(),
                value_type: "integer".to_string(),
                resource_value: Some(ResourceValue::IntVal(1000)),
            },
        ])
    }
}

// Known creative/compute CLI tools to probe on all platforms.
// Maps (executable_name, capability_name) — the capability name becomes the `installed_app_*` key.
const KNOWN_CLI_TOOLS: &[(&str, &str)] = &[
    ("ffmpeg",       "ffmpeg"),
    ("blender",      "blender"),
    ("inkscape",     "inkscape"),
    ("HandBrakeCLI", "handbrake"),
    ("gimp",         "gimp"),
    ("convert",      "imagemagick"),
    ("python3",      "python3"),
    ("node",         "node"),
    ("docker",       "docker"),
    ("claude",       "claude"),
    ("ollama",       "ollama"),
];

fn discover_cli_tools() -> Vec<String> {
    let path_env = std::env::var("PATH").unwrap_or_default();

    let mut search_dirs: Vec<std::path::PathBuf> = path_env
        .split(':')
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .collect();

    // Prepend Homebrew paths (Apple Silicon first, then Intel) so they are found
    // even when the agent process was not launched from a login shell.
    for dir in &["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin", "/usr/local/sbin"] {
        let p = std::path::PathBuf::from(dir);
        if p.exists() && !search_dirs.contains(&p) {
            search_dirs.insert(0, p);
        }
    }

    let mut found = Vec::new();
    for (exe, name) in KNOWN_CLI_TOOLS {
        for dir in &search_dirs {
            if dir.join(exe).exists() {
                found.push(name.to_string());
                break;
            }
        }
    }
    found
}

// 5. Software Application Profiler (Organic Software Discovery)
pub struct SoftwareProfiler;

impl Profiler for SoftwareProfiler {
    async fn profile(&self) -> Result<Vec<DeviceCapability>, String> {
        let discovered_apps = tokio::task::spawn_blocking(move || {
            let mut apps = Vec::new();
            let is_macos = cfg!(target_os = "macos");
            let is_windows = cfg!(target_os = "windows");

            if is_macos {
                // GUI app bundles from /Applications
                if let Ok(entries) = std::fs::read_dir("/Applications") {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |e| e == "app") {
                            if let Some(name) = path.file_stem() {
                                let app_name = name.to_string_lossy().to_lowercase()
                                    .replace(' ', "_");
                                apps.push(app_name);
                            }
                        }
                    }
                }
            } else if is_windows {
                if let Ok(entries) = std::fs::read_dir("C:\\Program Files") {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            if let Some(name) = path.file_name() {
                                apps.push(name.to_string_lossy().to_lowercase().replace(' ', "_"));
                            }
                        }
                    }
                }
            } else {
                if let Ok(entries) = std::fs::read_dir("/usr/share/applications") {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |e| e == "desktop") {
                            if let Some(name) = path.file_stem() {
                                apps.push(name.to_string_lossy().to_lowercase().replace(' ', "_"));
                            }
                        }
                    }
                }
            }

            // Merge in CLI tools discovered from PATH / Homebrew — no fabrication.
            for name in discover_cli_tools() {
                if !apps.contains(&name) {
                    apps.push(name);
                }
            }

            apps
        })
        .await
        .unwrap_or_default();

        let mut caps = Vec::new();
        for app in discovered_apps {
            caps.push(DeviceCapability {
                resource_name: format!("installed_app_{}", app),
                value_type: "boolean".to_string(),
                resource_value: Some(ResourceValue::BoolVal(true)),
            });
        }

        Ok(caps)
    }
}

// --- 6. Medium Profiles Adaptation Layer ---
pub fn adapt_to_medium_profile(raw_capabilities: &[DeviceCapability]) -> Vec<DeviceCapability> {
    let mut adapted = Vec::new();

    // Core variables to parse
    let mut physical_cores = 0i64;
    let mut has_gpu = false;
    let mut total_ram = 0.0f64;
    let mut latency_ms = 100.0f64;
    let mut discovered_apps = Vec::new();

    for cap in raw_capabilities {
        if cap.resource_name.starts_with("installed_app_") {
            if let Some(app_name) = cap.resource_name.strip_prefix("installed_app_") {
                discovered_apps.push(app_name.to_string());
            }
            continue;
        }

        match cap.resource_name.as_str() {
            "cpu_cores" => {
                if let Some(ResourceValue::IntVal(v)) = cap.resource_value {
                    physical_cores = v;
                }
            }
            "has_cuda" | "has_metal" => {
                if let Some(ResourceValue::BoolVal(v)) = cap.resource_value {
                    if v {
                        has_gpu = true;
                    }
                }
            }
            "total_memory_gb" => {
                if let Some(ResourceValue::DoubleVal(v)) = cap.resource_value {
                    total_ram = v;
                }
            }
            "link_latency_ms" => {
                if let Some(ResourceValue::DoubleVal(v)) = cap.resource_value {
                    latency_ms = v;
                }
            }
            _ => {}
        }
    }

    // Adapt CPU Core Class
    let cpu_class = if physical_cores > 16 {
        "heavy_parallel"
    } else if physical_cores >= 8 {
        "multicore_standard"
    } else {
        "low_power_edge"
    };

    adapted.push(DeviceCapability {
        resource_name: "cpu_class".to_string(),
        value_type: "string".to_string(),
        resource_value: Some(ResourceValue::StringVal(cpu_class.to_string())),
    });

    // Adapt GPU/Accelerator Class
    let gpu_class = if has_gpu {
        if cfg!(target_os = "macos") {
            "unified_gpu"
        } else {
            "vram_gpu"
        }
    } else {
        "none"
    };

    adapted.push(DeviceCapability {
        resource_name: "accelerator_class".to_string(),
        value_type: "string".to_string(),
        resource_value: Some(ResourceValue::StringVal(gpu_class.to_string())),
    });

    // Adapt Memory Tier
    let memory_tier = if total_ram > 32.0 {
        "high_throughput"
    } else if total_ram >= 16.0 {
        "standard_capacity"
    } else {
        "constrained"
    };

    adapted.push(DeviceCapability {
        resource_name: "memory_tier".to_string(),
        value_type: "string".to_string(),
        resource_value: Some(ResourceValue::StringVal(memory_tier.to_string())),
    });

    // Adapt Low Latency Ready
    let low_latency_ready = latency_ms < 2.0;

    adapted.push(DeviceCapability {
        resource_name: "low_latency_ready".to_string(),
        value_type: "boolean".to_string(),
        resource_value: Some(ResourceValue::BoolVal(low_latency_ready)),
    });

    // Adapt Creative Capability Classes organically!
    for app in discovered_apps {
        adapted.push(DeviceCapability {
            resource_name: format!("creative_capability_{}", app),
            value_type: "string".to_string(),
            resource_value: Some(ResourceValue::StringVal(format!("{}_execution", app))),
        });
    }

    adapted
}
