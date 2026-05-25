# ADR-002: Architectural Refinement of the Seamless Swarm Computing Platform

## Status

Accepted

## Date

2026-05-25

## Technical Story

This architecture decision record refines the physical, network, transport, and security topologies of the Seamless Swarm, succeeding the preliminary exploration phase documented in ADR-001. It establishes concrete engineering specifications to transition the platform's core coordination, local device discovery, communication transport, and physical node authentication into a production-grade, highly secure, local distributed computing network.

## Context and Problem Statement

The Seamless Swarm is designed as a localized computing paradigm that pools adjacent physical hardware capabilities into a unified execution layer. Access to this decentralized cluster is managed via pluggable physical hardware Node Keys, which register host resource profiles with an always-on Compute Module. In the initial system evaluation (ADR-001), five core structural assumptions were identified: reliance on physical key presence as the exclusive trust boundary, dynamic ephemeral capability discovery via an on-demand Scout Model, intent-level task routing, medium-adaptive host interfaces, and a centralized Compute Module coordinator.

However, several critical architectural risks and technical forks remained unresolved, threatening the stability and security of the platform :

- **Fork A (Compute Module Hosting)**: The system demands an always-on coordinator but lacks a designated physical environment. Relying on a software-elected node introduces extreme leader-election complexity, high topology volatility, and risk of entire swarm state loss if the host machine is shut down or unplugged.
- **Fork B (Transport Layer Overhead)**: The initial proposed distribution framework, Ray, is optimized for stable, low-latency, cloud-centric data center topologies with fixed IP allocations. In a highly volatile local mesh where client nodes join and leave constantly, Ray introduces severe control-store latency, heavy memory footprints, and restrictive Python-runtime dependencies.
- **Security and Physical Boundary Weaknesses**: The assumption that physical possession of a generic key equals membership is vulnerable to key cloning, physical theft, and unauthorized network injection, as no software-layer identity or hardware-enforced cryptographic validation is utilized.
- **Radio and Physical Environmental Constraints**: Operating over the high-frequency $6\text{ GHz}$ wireless spectrum restricts physical coverage to an indoor footprint of $15\text{--}30\,\text{m}$. Signal attenuation through partition walls causes frequent node dropouts and reconnections, demanding a highly resilient, zero-configuration local discovery and transport stack capable of handling rapid churn.

To address these vulnerabilities, this record establishes a unified architectural framework that secures the physical boundaries, minimizes communication latency, and provides deterministic execution and recovery mechanisms.

## Decision Drivers

The engineering trade-offs and protocol selections are governed by five primary decision drivers:

- **Topology Churn Resilience**: The transport and coordination layers must absorb frequent, unannounced node disconnections and reconnections without cluster state degradation or cascading timeouts.
- **Resource and CPU Efficiency**: Local background agents running on contributing host systems must operate with a negligible memory and CPU footprint to ensure that resource contribution does not degrade the host's primary user experience.
- **Zero-Configuration Local Operation**: The system must assemble and configure itself automatically in off-grid, air-gapped, or highly restricted local area networks (LANs) without requiring active DHCP servers, external internet access, or manual network mapping.
- **Cryptographic Swarm Integrity**: Physical possession of a Node Key must be bound to a hardware-enforced, uncloneable identity to prevent rogue device injection and spoofing attacks.
- **Profile-Driven Execution Determinism**: Task recovery and dispatching must adapt dynamically based on the specific capability profiles registered during device initialization.

## Considered Options

The engineering team evaluated several technical candidates across the three core subsystems requiring architectural resolution.

### Compute Module Hosting Topologies

The hosting topology dictates where the persistent orchestration layer resides and how swarm state is maintained.

| **Architectural Paradigm**                 | **Implementation Cost**                      | **Swarm Topology Impact**                        | **Failure Modes & Complexity**                                                                  | **State Management**                                                                   |
| ------------------------------------------ | -------------------------------------------- | ------------------------------------------------ | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| **Software-Elected Node (Consensus)**      | Low (utilizes existing host hardware)        | High instability during node departures          | Extremely high; requires complex consensus protocols (e.g., Raft) to handle split-brain events. | Volatile; state must be replicated across multiple transient nodes.                    |
| **Dedicated Hardware Hub (ARM Appliance)** | Medium (requires dedicated ARM hardware SKU) | Establishes a permanent, stable topology anchor. | Low; the hub operates as a dedicated, always-on single coordinator.                             | Persistent; live capability registry and task queue reside on dedicated local storage. |

