# Scout Capability Discovery Model & Profile Adaptation

The `host-background-agent` runs an internal profiling engine called the **Scout Model**. The Scout Model is responsible for discovering raw workstation hardware parameters and adapting them into uniform, platform-agnostic capability profiles specified by our Protobuf contracts (`DeviceCapability`).

---

## 1. Hardware Profiling Taxonomy

On startup and during active heartbeats, the Scout Model executes system-level APIs to profile available resources across four main categories:

### 1.1 Compute Resources (CPU)
- **Thread Count:** Queries logical and physical CPU cores (using `sysinfo` or native OS sysctls).
- **Instruction Extensions:** Checks for SIMD capabilities (AVX2, AVX-512, ARM Neon) to gauge vector performance capacity.
- **Clock Frequencies:** Captures active core throttling and nominal maximum frequencies.

### 1.2 Graphical & Machine Learning Accelerators (GPU)
- **APIs Detected:** Profiles graphics interfaces based on operating system platforms:
  - **macOS:** Metal framework feature sets, Unified Memory allocated, and Apple Silicon GPU cores.
  - **Linux/Windows:** CUDA cores, OpenCL platforms, Vulkan devices, and dedicated VRAM capacity.
- **Model Tensors:** Benchmarks basic matrix-multiplication operations to index raw FLOPS.

### 1.3 Memory & High-Speed I/O
- **System RAM:** Available vs. total physical system memory.
- **Local NVMe Capacity:** Checks temporary scratch space directories (`/tmp` or specialized write caches) and measures disk write bandwidth.

### 1.4 Network Interface parameters
- **Link Bandwidth:** Measures active link-local bandwidth.
- **Latency Jitter:** Calculates average latency to the nearest gateway to assign low-latency ratings.

---

## 2. Adaptation Layer: `medium-profiles`

Raw system values vary wildly across heterogeneous hardware (e.g., a 64-core Linux threadripper workstation vs. an 8-core Apple M3 laptop). The `medium-profiles` adaptation layer normalizes these into structured categories to simplify scheduling:

```
+---------------------------+
| Raw OS-Specific Metrics   | (sysctl, CUDA, Metal APIs)
+-------------+-------------+
              |
              v [Scout Model Profiling]
+---------------------------+
|   Normalizing Adaptation  | (Translates thread counts, FLOPS, and VRAM)
+-------------+-------------+
              |
              v [Medium Profiles Adaptation]
+---------------------------+
|     Device Capability     | (Protobuf representation in topology.proto)
+---------------------------+
```

### 2.1 Capability Normalization Matrix
Raw metrics map to the following standardized Protobuf `DeviceCapability` fields:

| Raw Parameter | Adapted Capability Name | Type | Value Representation |
| --- | --- | --- | --- |
| Physical cores > 16 | `cpu_class` | String | `"heavy_parallel"` |
| Dedicated CUDA/Metal support | `accelerator_class` | String | `"vram_gpu"` / `"unified_gpu"` |
| Available RAM > 32GB | `memory_tier` | String | `"high_throughput"` |
| Average ping < 2ms | `low_latency_ready` | Bool | `true` |

---

## 3. Development Guidelines

### 3.1 Implementation of a New Profiler
When adding a profiler for a new device type or operating system:
1. Implement the `Profiler` trait inside `host-background-agent/src/scout/`.
2. Do not use blocking kernel calls directly on the main thread; offload heavy hardware probes (like running CUDA/Metal FLOPS benchmarks) to a separate `tokio::task::spawn_blocking` task.
3. Serialize the output directly into the standard `DeviceCapability` protobuf structure before streaming to the central hub.
