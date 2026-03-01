use std::sync::Arc;

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use libp2p::{StreamProtocol, rendezvous};
use tracing::{error, info};

use super::swarm::{
    connect_and_identify, drive_client_swarm, wait_for_mdns_and_connect, wait_for_peer_connection,
    wait_for_rendezvous_discovery,
};
use super::{DialMode, build_client_swarm, start_listeners};
use crate::tunnel::{self, TunnelRequest};
use crate::{
    DialOpt, DialTarget, forward, jump, netcat, peer_id_from_multiaddr,
    relay_base_from_circuit_address, resolve_identity,
};

pub async fn run_dial(
    opt: DialOpt,
    tunnel_proto: StreamProtocol,
    jump_proto: StreamProtocol,
    namespace: &str,
) -> Result<()> {
    let mode = parse_dial_mode(&opt);
    let relay_address_opt = opt.relay_address.clone();

    let local_key = resolve_identity(opt.identity_file.as_deref(), opt.secret_key_seed)?;

    let mut swarm = build_client_swarm(local_key, namespace).await?;
    start_listeners(&mut swarm)?;

    if !opt.jump.is_empty() {
        let target_addr_str = match &opt.target {
            DialTarget::RelayCircuit(addr) => addr.to_string(),
            DialTarget::Mdns(_) => bail!("cannot use --jump with mDNS targets"),
            DialTarget::PairingCode(_) => {
                bail!("cannot use --jump with pairing codes (resolve the code first)")
            },
        };

        let mut hops: Vec<String> = opt.jump[1..].iter().map(|a| a.to_string()).collect();
        hops.push(target_addr_str);

        let first_jump = &opt.jump[0];
        info!(jump = %first_jump, chain_len = hops.len(), "initiating jump chain");

        connect_and_identify(&mut swarm, first_jump).await?;

        let first_peer = peer_id_from_multiaddr(first_jump)
            .ok_or_else(|| eyre!("missing peer id in --jump address"))?;

        let mut open_control = swarm.behaviour_mut().stream.new_control();

        tokio::spawn(async move {
            if let Err(err) = drive_client_swarm(swarm).await {
                error!(%err, "dialer swarm task exited");
            }
        });

        let mut jump_stream = open_control
            .open_stream(first_peer, jump_proto.clone())
            .await
            .map_err(|err| eyre!("failed to open jump stream: {err}"))?;

        let chain = jump::JumpChain { hops };
        jump::write_jump_chain(&mut jump_stream, &chain).await?;

        let result = jump::read_jump_result(&mut jump_stream).await?;
        match result {
            jump::JumpResult::Ok => {
                info!("jump chain established");
            },
            jump::JumpResult::Error(e) => {
                bail!("jump chain failed: {e}");
            },
        }

        match &mode {
            DialMode::Netcat => {
                tunnel::write_tunnel_request(&mut jump_stream, &TunnelRequest::Netcat).await?;
                netcat::run_netcat(&mut jump_stream).await?;
            },
            DialMode::LocalForward { .. } => {
                bail!(
                    "local forward through jump chains is not supported yet; use direct relay circuit or pairing code"
                );
            },
            DialMode::ReverseForward { .. } => {
                bail!(
                    "reverse forward through jump chains is not supported yet; use direct relay circuit or pairing code"
                );
            },
            DialMode::Socks5 { .. } => {
                bail!(
                    "socks5 through jump chains is not supported yet; use direct relay circuit or pairing code"
                );
            },
        }

        return Ok(());
    }

    let remote_peer_id = match opt.target {
        DialTarget::RelayCircuit(ref addr) => {
            let (relay_address, remote_peer_id) = relay_base_from_circuit_address(addr)?;
            connect_and_identify(&mut swarm, &relay_address).await?;
            info!(relay_circuit_address = %addr, "dialing remote through relay circuit");
            swarm.dial(addr.clone())?;
            wait_for_peer_connection(&mut swarm, remote_peer_id).await?;
            remote_peer_id
        },
        DialTarget::Mdns(peer_id) => {
            info!(%peer_id, "waiting for mDNS discovery");
            wait_for_mdns_and_connect(&mut swarm, peer_id).await?;
            peer_id
        },
        DialTarget::PairingCode(ref code) => {
            let relay_address = relay_address_opt
                .as_ref()
                .ok_or_else(|| eyre!("--relay-address is required when using pairing codes"))?;
            let relay_peer_id = peer_id_from_multiaddr(relay_address)
                .ok_or_else(|| eyre!("missing relay peer id in --relay-address"))?;

            connect_and_identify(&mut swarm, relay_address).await?;

            let namespace = rendezvous::Namespace::new(code.clone())
                .map_err(|err| eyre!("invalid pairing code: {err}"))?;

            swarm
                .behaviour_mut()
                .rendezvous
                .discover(Some(namespace), None, None, relay_peer_id);

            wait_for_rendezvous_discovery(&mut swarm, relay_peer_id).await?
        },
    };

    let control = swarm.behaviour_mut().stream.new_control();
    let incoming = swarm
        .behaviour_mut()
        .stream
        .new_control()
        .accept(tunnel_proto.clone())
        .wrap_err("failed to set stream accept protocol")?;

    tokio::spawn(async move {
        if let Err(err) = drive_client_swarm(swarm).await {
            error!(%err, "dialer swarm task exited");
        }
    });

    match mode {
        DialMode::Netcat => {
            let mut stream =
                forward::open_stream(control, remote_peer_id, tunnel_proto.clone()).await?;
            tunnel::write_tunnel_request(&mut stream, &TunnelRequest::Netcat).await?;
            netcat::run_netcat(&mut stream).await?;
        },
        DialMode::LocalForward { bind_port, target } => {
            forward::run_local_forward(control, remote_peer_id, bind_port, target, tunnel_proto)
                .await?;
        },
        DialMode::ReverseForward {
            bind_port,
            target,
            gateway_ports,
        } => {
            forward::request_reverse_forward(
                control.clone(),
                remote_peer_id,
                bind_port,
                target.to_string(),
                gateway_ports,
                tunnel_proto,
            )
            .await?;

            forward::run_incoming_reverse_handler(incoming).await?;
        },
        DialMode::Socks5 { bind_port } => {
            forward::run_socks5(control, remote_peer_id, bind_port, tunnel_proto).await?;
        },
    }

    Ok(())
}

fn parse_dial_mode(opt: &DialOpt) -> DialMode {
    if let Some(spec) = &opt.local_forward {
        return DialMode::LocalForward {
            bind_port: spec.bind_port,
            target: Arc::from(spec.target.as_str()),
        };
    }

    if let Some(spec) = &opt.remote_forward {
        return DialMode::ReverseForward {
            bind_port: spec.bind_port,
            target: Arc::from(spec.target.as_str()),
            gateway_ports: opt.gateway_ports,
        };
    }

    if let Some(bind_port) = opt.socks {
        return DialMode::Socks5 { bind_port };
    }

    DialMode::Netcat
}
