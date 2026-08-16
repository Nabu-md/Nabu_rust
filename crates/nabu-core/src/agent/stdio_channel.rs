//! StdioChannel — async reader/writer over a child process's stdin/stdout pipes.
//!
//! [`StdioChannel`] wraps a `tokio::process::ChildStdin` and `tokio::process::ChildStdout`,
//! providing high-level async methods for sending and receiving newline-delimited JSON
//! (NDJSON) messages. It reuses the framing helpers from `crate::io_stream` and
//! the `crate::rpc` types for JSON-RPC message exchange.
//!
//! ## Thread Safety
//!
//! The writer half is protected by a `tokio::sync::Mutex` so that multiple
//! async tasks can call `send()` concurrently. The reader half is owned by
//! a single `recv_loop()` task — only one reader should drive the channel.
//!
//! ## Usage
//!
//! ```no_run
//! # use nabu_core::agent::StdioChannel;
//! # use nabu_core::rpc::Request;
//! # async fn example(mut channel: StdioChannel) {
//! // Send a JSON-RPC request
//! let req = Request::new(1, "ping", None);
//! channel.send(&req).await.unwrap();
//!
//! // Read a line (the response will be deserialized by the caller)
//! let line = channel.recv_line().await.unwrap();
//! # }
//! ```

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::io_stream::framing::{decode_message_bytes, encode_message};
use crate::io_stream::{TransportError, TransportResult};
use crate::rpc::{Request, Response};

/// A bidirectional NDJSON channel over a child process's stdin/stdout.
///
/// Created from a spawned child's stdin and stdout handles. The writer half
/// is shared via `Arc<Mutex<...>>` internally so multiple tasks can send
/// concurrently. The reader is intended to be driven by a single
/// `recv_loop()` task.
pub struct StdioChannel {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
}

impl StdioChannel {
    /// Create a new channel from a child's stdin and stdout handles.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
        }
    }

    /// Encode and send a serializable value as a single NDJSON line on stdin.
    pub async fn send<T: serde::Serialize>(&self, value: &T) -> TransportResult<()> {
        let encoded = encode_message(value)?;
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(encoded.as_bytes())
            .await
            .map_err(TransportError::io)?;
        stdin.flush().await.map_err(TransportError::io)?;
        Ok(())
    }

    /// Encode and send a JSON-RPC [`Request`] on stdin.
    pub async fn send_request(&self, request: &Request) -> TransportResult<()> {
        self.send(request).await
    }

    /// Encode and send a JSON-RPC [`Response`] on stdout (i.e., write to the child's stdin).
    pub async fn send_response(&self, response: &Response) -> TransportResult<()> {
        self.send(response).await
    }

    /// Read one line from stdout and return the raw string (trimmed).
    ///
    /// Blank lines are skipped.
    /// Returns `Ok(None)` on EOF.
    pub async fn recv_line(&self) -> TransportResult<Option<String>> {
        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = stdout
                .read_line(&mut line)
                .await
                .map_err(TransportError::io)?;

            if bytes_read == 0 {
                return Ok(None);
            }

            if line.trim().is_empty() {
                continue;
            }

            return Ok(Some(line.trim().to_string()));
        }
    }

    /// Read one line from stdout and deserialize it into `T`.
    ///
    /// Blank lines are skipped. Returns `Ok(None)` on EOF.
    pub async fn recv<T: serde::de::DeserializeOwned>(&self) -> TransportResult<Option<T>> {
        loop {
            match self.recv_line().await? {
                Some(line) => {
                    return decode_message::<T>(&line).map(Some);
                }
                None => return Ok(None),
            }
        }
    }

    /// Read one line from stdout and deserialize it as a JSON-RPC [`Response`].
    pub async fn recv_response(&self) -> TransportResult<Option<Response>> {
        self.recv::<Response>().await
    }

    /// Read one line from stdout and deserialize it as a JSON-RPC [`Request`].
    pub async fn recv_request(&self) -> TransportResult<Option<Request>> {
        self.recv::<Request>().await
    }

    /// Flush the stdin write buffer.
    pub async fn flush(&self) -> TransportResult<()> {
        let mut stdin = self.stdin.lock().await;
        stdin.flush().await.map_err(TransportError::io)
    }
}
