use std::sync::Arc;

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use futures::StreamExt;
use libp2p::core::multiaddr::Protocol;
use libp2p::{StreamProtocol, rendezvous};
use tracing::{error, info, warn};

use super::swarm::{connect_and_identify, drive_client_swarm};
use super::{build_client_swarm, start_listeners};
use crate::tunnel::{self, TunnelRequest};
use crate::{
    ListenOpt, forward, netcat, pairing, peer_id_from_multiaddr, resolve_identity,
    validate_relay_address,
};

pub async fn run_listen(
    opt: ListenOpt,
    tunnel_proto: StreamProtocol,
    namespace: &str,
) -> Result<()> {
    if let Some(ref addr) = opt.relay_address {
        validate_relay_address(addr)
            .wrap_err_with(|| format!("invalid --relay-address '{addr}'"))?;
    }

    let local_key = resolve_identity(opt.identity_file.as_deref(), opt.secret_key_seed)?;
    let local_peer_id = local_key.public().to_peer_id();

    let mut swarm = build_client_swarm(local_key, namespace).await?;

    // Always start TCP+QUIC listeners (needed for mDNS direct connections)
    start_listeners(&mut swarm)?;

    // If relay address provided, connect and register circuit reservation
    if let Some(ref relay_address) = opt.relay_address {
        connect_and_identify(&mut swarm, relay_address).await?;

        let circuit_addr = relay_address.clone().with(Protocol::P2pCircuit);
        swarm.listen_on(circuit_addr)?;

        let external_circuit_addr = relay_address
            .clone()
            .with(Protocol::P2pCircuit)
            .with(Protocol::P2p(local_peer_id));
        swarm.add_external_address(external_circuit_addr);

        let relay_peer_id = peer_id_from_multiaddr(relay_address)
            .ok_or_else(|| eyre!("missing relay peer id in --relay-address"))?;
        let code = opt.code.clone().unwrap_or_else(pairing::generate_code);
        let namespace = rendezvous::Namespace::new(code.clone())
            .map_err(|err| eyre!("invalid pairing code namespace: {err}"))?;

        swarm
            .behaviour_mut()
            .rendezvous
            .register(namespace, relay_peer_id, None)?;
        info!(%code, "Pairing code: {code}");

        let dial_address = relay_address
            .clone()
            .with(Protocol::P2pCircuit)
            .with(Protocol::P2p(local_peer_id));
        info!(%dial_address, "Relay dial address: {dial_address}");
    }

    // Always print mDNS address
    info!("LAN dial address: /mdns/{local_peer_id}");

    let mut incoming = swarm
        .behaviour_mut()
        .stream
        .new_control()
        .accept(tunnel_proto.clone())
        .wrap_err("failed to set stream accept protocol")?;

    let open_control = swarm.behaviour_mut().stream.new_control();
    let allowed_peers = Arc::new(opt.allowed_peers.clone());

    tokio::spawn(async move {
        if let Err(err) = drive_client_swarm(swarm).await {
            error!(%err, "listener swarm task exited");
        }
    });

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("listener shutting down");
                break;
            }
            result = incoming.next() => {
                let Some((peer, mut stream)) = result else {
                    bail!("listener tunnel accept loop ended unexpectedly");
                };
                let open_control = open_control.clone();
                let allowed_peers = Arc::clone(&allowed_peers);
                let tunnel_proto = tunnel_proto.clone();
                tokio::spawn(async move {
                    if !allowed_peers.is_empty() && !allowed_peers.contains(&peer) {
                        warn!(%peer, "rejected tunnel request from unauthorized peer");
                        return;
                    }

                    let request = match tunnel::read_tunnel_request(&mut stream).await {
                        Ok(request) => request,
                        Err(err) => {
                            warn!(%peer, %err, "failed reading tunnel request");
                            return;
                        }
                    };

                    match request {
                        TunnelRequest::Netcat => {
                            info!(%peer, "netcat stream accepted");
                            if let Err(err) = netcat::run_netcat(&mut stream).await {
                                warn!(%peer, %err, "netcat stream ended with error");
                            }
                        }
                        TunnelRequest::LocalForward { target } => {
                            info!(%peer, %target, "local-forward request accepted");
                            let target_ref = target.clone();
                            if let Err(err) = forward::handle_forward_to_target(stream, target).await {
                                error!(%peer, target = %target_ref, "local-forward bridge failed: {err:#}");
                            }
                        }
                        TunnelRequest::ReverseForward { bind_port, target, gateway_ports } => {
                            info!(%peer, %bind_port, %target, %gateway_ports, "reverse-forward bind request accepted");
                            let result = forward::start_reverse_listener(
                                open_control.clone(),
                                peer,
                                bind_port,
                                Arc::from(target.as_str()),
                                gateway_ports,
                                tunnel_proto,
                            )
                            .await;

                            let ack_result = match &result {
                                Ok(()) => Ok(()),
                                Err(err) => Err(format!("{err:#}")),
                            };
                            if let Err(err) = forward::send_reverse_ack(&mut stream, ack_result).await {
                                warn!(%peer, %err, "failed to write reverse-forward ack");
                            }

                            if let Err(err) = result {
                                error!(%peer, "reverse-forward bind failed: {err:#}");
                            }
                        }
                    }
                });
            }
        }
    }
    Ok(())
}
