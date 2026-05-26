# Open Questions & Architectural Unresolveds

This document tracks active open questions and unresolved architectural parameters for the Seamless Swarm Computing Platform. As each question is formally resolved through engineering validation, it must be documented as an Architecture Decision Record and linked here.

---

## 1. Network Fallback & Wi-Fi NAN Implementation

- **Status:** Open
- **Question:** If corporate or enterprise router configurations drop/strip multicast UDP packets (preventing standard mDNS discovery on Port 5353), how will the background agent negotiate physical proximity connection using Wi-Fi Neighbor Awareness Networking (NAN) link-layer frames?
- **Implication:** Affects cross-platform network APIs (macOS CoreWLAN, Linux wpa_supplicant, Windows Wi-Fi Direct / Wi-Fi APIs).
- **Resolution Criteria:** Successful transmission of capability frames on a segregated subnet via physical Wi-Fi interfaces, validated on all three target platforms.

---

## 2. ATECC608 Serial Bus Interface: SWI vs I2C [PROTOTYPE DECISION MADE — PRODUCTION OPEN]

- **Status:** Partially resolved
- **Prototype decision:** I2C is used for the prototype PCB spin. Standard, debuggable, well-supported across Linux/macOS/Windows host stacks via PKCS#11. Pull-ups routed accordingly.
- **Production question:** Whether to migrate to SWI for reduced pin count and smaller PCB footprint on the production dongle remains unresolved.
- **Implication:** SWI reduces routing complexity at the cost of increased timing sensitivity and less mature toolchain support. Decision gates production ECAD.
- **Resolution Criteria:** Validation of board space savings, timing reliability, and HAL portability on pre-production evaluation rigs.

---

## 3. Stateful Task Checkpointing Mechanism & Storage

- **Status:** Open
- **Question:** What serialization protocol and persistence model will be used for stateful task checkpoints (e.g., rendering progress frames)? Should the background agent stream checkpoints back to the Compute Module Hub over NNG, or write them to a local scratch partition on the contributing node?
- **Implication:** Streaming over NNG keeps the Hub as the single source of truth but adds bandwidth overhead on high-frequency state updates. Local scratch is faster but means state is lost if the node drops before a sync. Affects the Stateful & Long-Running recovery path defined in ADR-002.
- **Resolution Criteria:** Benchmarking storage overhead and network saturation under a 50MB state update load on a representative 6GHz link.

---

## 4. Swarm Heartbeat Interval & Split-Brain Mitigation [RESOLVED]

- **Status:** Resolved — ADR-003
- **Resolution:** 2s heartbeat interval; 5s eviction timeout (2 missed heartbeats + 1s jitter buffer). Split-brain handled by local autonomy fallback + randomised exponential backoff mDNS re-discovery (1s–30s). Task reallocation triggered immediately on eviction via recovery action matrix.
- **Reference:** ADR-003: Swarm Heartbeat and Eviction Mechanics

---

## 5. Compute Module Hosting Topology [RESOLVED]

- **Status:** Resolved — ADR-002
- **Resolution:** Dedicated ARM appliance (Raspberry Pi 5 class for prototype). Eliminates leader election complexity and swarm state volatility. Always-on, single coordinator. Not a contributing node — orchestration only.
- **Reference:** ADR-002: Architectural Refinement

---

## 6. Transport and Distribution Stack [RESOLVED]

- **Status:** Resolved — ADR-002
- **Resolution:** Ray rejected. Stack is mDNS for zero-config discovery + NNG (Nanomsg Next Generation) for sub-millisecond control transport. Custom lightweight scheduler in `compute-module-core`. Wi-Fi NAN as enterprise network fallback (see Open Question 1).
- **Reference:** ADR-002: Architectural Refinement

---

## 7. Swarm Credential Security Model [RESOLVED]

- **Status:** Resolved — ADR-002
- **Resolution:** ATECC608A secure element embedded in every Node Key and Access Key. ECDSA challenge-response on connect. ECDH-derived ephemeral AES-128 session keys for all NNG frames. Hub holds static thumbprint certificates per device serial. Trust&GO / TrustFLEX pre-provisioning — no external CA required, fully off-grid operable.
- **Reference:** ADR-002: Architectural Refinement

---

## 8. Task Failure Taxonomy [RESOLVED]

- **Status:** Resolved — ADR-002
- **Resolution:** Three-tier taxonomy declared per capability in the Scout Model profile: Stateless/Idempotent (auto-retry), Stateful/Long-Running (checkpoint + resume), Interactive/Low-Latency (immediate error surface, no retry). Recovery strategy applied by Compute Module scheduler per task class.
- **Reference:** ADR-002: Architectural Refinement

---

## 9. OS Detection and HID Injection Reliability [NEW]

- **Status:** Open
- **Question:** The CH552G on the Node Key dongle injects keystrokes to open a terminal and execute the correct Scout script (`.sh` / `.command` / `.ps1`) for the host OS. The injection sequence must be reliable across Windows 10/11, macOS 13+, and major Linux desktop environments (GNOME, KDE, etc.). What is the detection mechanism, and how are edge cases (locked screen, non-default shell, restricted execution policy on Windows) handled?
- **Implication:** If injection fails silently, the node never registers with the Hub and the user has no feedback. Determines reliability of the zero-install onboarding flow.
- **Resolution Criteria:** Successful Scout script execution on a representative matrix of host OS versions and configurations without manual intervention.

---

## 10. 6GHz Regulatory Compliance — NZ and Target Markets [NEW]

- **Status:** Resolved
- **Question:** 6GHz Wi-Fi operation in New Zealand and other target markets is subject to indoor-only unlicensed rules and in some jurisdictions AFC (Automated Frequency Coordination) requirements. Does the AX210-based prototype operate within the permitted indoor low-power (LPI) rules in NZ, AU, and relevant EU/US markets without AFC?
- **Implication:** If AFC is required in any target market, the dongle hardware and Hub AP configuration must support it. Affects antenna design and regulatory approval path for any commercial release.
- **Resolution Criteria:** Confirm NZ RSM / MBIE spectrum rules for 6GHz LPI indoor operation. Map against target market regulatory requirements before putting prototype hardware in front of external users.
- **Resolution:** The AX210 prototype operates within NZ indoor LPI rules. AFC is not required in NZ for indoor operation. The prototype is compliant with NZ regulations.

---

## 11. Licensed Software Routing Policy [NEW]

- **Status:** Open — flagged as policy decision, not purely technical
- **Question:** The Scout Model registers capabilities based on installed software (e.g. Ableton Live → `audio-production`). Routing a task to that capability on behalf of a remote user likely violates the software's EULA. ADR-002 notes this is constrained by policy to license-holder-only execution, but the enforcement mechanism is not defined.
- **Implication:** Without enforcement, the swarm may route licensed software tasks to nodes where the requestor is not the license holder. Legal and compliance risk for any commercial deployment.
- **Resolution Criteria:** Define the policy enforcement layer — whether capability registration for licensed software requires a local attestation step, or whether it is simply excluded from remote routing by default.
