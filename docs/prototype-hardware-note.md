# Seamless System — Prototype Hardware Note

**Status:** Working document  
**Date:** 2026-05-27  
**Scope:** Prototype build only. Not a production BOM. Enclosures are 3D printed.

---

## Overview

Three physical units to prototype:

1. **Compute Module Hub** — dedicated always-on ARM appliance, runs `compute-module-core`
2. **Node Key Dongle** — plugs into contributing workstations, triggers Scout Model, connects to swarm
3. **Access Key Dongle** — plugs into mediums (displays, controllers, etc.), presents swarm UI for that device type

Node Key and Access Key share the same PCB design. Firmware and flash contents differ.

---

## 1. Compute Module Hub

Raspberry Pi 5 running `compute-module-core`. Acts as the permanent swarm anchor: hosts the mDNS responder, NNG broker, ephemeral registry, and heartbeat eviction loop (2s interval / 5s timeout per ADR-003).

| Part                   | Purpose                                           | ~Cost |
| ---------------------- | ------------------------------------------------- | ----- |
| Raspberry Pi 5 (4GB)   | Main compute — orchestrator, NNG broker, registry | ~$60  |
| M.2 HAT+ for Pi 5      | Exposes PCIe slot for AX210                       | ~$12  |
| Intel AX210 (M.2 2230) | 6GHz AP — all Node/Access Keys connect here       | ~$18  |
| SD card (32GB)         | OS + `compute-module-core`                        | ~$8   |
| USB-C power supply     | Always-on operation                               | ~$10  |
| Enclosure              | 3D printed, ventilated                            | —     |

**~$108 total**

The AX210 runs in hostapd AP mode on 6GHz. The Pi 5 does not do any heavy computation — it orchestrates only. 4GB RAM is headroom, not a requirement.

---

## 2. Node Key Dongle

Plugs into a contributing workstation via USB-A. On plug-in:

1. CH552 presents to host as USB HID keyboard + USB mass storage simultaneously
2. HID injects keystrokes to open a terminal and execute the Scout script off the mass storage partition
3. Scout script runs on the host, discovers capabilities, outputs capability profile JSON
4. Host OS natively drives the AX210 (visible as a USB network adapter via the hub IC) and starts `host-background-agent`
5. `host-background-agent` registers the capability profile with the Hub and begins heartbeating every 2s
6. CH552 is idle for the remainder of the session

The host does all the work after the initial trigger. The dongle is credential + radio + ignition.

| Part                        | Purpose                                                                                              | ~Cost |
| --------------------------- | ---------------------------------------------------------------------------------------------------- | ----- |
| CH552G                      | USB HID keyboard + mass storage controller. Injects terminal command on plug-in, idles thereafter    | ~$1   |
| Intel AX210 (M.2 2230)      | 6GHz radio — host OS drives this natively as a USB NIC via the hub                                   | ~$18  |
| RTL8153 (M.2-to-USB bridge) | Bridges AX210 PCIe into USB so it can hang off the CH334 hub                                         | ~$4   |
| CH334 USB hub IC            | Combines CH552 + AX210/RTL8153 on a single USB-A port to the host                                    | ~$2   |
| ATECC608A                   | Secure element. Stores swarm credentials, performs ECDSA challenge-response auth with Hub on connect | ~$2   |
| U.FL pigtail + stub antenna | Routes 6GHz signal outside the 3D printed enclosure                                                  | ~$3   |
| USB-A through-hole plug     | Host connection                                                                                      | ~$1   |
| Passives                    | Decoupling caps, I2C pull-ups for ATECC608                                                           | ~$1   |

**~$32 per unit**

### Notes

- **ATECC608 interface:** I2C for the prototype. SWI remains an open question for production (see Open Questions). I2C is standard, well-supported across platforms, and easier to debug on a first-spin board.
- **Scout scripts:** Three variants stored on the mass storage partition — `.sh` (Linux), `.command` (macOS), `.ps1` (Windows). HID injection detects OS and runs the appropriate one, or opens a terminal and presents a simple menu if detection is ambiguous.
- **ATECC608 provisioning:** Use Microchip Trust&GO or TrustFLEX pre-provisioned profiles to avoid shipping raw private keys. Hub holds static thumbprint certificates per ADR-002. No CA required.

---

## 3. Access Key Dongle

Identical PCB to Node Key. Differences are software/firmware only.

| Difference           | Detail                                                           |
| -------------------- | ---------------------------------------------------------------- |
| CH552 firmware       | Loads Medium Profile Software on plug-in instead of Scout script |
| Flash contents       | Medium profile ID + medium-specific UI config. No Scout scripts. |
| ATECC608 credentials | Same crypto model, different swarm role identifier               |
| USB plug             | May need USB-C variant depending on target medium                |

Same fab order, same assembly, different flash. One board design covers both SKUs.

**Current medium profiles (per spec):**

| Medium                | Swarm surface                                 |
| --------------------- | --------------------------------------------- |
| DJ Controller         | Raw compute, DJ software nodes, audio routing |
| E-ink Display         | Documents, sheet music, low-bandwidth content |
| Hologram Projector    | 3D content, high-compute rendering pipeline   |
| Standard Monitor / TV | General display, full desktop interface       |
| Kindle                | Reading library, annotations                  |

---

## Shared PCB Notes

One 2-layer PCB design for both dongle SKUs. Required footprints:

- M.2 2230 slot (AX210)
- CH552G (LQFP-48)
- RTL8153 (QFN)
- CH334 hub IC
- ATECC608A (UDFN-8 or SOT-23)
- U.FL connector
- USB-A through-hole plug (host-facing)
- USB-C port (debug/flashing only — not exposed on final enclosure)
- Decoupling caps, I2C pull-ups

**Fabrication:** JLCPCB 2-layer, ~$15 for 5 units. Most components available via LCSC for partial assembly.

---

## Enclosures

All 3D printed, PLA or PETG.

| Unit                  | Form factor                | Notes                                                              |
| --------------------- | -------------------------- | ------------------------------------------------------------------ |
| Compute Module Hub    | Small rectangular box      | Ventilation slots, SD card access, USB-C power inlet, antenna exit |
| Node Key / Access Key | USB stick, slightly chunky | U.FL antenna stub exits rear or side. Snap-fit two-part shell.     |

Antenna placement is the main constraint — needs a clear exit point and ideally a straight line of sight. Design the enclosure around the U.FL pigtail routing first.

---

## Open Items Blocking PCB Spin

1. **ATECC608 SWI vs I2C** — using I2C for prototype, but confirm before routing
2. **OS detection logic in CH552 firmware** — needs to be validated before HID script injection is reliable across Windows/macOS/Linux
3. **AX210 regulatory compliance** — 6GHz operation in NZ is subject to indoor-only unlicensed rules; confirm before putting hardware in front of people
