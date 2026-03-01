use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use color_eyre::eyre::{self, WrapErr};
use futures::StreamExt;
use futures::io::AsyncReadExt as _;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream as p2pstream;
use tokio::net::TcpListener;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{error, info, warn};

use super::{handle_forward_to_target, open_stream};
use crate::tunnel::{self, REVERSE_OK, TunnelRequest};

pub async fn start_reverse_listener(
    control: p2pstream::Control,
    remote_peer: PeerId,
    bind_port: u16,
    target: Arc<str>,
    gateway_ports: bool,
    protocol: StreamProtocol,
) -> eyre::Result<()> {
    let bind_ip = if gateway_ports {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    };
    let bind_addr = SocketAddr::new(bind_ip.into(), bind_port);
    let listener = TcpListener::bind(bind_addr)
        .await
        .wrap_err_with(|| format!("failed to bind reverse forward on {bind_addr}"))?;
    info!(%bind_port, %target, "reverse forward bound on listener side");

    tokio::spawn(async move {
        loop {
            let accepted = listener.accept().await;
            let (mut inbound, peer_addr) = match accepted {
                Ok(value) => value,
                Err(err) => {
                    error!(%err, "reverse listener accept failed");
                    continue;
                },
            };

            let target = Arc::clone(&target);
            let control = control.clone();
            let protocol = protocol.clone();

            tokio::spawn(async move {
                let mut stream = match open_stream(control, remote_peer, protocol).await {
                    Ok(s) => s,
                    Err(err) => {
                        error!(%peer_addr, %err, "cannot open p2p stream for reverse-forward");
                        return;
                    },
                };

                let req = TunnelRequest::ReverseForward {
                    bind_port: 0,
                    target: target.to_string(),
                    gateway_ports: false,
                };

                if let Err(err) = tunnel::write_tunnel_request(&mut stream, &req).await {
                    error!(%peer_addr, %err, "cannot send reverse-forward request");
                    return;
                }

                let mut compat_stream = stream.compat();
                match tunnel::tunnel_copy(&mut inbound, &mut compat_stream).await {
                    Ok((tx, rx)) => info!(%peer_addr, tx, rx, "reverse-forward session closed"),
                    Err(err) => warn!(%peer_addr, %err, "reverse-forward session error"),
                }
            });
        }
    });

    Ok(())
}

pub async fn request_reverse_forward(
    mut control: p2pstream::Control,
    remote_peer: PeerId,
    bind_port: u16,
    target: String,
    gateway_ports: bool,
    protocol: StreamProtocol,
) -> eyre::Result<()> {
    let mut stream = control
        .open_stream(remote_peer, protocol)
        .await
        .map_err(|err| io::Error::other(err.to_string()))
        .wrap_err("failed to open p2p stream for reverse-forward request")?;
    let req = TunnelRequest::ReverseForward {
        bind_port,
        target,
        gateway_ports,
    };
    tunnel::write_tunnel_request(&mut stream, &req)
        .await
        .wrap_err("failed to send reverse-forward request")?;

    let mut ack = [0u8; 1];
    stream.read_exact(&mut ack).await?;
    if ack[0] != REVERSE_OK {
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf).await?;
        let len = u16::from_be_bytes(len_buf) as usize;
        let mut reason_buf = vec![0u8; len];
        stream.read_exact(&mut reason_buf).await?;
        let reason = String::from_utf8_lossy(&reason_buf);
        eyre::bail!("reverse forward rejected by listener: {reason}");
    }

    info!(%bind_port, "reverse forward request accepted");
    Ok(())
}

pub async fn run_incoming_reverse_handler(
    mut incoming: p2pstream::IncomingStreams,
) -> eyre::Result<()> {
    while let Some((peer, mut stream)) = incoming.next().await {
        let request = match tunnel::read_tunnel_request(&mut stream).await {
            Ok(request) => request,
            Err(err) => {
                warn!(%peer, %err, "failed to parse incoming reverse stream request");
                continue;
            },
        };

        match request {
            TunnelRequest::ReverseForward { target, .. } => {
                info!(%peer, %target, "reverse-forward connection dispatched");
                tokio::spawn(async move {
                    if let Err(err) = handle_forward_to_target(stream, target).await {
                        error!(%peer, "{err:#}");
                    }
                });
            },
            other => {
                warn!(%peer, ?other, "unexpected incoming request in reverse-forward mode");
            },
        }
    }

    eyre::bail!("incoming reverse stream accept loop ended unexpectedly")
}
