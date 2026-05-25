# Open Questions & Architectural Unresolveds

This document tracks active open questions and unresolved architectural parameters for the Seamless Swarm Computing Platform. As each question is formally resolved through engineering validation, it must be documented as an Architecture Decision Record (ADR-003, ADR-004, etc.) and implemented.

---

## 1. Network Fallback & Wi-Fi NAN Implementation

- **Question:** If corporate or enterprise router configurations drop/strip multicast UDP packets (preventing standard mDNS discovery on Port 5353), how will the background agent negotiate physical proximity connection using Wi-Fi Neighbor Awareness Networking (NAN) link-layer frames?
- **Implication:** Affects cross-platform network APIs (macOS CoreWLAN, Linux wpa_supplicant, Windows Wi-Fi APIs).
- **Resolution Criteria:** Successful transmission of capability frames on a segregated subnet via physical Wi-Fi interfaces.

## 2. ATECC608 Serial Bus Interface: SWI vs I2C

- **Question:** Which communication interface will be routed on the production USB-C Dongle footprint for the ATECC608 device? Single Wire Interface (SWI) reduces pin count but increases timing sensitivity; I2C is standard but requires routing serial data, clock, and pull-ups.
- **Implication:** Direct impact on ECAD schematics (`node-key-dongle`) and embedded C HAL (`node-key-secure`).
- **Resolution Criteria:** Validation of board space, power metrics, and timing reliability on pre-production evaluation rigs.

## 3. Stateful Task Checkpointing Mechanism & Storage

- **Question:** What serialization protocol and persistence model will be used for stateful task checkpoints (e.g., rendering progress frames)? Should the background agent stream checkpoints back to the Compute Module Hub over NNG, or save them to a local scratch partition?
- **Implication:** Affects bandwidth overhead on high-frequency state updates.
- **Resolution Criteria:** Benchmarking storage overhead and network saturation under a 50MB state update load.

## 4. Swarm Heartbeat Interval & Split-Brain Mitigation

- **Question:** What is the optimal heartbeat interval for workstation nodes, and what is the timeout threshold before a node is marked as offline? How will the centralized orchestrator handle split-brain partitions if a group of nodes can see each other but lose contact with the Hub?
- **Implication:** Impacts overall system scheduling stability and task reassignment latency.
- **Resolution Criteria:** Modeling false-positive node departures against network jitter under synthetic traffic load.
