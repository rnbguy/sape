use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use color_eyre::eyre::{self, WrapErr};
use libp2p::{PeerId, Stream};
use libp2p_stream as p2pstream;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{error, info, warn};

use crate::tunnel::{self, TunnelRequest};
use super::open_stream;

pub async fn run_local_forward(
    control: p2pstream::Control,
    remote_peer: PeerId,
    bind_port: u16,
    target: Arc<str>,
) -> eyre::Result<()> {
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), bind_port);
    let listener = TcpListener::bind(addr)
        .await
        .wrap_err_with(|| format!("failed to bind local forward on {addr}"))?;
    info!(%bind_port, %target, "local forward is listening");

    loop {
        let (mut inbound, peer_addr) = listener.accept().await?;
        let control = control.clone();
        let target = Arc::clone(&target);

        tokio::spawn(async move {
            let mut stream = match open_stream(control, remote_peer, crate::protocol::tunnel_protocol(crate::protocol::DEFAULT_NAMESPACE)).await {
                Ok(s) => s,
                Err(err) => {
                    error!(%peer_addr, %err, "cannot open p2p stream for local-forward");
                    return;
                }
            };
            let req = TunnelRequest::LocalForward {
                target: target.to_string(),
            };
            if let Err(err) = tunnel::write_tunnel_request(&mut stream, &req).await {
                error!(%peer_addr, %err, "cannot send local-forward request");
                return;
            }

            let mut compat_stream = stream.compat();
            match tunnel::tunnel_copy(&mut inbound, &mut compat_stream).await {
                Ok((tx, rx)) => info!(%peer_addr, tx, rx, "local-forward session closed"),
                Err(err) => warn!(%peer_addr, %err, "local-forward session error"),
            }
        });
    }
}

pub async fn handle_forward_to_target(stream: Stream, target: String) -> eyre::Result<()> {
    let mut outbound = TcpStream::connect(&target)
        .await
        .wrap_err_with(|| format!("cannot connect to target {target}"))?;
    let mut compat_stream = stream.compat();
    let (tx, rx) = tunnel::tunnel_copy(&mut outbound, &mut compat_stream).await?;
    info!(%target, tx, rx, "forward-to-target session closed");
    Ok(())
}
