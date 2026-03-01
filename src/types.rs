use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use libp2p::core::multiaddr::{Multiaddr, Protocol};
use libp2p::{identity, PeerId};

use crate::pairing;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AddressError {
    #[error("missing '/p2p/<relay-peer-id>' suffix")]
    MissingPeerId,
    #[error("unsupported transport; use '/tcp/<port>/p2p/...' or '/udp/<port>/quic-v1/p2p/...'")]
    UnsupportedTransport,
    #[error("missing '/p2p-circuit' in relay circuit address")]
    MissingCircuit,
    #[error("missing listener '/p2p/<listener-peer-id>' suffix in circuit address")]
    MissingListenerPeerId,
}

#[derive(Debug, thiserror::Error)]
pub enum DialTargetError {
    #[error("invalid peer id in /mdns/ address: {0}")]
    InvalidPeerId(String),
    #[error("invalid multiaddr: {0}")]
    InvalidMultiaddr(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ForwardSpecError {
    #[error("expected '<bind-port>:<host>:<port>', got '{0}'")]
    MissingSeparator(String),
    #[error("invalid bind port '{0}'")]
    InvalidPort(String),
    #[error("target must be '<host>:<port>', got '{0}'")]
    InvalidTarget(String),
}

// ---------------------------------------------------------------------------
// Keypair helpers
// ---------------------------------------------------------------------------

pub fn generate_ed25519(secret_key_seed: u8) -> identity::Keypair {
    let mut bytes = [0u8; 32];
    bytes[0] = secret_key_seed;
    identity::Keypair::ed25519_from_bytes(bytes).expect("invalid ed25519 bytes")
}

pub fn resolve_keypair(seed: Option<u8>) -> identity::Keypair {
    seed.map_or_else(identity::Keypair::generate_ed25519, generate_ed25519)
}

pub fn load_or_create_identity(path: &Path) -> Result<identity::Keypair, std::io::Error> {
    if path.exists() {
        let bytes = fs::read(path)?;
        identity::Keypair::from_protobuf_encoding(&bytes)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
    } else {
        let keypair = identity::Keypair::generate_ed25519();
        let bytes = keypair
            .to_protobuf_encoding()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
        fs::write(path, &bytes)?;
        Ok(keypair)
    }
}

pub fn resolve_identity(
    identity_file: Option<&Path>,
    secret_key_seed: Option<u8>,
) -> Result<identity::Keypair, std::io::Error> {
    if let Some(path) = identity_file {
        load_or_create_identity(path)
    } else {
        Ok(resolve_keypair(secret_key_seed))
    }
}

// ---------------------------------------------------------------------------
// Address validation
// ---------------------------------------------------------------------------

pub fn validate_relay_address(addr: &Multiaddr) -> Result<(), AddressError> {
    let mut has_p2p = false;
    let mut has_tcp = false;
    let mut has_udp = false;
    let mut has_quic_v1 = false;

    for protocol in addr.iter() {
        match protocol {
            Protocol::P2p(_) => has_p2p = true,
            Protocol::Tcp(_) => has_tcp = true,
            Protocol::Udp(_) => has_udp = true,
            Protocol::QuicV1 => has_quic_v1 = true,
            _ => {}
        }
    }

    if !has_p2p {
        return Err(AddressError::MissingPeerId);
    }

    if has_tcp || (has_udp && has_quic_v1) {
        return Ok(());
    }

    Err(AddressError::UnsupportedTransport)
}

pub fn relay_base_from_circuit_address(
    addr: &Multiaddr,
) -> Result<(Multiaddr, PeerId), AddressError> {
    let mut relay_base = Multiaddr::empty();
    let mut seen_circuit = false;
    let mut remote_peer = None;

    for protocol in addr.iter() {
        if !seen_circuit {
            if matches!(protocol, Protocol::P2pCircuit) {
                seen_circuit = true;
                continue;
            }
            relay_base.push(protocol);
            continue;
        }

        if let Protocol::P2p(peer_id) = protocol {
            remote_peer = Some(peer_id);
        }
    }

    if !seen_circuit {
        return Err(AddressError::MissingCircuit);
    }

    let remote_peer = remote_peer.ok_or(AddressError::MissingListenerPeerId)?;
    validate_relay_address(&relay_base)?;
    Ok((relay_base, remote_peer))
}

pub fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|protocol| match protocol {
        Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Data types with FromStr
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum DialTarget {
    Mdns(PeerId),
    RelayCircuit(Multiaddr),
    PairingCode(String),
}

impl FromStr for DialTarget {
    type Err = DialTargetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if pairing::is_pairing_code(s) {
            return Ok(Self::PairingCode(s.to_string()));
        }

        if let Some(peer_str) = s.strip_prefix("/mdns/") {
            let peer_id = PeerId::from_str(peer_str)
                .map_err(|e| DialTargetError::InvalidPeerId(e.to_string()))?;
            return Ok(Self::Mdns(peer_id));
        }

        let addr: Multiaddr = s.parse().map_err(|e: libp2p::multiaddr::Error| {
            DialTargetError::InvalidMultiaddr(e.to_string())
        })?;
        Ok(Self::RelayCircuit(addr))
    }
}

#[derive(Debug, Clone)]
pub struct ForwardSpec {
    pub bind_port: u16,
    pub target: String,
}

impl FromStr for ForwardSpec {
    type Err = ForwardSpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (bind, target) = s
            .split_once(':')
            .ok_or_else(|| ForwardSpecError::MissingSeparator(s.to_string()))?;

        let bind_port: u16 = bind
            .parse()
            .map_err(|_| ForwardSpecError::InvalidPort(bind.to_string()))?;

        if target.is_empty() || !target.contains(':') {
            return Err(ForwardSpecError::InvalidTarget(target.to_string()));
        }

        Ok(Self {
            bind_port,
            target: target.to_string(),
        })
    }
}

impl fmt::Display for ForwardSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.bind_port, self.target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn dial_target_mdns_valid() {
        let peer_id = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let input = format!("/mdns/{peer_id}");
        let target = DialTarget::from_str(&input).expect("mdns target should parse");
        assert!(matches!(target, DialTarget::Mdns(id) if id == peer_id));
    }

