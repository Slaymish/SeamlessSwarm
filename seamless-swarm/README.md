# Seamless Swarm Monorepo

Welcome to the monolithic repository for the Seamless Swarm Computing Platform. This repository contains all hardware, firmware, and software designs, services, and utilities.

## Repository Structure

- `proto/`: Shared wire protocol contracts (protobuf definitions).
- `hardware/`: Electrical schematics and layouts for the hardware appliance hub and key dongles.
- `firmware/`: Bare-metal/RTOS code for embedded systems (e.g. Node Key Dongle ATECC608 logic).
- `software/`: Rust-based core orchestrator, cross-platform host agents, and profiles.
- `tools/`: Cryptographic provisioning and factory testing CLI utilities.
- `.github/`: Centralized CI/CD workflow pipelines.

## Development Environment Setup

### 1. Prerequisites

Ensure you have the following toolchains installed on your host system:

- **Rust Toolchain:** (v1.70 or newer)
- **CMake:** Required to compile the `nng` socket transport library from source.
- **ARM GCC Toolchain:** (`arm-none-eabi-gcc`) Required for building embedded firmware targets.

To install prerequisites on macOS:

```bash
brew install cmake gcc-arm-none-eabi
```

### 2. Workspace Commands

This monorepo utilizes a unified Cargo workspace. Run these commands from the root directory (`seamless-swarm/`):

- **Check all Rust projects:**
  ```bash
  cargo check
  ```
- **Run the test suite:**
  ```bash
  cargo test
  ```
- **Lint the codebase:**
  ```bash
  cargo clippy --workspace
  ```
- **Build optimized release targets:**
  ```bash
  cargo build --release
  ```
