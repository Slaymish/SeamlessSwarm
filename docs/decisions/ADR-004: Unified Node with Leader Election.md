# ADR 001 — Unified Node Binary with mDNS Leader Election

**Status**: Accepted  
**Date**: 2026-05-27

---

## Context

The system previously had two separate binaries:

| Binary | Role |
|--------|------|
| `compute-module-core` | Hub — accepts connections, stores node registry, dispatches tasks, serves the Medium CLI |
| `host-background-agent` | Agent — discovers capabilities, authenticates to the hub, executes tasks |

This split created two problems:

1. **Hub capabilities were invisible.** The machine running the hub could not contribute its own CPU, GPU, or software capabilities to the swarm. Capabilities were only collected from separate agent instances.
2. **Single point of failure.** If the hub process died, the entire swarm stalled until a human restarted it on a fixed machine. There was no automatic recovery.

---

## Decision

**Merge the two binaries into a single `seamless-node` binary.** Every machine in the swarm runs the same program. One node acts as the leader (hub); all others are followers (workers). Leadership is determined automatically via a deterministic election over mDNS-discovered peers.

### Leader election algorithm

This is a **bully-lite** variant: no voting rounds, no network consensus.

**Rule**: the node with the **lexicographically smallest UUID** among all live peers (including itself) is the leader.

**Liveness**: each node broadcasts `SEAMLESS-SWARM:PEER:<id>` over mDNS multicast every 2 s. A peer is considered live if it was seen within the last 10 s. When leading, a node additionally broadcasts `SEAMLESS-SWARM:LEADER:<id>:<ip>` so followers can discover the hub address.

**Startup window**: on boot, a node waits 5 s before acting, giving existing peers time to announce.

**Failover path**:
1. The leader's mDNS LEADER announcements go silent.
2. After 10 s, all followers see `current_leader() → None`.
3. Each follower re-evaluates `i_should_be_leader()`.
4. The node with the smallest UUID among live peers starts the hub.
5. It announces its leadership; others connect as followers.

**Leader step-down**: if a node with a smaller UUID appears after election, the current leader detects this in its monitoring loop, clears its leader flag, and returns — dropping the hub sockets.

### Hub node also runs as a worker

When a node becomes the leader, it:
1. Starts the NNG hub server (binds on `0.0.0.0:5555–5560`).
2. Registers its own capabilities directly into the in-process `EphemeralRegistry` (no round-trip needed).
3. Also connects to its own hub via loopback (`127.0.0.1:5555`) to receive and execute tasks like any follower.

---

## Consequences

### Benefits
- One binary to deploy and maintain.
- Any node can become the leader — no fixed hardware dependency.
- The leader's capabilities are now available to the swarm.
- Failover is automatic and requires no human intervention.

### Trade-offs and known limitations

| Limitation | Detail |
|-----------|--------|
| Startup race | Two nodes starting within 5 s of each other may briefly both see no peers. The 5 s startup window reduces this to near-zero in practice on a LAN. |
| Leader step-down cost | When a smaller-ID node appears and displaces an existing leader, in-flight tasks on that leader's hub are lost. Tasks in the scheduler are not persisted across leader transitions. Future work: use Raft or a replicated log. |
| Hub is still a single-node bottleneck | Task registry and scheduler live in the leader's memory. There is no replication of task state across followers. Failover clears all pending tasks. |
| mDNS scope | Peer discovery only works within a single Layer-2 broadcast domain. Cross-subnet swarms would need a gossip overlay or a static seed list. |
| Port conflicts | All nodes use the same fixed ports (5555–5560). Only one leader may exist per network segment. |

### Alternatives considered

| Alternative | Why rejected |
|------------|--------------|
| **Raft consensus** (e.g. via `openraft`) | Correct under network partitions but adds significant complexity — quorum management, log replication, snapshot transfer. Out of scope for current phase. |
| **External coordinator** (etcd, ZooKeeper, Consul) | Operational overhead; introduces an external dependency that itself needs HA. |
| **Static hub designation via config** | Defeats the goal of removing the single point of failure. |
| **Random leader with re-election** | Non-deterministic; requires more message rounds to converge. Min-ID is instant and converges with zero messages beyond the mDNS broadcasts already required. |

---

## Files changed

| Change | File |
|--------|------|
| Removed hub binary | `software/compute-module-core/src/main.rs` — deleted |
| Hub bind address made configurable | `software/compute-module-core/src/server.rs` — `SwarmHubServer::new` now takes `bind_ip: String` |
| Removed circular dev-dep | `software/compute-module-core/Cargo.toml` — dropped `host-background-agent` dev-dep |
| Unified node binary | `software/host-background-agent/` — package renamed to `seamless-node`; `main.rs` rewritten |
| Leader election | `software/host-background-agent/src/election/mod.rs` — new module |
| mDNS listener + local IP detection | `software/host-background-agent/src/transport/mod.rs` — `run_mdns_broadcaster`, `run_mdns_listener`, `get_local_ip` added |
| Proto message | `proto/topology.proto` — `PeerAnnounce` message added (MsgType 20) |
| Makefile | `Makefile` — `hub` + `agent` targets replaced by single `node` target |
