//! # ACP Client State Machine
//!
//! Tracks the lifecycle of an ACP client connection from the moment it is
//! created until the transport is closed.
//!
//! Nabu is the ACP **client**. The connection state machine ensures that
//! protocol methods are called in the correct order:
//!
//! ```text
//! Disconnected → Initializing → Connected → Closed
//!                    └──→ (on error) → Closed
//! ```
//!
//! Once connected, the client can create and manage sessions. Each session
//! has its own sub-state tracked in [`SessionState`]:
//!
//! ```text
//! None → Pending → Active → Closed
//! ```

use crate::acp::error::AcpError;
use crate::acp::types::{ProtocolVersion, SessionId};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The negotiated capabilities exchanged during `initialize`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NegotiatedCapabilities {
    /// The protocol version both sides agreed to use.
    pub protocol_version: ProtocolVersion,
    /// The agent's advertised capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_capabilities: Option<crate::acp::types::AgentCapabilities>,
    /// The client's advertised capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_capabilities: Option<crate::acp::types::ClientCapabilities>,
    /// The agent's implementation metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_info: Option<crate::acp::types::Implementation>,
    /// The client's implementation metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<crate::acp::types::Implementation>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// High-level connection state for the ACP client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// The client has been created but `initialize` has not been sent.
    Disconnected,
    /// `initialize` has been sent; waiting for the agent's response.
    Initializing,
    /// The `initialize` exchange completed; the client is connected and may
    /// create/manage sessions.
    Connected,
    /// The connection has been closed (either by the client or the transport).
    Closed,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl ConnectionState {
    /// Returns `true` if the client can send requests to the agent.
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Returns `true` if `initialize` has been sent but not yet acknowledged.
    pub fn is_initializing(&self) -> bool {
        matches!(self, ConnectionState::Initializing)
    }

    /// Returns `true` if the connection has been permanently closed.
    pub fn is_closed(&self) -> bool {
        matches!(self, ConnectionState::Closed)
    }
}

/// Sub-state for a single session within the connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The session has been created on the agent but no prompt has been sent.
    Pending,
    /// A prompt turn is in progress. The agent may send `session/update`
    /// notifications and agent→client requests.
    Active,
    /// The session has been closed via `session/close`.
    Closed,
}

/// Runtime state for a single session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub session_id: SessionId,
    pub state: SessionState,
    /// The working directory for this session.
    pub cwd: String,
}

/// Mutable connection state owned by the ACP client.
///
/// This struct is the single source of truth for connection progress,
/// negotiated capabilities, and all known sessions.
#[derive(Debug, Default)]
pub struct ClientState {
    /// Current connection lifecycle state.
    pub connection_state: ConnectionState,
    /// Negotiated capabilities from the `initialize` exchange.
    pub capabilities: Option<NegotiatedCapabilities>,
    /// Map of session ID → session runtime state.
    pub sessions: HashMap<SessionId, SessionEntry>,
    /// The ID of the session currently being prompted (if any).
    pub active_session: Option<SessionId>,
}

impl ClientState {
    /// Create a fresh state in the `Disconnected` state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Transition to `Initializing` — called when the client sends `initialize`.
    ///
    /// Returns an error if the client is not in `Disconnected` state.
    pub fn transition_to_initializing(&mut self) -> Result<(), crate::acp::error::AcpError> {
        if self.connection_state != ConnectionState::Disconnected {
            return Err(crate::acp::error::AcpError::invalid_state(format!(
                "cannot start initialization from state {:?}",
                self.connection_state
            )));
        }
        self.connection_state = ConnectionState::Initializing;
        Ok(())
    }

    /// Transition to `Connected` and store the negotiated capabilities.
    ///
    /// Returns an error if the client is not in `Initializing` state.
    pub fn transition_to_connected(
        &mut self,
        caps: NegotiatedCapabilities,
    ) -> Result<(), crate::acp::error::AcpError> {
        if self.connection_state != ConnectionState::Initializing {
            return Err(crate::acp::error::AcpError::invalid_state(format!(
                "cannot complete initialization from state {:?}",
                self.connection_state
            )));
        }
        self.connection_state = ConnectionState::Connected;
        self.capabilities = Some(caps);
        Ok(())
    }

    /// Transition to `Closed`.
    pub fn close(&mut self) {
        self.connection_state = ConnectionState::Closed;
        self.active_session = None;
    }

