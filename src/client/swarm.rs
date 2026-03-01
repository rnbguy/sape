use std::time::Duration;

use color_eyre::eyre::{bail, eyre, Result};
use futures::StreamExt;
use libp2p::{
    PeerId,
    core::multiaddr::{Multiaddr, Protocol},
    identify, mdns, rendezvous,
    swarm::SwarmEvent,
};
use tokio::time::timeout;
use tracing::{info, warn};

use super::{ClientBehaviour, ClientBehaviourEvent};

const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const PEER_DIAL_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn connect_and_identify(
    swarm: &mut libp2p::Swarm<ClientBehaviour>,
    relay_address: &Multiaddr,
) -> Result<()> {
    info!(relay_address = %relay_address, "dialing relay");
    swarm.dial(relay_address.clone())?;

    let mut learned_observed_addr = false;
    let mut told_relay_observed_addr = false;

    timeout(RELAY_CONNECT_TIMEOUT, async {
        while !(learned_observed_addr && told_relay_observed_addr) {
            let Some(event) = swarm.next().await else {
                bail!("client setup swarm stream ended unexpectedly");
            };

            match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!(address = %address, "client listening");
                }
                SwarmEvent::Behaviour(ClientBehaviourEvent::Identify(identify::Event::Sent {
                    ..
                })) => {
                    told_relay_observed_addr = true;
                }
                SwarmEvent::Behaviour(ClientBehaviourEvent::Identify(identify::Event::Received {
                    info: identify::Info { observed_addr, .. },
                    ..
                })) => {
                    info!(%observed_addr, "observed address from relay");
                    learned_observed_addr = true;
                }
                SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                    warn!(?peer_id, %error, "outgoing connection error during setup");
                }
                _ => {}
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| eyre!("relay connect timed out after 30s"))??;

    Ok(())
}

