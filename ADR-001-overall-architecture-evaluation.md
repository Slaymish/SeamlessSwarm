# ADR-001: Seamless System — Overall Architecture Evaluation

**Status:** Proposed  
**Date:** 2026-05-25  
**Deciders:** Seamless System founding team  
**Stage:** Early exploration

---

## Context

Seamless System is a local swarm computing platform that pools the compute power and installed capabilities of nearby devices into a unified layer, accessible through pluggable Access Keys. A hardware Node Key connects each contributing machine to the swarm; a Scout Model runs at plug-in time to discover and register capabilities; a Compute Module acts as the always-on orchestrator.

This ADR evaluates the overall architecture at early exploration stage — not to choose between two concrete options, but to identify the structural bets being made, the risks they carry, and the questions that must be resolved before design can solidify.

---

## The Core Architectural Bets

The design makes several deliberate, load-bearing choices that shape everything downstream:

1. **Physical proximity as the trust and membership boundary.** The dongle is the credential. Presence of the key = membership; absence = eviction. There is no software-layer identity beyond this.
2. **Ephemeral capability state.** Nothing about a node's capabilities is persisted on the key or in the cloud. The Scout Model re-discovers everything fresh at each plug-in.
3. **Intent-level routing.** Users issue requests; the Compute Module translates those into node assignments. Users never specify where tasks run.
4. **Medium-adaptive presentation.** The same swarm presents differently per device type via Medium Profile Software.
5. **The Compute Module as the single coordinator.** One always-on component holds the live capability registry, the task queue, and recovery logic.

These are the right bets for the product vision. The risks below are not arguments against the design — they are the implications of these choices that must be handled explicitly.

---

## Options Considered

*This is an evaluation, not a two-option decision. The "options" here are the unresolved forks within the existing design.*

### Fork A: Compute Module as dedicated hardware vs. elected node

The spec describes the Compute Module as "always-on" but does not specify what it runs on. This is the most consequential unresolved question in the architecture.

| Dimension | Dedicated hardware | Elected node (e.g. first Node Key plugged in) |
|---|---|---|
| Reliability | High — no dependency on member nodes | Low — drops if that machine leaves |
| Cost | Adds a SKU; adds to BOM | No extra hardware |
| Bootstrap complexity | Simple — known address | Requires leader election protocol |
| User mental model | Clear ("the hub") | Confusing if the "hub" walks out the room |

**Recommendation:** Dedicated hardware. The "always-on" framing in the spec implies this, and the user experience of a swarm that degrades or loses state when a laptop leaves the room is a serious product risk. A low-power ARM device (Raspberry Pi class) is sufficient — the Compute Module does orchestration, not computation.

---

### Fork B: Underlying distribution layer — Ray vs. alternatives

The spec identifies Ray as the strongest candidate. This deserves scrutiny.

| Dimension | Ray | libp2p + custom scheduler | ZeroMQ / nanomsg |
|---|---|---|---|
| Maturity | High (production at scale) | Medium | High |
| Local mesh fit | Poor — designed for cloud/cluster; assumes stable topology | Good — built for dynamic peer networks | Medium |
| Python lock-in | Yes | No (Go/Rust implementations) | No |
| Fault tolerance | Built-in (actor model, object store) | DIY | DIY |
| Task checkpointing | Supported | Manual | Manual |
| Community / docs | Excellent | Good | Good |
| Topology assumption | Stable IPs, low churn | High churn, dynamic peers | Point-to-point |

**Recommendation:** Ray is a poor fit for a high-churn local mesh. Its design assumes relatively stable cluster membership; Seamless nodes join and leave constantly. The overhead of Ray's GCS (Global Control Store) and object store is also poorly matched to a local LAN where tasks may complete in milliseconds.

A more appropriate stack would be: **mDNS or a custom 6GHz broadcast beacon** for discovery, **gRPC or nanomsg** for task dispatch, and a **custom lightweight scheduler** in the Compute Module. The fault tolerance Ray provides can be replicated at the application layer with much less complexity than adopting a framework built for different constraints.

This is the highest-priority technical decision to resolve, as it affects the Compute Module, the Node Key software, and the on-wire protocol.

---

### Fork C: Task failure handling — retry vs. surface error

The spec notes this as an open question. Both approaches are valid; the right answer depends on task type.

| Task type | Recommended approach |
|---|---|
| Idempotent, stateless (e.g. image conversion) | Retry automatically on another capable node |
| Stateful or long-running (e.g. 3D render in progress) | Checkpoint + resume if possible; surface error if not |
| Interactive / low-latency (e.g. audio routing) | Surface immediately — retry latency is unacceptable |

**Recommendation:** Implement a task taxonomy (stateless / checkpointed / interactive) that the capability profile declares per capability. The Compute Module applies the appropriate recovery strategy per task type. This avoids a one-size-fits-all policy that will fail at least one important use case.

---

## Risks and Gaps

