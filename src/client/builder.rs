use std::time::Duration;

use color_eyre::eyre::Result;
use libp2p::{autonat, identify, mdns, noise, ping, rendezvous, tcp, upnp, yamux};
use libp2p_stream as p2pstream;
use rand::rngs::OsRng;

use super::ClientBehaviour;

pub(crate) async fn build_client_swarm(
    keypair: libp2p::identity::Keypair,
    namespace: &str,
) -> Result<libp2p::Swarm<ClientBehaviour>> {
    let peer_id = keypair.public().to_peer_id();
    let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_dns()?
        .with_websocket(noise::Config::new, yamux::Config::default)
        .await?
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|keypair, relay_behaviour| ClientBehaviour {
            relay_client: relay_behaviour,
            ping: ping::Behaviour::new(
                ping::Config::default().with_interval(Duration::from_secs(3)),
            ),
            identify: identify::Behaviour::new(
                identify::Config::new(
                    crate::protocol::client_identify_protocol(namespace),
                    keypair.public(),
                )
                .with_agent_version(format!("sape/{}", env!("CARGO_PKG_VERSION"))),
            ),
            dcutr: libp2p::dcutr::Behaviour::new(keypair.public().to_peer_id()),
            stream: p2pstream::Behaviour::new(),
            mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                .expect("mDNS behaviour creation failed"),
            rendezvous: rendezvous::client::Behaviour::new(keypair.clone()),
            autonat: autonat::v2::client::Behaviour::new(
                OsRng,
                autonat::v2::client::Config::default(),
            ),
            upnp: upnp::tokio::Behaviour::default(),
        })?
        .with_swarm_config(|config| config.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();

    Ok(swarm)
}

pub(crate) fn start_listeners(swarm: &mut libp2p::Swarm<ClientBehaviour>) -> Result<()> {
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    Ok(())
}
