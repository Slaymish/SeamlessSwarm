# Seamless Swarm: Engineering Execution Roadmap

This document outlines the phased roadmap and task checklist required to move the Seamless Swarm Computing Platform from an initial monorepo scaffold to a production-validated Minimum Viable Product (MVP).

---

## Phase 1: Transport & Zero-Config Discovery MVP

**Goal:** Establish low-latency link-local discovery and high-performance communication between host workstation nodes and the ARM appliance.

- [x] **mDNS Custom Responder:** Implement a native multicast-DNS socket listener in `host-background-agent` to advertise and resolve link-local addresses without a central DHCP server.
- [x] **NNG Communication Loop:** Implement raw TCP and NNG socket wrappers (`nng` crate) supporting request-reply patterns for authorization challenges and push-pull queues for task delivery.
- [x] **Packet-Drop Validation:** Benchmark node discovery join times under simulated enterprise network drops (up to 15% packet loss) to verify collision-backoff resilience.

---

## Phase 2: Hardware-Assisted Provisioning & Verification

**Goal:** Lock down the cryptographic boundary using ECDSA hardware challenge-response and local static thumbprint databases.

- [ ] **ATECC608 Driver Layer:** Replace the stub implementation in `firmware/node-key-secure/atecc608.c` with real I2C hardware communication targeting Linux `/dev/i2c-N`. Current code hardcodes a fake serial and generates signatures via XOR — no actual chip communication exists yet:
  - Add `int fd` bus handle to `atecc608_device_t`; open/close the I2C device file in `atecc608_init`
  - Implement ATECC608 wakeup-token and sleep/idle sequencing around every command group (required by the chip before any command)
  - Implement slot-parameterised Sign command using `ATECC608_KEY_SLOT`; `ATECC608_KEY_SLOT 0` is defined but currently unreferenced
  - Implement slot-parameterised GenKey command for key provisioning
  - Validate against real ATECC608A hardware on the Pi over I2C before PCB spin
- [ ] **CH552 HID Injection Firmware:** Implement the CH552G USB composite device firmware that triggers Scout on plug-in (flagged open item in hardware note — no CH552 firmware exists yet):
  - Present simultaneously as USB HID keyboard and USB mass storage to the host
  - Implement OS detection logic from USB enumeration signals to select the correct Scout script (`.sh` / `.command` / `.ps1`)
  - Inject keystrokes to open a terminal and execute the appropriate script from the mass storage partition
  - Fall back to an interactive menu if OS detection is ambiguous
  - Validate across Windows, macOS, and Linux hosts before hardware handoff
- [ ] **Secure Element Simulation Boundary:** Gate `SimulatedSecureElement` in `host-background-agent` behind a compile-time feature flag so the simulated key path (`simulated_node_key.der`) is unreachable in production builds. Real hardware builds must route through the ATECC608 I2C driver, not the software P-256 sim.
- [x] **Provisioning CLI Tool:** Expand `tools/provision-keys` to inject and lock private keys in Slot 0 during provisioning and output matched public key hex thumbprints.
- [x] **Verification Logic:** Integrate SHA-256 and ECDSA public-key signature verification in the `compute-module-core` authentication handler to validate high-entropy challenge responses against a local trusted thumbprint file.

---

## Phase 3: Profile-Driven Scheduler & Resource Registry

**Goal:** Implement active swarm orchestration with resilient failure isolation mapping to the three-tier task taxonomy.

- [x] **Dynamic Capability Discovery:** Implement system-level profiling (CPU cores, memory, OS platform, GPU accelerators) in `host-background-agent`'s Scout Engine and stream updates over NNG.
- [x] **Ephemeral Registry indexing:** Optimize the thread-safe `EphemeralRegistry` in `compute-module-core` to dynamically index and expire profiles upon heartbeat timeout.
- [x] **Capability-Driven Task Routing:** Tasks declare `required_capabilities` (e.g. `ffmpeg_execution`, `blender_execution`) in `TaskDefinition`. The scheduler filters the node pool to only capable nodes before dispatch; tasks without a matching node remain `Pending` until one appears. Covered by `test_capability_driven_dispatch`.
- [x] **Capability Execution Dispatch (Agent):** Agent probes each required capability against the real local environment before accepting a task — invoking actual binaries (`ffmpeg -version`, `blender --version`, app-bundle presence checks). Tasks failing the probe are rejected with a `Failed` progress report rather than silently executing.
- [ ] **Task Taxonomy Scheduling:**
  - **Stateless:** Distribute over parallel worker queues with immediate automated reassignment.
  - **Stateful:** Implement periodic metadata tracking with last-known snapshot recovery.
  - **Interactive:** Deliver via low-latency direct TCP streaming, bypassing retries and dropping immediately to trigger local host fallback upon packet loss.

---

## Phase 4: System Integration & Usability Testing

**Goal:** Validate entire swarm lifecycle, authentication bounds, and failure modes under stress-test conditions.

- [x] **E2E Swarm Simulator:** Develop a simulation script to spin up multiple mock host-agents, connect them to a simulated compute-module-core, and assert total capability indexing and scheduling.
- [x] **Fault-Injection Test Suite:** Run simulated physical disconnects of workstation nodes during active execution of Stateless, Stateful, and Interactive tasks to verify the automated recovery actions.
- [ ] **NAN Fallback Verification:** Test local ad-hoc communication fallback using physical proximity frames when access points strip multicast UDP packets.
