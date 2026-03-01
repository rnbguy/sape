# sape

Single-binary (`sape`) libp2p NAT traversal and tunneling tool in Rust.

It provides:
- `relay`: circuit relay server (rendezvous + relay v2)
- `listen`: registers on relay and/or enables mDNS LAN discovery, prints dial-ready addresses
- `dial`: connects to listener via relay circuit or mDNS LAN, starts a tunnel mode (`netcat`, `-L`, `-R`, `-D`)

Architecture details (with top-to-bottom flow diagrams):
- `docs/architecture.md`

## Build

```bash
cargo build --release
```

Binary path:

```bash
target/release/sape
```

## Usage

Top-level help:

```bash
./target/release/sape --help
```

### 1) Run relay (public VPS)

```bash
RUST_LOG=info ./target/release/sape relay --port 4001
```

Enable IPv6 listening:

```bash
RUST_LOG=info ./target/release/sape relay --port 4001 --use-ipv6
```

Short alias:

```bash
RUST_LOG=info ./target/release/sape r --port 4001
```

Copy relay peer id from logs and build relay address:

```text
/ip4/<RELAY_PUBLIC_IP>/tcp/4001/p2p/<RELAY_PEER_ID>
```

### 2) Run listener (machine A)

With relay (remote NAT traversal):

```bash
RUST_LOG=info ./target/release/sape listen \
  --relay-address /ip4/<RELAY_PUBLIC_IP>/tcp/4001/p2p/<RELAY_PEER_ID>
```

LAN-only (no relay needed):

```bash
RUST_LOG=info ./target/release/sape listen
```

Listener logs dial addresses:

```text
LAN dial address: /mdns/<LISTENER_PEER_ID>
Relay dial address: /ip4/<RELAY_PUBLIC_IP>/tcp/4001/p2p/<RELAY_PEER_ID>/p2p-circuit/p2p/<LISTENER_PEER_ID>
```

### 3) Run dialer (machine B)

Via relay (remote):

```bash
RUST_LOG=info ./target/release/sape dial \
  /ip4/<RELAY_PUBLIC_IP>/tcp/4001/p2p/<RELAY_PEER_ID>/p2p-circuit/p2p/<LISTENER_PEER_ID>
```

Via mDNS (same LAN, direct P2P, no relay):

```bash
RUST_LOG=info ./target/release/sape dial /mdns/<LISTENER_PEER_ID>
```

## Tunnel Modes

Only one tunnel mode can be active per dial: `-L`, `-R`, or `-D` (mutually exclusive). Short alias: `sape d`.

### Netcat (default)

Bidirectional stdin/stdout stream (works with both relay and mDNS):

```bash
sape dial <TARGET>
```

Where `<TARGET>` is either `/mdns/<PEER_ID>` or a full relay circuit address.

### Local forward (`-L`)

Bind local `8080` on dialer and forward to listener-side `localhost:3000`:

```bash
RUST_LOG=info ./target/release/sape dial -L 8080:localhost:3000 <FULL_CIRCUIT_ADDRESS>
```

### Reverse forward (`-R`)

Bind `9090` on listener side and forward back to dialer-side `localhost:3000`:

```bash
RUST_LOG=info ./target/release/sape dial -R 9090:localhost:3000 <FULL_CIRCUIT_ADDRESS>
```

Expose the listener-side bind on all interfaces (SSH `GatewayPorts` behavior):

```bash
RUST_LOG=info ./target/release/sape dial -R 9090:localhost:3000 -g <FULL_CIRCUIT_ADDRESS>
```

Without `-g`, reverse binds use `127.0.0.1` on the listener side.

### SOCKS5 dynamic proxy (`-D`)

Run local mixed SOCKS5 + HTTP CONNECT proxy on dialer at `127.0.0.1:1080`:

```bash
RUST_LOG=info ./target/release/sape dial -D 1080 <FULL_CIRCUIT_ADDRESS>
```

SOCKS5 example with remote DNS resolution:

```bash
curl --socks5-hostname 127.0.0.1:1080 https://example.com
```

HTTP CONNECT example on the same `-D` port:

```bash
curl -x http://127.0.0.1:1080 https://example.com
```

DNS behavior in `-D` mode:
- `--socks5-hostname` (`socks5h`) resolves DNS on the remote side through the tunnel
- plain `--socks5` may resolve DNS locally in the client process
- forcing system-wide DNS interception requires a TUN-based VPN and elevated privileges, which this project intentionally avoids

### Pairing Codes

Listener with relay prints a human-friendly pairing code:

