use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use color_eyre::eyre::{self, WrapErr};
use fast_socks5::server::Socks5ServerProtocol;
use fast_socks5::Socks5Command;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream as p2pstream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{error, info, warn};

use crate::tunnel::{self, TunnelRequest};
use super::open_stream;

pub async fn run_socks5(
    control: p2pstream::Control,
    remote_peer: PeerId,
    bind_port: u16,
    protocol: StreamProtocol,
) -> eyre::Result<()> {
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), bind_port);
    let listener = TcpListener::bind(addr)
        .await
        .wrap_err_with(|| format!("failed to bind proxy on {addr}"))?;
    info!(%bind_port, "socks5+http proxy listening");

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        let control = control.clone();
        let protocol = protocol.clone();

        tokio::spawn(async move {
            // Peek first byte to detect protocol
            let mut peek_buf = [0u8; 1];
            if let Err(err) = socket.peek(&mut peek_buf).await {
                error!(%peer_addr, %err, "peek failed");
                return;
            }

            let result = match peek_buf[0] {
                0x05 => handle_socks5_client(socket, control, remote_peer, protocol.clone()).await,
                b'C' => handle_http_connect(socket, control, remote_peer, protocol).await,
                other => {
                    warn!(%peer_addr, first_byte = other, "unknown proxy protocol, expected SOCKS5 or HTTP CONNECT");
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!(%peer_addr, %err, "proxy session failed");
            }
        });
    }
}

async fn handle_socks5_client(
    socket: TcpStream,
    control: p2pstream::Control,
    remote_peer: PeerId,
    protocol: StreamProtocol,
) -> eyre::Result<()> {
    let proto = Socks5ServerProtocol::accept_no_auth(socket).await.map_err(|e| eyre::eyre!(e))?;
    let (proto, cmd, target_addr) = proto.read_command().await.map_err(|e| eyre::eyre!(e))?;
    if cmd != Socks5Command::TCPConnect { eyre::bail!("SOCKS5: only CONNECT supported, got {cmd:?}"); }
    let target = target_addr.to_string();
    info!(%target, "socks5 connecting");

    let mut stream = open_stream(control, remote_peer, protocol)
        .await
        .wrap_err("p2p tunnel broken, cannot open stream for socks5")?;
    tunnel::write_tunnel_request(&mut stream, &TunnelRequest::LocalForward { target: target.clone() })
        .await
        .wrap_err("failed to send socks5 tunnel request")?;

    let mut socket = proto.reply_success(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).await.map_err(|e| eyre::eyre!(e))?;

    let mut compat_stream = stream.compat();
    let (tx, rx) = tunnel::tunnel_copy(&mut socket, &mut compat_stream).await?;
    info!(%target, tx, rx, "socks5 session closed");
    Ok(())
}

async fn handle_http_connect(
    mut socket: TcpStream,
    control: p2pstream::Control,
    remote_peer: PeerId,
    protocol: StreamProtocol,
) -> eyre::Result<()> {
    // Read request line + headers
    let target = {
        let mut reader = BufReader::new(&mut socket);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).await?;
        
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() < 3 || parts[0] != "CONNECT" {
            drop(reader);
            socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await?;
            eyre::bail!("invalid HTTP CONNECT request: {first_line}");
        }
        
        let target = parts[1].to_string();
        
        // Consume remaining headers
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            if line.trim().is_empty() {
                break;
            }
        }
        
        target
    }; // reader dropped here, socket is free
    
    info!(%target, "http-connect proxying");
    
    let mut stream = open_stream(control, remote_peer, protocol)
        .await
        .wrap_err("p2p tunnel broken, cannot open stream for http-connect")?;
    tunnel::write_tunnel_request(
        &mut stream,
        &TunnelRequest::LocalForward { target: target.clone() },
    )
    .await
    .wrap_err("failed to send http-connect tunnel request")?;
    
    socket.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;
    
    let mut compat_stream = stream.compat();
    let (tx, rx) = tunnel::tunnel_copy(&mut socket, &mut compat_stream).await?;
    info!(%target, tx, rx, "http-connect session closed");
    Ok(())
}