### Local Discovery and Transport Protocol Candidates

The discovery and transport layer must facilitate zero-configuration device pairing and ultra-low-latency task dispatching over volatile local connections.

| **Protocol Stack**        | **Latency Profile (Payloads <64 KB)**                            | **Memory & CPU Footprint**                            | **Thread Safety & Concurrency**                                        | **Local Network Compatibility**                                        |
| ------------------------- | ---------------------------------------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| **Ray Cluster Framework** | High overhead; unsuitable for local mesh.                        | High; heavy Python runtime dependency.                | Thread-safe, but managed via a complex global control store.           | Poor; optimized for static data center IPs, not local high-churn LANs. |
| **gRPC over HTTP/2**      | Moderate; HTTP/2 framing introduces parsing overhead.            | Moderate; requires structured Protobuf serialization. | High; native HTTP/2 multiplexing simplifies concurrent streaming.      | Highly dependent on stable IP routing and DNS configuration.           |
| **ZeroMQ**                | Very low; optimized for high-throughput messaging.               | Low; efficient C++ core implementation.               | Poor; sockets are not thread-safe and require external mutex wrapping. | Good; peer-to-peer transport abstractions over raw sockets.            |
| **Nanomsg (NNG)**         | Lowest; streamlined internal mechanism for small control frames. | Lowest; ultra-lightweight and highly efficient.       | High; designed with built-in thread safety and simplified APIs.        | Excellent; supports zero-configuration Scalability Protocols (SP).     |

### Node Key Security and Identity Paradigms

Security paradigms define how the platform authenticates joining nodes and mitigates the risk of key cloning or physical theft.

| **Security Paradigm**                  | **Key Cloning Risk**                                            | **Network Access Control**                                  | **Hardware Cost Impact**                                            | **Identity Mechanism**                                                     |
| -------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| **Physical Possession Only**           | Critical; raw keys can be cloned or duplicated.                 | No authentication beyond basic physical presence detection. | Zero additional cost; uses generic USB storage or serial interface. | No hardware or software-layer identity exists.                             |
| **Software-Based TLS Certificates**    | High; private keys can be extracted from host OS filesystems.   | Standard TLS handshake validates joining nodes.             | Zero; relies on software cryptographic implementations.             | Host-bound software certificates; easily invalidated by OS reinstallation. |
| **Hardware Secure Element (ATECC608)** | Negligible; private keys are isolated in tamper-proof hardware. | Strong ECDSA challenge-response validation.                 | Low (appends a micro-dollar secure chip to the Node Key BOM).       | Guaranteed unique 72-bit serial number and hardware-locked keys.           |

## Decision Outcome

The Seamless Swarm platform adopts a dedicated, low-power ARM-based hardware appliance to host the always-on Compute Module coordinator, establishing a permanent, local topological anchor. The heavy, cloud-centric Ray cluster framework is rejected in favor of a dual-layer local discovery stack combining Multicast DNS (mDNS/DNS-SD) with link-layer Wi-Fi Neighbor Awareness Networking (NAN) fallback to ensure seamless device pairing in complex network topologies. Message transport is standardizing on Nanomsg (NNG) for all control, orchestration, and status frames under $64\text{ KB}$ to minimize overhead and CPU consumption on transient nodes.

To secure the physical boundary, every physical Node Key must integrate an on-board Microchip ATECC608 secure element, enforcing Elliptic Curve Digital Signature Algorithm (ECDSA) challenge-response authentication before a node's dynamic capabilities are registered by the Compute Module. Task execution and fault recovery are managed by the Compute Module utilizing a dynamic, profile-declared task failure taxonomy.

## Technical Design and Mechanism

### Compute Module Coordination and Persistence

