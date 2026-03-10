use common::error::AppError;
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Sends newline-delimited JSON over TCP.
pub async fn write_json<T: Serialize>(stream: &mut TcpStream, msg: &T) -> Result<(), AppError> {
    let mut body = serde_json::to_vec(msg)?;
    body.push(b'\n');
    stream.write_all(&body).await?;
    Ok(())
}

/// Reads newline-delimited JSON from TCP.
pub async fn read_json<T: DeserializeOwned>(stream: TcpStream) -> Result<T, AppError> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Err(AppError::Protocol("connection closed".into()));
    }
    let msg = serde_json::from_str::<T>(line.trim())?;
    Ok(msg)
}
