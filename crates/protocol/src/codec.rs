use common::error::AppError;
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Sends newline-delimited JSON over an async writer.
pub async fn write_json<T: Serialize, W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &T,
) -> Result<(), AppError> {
    let mut body = serde_json::to_vec(msg)?;
    body.push(b'\n');
    writer.write_all(&body).await?;
    Ok(())
}

/// Reads newline-delimited JSON from an async buffered reader.
pub async fn read_json<T: DeserializeOwned, R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<T, AppError> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Err(AppError::Protocol("connection closed".into()));
    }
    let msg = serde_json::from_str::<T>(line.trim())?;
    Ok(msg)
}
