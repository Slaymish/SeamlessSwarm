# Seamless Swarm Monorepo

Welcome to the monolithic repository for the Seamless Swarm Computing Platform. This repository contains all hardware, firmware, and software designs, services, and utilities.

## Repository Structure

- `proto/`: Shared wire protocol contracts (protobuf definitions).
- `hardware/`: Electrical schematics and layouts for the hardware appliance hub and key dongles.
- `firmware/`: Bare-metal/RTOS code for embedded systems (e.g. Node Key Dongle ATECC608 logic).
- `software/`: Rust-based core orchestrator, cross-platform host agents, and profiles.
- `tools/`: Cryptographic provisioning and factory testing CLI utilities.
- `.github/`: Centralized CI/CD workflow pipelines.
