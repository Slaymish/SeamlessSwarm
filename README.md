# Seamless Swarm: Computing Platform

Seamless Swarm is a secure, high-performance, and decentralized local resource orchestration platform. The platform dynamically pools idle compute power from local host workstations into an aggregated execution mesh, controlled by a headless ARM Appliance, and hardware-secured via USB-C Node Key Dongles.

---

## 1. High-Level System Architecture

The platform relies on a secure orchestration loop bridging three physical boundaries:

1. **The Compute Module Hub (Central Orchestrator):** A headless low-power ARM SoC appliance running `compute-module-core`. It manages the global ephemeral resource registry, receives tasks, and schedules them over a dedicated Wi-Fi 6E/7 direct mesh.
2. **The Host Workstation (Swarm Node):** Workstations run a cross-platform background agent (`host-background-agent`) that continuously profiles local hardware (CPU, GPU, RAM) via a local _Scout Model_ and offers available capacity to the mesh.
3. **The Node Key Dongle (Cryptographic Boundary):** A USB-C peripheral containing a Microchip `ATECC608` secure element. It performs hardware-locked ECDSA P-256 challenge-response handshakes to authenticate joining workstations, making private keys impossible to clone or extract.

---

## 2. Platform Documentation

For specialized specifications and tracking, refer directly to the architectural specs and logs:

- **[System Blueprint](System%20Architecture%20and%20Repository%20Blueprint.md)** (Defines wire framing protocols, dynamic interactions, and execution taxonomies).
- **[Engineering Roadmap](ROADMAP.md)** (Defines Phase 1 to Phase 4 development milestones).
- **[Open Questions Tracker](Open%20Questions.md)** (Tracks unresolved physical and networking parameters).
- **[Scout Model Guide](docs/Scout-Capability-Discovery-Model.md)** (Hardware discovery and profiles adaptation).
- **[NNG Transport Guide](docs/High-Performance-NNG-Framing.md)** (Peer discovery, mDNS, and raw socket fallbacks).
- **[Secure Cryptographic Boundary](docs/ATECC608-Cryptographic-Boundary.md)** (Slot layouts, I2C interface, and handshakes).
- **[ADR Log]**:
  - **[ADR-001](ADR-001-overall-architecture-evaluation.md)** (Transport & security evaluation).
  - **[ADR-002](ADR-002:%20Architectural%20Refinement.md)** (Securing nodes via ATECC608 and NNG).
  - **[ADR-003](ADR-003:%20Swarm%20Heartbeat%20and%20Eviction%20Mechanics.md)** (Node departure timeout & eviction).

---

## 3. Monorepo Modules Directory

This single, monolithic repository contains all hardware layout, bare-metal firmware, software microservices, and provisioning scripts:

### [Protocol Layer](proto/)

- Shared Protobuf wire-protocol contracts.
- **[Wire Definitions](proto/topology.proto)** (capability profiles, handshakes, and task envelopes).

### [Hardware Designs](hardware/)

- Schematics, board layouts, and ECAD files.
- **[Hub Carrier Specs](hardware/compute-module-hub/README.md)**
- **[Secure Key Dongle Specs](hardware/node-key-dongle/README.md)**

### [Embedded Firmware](firmware/)

- Bare-metal C-based cryptographic micro-code.
- **[ATECC608 HAL firmware](firmware/node-key-secure/)**

### [Software Orchestration](software/)

- **[compute-module-core](software/compute-module-core/)** (Rust-based in-memory registry, P-256 challenge validation, and fault-isolated scheduler).
- **[host-background-agent](software/host-background-agent/)** (Cross-platform daemon executing the Scout capability model and hosting the socket2 mDNS responder and NNG transport wrappers).
- **[medium-profiles](software/medium-profiles/README.md)** (Interface adaptation wrappers for workstation types).

### [Cryptographic Tooling](tools/)

- Factory and testing key provisioning CLI.
- **[Provision CLI](tools/provision-keys/)** (Generates tokens, computes SHA-256 static thumbprints, and locks down private keys on Slot 0).

---

## 4. Developer Quick Start

### Prerequisites

Install the following compilers and toolchains on your host machine:

- **Rust Toolchain** (v1.70 or newer)
- **CMake** (required to compile NNG sockets from source)
- **Protobuf Compiler** (`protoc` compiler required by `prost-build`)
- **ARM GCC Toolchain** (`gcc-arm-embedded` cask required for firmware compilation)

To install all prerequisites on macOS:

```bash
brew install cmake protobuf
brew install --cask gcc-arm-embedded
```

### Workspace Commands

Check and verify the entire Cargo monorepo workspace directly from the root folder:

- **Check compilation:**
  ```bash
  cargo check
  ```
- **Run the workspace test suite (17 tests):**
  ```bash
  cargo test
  ```
- **Lint all software packages:**
  ```bash
  cargo clippy --workspace -- -D warnings
  ```
- **Build optimized release binaries:**
  ```bash
  cargo build --release
  ```