pub(crate) async fn wait_for_peer_connection(
    swarm: &mut libp2p::Swarm<ClientBehaviour>,
    expected_peer: PeerId,
) -> Result<()> {
    timeout(PEER_DIAL_TIMEOUT, async {
        loop {
            let Some(event) = swarm.next().await else {
                bail!("client dial swarm stream ended unexpectedly");
            };

            match event {
                SwarmEvent::ConnectionEstablished {
                    peer_id, endpoint, ..
                } => {
                    info!(%peer_id, ?endpoint, "connection established");
                    if peer_id == expected_peer {
                        return Ok(());
                    }
                }
                SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                    warn!(?peer_id, %error, "outgoing connection error before tunnel startup");
                }
                SwarmEvent::Behaviour(event) => {
                    info!(?event, "client behaviour event during dial");
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| eyre!("peer dial timed out after 30s"))?
}

pub(crate) fn log_mdns_event(event: &mdns::Event) {
    match event {
        mdns::Event::Discovered(peers) => {
            for (peer_id, addr) in peers {
                info!(%peer_id, %addr, "mDNS discovered peer");
            }
        }
        mdns::Event::Expired(peers) => {
            for (peer_id, addr) in peers {
                info!(%peer_id, %addr, "mDNS peer expired");
            }
        }
    }
}

pub(crate) async fn wait_for_mdns_and_connect(
    swarm: &mut libp2p::Swarm<ClientBehaviour>,
    expected_peer: PeerId,
) -> Result<()> {
    loop {
        let Some(event) = swarm.next().await else {
            bail!("swarm stream ended while waiting for mDNS discovery");
        };

        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(address = %address, "client listening");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                log_mdns_event(&mdns::Event::Discovered(peers.clone()));
                for (peer_id, addr) in peers {
                    if peer_id == expected_peer {
                        info!(%peer_id, %addr, "found target peer via mDNS, dialing");
                        swarm.dial(addr)?;
                        return wait_for_peer_connection(swarm, expected_peer).await;
                    }
                }
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                log_mdns_event(&mdns::Event::Expired(peers));
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!(%peer_id, ?endpoint, "connection established");
                if peer_id == expected_peer {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
}

pub(crate) async fn wait_for_rendezvous_discovery(
    swarm: &mut libp2p::Swarm<ClientBehaviour>,
    rendezvous_point: PeerId,
) -> Result<PeerId> {
    timeout(PEER_DIAL_TIMEOUT, async {
        loop {
            let Some(event) = swarm.next().await else {
                bail!("swarm stream ended while waiting for rendezvous discovery");
            };

            match event {
                SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                    rendezvous::client::Event::Discovered {
                        registrations,
                        rendezvous_node,
                        ..
                    },
                )) => {
                    if rendezvous_node != rendezvous_point {
                        continue;
                    }

                    let Some(registration) = registrations.first() else {
                        continue;
                    };

                    let peer = registration.record.peer_id();
                    for address in registration.record.addresses() {
                        let address = if matches!(address.iter().last(), Some(Protocol::P2p(id)) if id == peer)
                        {
                            address.clone()
                        } else {
                            address.clone().with(Protocol::P2p(peer))
                        };

                        info!(%peer, %address, "discovered peer via pairing code");
                        if let Err(err) = swarm.dial(address) {
                            warn!(%peer, %err, "failed dialing discovered address");
                        }
                    }

                    wait_for_peer_connection(swarm, peer).await?;
                    return Ok(peer);
                }
                SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                    rendezvous::client::Event::DiscoverFailed {
                        error,
                        namespace,
                        rendezvous_node,
                    },
                )) => {
                    if rendezvous_node == rendezvous_point {
                        bail!(
                            "rendezvous discovery failed for namespace {:?}: {:?}",
                            namespace,
                            error
                        );
                    }
                }
                SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                    warn!(?peer_id, %error, "outgoing connection error during rendezvous discovery");
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| eyre!("rendezvous discovery timed out after 30s"))?
}

pub(crate) async fn drive_client_swarm(mut swarm: libp2p::Swarm<ClientBehaviour>) -> Result<()> {
    loop {
        let Some(event) = swarm.next().await else {
            bail!("client swarm stream ended unexpectedly");
        };

        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(address = %address, "client listening");
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!(%peer_id, ?endpoint, "connection established");
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                cause,
                endpoint,
                ..
            } => {
                warn!(%peer_id, ?endpoint, ?cause, "connection closed");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::RelayClient(event)) => {
                info!(?event, "relay client event");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Dcutr(event)) => {
                info!(?event, "dcutr event");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Identify(event)) => {
                info!(?event, "identify event");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Stream(event)) => {
                info!(?event, "stream event");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Mdns(event)) => {
                log_mdns_event(&event);
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(event)) => {
                match event {
                    rendezvous::client::Event::Registered {
                        namespace,
                        ttl,
                        rendezvous_node,
                    } => {
                        info!(?namespace, ?ttl, %rendezvous_node, "rendezvous registration accepted");
                    }
                    rendezvous::client::Event::Discovered {
                        registrations,
                        rendezvous_node,
                        ..
                    } => {
                        info!(count = registrations.len(), %rendezvous_node, "rendezvous discovery result");
                    }
                    rendezvous::client::Event::RegisterFailed {
                        namespace,
                        error,
                        rendezvous_node,
                    } => {
                        warn!(?namespace, ?error, %rendezvous_node, "rendezvous registration failed");
                    }
                    rendezvous::client::Event::DiscoverFailed {
                        namespace,
                        error,
                        rendezvous_node,
                    } => {
                        warn!(?namespace, ?error, %rendezvous_node, "rendezvous discovery failed");
                    }
                    other => {
                        info!(?other, "rendezvous client event");
                    }
                }
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Autonat(event)) => {
                info!(?event, "autonat event");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Upnp(event)) => {
                info!(?event, "upnp event");
            }
            SwarmEvent::Behaviour(ClientBehaviourEvent::Ping(_)) => {}
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!(?peer_id, %error, "outgoing connection error");
            }
            _ => {}
        }
    }
}
