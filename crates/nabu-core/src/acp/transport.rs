//! # ACP Transport Abstraction
//!
//! Provides a transport trait and concrete implementations for sending and
//! receiving JSON-RPC 2.0 messages over the Agent Communication Protocol.
//!
//! Nabu is the ACP **client**. The transport connects to an external ACP
//! **agent** process. For local agents, the transport is stdio — messages
//! are newline-delimited JSON-RPC 2.0.
//!
//! ## Process spawning
//!
//! The ACP layer does **NOT** spawn processes. Phase 3b owns process creation.
//! Callers provide either:
//! - Pre-connected `std::process::Child` stdin/stdout handles (via tokio
//!   async wrappers)
//! - A `tokio::io::duplex` pipe pair for in-process testing
//!
//! ## Wire format
//!
//! Messages are sent as single-line JSON strings terminated by `\n`.
//! Messages MUST NOT contain embedded newlines (per the ACP specification).
//! `stderr` is reserved for agent logging and is not parsed as ACP protocol
//! data.
//!
//! ## Concurrency model
//!
//! The [`StdioTransport`] can be used directly (with `&mut self` for both
//! read and write) or split into separate [`StdioReader`] and [`StdioWriter`]
//! halves via [`StdioTransport::split`], allowing the ACP client to spawn a
//! background message loop with the reader while retaining the writer for
//! sending requests. Both halves are `Send + Unpin`.

use crate::acp::error::{AcpError, ErrorKind};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

// ===========================================================================
// Transport trait
// ===========================================================================

/// A bidirectional transport for JSON-RPC 2.0 messages.
///
/// Implementations include [`StdioTransport`] (real pipes) and
/// [`MockTransport`] (in-memory channels for testing).
///
/// All methods are `async` because JSON-RPC I/O is inherently asynchronous.
pub trait Transport: Send + Unpin {
    /// Send a JSON value as a single newline-terminated line.
    async fn send_json(&mut self, msg: &Value) -> Result<(), AcpError>;

    /// Send a pre-serialized JSON string as a single newline-terminated line.
    async fn send_raw(&mut self, json: String) -> Result<(), AcpError> {
        let line = if !json.ends_with('\n') {
            format!("{}\n", json)
        } else {
            json
        };
        self.send_bytes(line.as_bytes()).await
    }

    /// Send raw bytes (including any framing) to the transport.
    async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AcpError>;

    /// Read the next complete message line. Returns `None` at EOF.
    async fn read_line(&mut self) -> Result<Option<String>, AcpError>;
}

// ===========================================================================
// StdioTransport — real pipes
// ===========================================================================

/// Stdio transport backed by pre-connected async stdin/stdout handles.
///
/// Wraps any `tokio::io::AsyncRead + AsyncWrite` pair (typically obtained
/// from a `std::process::Child`'s `stdin`/`stdout` after the process has
/// been spawned by Phase 3b).
///
/// Messages are newline-delimited JSON-RPC 2.0. The read half is buffered
/// (8 KB) for efficient line-by-line consumption.
///
/// ## Splitting for concurrency
///
/// Use [`StdioTransport::split`] to obtain separate reader and writer handles
/// for concurrent read/write access. The reader can be moved to a background
/// task (the message loop) while the writer is kept by the ACP client.
pub struct StdioTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
}

impl<R, W> StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    /// Create a new stdio transport from pre-connected read/write halves.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::with_capacity(8192, reader),
            writer,
        }
    }

    /// Split the transport into separate reader and writer halves.
    ///
    /// The two halves can be moved into different async tasks. This is the
    /// recommended way to use `StdioTransport` with the `AcpClient`, which
    /// needs concurrent reading (for the message loop) and writing (for
    /// sending requests).
    pub fn split(self) -> (StdioReader<R>, StdioWriter<W>) {
        (
            StdioReader {
                reader: self.reader,
            },
            StdioWriter {
                writer: self.writer,
            },
        )
    }
}

/// Reader half of a split [`StdioTransport`].
///
/// Reads newline-delimited JSON-RPC messages from the agent's stdout.
/// Can be moved to a background task.
pub struct StdioReader<R> {
    reader: BufReader<R>,
}

/// Writer half of a split [`StdioTransport`].
///
/// Sends JSON-RPC messages to the agent's stdin. Can be moved to a
/// different task than the reader.
pub struct StdioWriter<W> {
    writer: W,
}

impl<R: AsyncRead + Unpin + Send> StdioReader<R> {
    /// Read the next complete message line. Returns `None` at EOF.
    async fn read_line_inner(&mut self) -> Result<Option<String>, AcpError> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(AcpError::from)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        if trimmed.is_empty() {
            self.read_line_inner().await
        } else {
            Ok(Some(trimmed))
        }
    }
}

impl<R: AsyncRead + Unpin + Send> Transport for StdioReader<R> {
    async fn send_json(&mut self, _msg: &Value) -> Result<(), AcpError> {
        Err(AcpError::new(
            ErrorKind::Internal,
            "StdioReader does not support sending",
        ))
    }

