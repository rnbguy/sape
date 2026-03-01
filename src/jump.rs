use std::io;

use futures::io::{AsyncReadExt, AsyncWriteExt};
use libp2p::Stream;

const MAX_JUMP_REQUEST_SIZE: usize = 65_536;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct JumpChain {
    pub hops: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum JumpResult {
    Ok,
    Error(String),
}


pub async fn read_jump_chain(stream: &mut Stream) -> io::Result<JumpChain> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_JUMP_REQUEST_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("jump request too large: {len} bytes (max {MAX_JUMP_REQUEST_SIZE})"),
        ));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    postcard::from_bytes::<JumpChain>(&buf)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

pub async fn write_jump_chain(stream: &mut Stream, chain: &JumpChain) -> io::Result<()> {
    let bytes = postcard::to_allocvec(chain)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}

pub async fn read_jump_result(stream: &mut Stream) -> io::Result<JumpResult> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_JUMP_REQUEST_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("jump result too large: {len} bytes"),
        ));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    postcard::from_bytes::<JumpResult>(&buf)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

pub async fn write_jump_result(stream: &mut Stream, result: &JumpResult) -> io::Result<()> {
    let bytes = postcard::to_allocvec(result)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}
