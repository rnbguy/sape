use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use futures::StreamExt;
use libp2p::{
    PeerId, Stream, StreamProtocol,
    core::multiaddr::{Multiaddr, Protocol},
    autonat, identify, noise, ping, relay, rendezvous,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use libp2p_stream as p2pstream;
use tokio::sync::mpsc;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{info, warn};

use crate::{RelayOpt, jump, peer_id_from_multiaddr, resolve_identity};

enum SwarmCommand {
    Dial(Multiaddr),
}

#[derive(NetworkBehaviour)]
struct ServerBehaviour {
    relay: relay::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    rendezvous: rendezvous::server::Behaviour,
    autonat: autonat::v2::server::Behaviour,
    stream: p2pstream::Behaviour,
}

pub async fn run_relay(
    opt: RelayOpt,
    tunnel_proto: StreamProtocol,
    jump_proto: StreamProtocol,
    namespace: &str,
) -> Result<()> {
    let local_key = resolve_identity(opt.identity_file.as_deref(), opt.secret_key_seed)?;
    let relay_peer_id = local_key.public().to_peer_id();

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
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
        .with_behaviour(|key| ServerBehaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), relay::Config::default()),
            ping: ping::Behaviour::new(ping::Config::new()),
            identify: identify::Behaviour::new(
                identify::Config::new(
                    crate::protocol::relay_identify_protocol(namespace),
                    key.public(),
                )
                .with_agent_version(format!("sape/{}", env!("CARGO_PKG_VERSION"))),
            ),
            rendezvous: rendezvous::server::Behaviour::new(rendezvous::server::Config::default()),
            autonat: autonat::v2::server::Behaviour::default(),
            stream: p2pstream::Behaviour::new(),
        })?
        .build();

    let mut incoming_jump = swarm
        .behaviour_mut()
        .stream
        .new_control()
        .accept(jump_proto.clone())
        .wrap_err("failed to set jump stream accept protocol")?;
    let open_control = swarm.behaviour_mut().stream.new_control();

    let (dial_tx, mut dial_rx) = mpsc::channel::<SwarmCommand>(32);

    let ip_protocol = if opt.use_ipv6 {
        Protocol::from(Ipv6Addr::UNSPECIFIED)
    } else {
        Protocol::from(Ipv4Addr::UNSPECIFIED)
    };

    swarm.listen_on(
        Multiaddr::empty()
            .with(ip_protocol.clone())
            .with(Protocol::Tcp(opt.port)),
    )?;
    swarm.listen_on(
        Multiaddr::empty()
            .with(ip_protocol)
            .with(Protocol::Udp(opt.port))
            .with(Protocol::QuicV1),
    )?;
    swarm.listen_on(
        Multiaddr::empty()
            .with(if opt.use_ipv6 {
                Protocol::from(Ipv6Addr::UNSPECIFIED)
            } else {
                Protocol::from(Ipv4Addr::UNSPECIFIED)
            })
            .with(Protocol::Tcp(opt.port + 1))
            .with(Protocol::Ws("/".into())),
    )?;

    info!(%relay_peer_id, "relay server started");

    let dial_tx_clone = dial_tx.clone();
    let open_control_clone = open_control.clone();
    let tunnel_proto_clone = tunnel_proto.clone();
    let jump_proto_clone = jump_proto.clone();
    tokio::spawn(async move {
        while let Some((peer, stream)) = incoming_jump.next().await {
            let dial_tx = dial_tx_clone.clone();
            let open_control = open_control_clone.clone();
            let tunnel_proto = tunnel_proto_clone.clone();
            let jump_proto = jump_proto_clone.clone();
            tokio::spawn(async move {
                if let Err(err) =
                    handle_jump_request(peer, stream, dial_tx, open_control, tunnel_proto, jump_proto)
                        .await
                {
                    warn!(%peer, %err, "jump request failed");
                }
            });
        }
    });

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("relay shutting down");
                break;
            }
            event = swarm.next() => {
                let Some(event) = event else {
                    bail!("server swarm stream ended unexpectedly");
                };

                match event {
                    SwarmEvent::Behaviour(ServerBehaviourEvent::Identify(identify::Event::Received {
                        info: identify::Info { observed_addr, .. },
                        ..
                    })) => {
                        swarm.add_external_address(observed_addr);
                    }
                    SwarmEvent::Behaviour(ServerBehaviourEvent::Rendezvous(event)) => {
                        info!(?event, "rendezvous server event");
                    }
                    SwarmEvent::Behaviour(ServerBehaviourEvent::Autonat(event)) => {
                        info!(?event, "autonat server event");
                    }
                    SwarmEvent::Behaviour(event) => {
                        info!(?event, "relay behaviour event");
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        let relay_address = address.clone().with(Protocol::P2p(relay_peer_id));
                        info!(address = %relay_address, %relay_peer_id, "relay listening address");
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        warn!(?peer_id, %error, "relay outgoing connection error");
                    }
                    _ => {}
                }
            }
            Some(command) = dial_rx.recv() => {
                match command {
                    SwarmCommand::Dial(addr) => {
                        if let Err(err) = swarm.dial(addr.clone()) {
                            warn!(%addr, %err, "failed to dial jump target");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_jump_request(
    peer: PeerId,
    mut inbound: Stream,
    dial_tx: mpsc::Sender<SwarmCommand>,
    mut open_control: p2pstream::Control,
    tunnel_proto: StreamProtocol,
    jump_proto: StreamProtocol,
) -> Result<()> {
    let chain = jump::read_jump_chain(&mut inbound)
        .await
        .wrap_err("failed reading jump chain")?;

    if chain.hops.is_empty() {
        jump::write_jump_result(
            &mut inbound,
            &jump::JumpResult::Error("empty hop chain".to_string()),
        )
        .await?;
        bail!("empty jump chain from {peer}");
    }

    let next_hop_str = &chain.hops[0];
    let next_hop: Multiaddr = next_hop_str
        .parse()
        .map_err(|e| eyre!("invalid multiaddr in jump chain: {e}"))?;
    let next_peer = peer_id_from_multiaddr(&next_hop)
        .ok_or_else(|| eyre!("missing peer id in jump hop: {next_hop_str}"))?;

    info!(%peer, %next_hop, remaining = chain.hops.len() - 1, "processing jump request");

    dial_tx
        .send(SwarmCommand::Dial(next_hop))
        .await
        .map_err(|_| eyre!("swarm dial channel closed"))?;

    let remaining_hops = chain.hops[1..].to_vec();
    let outbound_protocol = if remaining_hops.is_empty() {
        tunnel_proto
    } else {
        jump_proto
    };

    let mut outbound = None;
    for attempt in 0..10 {
        match open_control
            .open_stream(next_peer, outbound_protocol.clone())
            .await
        {
            Ok(stream) => {
                outbound = Some(stream);
                break;
            }
            Err(_) if attempt < 9 => {
                tokio::time::sleep(Duration::from_millis(500 * (attempt + 1) as u64)).await;
            }
            Err(err) => {
                let error_msg = format!("failed to connect to hop {next_peer}: {err}");
                jump::write_jump_result(&mut inbound, &jump::JumpResult::Error(error_msg.clone()))
                    .await?;
                bail!(error_msg);
            }
        }
    }

    let mut outbound = outbound.ok_or_else(|| eyre!("unreachable"))?;

    if !remaining_hops.is_empty() {
        let remaining_chain = jump::JumpChain {
            hops: remaining_hops,
        };
        jump::write_jump_chain(&mut outbound, &remaining_chain).await?;

        let result = jump::read_jump_result(&mut outbound).await?;
        match result {
            jump::JumpResult::Ok => {}
            jump::JumpResult::Error(e) => {
                jump::write_jump_result(&mut inbound, &jump::JumpResult::Error(e.clone())).await?;
                bail!("downstream jump failed: {e}");
            }
        }
    }

    jump::write_jump_result(&mut inbound, &jump::JumpResult::Ok).await?;
    info!(%peer, %next_peer, "jump bridge established");

    let mut inbound_compat = inbound.compat();
    let mut outbound_compat = outbound.compat();
    let _ = tokio::io::copy_bidirectional(&mut inbound_compat, &mut outbound_compat).await;

    Ok(())
}
