# Seamless Swarm: Engineering Execution Roadmap

This document outlines the phased roadmap and task checklist required to move the Seamless Swarm Computing Platform from an initial monorepo scaffold to a production-validated Minimum Viable Product (MVP).

---

## Phase 1: Transport & Zero-Config Discovery MVP

**Goal:** Establish low-latency link-local discovery and high-performance communication between host workstation nodes and the ARM appliance.

- [x] **mDNS Custom Responder:** Implement a native multicast-DNS socket listener in `host-background-agent` to advertise and resolve link-local addresses without a central DHCP server.
- [x] **NNG Communication Loop:** Implement raw TCP and NNG socket wrappers (`nng` crate) supporting request-reply patterns for authorization challenges and push-pull queues for task delivery.
- [ ] **Packet-Drop Validation:** Benchmark node discovery join times under simulated enterprise network drops (up to 15% packet loss) to verify collision-backoff resilience.

---

## Phase 2: Hardware-Assisted Provisioning & Verification

**Goal:** Lock down the cryptographic boundary using ECDSA hardware challenge-response and local static thumbprint databases.

- [ ] **ATECC608 Driver Layer:** Finalize the bare-metal C SWI/I2C communication library in `firmware/node-key-secure` to execute hardware-locked private key operations.
- [x] **Provisioning CLI Tool:** Expand `tools/provision-keys` to inject and lock private keys in Slot 0 during provisioning and output matched public key hex thumbprints.
- [x] **Verification Logic:** Integrate SHA-256 and ECDSA public-key signature verification in the `compute-module-core` authentication handler to validate high-entropy challenge responses against a local trusted thumbprint file.

---

## Phase 3: Profile-Driven Scheduler & Resource Registry

**Goal:** Implement active swarm orchestration with resilient failure isolation mapping to the three-tier task taxonomy.

- [x] **Dynamic Capability Discovery:** Implement system-level profiling (CPU cores, memory, OS platform, GPU accelerators) in `host-background-agent`'s Scout Engine and stream updates over NNG.
- [x] **Ephemeral Registry indexing:** Optimize the thread-safe `EphemeralRegistry` in `compute-module-core` to dynamically index and expire profiles upon heartbeat timeout.
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
