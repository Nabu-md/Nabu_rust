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
//! - Pre-connected `std::process::Child` stdin/stdout handles
//! - Any `tokio::io::AsyncRead + AsyncWrite` pair (e.g. a pipe for testing)
//!
//! ## Wire format
//!
//! Messages are sent as single-line JSON strings terminated by `\n`.
//! Messages MUST NOT contain embedded newlines (per the ACP specification).
//! `stderr` is reserved for agent logging and is not parsed as ACP protocol
//! data.

use crate::acp::error::{AcpError, ErrorKind};
use serde::Serialize;
use serde_json::Value;
use std::pin::Pin;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

// ===========================================================================
// Transport trait
// ===========================================================================

/// A bidirectional transport for JSON-RPC 2.0 messages.
///
/// Implementations include [`StdioTransport`] (real pipes) and
/// [`MockTransport`] (in-memory channels for testing).
///
/// All methods are `async` because JSON-RPC I/O is inherently asynchronous
/// (the client sends a request, then must concurrently read notifications
/// and responses from the agent).
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
/// This struct wraps any `tokio::io::AsyncRead + AsyncWrite` pair (typically
/// obtained from a `std::process::Child`'s `stdin`/`stdout` after the
/// process has been spawned by Phase 3b).
///
/// Messages are newline-delimited JSON-RPC 2.0. The read half is buffered
/// (8 KB) for efficient line-by-line consumption.
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
    ///
    /// The caller (Phase 3b) is responsible for spawning the agent process
    /// and extracting the stdin/stdout handles. This transport simply wraps
    /// them.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::with_capacity(8192, reader),
            writer,
        }
    }
}

impl<R, W> Transport for StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    async fn send_json(&mut self, msg: &Value) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        self.send_bytes(json.as_bytes()).await
    }

    async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AcpError> {
        self.writer.write_all(bytes).await.map_err(AcpError::from)?;
        if !bytes.ends_with(b"\n") {
            self.writer.write_all(b"\n").await.map_err(AcpError::from)?;
        }
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
            return Ok(None); // EOF
        }
        // Trim trailing newline characters
        let line = line.trim_end_matches(['\n', '\r']).to_string();
        if line.is_empty() {
            // Skip empty lines (whitespace-only)
            self.read_line().await
        } else {
            Ok(Some(line))
        }
    }
}

// ===========================================================================
// MockTransport — in-memory test transport
// ===========================================================================

/// A test transport that uses in-memory channels instead of pipes.
///
/// Created via [`MockTransport::pair()`], which returns both the client-side
/// transport and a [`MockPeer`] representing the agent. Tests write mock
/// agent responses via `MockPeer::feed()` and read client requests via
/// `MockPeer::try_recv_client_msg()`.
///
/// The transport is fully synchronous in practice (channels are unbounded),
/// so it works in both `#[tokio::test]` async tests and `#[test]` sync
/// tests when combined with `block_on`.
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

    /// Feed a JSON message to the client's inbound channel.
    /// This simulates the agent sending a message to the client.
    pub fn feed(&self, msg: &impl Serialize) -> Result<(), AcpError> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AcpError::new(ErrorKind::MalformedRequest, e.to_string()))?;
        self.outbound
            .send(json)
            .map_err(|_| AcpError::transport_closed("mock peer closed"))?;
        Ok(())
    }

    /// Feed a raw JSON string to the client (as if sent by the agent).
    pub fn feed_raw(&self, json: &str) -> Result<(), AcpError> {
        self.outbound
            .send(json.to_string())
            .map_err(|_| AcpError::transport_closed("mock peer closed"))?;
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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[tokio::test]
    async fn mock_transport_client_sends_peer_receives() {
        let (mut client, mut peer) = MockTransport::pair();

        // Client sends a message
        let sent = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        client.send_json(&sent).await.unwrap();

        // Peer receives it
        let received = peer.recv_client_msg().await;
        assert_eq!(
            received,
            Some(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#.to_string())
        );
    }

    #[tokio::test]
    async fn mock_transport_peer_feeds_client_reads() {
        let (mut client, peer) = MockTransport::pair();

        // Peer feeds a response
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"protocolVersion": 1}
        });
        peer.feed(&response).unwrap();

        // Client reads it
        let received = client.read_line().await.unwrap();
        assert!(received.is_some());
        let line = received.unwrap();
        assert!(line.contains("\"protocolVersion\":1"));
    }

    #[tokio::test]
    async fn mock_transport_eof_when_peer_dropped() {
        let (mut client, peer) = MockTransport::pair();

        // Feed nothing and drop the peer
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

        // Client sends a request
        let req = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "session/prompt"});
        client.send_json(&req).await.unwrap();

        // Peer sends a response
        let resp =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {"stop_reason": "end_turn"}});
        peer.feed(&resp).unwrap();

        // Client reads the response
        let line = client.read_line().await.unwrap().unwrap();
        assert!(line.contains("end_turn"));

        // Peer reads the request
        let req_line = peer.recv_client_msg().await.unwrap();
        assert!(req_line.contains("session/prompt"));
    }

    #[tokio::test]
    async fn stdio_transport_from_pipe_pair() {
        // Test StdioTransport using tokio's pipe pair
        let (client_read, agent_write) = tokio::io::duplex(8192);
        let (agent_read, client_write) = tokio::io::duplex(8192);

        let mut client = StdioTransport::new(client_read, client_write);

        // Simulate the agent writing a response
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut writer = agent_write;
            writer
                .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n")
                .await
                .unwrap();
            writer.flush().await.unwrap();
        });

        let line = client.read_line().await.unwrap();
        assert!(line.is_some());
        assert!(line.unwrap().contains("ok"));
    }
}