    #[test]
    fn dial_target_mdns_invalid_peer() {
        assert!(DialTarget::from_str("/mdns/notapeerid").is_err());
    }

    #[test]
    fn dial_target_relay_circuit() {
        let peer1 = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let peer2 = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{peer1}/p2p-circuit/p2p/{peer2}");
        let target = DialTarget::from_str(&addr).expect("circuit target should parse");
        assert!(matches!(target, DialTarget::RelayCircuit(_)));
    }

    #[test]
    fn dial_target_pairing_code() {
        let target = DialTarget::from_str("42-river-ocean").expect("pairing code should parse");
        assert!(matches!(target, DialTarget::PairingCode(code) if code == "42-river-ocean"));
    }

    #[test]
    fn forward_spec_valid() {
        let spec = ForwardSpec::from_str("8080:localhost:3000").expect("valid forward spec");
        assert_eq!(spec.bind_port, 8080);
        assert_eq!(spec.target, "localhost:3000");
    }

    #[test]
    fn forward_spec_missing_separator() {
        assert!(ForwardSpec::from_str("8080").is_err());
    }

    #[test]
    fn forward_spec_invalid_port() {
        assert!(ForwardSpec::from_str("abc:localhost:3000").is_err());
    }

    #[test]
    fn forward_spec_invalid_target() {
        assert!(ForwardSpec::from_str("8080:noport").is_err());
    }

    #[test]
    fn forward_spec_display() {
        let spec = ForwardSpec {
            bind_port: 8080,
            target: "localhost:3000".to_string(),
        };
        assert_eq!(spec.to_string(), "8080:localhost:3000");
    }

    #[test]
    fn validate_relay_address_tcp() {
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{peer}")
            .parse()
            .expect("valid tcp relay address");
        assert!(validate_relay_address(&addr).is_ok());
    }

    #[test]
    fn validate_relay_address_quic() {
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/udp/4001/quic-v1/p2p/{peer}")
            .parse()
            .expect("valid quic relay address");
        assert!(validate_relay_address(&addr).is_ok());
    }

    #[test]
    fn validate_relay_address_missing_p2p() {
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().expect("valid multiaddr");
        assert!(validate_relay_address(&addr).is_err());
    }

    #[test]
    fn validate_relay_address_no_transport() {
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/p2p/{peer}")
            .parse()
            .expect("valid multiaddr");
        assert!(validate_relay_address(&addr).is_err());
    }

    #[test]
    fn relay_base_from_circuit_valid() {
        let relay_peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let target_peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr: Multiaddr =
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay_peer}/p2p-circuit/p2p/{target_peer}")
                .parse()
                .expect("valid circuit address");
        let (base, peer) = relay_base_from_circuit_address(&addr).expect("extract relay base");
        assert_eq!(peer, target_peer);
        let expected_base: Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay_peer}")
            .parse()
            .expect("valid relay base");
        assert_eq!(base, expected_base);
    }

    #[test]
    fn relay_base_from_circuit_missing_circuit() {
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{peer}")
            .parse()
            .expect("valid relay address");
        assert!(relay_base_from_circuit_address(&addr).is_err());
    }

    #[test]
    fn peer_id_from_multiaddr_present() {
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{peer}")
            .parse()
            .expect("valid relay address");
        assert_eq!(peer_id_from_multiaddr(&addr), Some(peer));
    }

    #[test]
    fn peer_id_from_multiaddr_absent() {
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().expect("valid multiaddr");
        assert_eq!(peer_id_from_multiaddr(&addr), None);
    }

    #[test]
    fn resolve_keypair_with_seed() {
        let kp1 = resolve_keypair(Some(42));
        let kp2 = resolve_keypair(Some(42));
        assert_eq!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
    }

    #[test]
    fn resolve_keypair_without_seed() {
        let kp1 = resolve_keypair(None);
        let kp2 = resolve_keypair(None);
        assert_ne!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
    }

    #[test]
    fn generate_ed25519_is_deterministic_for_same_seed() {
        let kp1 = generate_ed25519(7);
        let kp2 = generate_ed25519(7);
        let kp3 = generate_ed25519(8);
        assert_eq!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
        assert_ne!(kp1.public().to_peer_id(), kp3.public().to_peer_id());
    }
}