    async fn send_bytes(&mut self, _bytes: &[u8]) -> Result<(), AcpError> {
        Err(AcpError::new(
            ErrorKind::Internal,
            "StdioReader does not support sending",
        ))
    }

    async fn read_line(&mut self) -> Result<Option<String>, AcpError> {
        self.read_line_inner().await
    }
}

impl<W: AsyncWrite + Unpin + Send> StdioWriter<W> {
    /// Send raw bytes (including any framing) to the transport.
    async fn send_bytes_inner(&mut self, bytes: &[u8]) -> Result<(), AcpError> {
        let mut data = bytes.to_vec();
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        self.writer.write_all(&data).await.map_err(AcpError::from)?;
        self.writer.flush().await.map_err(AcpError::from)?;
        Ok(())
    }
}

impl<W: AsyncWrite + Unpin + Send> Transport for StdioWriter<W> {
    async fn send_json(&mut self, msg: &Value) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        self.send_bytes(json.as_bytes()).await
    }

    async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AcpError> {
        self.send_bytes_inner(bytes).await
    }

    async fn read_line(&mut self) -> Result<Option<String>, AcpError> {
        Err(AcpError::new(
            ErrorKind::Internal,
            "StdioWriter does not support reading",
        ))
    }
}

// Full StdioTransport also implements Transport (both read and write)
impl<R, W> Transport for StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    async fn send_json(&mut self, msg: &Value) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        let mut data = json.into_bytes();
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        self.writer.write_all(&data).await.map_err(AcpError::from)?;
        self.writer.flush().await.map_err(AcpError::from)?;
        Ok(())
    }

    async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AcpError> {
        let mut data = bytes.to_vec();
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        self.writer.write_all(&data).await.map_err(AcpError::from)?;
        self.writer.flush().await.map_err(AcpError::from)?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, AcpError> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(AcpError::from)?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        if trimmed.is_empty() {
            self.read_line().await
        } else {
            Ok(Some(trimmed))
        }
    }
}

// ===========================================================================
// MockTransport — in-memory channel-based transport for testing
// ===========================================================================

/// A mock transport pair for testing, backed by in-memory channels.
///
/// Created via [`MockTransport::pair()`], which returns both the client-side
/// transport and a [`MockPeer`] representing the agent. Tests write mock
/// agent responses via `MockPeer::feed()` and read client requests via
/// `MockPeer::recv_client_msg()`.
///
/// The transport is fully async and uses `tokio::sync::mpsc` channels,
/// so it works in `#[tokio::test]` async tests.
pub struct MockTransport {
    inbound: mpsc::UnboundedReceiver<String>,
    outbound: mpsc::UnboundedSender<String>,
}

impl MockTransport {
    /// Create a mock transport pair.
    ///
    /// Returns `(client_transport, peer)`:
    /// - `client_transport`: what the ACP client reads/writes
    /// - `peer`: what tests use to feed responses and read requests
    pub fn pair() -> (Self, MockPeer) {
        let (tx_to_client, rx_from_peer) = mpsc::unbounded_channel::<String>();
        let (tx_from_client, rx_to_peer) = mpsc::unbounded_channel::<String>();

        let client = Self {
            inbound: rx_from_peer,
            outbound: tx_to_client,
        };

        let peer = MockPeer {
            inbound: rx_to_peer,
            outbound: tx_from_client,
        };

        (client, peer)
    }

    /// Feed a raw JSON string to the client from the peer (agent).
    #[allow(dead_code)]
    pub fn feed(&self, json: &str) -> Result<(), AcpError> {
        self.outbound
            .send(json.to_string())
            .map_err(|_| AcpError::transport_closed("channel transport closed"))?;
        Ok(())
    }
}

impl Transport for MockTransport {
    async fn send_json(&mut self, msg: &Value) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        self.send_bytes(json.as_bytes()).await
    }

    async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AcpError> {
        let s = String::from_utf8_lossy(bytes).to_string();
        let line = if !s.ends_with('\n') {
            format!("{}\n", s)
        } else {
            s
        };
        self.outbound
            .send(line)
            .map_err(|_| AcpError::transport_closed("mock transport closed"))?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, AcpError> {
        match self.inbound.recv().await {
            Some(line) => Ok(Some(line.trim_end_matches(['\n', '\r']).to_string())),
            None => Ok(None),
        }
    }
}

/// The "agent peer" side of a [`MockTransport::pair()`].
///
/// Use this in tests to:
/// - Feed JSON-RPC responses/notifications as if sent by the agent
/// - Read JSON-RPC requests that the client (Nabu) has sent
pub struct MockPeer {
    inbound: mpsc::UnboundedReceiver<String>,
    outbound: mpsc::UnboundedSender<String>,
}

