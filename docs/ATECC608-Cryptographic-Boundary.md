# ATECC608 Cryptographic Boundary & Hardware Key Dongle

The security of the Seamless Swarm Computing Platform is established by the hardware-locked private key inside the USB-C Node Key Dongle. The dongle integrates a Microchip `ATECC608B/C` CryptoAuthentication secure element connected via an internal bridge to the host operating system.

---

## 1. Hardware Integration Layout

The Node Key Dongle houses the `ATECC608` secure chip connected to an internal USB-to-I2C controller bridge (e.g. MCP2221A) to communicate directly with the host background agent.

```
+--------------------------------------------------------+
|                  Node Key USB-C Dongle                 |
|                                                        |
|   [Host OS] ---> [USB Bridge] ---> [I2C Bus] ---> [ATECC608]  |
|   (Agent Code)   (MCP2221A)        (SDA/SCL)      (Secure Chip) |
+--------------------------------------------------------+
```

### 1.1 Physical Routing Interface
- **I2C Serial Interface:** Operating at standard speed (100 kHz) or fast speed (400 kHz) with external 4.7kΩ pull-up resistors on the SDA and SCL lines.
- **Power Configuration:** Operating at 3.3V derived from the USB-C VBUS power line via a low-dropout (LDO) linear regulator.
- **Form Factor:** Ultra-compact PCB matching the standard width of a USB-C connector shielding to ensure zero mechanical interference on workstation ports.

---

## 2. ATECC608 Slot Configuration Map

The configuration zone of the ATECC608 is permanently locked during the provisioning phase (`/tools/provision-keys`) to prevent tampering or key extraction.

| Slot | Key Type | Write Configuration | Read Configuration | Intended Purpose |
| --- | --- | --- | --- | --- |
| **Slot 0** | ECC P-256 Private Key | **Locked / Never** | **Encrypted / Never** | Node Identity Private Key. Used to sign high-entropy authentication challenges. Cannot be read by the host OS. |
| **Slot 1** | ECC P-256 Public Key | **Locked / Never** | **Unrestricted / Plaintext** | Derived Node Public Key. Read by the host and exported to calculate the local static SHA-256 thumbprint. |
| **Slot 2** | AES-128 Symmetric Key | **Write encrypted** | **Never** | Used for encrypted I2C transport encryption (if local snooping protection is active). |

---

## 3. Cryptographic Challenge-Response Protocol

When a node joins the swarm, the central Hub generates a high-entropy 32-byte challenge token. The authentication lifecycle is executed as follows:

1. **Token Generation:** The Hub generates a cryptographically secure 32-byte random challenge token using `/dev/urandom` or standard Rust `rand::thread_rng` entropy.
2. **Dongle Dispatch:** The `host-background-agent` intercepts the challenge and sends an `ECDSA Sign` packet command over the USB-to-I2C bridge to the ATECC608 secure element, targeting Slot 0.
3. **Hardware Signature:** The ATECC608 computes the ECDSA P-256 signature internally over the SHA-256 hash of the challenge. The private key never leaves the chip silicon, rendering cloning impossible.
4. **Verification:** The signature is returned to the agent, forwarded to the Hub, and validated against the node's public key (already matched against the static thumbprints list).
