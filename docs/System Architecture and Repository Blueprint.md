# Seamless Swarm: Comprehensive System Architecture & Monorepo Blueprint

This document synthesizes the architectural decisions from `ADR-001` and `ADR-002` into a concrete technical specification. It defines the layout of the monolithic repository, maps the interaction of hardware and software boundaries, and establishes the blueprint for engineering execution.

---

## 1. Monorepo Structural Layout (`seamless-swarm/`)

To support rapid iteration across hardware development, firmware compilation, and software microservices, the project utilizes a single, monolithic repository. This ensures atomic commits across hardware BOM changes, protocol definitions, and agent software.

```text
seamless-swarm/
├── .github/                     # Monorepo CI/CD pipelines (hardware lints, firmware matrix, software tests)
├── assets/                      # Global documentation, schematics, and branding assets
├── proto/                       # Shared wire protocol contracts (NNG Scalability Protocols definitions)
│   └── topology.proto           # Structs for capability profiles and task definitions
├── hardware/                    # Hardware Design Files (ECAD)
│   ├── compute-module-hub/      # Carrier board schematics and PCB layout for ARM Appliance
│   └── node-key-dongle/         # USB-C Dongle schematics, PCB layout, and ATECC608 footprints
├── firmware/                    # Bare-metal / RTOS Firmware for Hardware Components
│   └── node-key-secure/         # C-based firmware managing ATECC608 SWI/I2C communication
├── software/                    # Host Software Stack & Microservices
│   ├── compute-module-core/     # Rust-based orchestration engine running on the ARM Appliance
│   │   ├── src/scheduler/       # Profile-Driven Scheduler (Stateless / Stateful / Interactive)
│   │   └── src/registry/        # In-memory ephemeral capability registry
│   ├── host-background-agent/   # Cross-platform background service (Linux/macOS/Windows)
│   │   ├── src/scout/           # Scout Model execution engine for capability discovery
│   │   └── src/transport/       # mDNS responder and Nanomsg (NNG) client wrapper
│   └── medium-profiles/         # Interface adaptation layers for varied device categories
└── tools/                       # Development, provisioning, and hardware testing utilities
    └── provision-keys/          # CLI tool for hardware-locking Node Keys with ECDSA thumprints
```

---

## 2. Dynamic Component Interaction Architecture

The entire platform relies on the secure orchestration loop between the host background agent, the hardware secure element, and the centralized ARM appliance. The diagram below maps how a node moves from local discovery to authenticated resource sharing.

### Swarm Initialization and Authentication Lifecycle

```
+------------------------+      +-----------------------+      +-------------------------+
| Host Workstation Node  |      |   Node Key Dongle     |      |   Compute Module Hub    |
| (Background Agent)     |      |   (ATECC608 Chip)     |      |     (ARM Appliance)     |
+-----------+------------+      +-----------+-----------+      +------------+------------+
            |                               |                               |
            | === 1. Hardware Insertion ==> |                               |
            |                               |                               |
            | 2. Broadcast mDNS Discovery (UDP Port 5353) ----------------> |
            | <--------- 3. Acknowledge Service Connection (NNG) ---------- |
            |                               |                               |
            | <--------- 4. Issue High-Entropy Token Challenge ------------ |
            |                               |                               |
            | --- 5. Forward Challenge ---> |                               |
            |                               | 6. Compute Cryptographic      |
            |                               |    ECDSA P-256 Signature      |
            | <--- 7. Return Signature ---- |                               |
            |                               |                               |
            | 8. Forward Complete Cryptographic Proof Bundle --------------> |
            |                                                               | 9. Verify via Static
            |                                                               |    Thumbprint Local DB
            | <========= 10. Swarm Authentication Granted ================= |
            |                                                               |
            | 11. Run Local Scout Model to Discover Capabilities            |
            | 12. Transmit Ephemeral Capability Profile Payload ----------> |
            |                                                               | 13. Index capabilities
            |                                                               |     into global registry
```

---

## 3. Subsystem Detailed Specifications

### 3.1 Hardware & Cryptographic Subsystem

* **The Compute Module Hub:** Built on a low-power ARM system-on-chip (SoC) reference platform. It features an integrated Wi-Fi 6E/7 radio interface capable of driving a direct $6\text{ GHz}$ mesh topology. It contains no user-facing UI, performing entirely as a headless orchestration anchor.
* **The Node Key Dongle:** Form-factor optimized USB-C peripheral. It houses a Microchip `ATECC608B/C` secure element connected over an internal I2C bus bridge.
* **Cryptographic Boundary:** The private key is injected into a hardware-locked zone during the manufacturing/provisioning stage (`/tools/provision-keys`). It cannot be extracted by the host operating system, effectively rendering key cloning impossible.

### 3.2 Network, Transport, and Discovery Layer

* **Zero-Config Topology:** The `host-background-agent` utilizes native link-local multicast addressing when corporate DHCP is unavailable. Hostnames resolve through an incremental backoff algorithm (e.g., `studio-node-1.local` updates to `studio-node-2.local` upon collision detection).
* **Network Resilience Fallback:** If enterprise network topography drops or strips multicast UDP packets due to Access Point layer segregation, the system drops back to Wi-Fi Neighbor Awareness Networking (NAN) link-layer frames, ensuring the swarm remains intact through physical proximity alone.
* **High-Performance Transmission Mesh:** Nanomsg (NNG) acts as the baseline framing layer. Control packets, state changes, and task allocations execute inside sub-millisecond windows. Payloads over $64\text{ KB}$ automatically open direct point-to-point raw TCP sockets to bypass frame orchestration bottlenecks.

### 3.3 Scheduling and Fault Isolation Engine

The `compute-module-core` scheduler implements a profile-driven execution taxonomy. It maps task types directly to strict failure modes to ensure reliable orchestration across a volatile local network:

| Task Taxonomy | Scheduling Protocol | Recovery Action Matrix |
| --- | --- | --- |
| **Stateless / Idempotent** *(e.g., Image Processing)* | Distributed Parallel Queue | **Automated Retry:** Immediate task reassignment to alternative matching node. |
| **Stateful / Long-Running** *(e.g., 3D Render Asset)* | Progress-Monitored Pipeline | **Checkpoint & Resume:** Pulls periodic progress metadata; falls back to last known snapshot upon node departure. |
| **Interactive / Low-Latency** *(e.g., Audio Routing)* | Raw Multiplexed Stream | **Immediate Failure Surface:** Bypasses retry loops entirely; drops execution immediately to force immediate local host fallback. |

---

## 4. Next Steps for Implementation

With this monolithic layout and technical specification approved, the core repository can be initialized. Execution should proceed along these three parallel tracks:

1. **Hardware & Firmware Track (`/hardware`, `/firmware`):** Finalize the ECAD schematics for the USB-C dongle form factor and write the initial C test harness to validate ECDSA challenge-response operations against the ATECC608 evaluation board.
2. **Transport & Discovery Track (`/software/host-background-agent`):** Build out the Rust mDNS responder alongside the NNG protocol wrapper to benchmark node join times under simulated network packet drop rates up to $15\%$.
3. **Scheduling Track (`/software/compute-module-core`):** Prototype the in-memory registry database and validate the three-tier task recovery taxonomy using simulated host dropouts.
