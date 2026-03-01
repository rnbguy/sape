use std::io;

use futures::io::AsyncWriteExt as _;
use libp2p::{PeerId, Stream, StreamProtocol};
use libp2p_stream as p2pstream;

use crate::tunnel::{REVERSE_FAILED, REVERSE_OK};

pub(crate) mod local;
pub(crate) mod reverse;
pub(crate) mod socks5;

pub use local::{handle_forward_to_target, run_local_forward};
pub use reverse::{request_reverse_forward, run_incoming_reverse_handler, start_reverse_listener};
pub use socks5::run_socks5;

pub async fn send_reverse_ack(stream: &mut Stream, result: Result<(), String>) -> io::Result<()> {
    match result {
        Ok(()) => {
            stream.write_all(&[REVERSE_OK]).await?;
        },
        Err(reason) => {
            stream.write_all(&[REVERSE_FAILED]).await?;
            let bytes = reason.as_bytes();
            let len = u16::try_from(bytes.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "reverse error too long"))?
                .to_be_bytes();
            stream.write_all(&len).await?;
            stream.write_all(bytes).await?;
        },
    }
    stream.flush().await
}

pub async fn open_stream(
    mut control: p2pstream::Control,
    remote_peer: PeerId,
    protocol: StreamProtocol,
) -> io::Result<Stream> {
    control
        .open_stream(remote_peer, protocol)
        .await
        .map_err(|err| io::Error::other(err.to_string()))
}