The Compute Module operates as a dedicated, low-power ARM-class hardware appliance, acting as the centralized coordinator and state-preservation hub for the swarm. Because this appliance focuses exclusively on lightweight orchestration, capability indexing, and task dispatching rather than raw computation, it runs continuously with an extremely low thermal and power envelope. The appliance maintains the live capability registry, coordinates the system's global task queue, and executes recovery routines, ensuring that the swarm's state remains intact even if all transient client machines depart. This dedicated hardware approach eliminates the latency and synchronization overhead of multi-node leader-election protocols.

### Zero-Configuration Local Discovery Network Stack

Device pairing and network discovery are achieved through a combination of Multicast DNS (mDNS) and DNS Service Discovery (DNS-SD), allowing nodes to assemble automatically without manual network administration.

#### Technical Parameters

- **Multicast Destination IPv4**: $224.0.0.251$
- **Multicast Destination IPv6**: $\text{FF02}::\text{FB}$
- **UDP Port**: $5353$
- **Addressing**: Self-assigned Link-Local addressing ($169.254.0.0/24$ for IPv4 or $\text{FE80}::/10$ for IPv6) when local DHCP infrastructure is absent, achieving complete zero-configuration networking.

Upon insertion of a Node Key, the background agent on the host machine initializes its local mDNS responder. The agent broadcasts a query seeking active services under the `.local` domain. To prevent naming collisions when multiple client nodes concurrently join the swarm, the mDNS responder executes automatic hostname conflict resolution. If a node attempts to register the hostname `node.local` and detects an existing record, the responder increments a numerical suffix, dynamically reassigning the hostname to `node-2.local`.

In dense physical environments, such as professional multi-room recording studios, traditional mDNS multicasting can suffer from access point isolation or high background packet noise. To guarantee reliable node discovery, the architecture implements a physical fallback utilizing the Wi-Fi Neighbor Awareness Networking (NAN) protocol (Wi-Fi Aware). Wi-Fi Aware operates directly at the link layer, enabling nearby devices to discover each other and exchange low-throughput synchronization beacons independent of any local IP routing infrastructure. This dual-layer discovery approach ensures that the "physical proximity" membership boundary remains operational, even in complex, radio-opaque or multi-floor studio deployments.

Enterprise-grade networks often deploy multi-AP mesh systems (e.g., TP-Link Deco) that segregate or drop multicast traffic across backhaul nodes to minimize broadcast storms, or enforce guest network device isolation, which blocks traditional mDNS traffic. In such environments, transitioning the mesh system to Access Point (AP) mode bridges the subnets into a flat, unsegmented Layer 2 network, restoring full mDNS discovery paths. For networks where AP mode cannot be configured, the background agent utilizes an mDNS reflector or proxy (such as Avahi or mDNSResponder) on the host's primary gateway to route discovery packets across virtual local area network (VLAN) boundaries.

### Low-Latency Transport Mesh via Nanomsg (NNG)

The communication backbone of the Seamless Swarm is built on Nanomsg (NNG), replacing the high-overhead, cloud-oriented Ray framework. While gRPC provides structured serialization and HTTP/2 multiplexing, its protocol overhead and CPU demands are ill-suited for resource-constrained local coordinators and mobile client nodes. ZeroMQ provides exceptional raw throughput, but its lack of native thread safety requires developers to implement complex mutex wrapping and isolate sockets to single threads, increasing implementation complexity.

NNG solves these limitations by offering built-in thread safety and a streamlined API, enabling high-performance parallel task execution. Crucially, local swarm operations (including capability registration and command dispatch) rely on small control payloads typically under $64\text{ KB}$. Technical benchmarks demonstrate that Nanomsg achieves the lowest latency, highest throughput, and lowest CPU utilization for payloads below this $64\text{ KB}$ threshold.

For larger payloads, such as binary model weights or raw assets, the Compute Module dynamically falls back to ZeroMQ or point-to-point TCP streams to maintain throughput efficiency, while NNG remains dedicated to control and message routing. NNG's native support for Scalability Protocols (SP) allows the Compute Module to maintain high-frequency heartbeats and direct peer-to-peer message routing without intermediate server brokers.

### Hardware-Enforced Cryptographic Root of Trust

