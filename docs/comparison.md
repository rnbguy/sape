# Competitive Comparison

How `sape` compares to existing tunneling, VPN, and port-forwarding tools.

## Quick Summary

| Feature | sape | SSH | chisel | bore | rathole | frp | ngrok | Tailscale | CF Tunnel | WireGuard | ZeroTier |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Language | Rust | C | Go | Rust | Rust | Go | Go | Go | Go | C/Go | C++ |
| `-L` local forward | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `-R` reverse forward | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| `-D` SOCKS5 proxy | ✅ | ✅ | ✅ | ❌ | ❌ | ✅* | ❌ | ✅* | ❌ | ❌ | ❌ |
| HTTP CONNECT proxy | ✅ | ❌ | ❌ | ❌ | ❌ | ✅* | ❌ | ✅* | ❌ | ❌ | ❌ |
| Netcat / stdio | ✅ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Multi-hop (`-J`) | ✅ | ✅ | ✅† | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| NAT traversal | ✅ auto | ❌ | ❌ | ❌ | ❌ | ⚠️ xtcp | ❌ | ✅ | ❌ | ❌ | ✅ |
| Hole punching (DCUtR) | ✅ | ❌ | ❌ | ❌ | ❌ | ⚠️ | ❌ | ✅ | ❌ | ❌ | ✅ |
| mDNS LAN discovery | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅‡ |
| End-to-end encryption | ✅ Noise | ✅ | ✅ SSH | ❌ | ✅ | ⚠️ | ✅ | ✅ WG | ✅ | ✅ WG | ✅ |
| No root/sudo | ✅ | ✅§ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| No account required | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ |
| No daemon required | ✅ | ✅§ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Single binary | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ |
| Self-hosted relay | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ⚠️ | ❌ | N/A | ⚠️ |
| Pairing codes | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

`*` via plugin or userspace mode  
`†` via upstream SOCKS/HTTP proxy, not native relay chaining  
`‡` UDP broadcast, not RFC 6762 mDNS  
`§` client only; `sshd` server requires root  

## Tool-by-Tool Comparison

### OpenSSH

The gold standard for `-L`, `-R`, `-D`, `-J`. `sape` deliberately uses SSH's flag conventions.

| Dimension | SSH | sape |
|---|---|---|
| NAT traversal | None — server needs open port or you chain jump hosts | Automatic via relay + DCUtR hole punching |
| LAN discovery | None — must know hostname/IP | mDNS auto-discovery, zero config |
| Setup | `sshd` on server + key auth | `sape relay` on a VPS (or mDNS for LAN) |
| Root required | Server: yes (`sshd`). Client: no | Neither side |
| Encryption | Negotiated (RSA, ECDSA, Ed25519, AES-GCM, ChaCha20) | Noise XX (Ed25519 + X25519 + ChaCha20-Poly1305) |
| Forward secrecy | Yes (ephemeral DH) | Yes (Noise handshake) |
| Protocol | TCP only | TCP + QUIC |
| Multi-hop | `ProxyJump` (`-J`) — arbitrary hops via SSH servers | `/sape/jump/1.0.0` — relay chaining (netcat mode) |

**When to use SSH**: you already have `sshd` running and don't need NAT traversal.  
**When to use sape**: both peers are behind NAT, you want zero-config LAN discovery, or you don't want to run `sshd`.

### chisel

Closest feature match — Go-based, supports `-L`, `-R`, `-D`, stdio. Tunnels over HTTP/WebSocket with SSH crypto.

| Dimension | chisel | sape |
|---|---|---|
| NAT traversal | None — server needs public endpoint | Automatic relay + DCUtR |
| LAN discovery | None | mDNS |
| Encryption | SSH (ECDSA P-256) + optional TLS/mTLS | Noise (Ed25519) — always on |
| Auth | username:password + fingerprint pinning | PeerID identity + `--allowed-peer` allowlist |
| Multi-hop | Via upstream SOCKS/HTTP proxy (`--proxy`) | Native relay chaining (`-J`) |
| Binary size | ~4.2 MB (Go) | ~3-4 MB (Rust, statically linked musl) |
| Protocol | HTTP/WebSocket over TCP | TCP + QUIC |
| UDP tunneling | Yes (`host:port/udp` syntax) | Not yet |

**When to use chisel**: you need UDP tunneling or HTTP proxy traversal.  
**When to use sape**: you need NAT traversal, mDNS LAN discovery, or self-hosted relay without a public IP.

### bore

