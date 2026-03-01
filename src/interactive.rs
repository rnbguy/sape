use std::str::FromStr;

use color_eyre::eyre::Result;
use inquire::validator::Validation;
use inquire::{Confirm, InquireError, Select, Text};
use libp2p::PeerId;
use libp2p::core::multiaddr::Multiaddr;

use crate::cli;
use crate::types::{DialTarget, ForwardSpec};

// ── Suggestion constants
// ──────────────────────────────────────────────────────

const MULTIADDR_SUGGESTIONS: &[&str] = &[
    "/ip4/",
    "/ip4/127.0.0.1/tcp/4001/p2p/",
    "/ip6/::1/tcp/4001/p2p/",
    "/dns4/relay.example.com/tcp/4001/p2p/",
];

const FORWARD_SUGGESTIONS: &[&str] = &[
    "8080:localhost:3000",
    "8080:localhost:8080",
    "3000:localhost:3000",
    "5432:localhost:5432",
    "6379:localhost:6379",
];

fn prefix_suggest(
    suggestions: &'static [&'static str],
) -> impl Fn(&str) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> + Clone {
    move |input: &str| {
        Ok(suggestions
            .iter()
            .filter(|s| s.starts_with(input))
            .map(|s| s.to_string())
            .collect())
    }
}

// ── Mode enums
// ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, derive_more::Display)]
enum Mode {
    #[display("Relay -- run a circuit relay server")]
    Relay,
    #[display("Listen -- register on relay, accept tunnels")]
    Listen,
    #[display("Dial -- connect to listener, start tunnel")]
    Dial,
}

#[derive(Clone, Copy, derive_more::Display)]
enum ListenConn {
    #[display("Relay (remote) -- use a relay for NAT traversal")]
    Relay,
    #[display("LAN only (mDNS) -- local network discovery only")]
    Lan,
}

#[derive(Clone, Copy, derive_more::Display)]
enum DialConn {
    #[display("Relay circuit address -- dial full relay circuit multiaddr")]
    Relay,
    #[display("mDNS (LAN) -- dial by peer id on local network")]
    Mdns,
    #[display("Pairing code -- dial using pairing code plus relay address")]
    Pairing,
}

#[derive(Clone, Copy, derive_more::Display)]
enum TunnelMode {
    #[display("Netcat (default) -- interactive stdin/stdout stream")]
    Netcat,
    #[display("Local forward (-L) -- expose local port and forward remotely")]
    Local,
    #[display("Reverse forward (-R) -- expose remote port and forward locally")]
    Reverse,
    #[display("SOCKS5 proxy (-D) -- start local SOCKS5/HTTP CONNECT proxy")]
    Socks,
}

// ── Public entry point
// ────────────────────────────────────────────────────────

pub fn run_interactive() -> Result<cli::Command> {
    match run_interactive_inner() {
        Ok(cmd) => Ok(cmd),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            std::process::exit(0)
        },
        Err(e) => Err(e.into()),
    }
}

fn run_interactive_inner() -> Result<cli::Command, InquireError> {
    let mode = Select::new(
        "What do you want to do?",
        vec![Mode::Relay, Mode::Listen, Mode::Dial],
    )
    .prompt()?;

    match mode {
        Mode::Relay => run_relay(),
        Mode::Listen => run_listen(),
        Mode::Dial => run_dial(),
    }
}

// ── Relay ─────────────────────────────────────────────────────────────────────

fn run_relay() -> Result<cli::Command, InquireError> {
    let port_raw = Text::new("Port")
        .with_placeholder("4001")
        .with_validator(|s: &str| {
            let v = s.trim();
            if v.is_empty() || v.parse::<u16>().is_ok() {
                Ok(Validation::Valid)
            } else {
                Ok(Validation::Invalid("invalid port".into()))
            }
        })
        .prompt()?;

    let use_ipv6 = Confirm::new("Use IPv6?").with_default(false).prompt()?;

    let port = if port_raw.trim().is_empty() {
        4001
    } else {
        port_raw.trim().parse::<u16>().expect("validated above")
    };

    Ok(cli::Command::Relay(cli::RelayOpt {
        use_ipv6,
        identity_file: None,
        secret_key_seed: None,
        port,
    }))
}

// ── Listen ────────────────────────────────────────────────────────────────────