The following are structural risks in the current design, ordered by severity.

### 🔴 Critical

**1. The Compute Module has no defined hardware home.**  
As above — this must be resolved before any other part of the architecture can be validated. If it is dedicated hardware, the product has a third SKU. If it is software-elected, the spec needs a leader election design.

**2. Capability composition is underspecified.**  
"A task that needs capabilities spread across multiple nodes is distributed automatically" is the hardest sentence in the spec to implement. Multi-node task decomposition requires: a way to split a task into sub-tasks, an interface contract between sub-tasks, and a way to reassemble results. This is a significant research and engineering problem. The spec treats it as solved. It is not.

**3. No security model beyond physical possession.**  
If a Node Key is lost or cloned, the attacker has full swarm access. There is no described revocation mechanism, no per-session authentication beyond the credential on the dongle, and no audit trail. For a system that can access licensed software and route arbitrary computation, this is a meaningful attack surface.

### 🟡 Significant

**4. Scout Model has no handling for licensed software.**  
Flagged as an open question in the spec. A capability profile that includes `audio-production` because Ableton is installed does not prove Ableton will run for the task requestor. Running licensed software on behalf of a remote user likely violates EULAs. This may not be solvable technically — it may require a policy decision (only route tasks initiated from the same machine that owns the license).

**5. 6GHz range is highly variable.**  
Wi-Fi 6E at 6GHz has a practical indoor range of 15–30 metres and is significantly attenuated by walls. "Physical proximity" as a membership criterion maps poorly to multi-room or multi-floor environments. A studio spanning two floors, for example, may have inconsistent membership. The spec should define the intended physical deployment footprint so the radio design can be validated against it.

**6. The Scout Model's privacy surface is undefined.**  
The Scout Model inspects installed software, available hardware, and exposed services on the host machine. What data is transmitted to the Compute Module? What stays local? For a Node Key that users plug into their personal machines, the privacy properties of capability discovery need to be explicit — both for user trust and for regulatory reasons in some markets.

**7. Bootstrap / discovery is not described.**  
How does a fresh Node Key find the Compute Module on the 6GHz mesh? If the Compute Module is dedicated hardware, its IP or mesh address must be discoverable. The spec assumes connection is established but does not describe the handshake. mDNS, a fixed SSID/BSSID, or a proprietary beacon protocol are all options — each has trade-offs in setup complexity and robustness.

### 🟢 Lower priority (but worth noting)

**8. Medium profiles are fixed per device type, but devices vary.**  
Two "Standard Monitor / TV" Access Keys may be connecting to very different devices — a 4K gaming monitor and a 1080p office display. The profile system as described serves the device category, not the device instance. This may be fine for v1 but will constrain the experience for power users.

**9. Multi-user / multi-request concurrency is not addressed.**  
Can two users issue requests to the same swarm simultaneously? If the swarm is in a shared space (studio, office), contention is likely. The Compute Module will need a scheduling policy — FIFO, priority, fair-share — that the spec does not describe.

**10. Node Key software maintenance.**  
The spec mentions "one-time setup per machine; thereafter runs as a background service." Background services require updates. How are Node Key software updates delivered and applied? A stale Node Key agent on a machine could register incorrect capabilities or fail to communicate with an updated Compute Module.

---

## Consequences

Proceeding with this architecture as-is:

**Becomes easier:**
- Hardware-gated membership is simple to reason about and removes the need for user accounts, passwords, or cloud identity
- Medium profiles give a clean extensibility story for new device categories
- Ephemeral capability state means no sync problems and no stale registry entries

**Becomes harder:**
- Capability composition across nodes will require significant design work before any multi-node tasks can be built
- Security incidents (lost key, compromised node) have no recovery path under the current model
- The product has a minimum of two hardware SKUs (Node Key, Access Key) and likely a third (Compute Module device)

**Must be revisited before build:**
- Compute Module hardware decision
- Underlying communication/scheduling stack (replace Ray assumption)
- Task taxonomy and recovery strategy
- Security / revocation model

---

## Action Items

1. [ ] Decide whether the Compute Module is dedicated hardware or software-elected — this gates all other design work
2. [ ] Prototype the node discovery and capability registration flow end-to-end using mDNS + gRPC (before committing to Ray)
3. [ ] Define the task decomposition interface: what does a "composable" capability look like, and what does the contract between sub-tasks on different nodes look like?
4. [ ] Write a threat model for the physical credential security model — enumerate loss/theft/clone scenarios and define acceptable mitigations
5. [ ] Define the Scout Model's data boundary: list exactly what is transmitted to the Compute Module vs. kept on the host
6. [ ] Resolve the licensed software policy question — likely a legal/product decision, not purely technical
7. [ ] Specify the intended deployment environment (room size, wall count, floor count) to validate the 6GHz radio assumption
8. [ ] Draft a concurrency model for the Compute Module's scheduler (how are simultaneous requests from multiple users or devices handled?)