Minimal Rust reverse tunnel tool. `bore local <port> --to <server>`.

| Dimension | bore | sape |
|---|---|---|
| Tunnel modes | Reverse only | Netcat, `-L`, `-R`, `-D` |
| Encryption | None (HMAC auth handshake only, traffic unencrypted) | Noise — end-to-end encrypted |
| NAT traversal | Relay only | Relay + DCUtR hole punching |
| LAN discovery | None | mDNS |
| Protocol | TCP only | TCP + QUIC |
| Public relay | `bore.pub` (free) | Self-hosted only |

**When to use bore**: quick one-off port expose, no encryption needed.  
**When to use sape**: you need encryption, bidirectional forwarding, or LAN discovery.

### rathole

Rust reverse tunnel with optional Noise encryption. Config-file driven.

| Dimension | rathole | sape |
|---|---|---|
| Tunnel modes | Reverse only | Netcat, `-L`, `-R`, `-D` |
| Encryption | None, TLS, or Noise (`Noise_NK_25519_ChaChaPoly_BLAKE2s`) | Noise XX (libp2p, always on) |
| NAT traversal | Relay only | Relay + DCUtR |
| Protocol | TCP + UDP + WebSocket transport | TCP + QUIC + WebSocket |
| Binary size | ~500 KiB (minimal) to ~1 MB | ~3-4 MB |
| Config | TOML config file (server + client) | CLI flags only |
| Auth | Per-service token (mandatory) | PeerID + `--allowed-peer` |

**When to use rathole**: tiny binary for embedded/router, UDP proxying, config-file automation.  
**When to use sape**: interactive CLI usage, bidirectional tunnels, NAT traversal.

### frp

Go-based, fully self-hosted, feature-rich reverse proxy. Two binaries (`frps` + `frpc`).

| Dimension | frp | sape |
|---|---|---|
| Tunnel modes | Reverse (TCP, UDP, HTTP, HTTPS, STCP, XTCP) + SOCKS5/HTTP proxy plugins | Netcat, `-L`, `-R`, `-D` (SOCKS5 + HTTP CONNECT) |
| NAT traversal | STUN-based P2P (`xtcp` mode, fragile on symmetric NAT) | DCUtR (more robust) + relay fallback |
| Encryption | TLS (default since v0.50) + optional AES per-proxy | Noise — end-to-end |
| Binaries | Two (`frps` server + `frpc` client) | One (`sape`) |
| Config | TOML/YAML/JSON config files | CLI flags |
| Protocol | TCP, UDP, HTTP, KCP, QUIC, WebSocket | TCP, QUIC, WebSocket |
| LAN discovery | None | mDNS |

**When to use frp**: HTTP/HTTPS virtual hosting, dashboard UI, UDP tunneling, KCP transport.  
**When to use sape**: single binary, CLI-first, reliable NAT traversal, LAN discovery.

### ngrok

SaaS tunneling platform. Proprietary agent binary.

| Dimension | ngrok | sape |
|---|---|---|
| Tunnel modes | Reverse only (HTTP, TCP, TLS endpoints) | Netcat, `-L`, `-R`, `-D` |
| Self-hosted | No (SaaS only, enterprise on-prem exists) | Fully self-hosted |
| Encryption | TLS (agent↔cloud); end-to-end TLS on enterprise | Noise — end-to-end, always |
| Account required | Yes (authtoken) | No |
| Pricing | Free tier limited; $8-$20+/mo | Free (self-hosted) |
| Agent source | Proprietary (closed since ~2018) | Open source |

**When to use ngrok**: quick HTTP demo, need custom domains, traffic inspection dashboard.  
**When to use sape**: no cloud dependency, self-hosted, bidirectional tunnels, no account.

### Tailscale

WireGuard-based VPN mesh with SaaS control plane.

| Dimension | Tailscale | sape |
|---|---|---|
| Architecture | Full L3 VPN (virtual IPs, subnet routing) | Application-layer tunnels (per-port) |
| NAT traversal | STUN + ICE → DERP relay fallback | DCUtR + circuit relay fallback |
| Encryption | WireGuard (ChaCha20-Poly1305 + Curve25519) | Noise XX (ChaCha20-Poly1305 + X25519) |
| Root required | Yes (creates TUN device) | No |
| Account required | Yes (OAuth/OIDC via IdP) | No |
| Daemon | Yes (`tailscaled` always running) | No — runs on demand |
| LAN discovery | MagicDNS (proprietary, not mDNS) | RFC 6762 mDNS |
| Self-hosted control plane | Headscale (unofficial OSS) | No control plane needed |
| Port forwarding | No native `-L`/`-R`/`-D` | Full SSH-style `-L`/`-R`/`-D` |

