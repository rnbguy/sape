use clap::{ArgGroup, Parser, Subcommand};
use libp2p::core::multiaddr::Multiaddr;

use crate::types::{DialTarget, ForwardSpec};

#[derive(Debug, Parser)]
#[command(name = "sape")]
#[command(about = "P2P tunneling over libp2p with NAT traversal")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Run a circuit relay server", alias = "r")]
    Relay(RelayOpt),
    #[command(about = "Register on relay and accept incoming tunnels", alias = "l")]
    Listen(ListenOpt),
    #[command(about = "Connect to a listener and start a tunnel", alias = "d")]
    Dial(DialOpt),
}

#[derive(Debug, Parser)]
pub struct RelayOpt {
    #[arg(long, default_value_t = false)]
    pub use_ipv6: bool,
    /// Path to Ed25519 identity file (created if missing)
    #[arg(long)]
    pub identity_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub secret_key_seed: Option<u8>,
    #[arg(long, default_value_t = 4001)]
    pub port: u16,
}

#[derive(Debug, Parser)]
pub struct ListenOpt {
    #[arg(long)]
    pub relay_address: Option<Multiaddr>,
    #[arg(long)]
    pub code: Option<String>,
    /// Path to Ed25519 identity file (created if missing)
    #[arg(long)]
    pub identity_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub secret_key_seed: Option<u8>,
    /// Restrict tunnel access to these peer IDs only. Can be specified multiple
    /// times.
    #[arg(long = "allowed-peer")]
    pub allowed_peers: Vec<libp2p::PeerId>,
}

#[derive(Debug, Parser)]
#[command(group(
    ArgGroup::new("tunnel_mode")
        .args(["local_forward", "remote_forward", "socks"])
        .multiple(false)
))]
pub struct DialOpt {
    /// Dial target: /mdns/<peer-id> for LAN or full relay circuit multiaddr
    pub target: DialTarget,
    #[arg(long)]
    pub relay_address: Option<Multiaddr>,
    /// Path to Ed25519 identity file (created if missing)
    #[arg(long)]
    pub identity_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub secret_key_seed: Option<u8>,
    #[arg(long = "jump", short = 'J')]
    pub jump: Vec<Multiaddr>,
    #[arg(short = 'L', long = "local-forward")]
    pub local_forward: Option<ForwardSpec>,
    #[arg(short = 'R', long = "remote-forward")]
    pub remote_forward: Option<ForwardSpec>,
    #[arg(short = 'D', long = "socks")]
    pub socks: Option<u16>,
    /// Allow remote hosts to connect to -R forwarded ports (binds 0.0.0.0
    /// instead of 127.0.0.1)
    #[arg(short = 'g', long = "gateway-ports")]
    pub gateway_ports: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MDNS_TARGET: &str = "/mdns/12D3KooWH3uVF6wv47WnArKHk5p6cvgCJEb74UTmxztmQDc298L3";

    #[test]
    fn dial_parses_socks_flag() {
        let cli = Cli::try_parse_from(["sape", "dial", "-D", "1080", VALID_MDNS_TARGET])
            .expect("dial -D should parse");

        match cli.command {
            Some(Command::Dial(opt)) => {
                assert_eq!(opt.socks, Some(1080));
                assert!(!opt.gateway_ports);
                assert!(opt.local_forward.is_none());
                assert!(opt.remote_forward.is_none());
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn dial_parses_reverse_forward_gateway_ports() {
        let cli = Cli::try_parse_from([
            "sape",
            "dial",
            "-R",
            "9090:localhost:3000",
            "-g",
            VALID_MDNS_TARGET,
        ])
        .expect("dial -R -g should parse");

        match cli.command {
            Some(Command::Dial(opt)) => {
                assert!(opt.gateway_ports);
                assert!(opt.socks.is_none());
                let reverse = opt.remote_forward.expect("remote-forward should be set");
                assert_eq!(reverse.bind_port, 9090);
                assert_eq!(reverse.target, "localhost:3000");
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn dial_rejects_multiple_tunnel_modes() {
        let err = Cli::try_parse_from([
            "sape",
            "dial",
            "-D",
            "1080",
            "-L",
            "8080:localhost:3000",
            VALID_MDNS_TARGET,
        ])
        .expect_err("-D and -L together should be rejected");

        let msg = err.to_string();
        assert!(
            msg.contains("cannot be used with"),
            "unexpected clap error: {msg}"
        );
    }
}