To secure the physical boundary, the system replaces unverified token possession with a hardware root of trust. Every physical Node Key dongle integrates an on-board Microchip ATECC608 secure element (specifically the ATECC608B or ATECC608C chip) communicating with the host node via I2C or Single-Wire Interface (SWI). This secure element features a standby current of $0.03\,\mu\text{A}$ and an active sleep current of less than $150\,\text{nA}$, ensuring zero impact on the battery life of connected mobile hosts.

The ATECC608 provides hardware-isolated key storage, protecting private keys against physical tampering, logical side-channel analysis, and host-level software exploits. All cryptographic calculations, including Elliptic Curve Digital Signature Algorithm (ECDSA) sign-verify operations, are executed directly inside the hardware co-processor using the NIST standard P-256 elliptic curve.

```
+------------------------+                     +------------------------+
|   Compute Module Hub   |                     | Client Node (Host Key) |
+-----------+------------+                     +-----------+------------+
            |                                              |
            | ----- 1. Send High-Entropy Challenge ----->  |
            |                                              |  -- 2. Forward to SWI --+
            |                                              |                         |
            |                                              |  <3. Sign (ECDSA P256) +
            |                                              |     via ATECC608 Sec El.
            | <----- 4. Return Digital Signature --------- |
            |                                              |
            | -- 5. Verify against Local static thumbprint |
            |                                              |
            | ===== 6. Derives Session Key via ECDH ====== |
            |                                              |
```

Upon insertion of the Node Key, the Scout Model establishes a connection to the Compute Module and initiates the cryptographic handshake :

1. **Challenge Generation**: The Compute Module generates a high-entropy, random challenge token.
2. **Hardware Signature**: The client host relays the challenge to the ATECC608 secure element. The chip signs the token using its internally locked private key and returns the ECDSA signature to the host.
3. **Authentication Verification**: The host transmits the signature back to the Compute Module. The Compute Module verifies the signature against pre-provisioned static thumbprint certificates associated with the Node Key's unique, guaranteed 72-bit serial number.
4. **Key Derivation and Encryption**: Following verification, the devices utilize Elliptic Curve Diffie-Hellman (ECDH) key agreement to derive an ephemeral AES-128 session key, encrypting all subsequent Nanomsg transport frames.

On Linux systems, the host background agent interfaces with the secure element via the standard PKCS#11 API, ensuring driver portability across a wide array of host distributions. Utilizing pre-provisioned TrustFLEX or Trust&GO device profiles avoids the logistical complexity of shipping raw private keys and eliminates the need for expensive third-party Certificate Authorities, keeping the system fully local and cost-efficient.

### Profile-Driven Task Failure Taxonomy

To maintain high fault tolerance without the synchronization overhead of a distributed database, the Compute Module's scheduler manages task execution using a three-tier recovery taxonomy. When a node registers via the Scout Model, its capability profile explicitly declares which recovery protocol is applied to its exported capabilities.

| **Task Classification**       | **Performance Profile**                                | **Recovery Protocol**                                                                                                                                                                                | **Example Swarm Operation**                                             |
| ----------------------------- | ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| **Stateless & Idempotent**    | Low sensitivity to latency; highly parallelizable.     | **Automated Retry**: Upon node departure or packet loss, the task is re-queued and dispatched to an alternative node with matching capabilities.                                                     | Image format transcoding; local data filtering.                         |
| **Stateful & Long-Running**   | High resource commitment; state-dependent.             | **Checkpoint & Resume**: Nodes periodically push progress snapshots to the Compute Module. On failure, the task is reassigned and resumes from the last known checkpoint.                            | 3D scene rendering; large-scale local compilation.                      |
| **Interactive & Low-Latency** | Extreme latency sensitivity; zero tolerance for delay. | **Immediate Error Surfacing**: Retries are bypassed. The system terminates the execution stream and surfaces a connection error to the host application instantly to allow immediate local fallback. | Real-time multi-channel audio routing; interactive interface rendering. |

## Consequences

The transition to a dedicated Compute Module, an mDNS/NNG-mediated transport stack, and a hardware-enforced cryptographic root of trust introduces several structural, operational, and financial trade-offs.