    /// Register a new session as `Pending`.
    pub fn register_session(&mut self, session_id: SessionId, cwd: String) {
        self.sessions.insert(
            session_id.clone(),
            SessionEntry {
                session_id: session_id.clone(),
                state: SessionState::Pending,
                cwd: cwd.clone(),
            },
        );
    }

    /// Mark a session as `Active` (prompt turn in progress).
    ///
    /// Returns an error if the session is unknown or already closed.
    pub fn activate_session(
        &mut self,
        session_id: &SessionId,
    ) -> Result<(), crate::acp::error::AcpError> {
        let entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| crate::acp::error::AcpError::unknown_session(session_id))?;

        if entry.state == SessionState::Closed {
            return Err(crate::acp::error::AcpError::invalid_state(format!(
                "cannot activate closed session: {}",
                session_id
            )));
        }

        entry.state = SessionState::Active;
        self.active_session = Some(session_id.clone());
        Ok(())
    }

    /// Mark a session as `Closed`.
    pub fn close_session(&mut self, session_id: &SessionId) {
        if let Some(entry) = self.sessions.get_mut(session_id) {
            entry.state = SessionState::Closed;
        }
        if self.active_session.as_deref() == Some(session_id.as_str()) {
            self.active_session = None;
        }
    }

    /// Remove a session from the registry (e.g. after `session/delete`).
    pub fn remove_session(&mut self, session_id: &SessionId) {
        self.sessions.remove(session_id);
        if self.active_session.as_deref() == Some(session_id.as_str()) {
            self.active_session = None;
        }
    }

    /// Returns `true` if the named session exists and is `Active`.
    pub fn is_session_active(&self, session_id: &SessionId) -> bool {
        self.sessions
            .get(session_id)
            .map(|e| e.state == SessionState::Active)
            .unwrap_or(false)
    }

    /// Returns the negotiated capabilities, or an error if `initialize`
    /// has not yet completed.
    pub fn capabilities_or_error(
        &self,
    ) -> Result<&NegotiatedCapabilities, crate::acp::error::AcpError> {
        self.capabilities.as_ref().ok_or_else(|| {
            crate::acp::error::AcpError::invalid_state(
                "connection not initialized; call initialize() first",
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Server-side state (used by AcpServer)
// ---------------------------------------------------------------------------
//
// These types track the server-side view of an ACP connection: whether
// `initialize` has been called (ProtocolState) and the lifecycle status of
// each session (SessionStatus). The client-side state machine lives above
// in ClientState / ConnectionState / SessionState.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProtocolState {
    /// `initialize` has not yet been called.
    #[default]
    Uninitialized,
    /// `initialize` has completed successfully.
    Initialized,
}

impl ProtocolState {
    /// Returns `true` if the protocol has been initialized.
    pub fn is_initialized(&self) -> bool {
        matches!(self, Self::Initialized)
    }

    /// Attempts to transition to [`Initialized`].
    pub fn initialize(self) -> Result<Self, AcpError> {
        match self {
            Self::Uninitialized => Ok(Self::Initialized),
            Self::Initialized => Err(AcpError::invalid_state(
                "ACP protocol is already initialized",
            )),
        }
    }
}

/// The lifecycle status of a single ACP session (server-side view).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SessionStatus {
    /// The session exists but no prompt has been processed yet.
    #[default]
    Created,
    /// A prompt turn is in progress.
    Active,
    /// The session has been closed.
    Closed,
}

impl SessionStatus {
    /// Returns `true` if the session can accept `session/prompt`.
    pub fn accepts_prompt(&self) -> bool {
        matches!(self, Self::Created | Self::Active)
    }

    /// Returns `true` if the session can be closed.
    pub fn can_end(&self) -> bool {
        matches!(self, Self::Created | Self::Active)
    }

    /// Transition to [`Active`] (after a prompt).
    pub fn after_prompt(self) -> Result<Self, AcpError> {
        match self {
            Self::Created | Self::Active => Ok(Self::Active),
            Self::Closed => Err(AcpError::invalid_state("session is closed")),
        }
    }

    /// Transition to [`Closed`] (after `session/close`).
    pub fn after_end(self) -> Result<Self, AcpError> {
        match self {
            Self::Created | Self::Active => Ok(Self::Closed),
            Self::Closed => Err(AcpError::invalid_state("session is already closed")),
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Active => write!(f, "active"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

/// Server-side runtime record for a single ACP session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub session_id: SessionId,
    pub status: SessionStatus,
    pub cwd: String,
    pub created_at: chrono::DateTime<Utc>,
    pub last_activity: chrono::DateTime<Utc>,
}

impl ServerEntry {
    pub fn new(cwd: String) -> Self {
        let now = Utc::now();
        Self {
            session_id: String::new(),
            status: SessionStatus::Created,
            cwd,
            created_at: now,
            last_activity: now,
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_transitions() {
        let mut state = ClientState::new();
        assert_eq!(state.connection_state, ConnectionState::Disconnected);

        state.transition_to_initializing().unwrap();
        assert_eq!(state.connection_state, ConnectionState::Initializing);

        state
            .transition_to_connected(NegotiatedCapabilities {
                protocol_version: 1,
                agent_capabilities: None,
                client_capabilities: None,
                agent_info: None,
                client_info: None,
                _meta: None,
            })
            .unwrap();
        assert_eq!(state.connection_state, ConnectionState::Connected);
        assert!(state.connection_state.is_connected());
    }

    #[test]
    fn cannot_initialize_from_connected_state() {
        let mut state = ClientState::new();
        state.transition_to_initializing().unwrap();
        state
            .transition_to_connected(NegotiatedCapabilities {
                protocol_version: 1,
                agent_capabilities: None,
                client_capabilities: None,
                agent_info: None,
                client_info: None,
                _meta: None,
            })
            .unwrap();

        // Should fail — already connected
        assert!(state.transition_to_initializing().is_err());
    }

    #[test]
    fn cannot_initialize_from_disconnected_after_already_initializing() {
        let mut state = ClientState::new();
        state.transition_to_initializing().unwrap();
        assert!(state.transition_to_initializing().is_err());
    }

    #[test]
    fn cannot_complete_init_from_disconnected() {
        let mut state = ClientState::new();
        let result = state.transition_to_connected(NegotiatedCapabilities::default());
        assert!(result.is_err());
    }

    #[test]
    fn close_sets_session_closed() {
        let mut state = ClientState::new();
        state.register_session("sess_1".to_string(), "/cwd".to_string());
        assert!(state.is_session_active("sess_1") == false);

        state.activate_session("sess_1").unwrap();
        assert_eq!(state.active_session.as_deref(), Some("sess_1"));

        state.close_session("sess_1");
        assert!(!state.is_session_active("sess_1"));
        assert!(state.active_session.is_none());
    }

    #[test]
    fn activate_unknown_session_errors() {
        let mut state = ClientState::new();
        let result = state.activate_session("nonexistent");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().kind,
            crate::acp::error::ErrorKind::UnknownSession
        );
    }

    #[test]
    fn activate_already_closed_session_errors() {
        let mut state = ClientState::new();
        state.register_session("sess_a".to_string(), "/cwd".to_string());
        state.activate_session("sess_a").unwrap();
        state.close_session("sess_a");

        let result = state.activate_session("sess_a");
        assert!(result.is_err());
    }

    #[test]
    fn remove_session_clears_active() {
        let mut state = ClientState::new();
        state.register_session("sess_x".to_string(), "/cwd".to_string());
        state.activate_session("sess_x").unwrap();
        assert_eq!(state.active_session.as_deref(), Some("sess_x"));

        state.remove_session("sess_x");
        assert!(!state.sessions.contains_key("sess_x"));
        assert!(state.active_session.is_none());
    }

    #[test]
    fn capabilities_or_error_when_uninitialized() {
        let state = ClientState::new();
        assert!(state.capabilities_or_error().is_err());
    }

    #[test]
    fn capabilities_or_error_when_connected() {
        let mut state = ClientState::new();
        state.transition_to_initializing().unwrap();
        state
            .transition_to_connected(NegotiatedCapabilities {
                protocol_version: 1,
                ..Default::default()
            })
            .unwrap();
        let caps = state.capabilities_or_error().unwrap();
        assert_eq!(caps.protocol_version, 1);
    }

    #[test]
    fn close_sets_connection_closed() {
        let mut state = ClientState::new();
        state.transition_to_initializing().unwrap();
        state
            .transition_to_connected(NegotiatedCapabilities::default())
            .unwrap();
        state.close();
        assert!(state.connection_state.is_closed());
        assert!(state.active_session.is_none());
    }
}