**When to use Tailscale**: full mesh VPN, device management, team SSO, all-traffic routing.  
**When to use sape**: lightweight per-port tunnels, no daemon, no account, no root.

### Cloudflare Tunnel

SaaS tunnel for exposing services behind Cloudflare's edge network.

| Dimension | Cloudflare Tunnel | sape |
|---|---|---|
| Tunnel modes | Reverse only (HTTP, SSH, RDP, TCP via Access) | Netcat, `-L`, `-R`, `-D` |
| Self-hosted | No (requires Cloudflare account + edge network) | Fully self-hosted |
| Account required | Yes | No |
| Protocol | QUIC (default) or HTTP/2 to Cloudflare edge | TCP + QUIC (peer-to-peer) |
| Root required | Yes (`cloudflared` service) | No |
| LAN discovery | None | mDNS |

**When to use CF Tunnel**: public-facing HTTPS services, DDoS protection, Cloudflare Access integration.  
**When to use sape**: peer-to-peer tunneling, no cloud dependency, bidirectional forwarding.

### WireGuard

Kernel-level VPN protocol. Fast, minimal, elegant.

| Dimension | WireGuard | sape |
|---|---|---|
| Architecture | L3 VPN (kernel module, full IP routing) | Application-layer tunnels |
| NAT traversal | Keepalives only — needs known endpoint | Automatic via relay + DCUtR |
| Root required | Yes (kernel module or TUN device) | No |
| Port forwarding | None — routes entire subnets via AllowedIPs | Per-port `-L`/`-R`/`-D` |
| Setup | Config files + manual key exchange on both peers | CLI flags, mDNS or relay address |
| LAN discovery | None | mDNS |
| Binary | `wg` + kernel module (or `wireguard-go`) | Single static binary |

**When to use WireGuard**: high-throughput L3 VPN, kernel-level performance, static site-to-site links.  
**When to use sape**: no root, no kernel module, per-port forwarding, automatic NAT traversal.

### ZeroTier

P2P virtual Ethernet (L2) mesh network with built-in NAT traversal.

| Dimension | ZeroTier | sape |
|---|---|---|
| Architecture | L2 virtual Ethernet (flat network, virtual switch) | Application-layer tunnels |
| NAT traversal | UDP hole punching + planet/moon relay | DCUtR + circuit relay |
| LAN discovery | UDP broadcast (~60s interval) | RFC 6762 mDNS |
| Root required | Yes (daemon, TUN device) | No |
| Account required | Yes (network controller, self-hostable) | No |
| Forward secrecy | No (static long-term keys in v1.x) | Yes (Noise handshake) |
| Port forwarding | None native — needs OS-level iptables/nftables | Native `-L`/`-R`/`-D` |
| Daemon | Yes (`zerotier-one` always running) | No — runs on demand |

**When to use ZeroTier**: virtual LAN for gaming, IoT mesh, multi-site L2 bridging.  
**When to use sape**: lightweight CLI tunnels, no daemon, no account, forward secrecy.

## What Makes `sape` Unique

`sape` is the only tool that combines all of these in a single binary:

1. **SSH-style port forwarding** (`-L`, `-R`, `-D`, `-J`) — only SSH and chisel offer this
2. **Automatic NAT traversal** (DCUtR hole punching + relay) — only Tailscale and ZeroTier do this
3. **mDNS LAN discovery** — zero-config direct P2P on same network
4. **No root, no account, no daemon** — run on demand as any user
5. **End-to-end Noise encryption** — relay cannot decrypt traffic
6. **Self-hosted relay** — no cloud dependency
7. **Human-friendly pairing codes** — `42-river-ocean` instead of multiaddresses

## Current Limitations

- **No UDP tunneling** — bore, rathole, frp, chisel, and WireGuard support UDP; `sape` does not yet
- **No HTTP virtual hosting** — frp and ngrok can route by HTTP Host header
- **Jump chaining limited to netcat mode** — SSH's `-J` works with all tunnel modes
- **No traffic inspection dashboard** — ngrok has a local web UI at `localhost:4040`
- **No Windows/macOS GUI** — Tailscale has native system tray apps
- **Binary size** — rathole achieves ~500 KiB; `sape` is ~3-4 MB due to libp2p
