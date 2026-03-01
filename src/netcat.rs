use std::io;

use libp2p::Stream;
use tokio::io::{join, stdin, stdout};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::tunnel;
use tracing::info;

pub async fn run_netcat(stream: &mut Stream) -> io::Result<()> {
    let mut stdio = join(stdin(), stdout());
    let mut compat_stream = stream.compat();
    let (tx, rx) = tunnel::tunnel_copy(&mut stdio, &mut compat_stream).await?;
    info!(tx, rx, "netcat session closed");
    Ok(())
}