fn run_listen() -> Result<cli::Command, InquireError> {
    let conn =
        Select::new("Connection type?", vec![ListenConn::Relay, ListenConn::Lan]).prompt()?;

    let relay_address = if matches!(conn, ListenConn::Relay) {
        let relay_raw = Text::new("Relay address")
            .with_placeholder("/ip4/203.0.113.10/tcp/4001/p2p/12D3KooW...")
            .with_autocomplete(prefix_suggest(MULTIADDR_SUGGESTIONS))
            .with_validator(|s: &str| {
                let v = s.trim();
                if v.is_empty() {
                    return Ok(Validation::Invalid("relay address cannot be empty".into()));
                }
                match v.parse::<Multiaddr>() {
                    Ok(_) => Ok(Validation::Valid),
                    Err(_) => Ok(Validation::Invalid("invalid relay multiaddr".into())),
                }
            })
            .prompt()?;
        Some(
            relay_raw
                .trim()
                .parse::<Multiaddr>()
                .expect("validated above"),
        )
    } else {
        None
    };

    let code_raw = Text::new("Pairing code? (optional)")
        .with_placeholder("empty = auto-generate")
        .prompt()?;

    let allowed_peers_raw = Text::new("Allowed peers? (optional)")
        .with_placeholder("12D3KooW...,12D3KooX... (empty = allow all)")
        .with_validator(|s: &str| {
            let v = s.trim();
            if v.is_empty() {
                return Ok(Validation::Valid);
            }
            for peer in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                if PeerId::from_str(peer).is_err() {
                    return Ok(Validation::Invalid("invalid peer id list".into()));
                }
            }
            Ok(Validation::Valid)
        })
        .prompt()?;

    let code = if code_raw.trim().is_empty() {
        None
    } else {
        Some(code_raw.trim().to_string())
    };

    let allowed_peers = allowed_peers_raw
        .trim()
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| PeerId::from_str(p).expect("validated above"))
        .collect();

    Ok(cli::Command::Listen(cli::ListenOpt {
        relay_address,
        code,
        identity_file: None,
        secret_key_seed: None,
        allowed_peers,
    }))
}

// ── Dial ──────────────────────────────────────────────────────────────────────