impl MockPeer {
    /// Feed a JSON-serializable message from the peer to the client.
    /// This is what the agent "sends."
    pub fn feed(&self, msg: &impl Serialize) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        self.outbound
            .send(json)
            .map_err(|_| AcpError::transport_closed("mock transport closed"))?;
        Ok(())
    }

    /// Feed a raw JSON string as if sent by the agent.
    pub fn feed_raw(&self, json: &str) -> Result<(), AcpError> {
        self.outbound
            .send(json.to_string())
            .map_err(|_| AcpError::transport_closed("mock transport closed"))?;
        Ok(())
    }

    /// Read the next request message the client sent to the agent (non-blocking).
    /// Returns `None` if the channel is empty or closed.
    pub fn try_recv_client_msg(&mut self) -> Option<String> {
        self.inbound.try_recv().ok()
    }

    /// Receive the next request message the client sent (async, blocking).
    pub async fn recv_client_msg(&mut self) -> Option<String> {
        self.inbound.recv().await
    }
}

impl Transport for MockPeer {
    async fn send_json(&mut self, msg: &Value) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        self.send_bytes(json.as_bytes()).await
    }

    async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AcpError> {
        let s = String::from_utf8_lossy(bytes).to_string();
        let line = if !s.ends_with('\n') {
            format!("{}\n", s)
        } else {
            s
        };
        self.outbound
            .send(line)
            .map_err(|_| AcpError::transport_closed("mock transport closed"))?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, AcpError> {
        match self.inbound.recv().await {
            Some(line) => Ok(Some(line.trim_end_matches(['\n', '\r']).to_string())),
            None => Ok(None),
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_transport_roundtrip() {
        let (mut client, mut peer) = MockTransport::pair();

        // Client sends a message
        let sent = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        client.send_json(&sent).await.unwrap();

        // Peer receives it
        let received = peer.recv_client_msg().await;
        assert!(received.is_some());
        assert!(received.unwrap().contains("initialize"));
    }

    #[tokio::test]
    async fn mock_transport_peer_to_client() {
        let (mut client, peer) = MockTransport::pair();

        // Peer feeds a response
        peer.feed_raw(r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#)
            .unwrap();

        // Client reads it
        let line = client.read_line().await.unwrap();
        assert!(line.is_some());
        assert!(line.unwrap().contains("protocolVersion"));
    }

    #[tokio::test]
    async fn mock_transport_eof_when_peer_dropped() {
        let (mut client, peer) = MockTransport::pair();
        drop(peer);

        let result = client.read_line().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn mock_transport_send_raw_adds_newline() {
        let (mut client, mut peer) = MockTransport::pair();

        client.send_raw("hello".to_string()).await.unwrap();
        let received = peer.recv_client_msg().await.unwrap();
        assert_eq!(received, "hello\n");
    }

    #[tokio::test]
    async fn mock_transport_bidirectional() {
        let (mut client, mut peer) = MockTransport::pair();

        // Client → peer
        client
            .send_json(&json!({"jsonrpc": "2.0", "id": 1, "method": "session/prompt"}))
            .await
            .unwrap();

        // Peer → client
        peer.feed_raw(r#"{"jsonrpc":"2.0","id":1,"result":{"stopReason":"end_turn"}}"#)
            .unwrap();

        // Client reads response
        let resp = client.read_line().await.unwrap().unwrap();
        assert!(resp.contains("end_turn"));

        // Peer reads request
        let req = peer.recv_client_msg().await.unwrap();
        assert!(req.contains("session/prompt"));
    }

    #[tokio::test]
    async fn stdio_transport_with_duplex() {
        let (client_read, agent_write) = tokio::io::duplex(8192);
        let (agent_read, client_write) = tokio::io::duplex(8192);

        let mut transport = StdioTransport::new(client_read, client_write);

        // Simulate agent writing
        tokio::spawn(async move {
            let mut writer = agent_write;
            writer
                .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();
        });

        let line = transport.read_line().await.unwrap();
        assert!(line.is_some());
        assert!(line.unwrap().contains("ok"));
    }

    #[tokio::test]
    async fn stdio_transport_split_read_write() {
        let (client_read, agent_write) = tokio::io::duplex(8192);
        let (agent_read, client_write) = tokio::io::duplex(8192);

        let transport = StdioTransport::new(client_read, client_write);
        let (mut reader, mut writer) = transport.split();

        // Agent writes a message
        tokio::spawn(async move {
            let mut w = agent_write;
            w.write_all(b"hello from agent\n").await.unwrap();
            w.flush().await.unwrap();
        });

        let line = reader.read_line().await.unwrap();
        assert_eq!(line, Some("hello from agent".to_string()));

        // Writer writes to agent's stdin
        writer.send_raw("hello to agent".to_string()).await.unwrap();

        let mut buf = [0u8; 256];
        let n = agent_read.read(&mut buf).await.unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();
        assert_eq!(received, "hello to agent\n");
    }
}
