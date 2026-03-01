use std::io;

use futures::io::{AsyncReadExt, AsyncWriteExt};
use libp2p::Stream;
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};

pub const REVERSE_OK: u8 = 0x00;
pub const REVERSE_FAILED: u8 = 0x01;
const MAX_REQUEST_SIZE: usize = 65_536;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum TunnelRequest {
    Netcat,
    LocalForward { target: String },
    ReverseForward { bind_port: u16, target: String, gateway_ports: bool },
}


pub async fn read_tunnel_request(stream: &mut Stream) -> io::Result<TunnelRequest> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_REQUEST_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("tunnel request too large: {len} bytes (max {MAX_REQUEST_SIZE})"),
        ));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    postcard::from_bytes::<TunnelRequest>(&buf)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

pub async fn write_tunnel_request(stream: &mut Stream, req: &TunnelRequest) -> io::Result<()> {
    let bytes = postcard::to_allocvec(req)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}

pub async fn tunnel_copy<A, B>(left: &mut A, right: &mut B) -> io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    copy_bidirectional(left, right).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_request_roundtrip_netcat() {
        let req = TunnelRequest::Netcat;
        let bytes = postcard::to_allocvec(&req).expect("serialize netcat request");
        let decoded: TunnelRequest = postcard::from_bytes(&bytes).expect("deserialize netcat request");
        assert!(matches!(decoded, TunnelRequest::Netcat));
    }

    #[test]
    fn tunnel_request_roundtrip_local_forward() {
        let req = TunnelRequest::LocalForward {
            target: "localhost:3000".to_string(),
        };
        let bytes = postcard::to_allocvec(&req).expect("serialize local forward request");
        let decoded: TunnelRequest =
            postcard::from_bytes(&bytes).expect("deserialize local forward request");
        match decoded {
            TunnelRequest::LocalForward { target } => assert_eq!(target, "localhost:3000"),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn tunnel_request_roundtrip_reverse_forward() {
        let req = TunnelRequest::ReverseForward {
            bind_port: 9090,
            target: "localhost:3000".to_string(),
            gateway_ports: false,
        };
        let bytes = postcard::to_allocvec(&req).expect("serialize reverse forward request");
        let decoded: TunnelRequest =
            postcard::from_bytes(&bytes).expect("deserialize reverse forward request");
        match decoded {
            TunnelRequest::ReverseForward { bind_port, target, gateway_ports } => {
                assert_eq!(bind_port, 9090);
                assert_eq!(target, "localhost:3000");
                assert!(!gateway_ports);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn tunnel_request_roundtrip_reverse_forward_gateway() {
        let req = TunnelRequest::ReverseForward {
            bind_port: 8080,
            target: "10.0.0.1:443".to_string(),
            gateway_ports: true,
        };
        let bytes = postcard::to_allocvec(&req).expect("serialize gateway reverse forward");
        let decoded: TunnelRequest =
            postcard::from_bytes(&bytes).expect("deserialize gateway reverse forward");
        match decoded {
            TunnelRequest::ReverseForward { bind_port, target, gateway_ports } => {
                assert_eq!(bind_port, 8080);
                assert_eq!(target, "10.0.0.1:443");
                assert!(gateway_ports);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn max_request_size_is_reasonable() {
        assert_eq!(MAX_REQUEST_SIZE, 65_536);
    }
}
