use std::sync::Arc;

use libp2p::swarm::NetworkBehaviour;
use libp2p::{autonat, dcutr, identify, mdns, ping, relay, rendezvous, upnp};
use libp2p_stream as p2pstream;

pub(crate) mod builder;
pub(crate) mod dial;
pub(crate) mod listen;
pub(crate) mod swarm;

pub(crate) use builder::{build_client_swarm, start_listeners};
pub use dial::run_dial;
pub use listen::run_listen;

#[derive(NetworkBehaviour)]
pub(crate) struct ClientBehaviour {
    pub(crate) relay_client: relay::client::Behaviour,
    pub(crate) ping: ping::Behaviour,
    pub(crate) identify: identify::Behaviour,
    pub(crate) dcutr: dcutr::Behaviour,
    pub(crate) stream: p2pstream::Behaviour,
    pub(crate) mdns: mdns::tokio::Behaviour,
    pub(crate) rendezvous: rendezvous::client::Behaviour,
    pub(crate) autonat: autonat::v2::client::Behaviour,
    pub(crate) upnp: upnp::tokio::Behaviour,
}

pub(crate) enum DialMode {
    Netcat,
    LocalForward {
        bind_port: u16,
        target: Arc<str>,
    },
    ReverseForward {
        bind_port: u16,
        target: Arc<str>,
        gateway_ports: bool,
    },
    Socks5 {
        bind_port: u16,
    },
}
