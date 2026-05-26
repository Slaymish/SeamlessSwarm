# ADR-003: Swarm Heartbeat and Eviction Mechanics

## Status
Approved

## Context
The Seamless Swarm Computing Platform relies on a central headless ARM Appliance (the Compute Module Hub) running `compute-module-core` to orchestrate task execution across a volatile network of workstation nodes. Because local workstations can disconnect, experience high latency, or suffer network dropouts, the central orchestrator must dynamically detect node departures to prevent task loss. 

We need a concrete, validated decision establishing:
1. The heartbeat interval for active workstations.
2. The eviction timeout threshold after which a node is safely marked as offline.
3. The mitigation mechanism to handle network splits and split-brain partition states.

## Decision
We have made the following structural decisions:

### 1. Heartbeat Interval and Timeout Threshold
- **Heartbeat Frequency:** The `host-background-agent` will broadcast/send heartbeats (capability profile state packages) every **2 seconds** over the high-performance NNG transmission channel.
- **Offline Eviction Threshold:** A node is marked as offline and evicted from the central hub's ephemeral registry if no heartbeat has been registered for **5 seconds** (accounting for 2 missed heartbeats + 1 second network jitter buffer).
- **Eviction Implementation:** The `EphemeralRegistry` implements a high-performance, thread-safe `evict_offline_nodes(current_time, timeout_threshold)` routine executed periodically on the central orchestrator main loop using in-place collection retention (`RwLock` map `retain` pattern).

### 2. Split-Brain & Partition Mitigation
- **Local Autonomy Fallback:** If a workstation node detects a heartbeat acknowledge timeout (fails to reach the central Hub for more than 5 seconds), it suspends remote task ingestion immediately.
- **Incremental Backoff Re-Discovery:** The workstation agent falls back to zero-config mDNS re-discovery with a randomized exponential backoff (starting at 1 second, backing off up to 30 seconds) to find and rejoin the Hub.
- **Task Reallocation:** Stateless and Stateful tasks assigned to the departed node are immediately rescheduled to alternative matching authenticated nodes using the core's recovery action matrix.

## Consequences
- **Pros:** Fast and automated failover detection under 5 seconds; minimizes task dropouts and pipeline stalls.
- **Cons:** Increases network control packet overhead (minor, given extremely small <1KB capability profile payloads).
- **Cons:** Demands synchronized clock assumptions for link-local relative elapsed time, mitigated by tracking time elapsed relative to local relative tick counters rather than absolute NTP wall clocks.
