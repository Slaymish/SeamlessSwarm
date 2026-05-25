# High-Performance NNG Framing & Zero-Config Network Transport

The network transport layer drives the sub-millisecond execution loop between the `host-background-agent` and `compute-module-core`. It combines link-local multicast-DNS (mDNS) discovery with Nanomsg Next Generation (NNG) Scalability Protocols.

---

## 1. Zero-Config mDNS Peer Discovery

The swarm operates entirely in zero-configuration environments where corporate DHCP servers might be unavailable or segregate multicast UDP packets.

```
       [Workstation Node]                             [Compute Hub]
                |                                           |
                | --- 1. Broadcasts Join Message ---------> | (UDP Port 5353)
                |     ("SEAMLESS-SWARM:REGISTER...")        |
                |                                           |
                | <--- 2. Direct connection established ---- | (TCP Port 5555 / NNG)
```

### 1.1 Multicast Networking Parameters
- **Multicast Group Address:** `224.0.0.251` (IPv4 standard link-local multicast)
- **Port:** `5353` (Standard mDNS Port)
- **Fallback Socket Reuse:** On systems where port 5353 is locked exclusively by a system daemon (e.g. Bonjour or Avahi), the agent falls back to binding to an ephemeral socket (port 0) and issues active multicast outgoing packets to `224.0.0.251:5353` to trigger a unicast return route.

---

## 2. NNG Scalability Protocols & Framing

Once discovery completes, communication is handed off to NNG sockets to bypass the overhead of standard HTTP headers and polling loops.

### 2.1 Protocol Topologies
We employ two distinct NNG socket paradigms based on the type of payload:
1. **Req/Rep (Request-Reply):** Used during the authentication handshake. The central hub issues high-entropy tokens to joining nodes, and the nodes return the ECDSA cryptographic signature.
2. **Push/Pull (Pipeline):** Used for heartbeats and task distribution. The background agents push heartbeat profiles to the central registry, and the central hub pushes queued task execution frames to available nodes.

### 2.2 Byte-Level Framing Protocol
Every NNG frame begins with a fixed-size header to ensure parsing safety:

```
+-------------------+---------------------+-------------------------+
| Length (4 Bytes)  | Message Type (1B)   |   Protobuf Payload      |
| [Big-Endian u32]  | [Handshake/Task/...] |   [Variable Size]       |
+-------------------+---------------------+-------------------------+
```

---

## 3. High-Throughput Point-to-Point TCP Fallback

While NNG is optimized for high-speed, sub-millisecond control packets and small capability payloads, it incurs frame orchestration overhead for large data buffers. 

To prevent network bottlenecks:
- **Payload Threshold:** Any task payload (such as a 3D render asset or raw image chunk) that exceeds **64 KB** bypasses the NNG message framing entirely.
- **P2P Socket Handshake:** The central hub opens a raw, direct TCP socket stream on an ephemeral port directly to the worker node.
- **Streaming Pipeline:** The payload is streamed over raw TCP with zero framing wrappers, maximizing raw network interface saturation. Once transmission completes, the raw socket is torn down, and scheduling states revert to the low-overhead NNG control plane.
