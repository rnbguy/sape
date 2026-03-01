# `sape` Tunnel Architecture (`netcat`, `-L`, `-R`, `-D`, mDNS)

This document explains the CLI-first tunnel architecture for:

- default netcat mode (`sape dial <TARGET>`)
- local forward (`-L`)
- reverse forward (`-R`, `-g/--gateway-ports`)
- proxy mode (`-D`, SOCKS5 + HTTP CONNECT)
- mDNS LAN discovery (direct P2P, no relay)

All inter-peer traffic is encrypted by libp2p Noise. The transport stack is
configured in `sape/src/client/builder.rs` using:

- TCP + Noise + Yamux
- QUIC
- WebSocket + Noise + Yamux
- Relay client (for circuit relay paths)

## Shared Control Plane

All tunnel modes use the same application protocol:

- protocol id: `/sape/tunnel/1.0.0`
- frame format: 4-byte length prefix + postcard payload
- request enum: `TunnelRequest::{Netcat, LocalForward, ReverseForward}`
- max request size: 64 KiB

Defined in `sape/src/tunnel.rs`.

## 1) Default Netcat Flow

```mermaid
sequenceDiagram
    participant D as Dialer
    participant R as Relay
    participant L as Listener

    L-->>R: listen
    Note over R: sape relay --port 4001
    Note over L: sape listen --relay-address RELAY_ADDR
    Note over D: sape dial TARGET

    D->>R: Connect (Noise-encrypted)
    R->>L: Forward circuit
    D->>L: Open /sape/tunnel/1.0.0 stream
    D->>L: Send TunnelRequest::Netcat
    Note over D: Bridge stdin → stream
    Note over L: Bridge stdin → stream
    D->>L: stdin bytes (encrypted)
    L->>D: stdin bytes (encrypted)
    Note over D,L: Bidirectional stdin/stdout until EOF
```

Notes:

- This is raw interactive stream bridging (stdin/stdout).
- Relay can forward packets, but cannot decrypt end-to-end payload.

## 2) Local Forward (`-L`) Flow

```mermaid
sequenceDiagram
    participant A as App
    participant D as Dialer
    participant R as Relay
    participant L as Listener
    participant T as Target

    L-->>R: listen
    Note over R: sape relay --port 4001
    Note over L: sape listen --relay-address RELAY_ADDR
    Note over D: sape dial -L BIND:HOST:PORT TARGET

    Note over D: Bind 127.0.0.1:BIND_PORT
    A->>D: Connect TCP BIND_PORT
    D->>R: Connect (Noise-encrypted)
    R->>L: Forward circuit
    D->>L: Open /sape/tunnel/1.0.0 stream
    D->>L: Send TunnelRequest::LocalForward(HOST:PORT)
    L->>T: Connect TCP HOST:PORT
    Note over A,T: Bidirectional copy: App ↔ Dialer ↔ Relay ↔ Listener ↔ Target
```

Notes:

- Local bind is on the dialer side.
- Forward target is reached from the listener side.

## 3) Reverse Forward (`-R`) Flow

```mermaid
sequenceDiagram
    participant T as Target
    participant D as Dialer
    participant R as Relay
    participant L as Listener
    participant C as Client

    L-->>R: listen
    Note over R: sape relay --port 4001
    Note over L: sape listen --relay-address RELAY_ADDR
    Note over D: sape dial -R BIND:HOST:PORT [-g] TARGET

    D->>R: Connect (Noise-encrypted)
    R->>L: Forward circuit
    D->>L: Open /sape/tunnel/1.0.0 stream
    D->>L: Send TunnelRequest::ReverseForward(BIND_PORT, HOST:PORT, gateway_ports)
    alt -g flag
        Note over L: Bind 0.0.0.0:BIND_PORT
    else default
        Note over L: Bind 127.0.0.1:BIND_PORT
    end
    L->>D: Send REVERSE_OK ack
    C->>L: Connect TCP BIND_PORT
    L->>D: Open new /sape/tunnel/1.0.0 stream
    D->>T: Connect TCP HOST:PORT
    Note over T,C: Bidirectional copy: Client ↔ Listener ↔ Relay ↔ Dialer ↔ Target
```

Notes:

- Without `-g`, listener-side bind uses `127.0.0.1`.
- With `-g`, listener-side bind uses `0.0.0.0` (SSH `GatewayPorts` behavior).

## 4) Proxy Mode (`-D`) Flow