### Positive Consequences

- **Consensus Elimination**: Deploying a dedicated ARM appliance for the Compute Module eliminates the need for complex, high-overhead leader-election and consensus algorithms on transient client devices, removing a significant source of cluster instability.
- **Resource Preservation on Contributed Nodes**: Replacing Ray with Nanomsg dramatically reduces the idle memory and CPU footprint of background agents on host devices, ensuring that resource contribution does not degrade local workstation performance.
- **Uncloneable Device Identity**: Integrating the ATECC608 secure element completely secures the swarm against key cloning, token duplication, and unauthorized network injection, establishing a rigorous physical security boundary.
- **Off-Grid Operational Independence**: Relying on pre-provisioned static thumbprint certificates and local ECDSA verification allows the system to remain fully secure, authenticated, and operational in off-grid, air-gapped, or highly restricted environments without external cloud connectivity.
- **Tailored Low-Latency Transport**: Optimizing the network stack with Nanomsg for control payloads under $64\text{ KB}$ guarantees sub-millisecond control latency, which is essential for responsive real-time swarm coordination.

### Negative Consequences

- **Increased Hardware Complexity and BOM**: Requiring a dedicated ARM appliance for the Compute Module and embedding ATECC608 secure element chips inside every Node Key increases the Bill of Materials (BOM) cost and forces the manufacturing team to manage multiple physical hardware SKUs.
- **Enterprise Network Multicast Restrictions**: In restricted corporate network environments, mDNS multicast packets may be actively blocked by routers or access points, requiring manual configuration of mDNS reflectors or relying on the slower, link-layer Wi-Fi Aware (NAN) fallback.
- **Custom Scheduling Maintenance**: Moving away from Ray forces the software engineering team to implement, optimize, and maintain a custom lightweight scheduler and task-routing engine within the Compute Module.

### Neutral Consequences

- **Accepted Spectrum Limitations**: Operating the local mesh over the $6\text{ GHz}$ wireless spectrum limits the physical range of the swarm to an indoor footprint of $15\text{--}30\,\text{m}$ due to wall attenuation. This physical range limitation is accepted as a beneficial constraint that reinforces physical presence as the ultimate membership barrier.
- **Local Software Licensing Policy**: Because capabilities are registered dynamically, task routing must respect software license agreements. Tasks involving licensed software (e.g., proprietary audio engines) are constrained by policy to run only on nodes owned by the active license holder to prevent compliance violations.

## Validation and Confirmation Mechanisms

To verify the integration and performance of the updated swarm architecture, the system must undergo three rigorous automated validation protocols:

### Local Network Recovery and Discovery Test

The stability of the mDNS discovery and Nanomsg transport layer must be validated under continuous stress testing to simulate extreme network churn. The system must maintain a stable connection state and execute automatic conflict resolution under three distinct operational phases:

1. **Initial Boot**: On key insertion, host discovery, cryptographic challenge-response, and capability registration must complete within $2.0\,\text{s}$.
2. **Intermediate Check (10 Minutes)**: The mesh must withstand simulated packet loss up to $15\%$ without dropping active nodes from the registry.
3. **Continuous Operation (24 Hours)**: The system must run under continuous synthetic task load for 24 hours, verifying that no socket leaks, memory fragmentation, or unhandled naming collisions occur.

### Physical Signal Attenuation and Churn Test

The swarm must undergo range and attenuation validation over the $6\text{ GHz}$ wireless spectrum. Client nodes must be physically moved through the target deployment environment to map performance at the physical boundaries ($15\text{--}30\,\text{m}$). The Compute Module scheduler must successfully execute graceful node eviction within $500\,\text{ms}$ of signal loss, and successfully trigger automatic task reassignment via the profile-declared failure taxonomy.

### Cryptographic Security Challenge

To confirm the effectiveness of the hardware root of trust, the security team must execute an automated penetration suite. The Compute Module must be subjected to connection attempts from simulated, cloned, and blank Node Keys. The system must successfully reject any connection that fails to provide a valid, hardware-signed ECDSA token generated by an authorized ATECC608 secure element, immediately blocking network access and logging the unauthorized attempt.