```bash
RUST_LOG=info ./target/release/sape listen --relay-address <RELAY_ADDRESS>
```

Dial using the code:

```bash
RUST_LOG=info ./target/release/sape dial <PAIRING_CODE> --relay-address <RELAY_ADDRESS>
```

You can also set a fixed code on listener:

```bash
RUST_LOG=info ./target/release/sape listen --relay-address <RELAY_ADDRESS> --code 42-river-ocean
```

### ProxyJump Relay Chaining

Chain relays with SSH-style `-J`:

```bash
RUST_LOG=info ./target/release/sape dial \
  -J <JUMP_RELAY_1> \
  -J <JUMP_RELAY_2> \
  <FINAL_CIRCUIT_TARGET>
```

Current limitation: jump chaining supports netcat mode only (not `-L`, `-R`, `-D`).

## Security

### Peer allowlist (`--allowed-peer`)

Restrict which peers can connect to the listener:

```bash
RUST_LOG=info ./target/release/sape listen \
  --relay-address <RELAY_ADDRESS> \
  --allowed-peer <PEER_ID_1> \
  --allowed-peer <PEER_ID_2>
```

When set, only the listed peer IDs are permitted to open tunnel streams. All other connections are rejected.

### Identity files (`--identity-file`)

Load a persistent Ed25519 keypair from a file instead of generating a random one each run:

```bash
RUST_LOG=info ./target/release/sape relay --port 4001 --identity-file relay.key
RUST_LOG=info ./target/release/sape listen --identity-file listener.key --relay-address <RELAY_ADDRESS>
RUST_LOG=info ./target/release/sape dial --identity-file dialer.key <TARGET>
```

This ensures a stable PeerID across restarts, which is required for `--allowed-peer` and for publishing stable relay addresses.

### Deterministic key seed (`--secret-key-seed`)

Generate a deterministic keypair from a numeric seed (useful for testing and Docker validation, NOT for production):

```bash
RUST_LOG=info ./target/release/sape relay --port 4001 --secret-key-seed 1
```

The same seed always produces the same PeerID.


## Termux (Android)

The `sape` binary is a **fully static ARM64 ELF** — it runs on Termux without any shared library dependencies.

### Transfer the pre-built binary

Build the ARM64 binary on a Linux x64 machine:

```bash
deno task build:arm64
# produces dist/sape-linux-arm64
```

Transfer to your Android device via ADB:

```bash
adb push dist/sape-linux-arm64 /data/local/tmp/sape
adb shell chmod +x /data/local/tmp/sape
# from Termux:
cp /data/local/tmp/sape ~/bin/sape
```

Or transfer via SSH/SCP if Termux has an SSH server running.

### Build from source in Termux

```bash
pkg install rust
git clone <this-repo>
cd sape
cargo build --release
./target/release/sape --help
```

### Network modes on Android

- **Relay mode**: Works on any network (WiFi or cellular). Requires a public relay server.
- **mDNS LAN discovery**: Works on WiFi only (multicast). No relay needed on the same LAN.

### Typical use-case

Run `sape listen` on your Android device. It registers with a relay and prints a dial address. Then `sape dial` from another machine using the printed address.

## Custom Namespace

By default, sape uses `sape` as the protocol namespace. You can override this with the `SAPE_NAMESPACE` environment variable to create isolated private networks:

```bash
SAPE_NAMESPACE=mynet sape relay --port 4001
SAPE_NAMESPACE=mynet sape listen --relay-address <RELAY_ADDRESS>
SAPE_NAMESPACE=mynet sape dial <TARGET>
```

Peers using different namespaces cannot communicate with each other.

## Docker Validation

The repository includes:
- `Dockerfile`
- `docker-compose.yml`

Validation command:

```bash
docker compose build --no-cache
docker compose up -d relay listener dialer
docker compose logs --no-color relay listener dialer
docker compose down
```

What this validates:
- relay starts and listens
- listener obtains relay reservation (`ReservationReqAccepted`)
- listener prints full circuit dial address
- dialer establishes outbound relay circuit (`OutboundCircuitEstablished`)
- DCUtR upgrade attempt occurs (`dcutr event`)

Important limitation:
- Docker compose uses a local bridge network. This is a strong integration test for protocol wiring and event flow, but it is **not** a full real-world NAT matrix test (home NAT, CGNAT, symmetric NAT, enterprise firewall).

## SCP to remote

Build and copy directly:

```bash
cargo build --release
scp target/release/sape <user>@<server-ip>:/tmp/
```
