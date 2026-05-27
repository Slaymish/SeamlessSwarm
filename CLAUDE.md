# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build everything
make build          # or: cargo build

# Run tests
make test           # or: cargo test

# Run a single test by name pattern
cargo test <pattern>                    # e.g. cargo test i_am_only_node
cargo test --package seamless-node      # tests for one crate
cargo test -- --nocapture               # show println! output

# Run the demo (3 terminals)
make node     # Terminal 1 — first node up becomes leader/hub
make node     # Terminal 2 — joins as follower
make medium   # Terminal 3 — interactive task UI

# Other
make provision  # Key provisioning CLI
make clean
```

## Architecture

### Crate layout

| Crate | Package name | Role |
|---|---|---|
| `software/compute-module-core` | `compute-module-core` | Library only. Hub server, scheduler, registry, auth. |
| `software/host-background-agent` | `seamless-node` | Binary + library. mDNS election, capability discovery, worker transport. |
| `tools/swarm-medium` | `swarm-medium` | Binary. Interactive CLI for submitting tasks and watching results. |
| `tools/provision-keys` | `provision-keys` | Binary. One-shot key provisioning tool. |

`seamless-node` depends on `compute-module-core`; no other cross-crate dependencies exist.

### Wire protocol

All messages share a single framing format: `[4-byte BE length][1-byte msg_type][protobuf payload]`. The schema lives in `proto/topology.proto` and is compiled by `prost-build` in each crate's `build.rs`.

| Port | Socket type | Purpose |
|---|---|---|
| 5555 | Req/Rep | Handshake / auth |
| 5556 | Push/Pull | Task distribution to workers |
| 5557 | Push/Pull | Heartbeat / capability profiles from workers |
| 5558 | Push/Pull | Task progress from workers → hub |
| 5559 | Req/Rep | Medium CLI → hub (status + task submit) |
| 5560 | Pub/Sub | Hub → Medium CLI (progress broadcast) |

### Leader election

Every node runs the same `seamless-node` binary. On startup, each node broadcasts `SEAMLESS-SWARM:PEER:<id>` to the mDNS multicast group (`224.0.0.251:5353`) every 2 s and listens for 5 s before assuming a role.

Election rule: **the node with the lexicographically smallest UUID among all live peers becomes leader.** A peer is considered offline after 10 s of silence. The leader additionally broadcasts `SEAMLESS-SWARM:LEADER:<id>:<ip>`. `ElectionState` (`software/host-background-agent/src/election/mod.rs`) owns this logic and its 6 unit tests are the canonical specification.

### Node roles

- **Leader** (`run_as_leader`): creates `EphemeralRegistry` + `ProfileScheduler`, self-registers its own capabilities, starts `SwarmHubServer` bound on `0.0.0.0`, then also spawns a loopback worker (`run_worker_on_loopback`) so the leader contributes its own compute to the swarm.
- **Follower** (`run_as_follower`): connects to `tcp://<leader_ip>:555x`, authenticates via ECDSA challenge-response, sends its `CapabilityProfile` (MsgType 5), then polls for tasks.
- The role loop re-evaluates whenever a role function returns (leader stepped down, or leader went offline).

### Capability discovery

`ScoutEngine` (`software/host-background-agent/src/scout/`) runs five async profilers (CPU, GPU, Memory, Network, Software). `SoftwareProfiler` scans `/Applications` (macOS), `/usr/share/applications` (Linux), and `KNOWN_CLI_TOOLS` (checked against PATH + Homebrew paths) to emit `installed_app_*` capabilities. `adapt_to_medium_profile` then converts raw metrics into normalised `cpu_class`, `accelerator_class`, `memory_tier`, `low_latency_ready`, and `creative_capability_*` keys used by the scheduler for task matching.

### Task routing

Tasks are submitted via the medium CLI with a list of `required_capabilities`. `ProfileScheduler.dispatch_pending_tasks()` matches tasks to nodes by capability intersection. The dispatch loop in `SwarmHubServer.run_task_sender_loop` only fires when there are pending tasks — it does not auto-generate tasks.

Task result text (e.g. Ollama LLM response) is carried back in `TaskProgress.result_text` (proto field 6) and displayed by the medium CLI.

### Protobuf codegen

Each crate that needs proto types has its own `build.rs` pointing at `../../proto/topology.proto`. The generated code is included with `include!(concat!(env!("OUT_DIR"), "/seamless_swarm.rs"))`. When adding a proto field, update all three build targets and regenerate.

### Secure element

`SimulatedSecureElement` (`software/host-background-agent/src/secure_element.rs`) is a software simulation of the planned ATECC608 hardware dongle. It generates an ephemeral P-256 key pair and a static thumbprint, and signs ECDSA challenges. The hub verifies signatures in `auth.rs` using the public key carried in `HandshakeResponse`.