fn run_dial() -> Result<cli::Command, InquireError> {
    let conn = Select::new(
        "Connection type?",
        vec![DialConn::Relay, DialConn::Mdns, DialConn::Pairing],
    )
    .prompt()?;

    let (target, relay_address) = match conn {
        DialConn::Relay => {
            let target_raw = Text::new("Target relay circuit address")
                .with_placeholder(
                    "/ip4/203.0.113.10/tcp/4001/p2p/<relay>/p2p-circuit/p2p/<listener>",
                )
                .with_autocomplete(prefix_suggest(MULTIADDR_SUGGESTIONS))
                .with_validator(|s: &str| {
                    let v = s.trim();
                    if v.is_empty() {
                        return Ok(Validation::Invalid("target address cannot be empty".into()));
                    }
                    match v.parse::<Multiaddr>() {
                        Ok(_) => Ok(Validation::Valid),
                        Err(_) => Ok(Validation::Invalid("invalid circuit multiaddr".into())),
                    }
                })
                .prompt()?;
            let addr = target_raw
                .trim()
                .parse::<Multiaddr>()
                .expect("validated above");
            (DialTarget::RelayCircuit(addr), None)
        },
        DialConn::Mdns => {
            let peer_raw = Text::new("Peer ID")
                .with_placeholder("12D3KooWH3uVF6wv47WnArKHk5p6cvgCJEb74UTmxztmQDc298L3")
                .with_autocomplete(
                    |input: &str| -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
                        Ok(std::iter::once("12D3KooW")
                            .filter(|s| s.starts_with(input))
                            .map(ToString::to_string)
                            .collect())
                    },
                )
                .with_validator(|s: &str| {
                    let v = s.trim();
                    if v.is_empty() {
                        return Ok(Validation::Invalid("peer id cannot be empty".into()));
                    }
                    match PeerId::from_str(v) {
                        Ok(_) => Ok(Validation::Valid),
                        Err(_) => Ok(Validation::Invalid("invalid peer id".into())),
                    }
                })
                .prompt()?;
            let peer_id = PeerId::from_str(peer_raw.trim()).expect("validated above");
            (DialTarget::Mdns(peer_id), None)
        },
        DialConn::Pairing => {
            let code_raw = Text::new("Pairing code")
                .with_placeholder("42-river-ocean")
                .with_validator(|s: &str| {
                    if s.trim().is_empty() {
                        Ok(Validation::Invalid("pairing code cannot be empty".into()))
                    } else {
                        Ok(Validation::Valid)
                    }
                })
                .prompt()?;
            let relay_raw = Text::new("Relay address")
                .with_placeholder("/ip4/203.0.113.10/tcp/4001/p2p/12D3KooW...")
                .with_autocomplete(prefix_suggest(MULTIADDR_SUGGESTIONS))
                .with_validator(|s: &str| {
                    let v = s.trim();
                    if v.is_empty() {
                        return Ok(Validation::Invalid("relay address cannot be empty".into()));
                    }
                    match v.parse::<Multiaddr>() {
                        Ok(_) => Ok(Validation::Valid),
                        Err(_) => Ok(Validation::Invalid("invalid relay multiaddr".into())),
                    }
                })
                .prompt()?;
            let relay = relay_raw
                .trim()
                .parse::<Multiaddr>()
                .expect("validated above");
            (
                DialTarget::PairingCode(code_raw.trim().to_string()),
                Some(relay),
            )
        },
    };

    let tunnel = Select::new(
        "Tunnel mode?",
        vec![
            TunnelMode::Netcat,
            TunnelMode::Local,
            TunnelMode::Reverse,
            TunnelMode::Socks,
        ],
    )
    .prompt()?;

    let (local_forward, remote_forward, socks, gateway_ports) = match tunnel {
        TunnelMode::Netcat => (None, None, None, false),
        TunnelMode::Local => {
            let forward_raw = Text::new("Forward spec")
                .with_placeholder("8080:localhost:3000")
                .with_autocomplete(prefix_suggest(FORWARD_SUGGESTIONS))
                .with_validator(|s: &str| {
                    let v = s.trim();
                    if v.is_empty() {
                        return Ok(Validation::Invalid("forward spec cannot be empty".into()));
                    }
                    match ForwardSpec::from_str(v) {
                        Ok(_) => Ok(Validation::Valid),
                        Err(_) => Ok(Validation::Invalid("invalid forward spec".into())),
                    }
                })
                .prompt()?;
            let forward = ForwardSpec::from_str(forward_raw.trim()).expect("validated above");
            (Some(forward), None, None, false)
        },
        TunnelMode::Reverse => {
            let forward_raw = Text::new("Forward spec")
                .with_placeholder("9090:localhost:3000")
                .with_autocomplete(prefix_suggest(FORWARD_SUGGESTIONS))
                .with_validator(|s: &str| {
                    let v = s.trim();
                    if v.is_empty() {
                        return Ok(Validation::Invalid("forward spec cannot be empty".into()));
                    }
                    match ForwardSpec::from_str(v) {
                        Ok(_) => Ok(Validation::Valid),
                        Err(_) => Ok(Validation::Invalid("invalid forward spec".into())),
                    }
                })
                .prompt()?;
            let gateway_ports = Confirm::new("Enable gateway ports?")
                .with_default(false)
                .prompt()?;
            let forward = ForwardSpec::from_str(forward_raw.trim()).expect("validated above");
            (None, Some(forward), None, gateway_ports)
        },
        TunnelMode::Socks => {
            let port_raw = Text::new("SOCKS5 port")
                .with_placeholder("1080")
                .with_validator(|s: &str| {
                    let v = s.trim();
                    if v.is_empty() || v.parse::<u16>().is_ok() {
                        Ok(Validation::Valid)
                    } else {
                        Ok(Validation::Invalid("invalid port".into()))
                    }
                })
                .prompt()?;
            let port = if port_raw.trim().is_empty() {
                1080
            } else {
                port_raw.trim().parse::<u16>().expect("validated above")
            };
            (None, None, Some(port), false)
        },
    };

    Ok(cli::Command::Dial(cli::DialOpt {
        target,
        relay_address,
        identity_file: None,
        secret_key_seed: None,
        jump: Vec::new(),
        local_forward,
        remote_forward,
        socks,
        gateway_ports,
    }))
}