```mermaid
sequenceDiagram
    participant A as App
    participant D as Dialer
    participant R as Relay
    participant L as Listener
    participant T as Target

    L-->>R: listen
    Note over R: sape relay --port 4001
    Note over L: sape listen --relay-address RELAY_ADDR
    Note over D: sape dial -D SOCKS_PORT TARGET

    Note over D: Bind 127.0.0.1:SOCKS_PORT
    A->>D: Connect TCP SOCKS_PORT
    Note over D: Peek first byte
    alt byte == 0x05
        A->>D: SOCKS5 CONNECT request
        D->>A: SOCKS5 reply OK
    else byte == ASCII
        A->>D: HTTP CONNECT host:port
        D->>A: HTTP 200 Connection Established
    end
    Note over D: Extract target host:port
    D->>R: Connect (Noise-encrypted)
    R->>L: Forward circuit
    D->>L: Open /sape/tunnel/1.0.0 stream
    D->>L: Send TunnelRequest::LocalForward(host:port)
    L->>T: Connect TCP host:port
    Note over A,T: Bidirectional copy: App ↔ Dialer ↔ Relay ↔ Listener ↔ Target
```

Notes:

- One local port serves both SOCKS5 and HTTP CONNECT.
- Protocol selection is automatic from the first byte.
- `socks5h`/`--socks5-hostname` keeps DNS resolution remote through tunnel.

## 5) Jump Chain (`-J`) Flow (netcat mode)

```mermaid
sequenceDiagram
    participant D as Dialer
    participant H1 as Hop1
    participant HN as HopN
    participant L as Listener

    L-->>HN: listen
    Note over H1: sape relay --port 4001
    Note over HN: sape relay --port 4001
    Note over L: sape listen --relay-address RELAY_ADDR
    Note over D: sape dial -J HOP1 -J HOP2 TARGET

    D->>H1: Open /sape/jump/1.0.0 stream
    D->>H1: Send JumpChain [hop2..hopN, listener]
    H1->>HN: Open /sape/jump/1.0.0 stream
    H1->>HN: Send JumpChain [hopN+1.., listener]
    HN->>L: Open /sape/tunnel/1.0.0 stream
    HN->>H1: Return JumpResult::Ok
    H1->>D: Return JumpResult::Ok
    D->>L: Send TunnelRequest::Netcat (on chained stream)
    Note over D,L: Bidirectional netcat over chained relay path
```

Notes:

- Jump control protocol id is `/sape/jump/1.0.0`.
- Jump tunnel data still uses `/sape/tunnel/1.0.0` after chain setup.
- Current implementation supports jump chaining for netcat mode only.

## 6) mDNS LAN Discovery Flow

When both peers are on the same LAN, mDNS enables direct peer discovery without
a relay server. The listener advertises its PeerID via multicast DNS, and the
dialer discovers it automatically. All tunnel modes (`netcat`, `-L`, `-R`, `-D`)
work identically after the direct connection is established.

```mermaid
sequenceDiagram
    participant D as Dialer
    participant L as Listener

    L-->>D: listen
    Note over L: sape listen
    Note over D: sape dial /mdns/LISTENER_PEER_ID

    Note over L: Bind TCP + QUIC (random ports)
    Note over D: Bind TCP + QUIC (random ports)
    Note over L: mDNS advertises PeerID on LAN
    D->>L: mDNS multicast discovery
    D->>L: Dial discovered address (TCP or QUIC)
    D->>L: Noise handshake (direct, single hop)
    Note over D,L: ConnectionEstablished
    D->>L: Open /sape/tunnel/1.0.0 stream
    D->>L: Send TunnelRequest (Netcat / -L / -R / -D)
    Note over D,L: Tunnel active (same as relay flows, minus relay hop)
```

Notes:

- No relay server is needed. Connection is direct TCP or QUIC on the LAN.
- Only a single Noise handshake occurs (vs 3 for relayed connections).
- mDNS is always enabled in the client swarm (`sape/src/client/builder.rs`).
- Listener CLI: `sape listen` (no `--relay-address`).
- Dialer CLI: `sape dial /mdns/<PEER_ID>`.

## Encryption Guarantees

- Inter-peer traffic for all six flows runs through libp2p encrypted channels.
- Relay nodes forward encrypted frames and do not terminate end-to-end
  application streams.
- Local loopback segments (app -> `127.0.0.1:...` proxy/forward bind) are local
  host traffic, not network-exposed encryption boundaries.

## Diagram Sources

Mermaid sources used for rendering are in:

- `docs/diagrams/netcat_flow.mmd`
- `docs/diagrams/local_forward_flow.mmd`
- `docs/diagrams/reverse_forward_flow.mmd`
- `docs/diagrams/socks_proxy_flow.mmd`
- `docs/diagrams/jump_flow.mmd`
- `docs/diagrams/mdns_flow.mmd`

ASCII renders (generated with
[`beautiful-mermaid`](https://www.npmjs.com/package/beautiful-mermaid) via Deno)
are in:

- `docs/diagrams/netcat_flow.txt`
- `docs/diagrams/local_forward_flow.txt`
- `docs/diagrams/reverse_forward_flow.txt`
- `docs/diagrams/socks_proxy_flow.txt`
- `docs/diagrams/jump_flow.txt`
- `docs/diagrams/mdns_flow.txt`

Regenerate all diagrams:

```bash
deno task docs:diagrams
```

All Mermaid sequence diagrams use top-to-bottom participant columns for
process-oriented visualization on GitHub.
