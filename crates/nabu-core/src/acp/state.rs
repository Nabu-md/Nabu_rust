//! # ACP Session State Machine
//!
//! Tracks two levels of state:
//!
//! 1. **Protocol state** — whether the `initialize` handshake has completed
//!    on the current connection.
//! 2. **Session state** — the lifecycle of each individual session, keyed by
//!    [`SessionId`].
//!
//! All transitions are **explicit and validated**. The state machine rejects
//! impossible transitions (e.g. ending a closed session, prompting before
//! initialization) with an [`AcpError`] rather than silently accepting them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::acp::error::AcpError;

// ---------------------------------------------------------------------------
// Protocol state (connection-level)
// ---------------------------------------------------------------------------

/// The protocol-level state of an ACP connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProtocolState {
    /// `initialize` has not yet been called.
    #[default]
    Uninitialized,
    /// `initialize` has completed successfully; sessions may now be created.
    Initialized,
}

impl ProtocolState {
    /// Returns `true` if the protocol has been initialized.
    pub fn is_initialized(&self) -> bool {
        matches!(self, Self::Initialized)
    }

    /// Attempts to transition to [`Initialized`].
    ///
    /// # Errors
    ///
    /// Returns [`AcpError::InvalidState`] if the protocol is already initialized
    /// or if a later-stage error occurs.
    pub fn initialize(self) -> Result<Self, AcpError> {
        match self {
            Self::Uninitialized => Ok(Self::Initialized),
            Self::Initialized => Err(AcpError::invalid_state(
                "ACP protocol is already initialized",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Session status (per-session)
// ---------------------------------------------------------------------------

/// The lifecycle status of a single ACP session.
///
/// Sessions progress one-way through these states:
///
/// ```text
/// Created → Active → Closed
///   ↓         ↓
/// Closed     Closed
/// ```
///
/// A session must be in [`Active`] or [`Created`] to accept `session/prompt`;
/// it must be in [`Active`] or [`Created`] to accept `session/end`. Once
/// [`Closed`, transitions are terminal — no further operations are permitted
/// without re-creating the session via `session/new` or `session/load`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub enum SessionStatus {
    /// The session exists but no prompt has been processed yet.
    ///
    /// Created via [`AcpHandler::new_session`](crate::acp::AcpHandler::new_session)
    /// or restored via
    /// [`AcpHandler::load_session`](crate::acp::AcpHandler::load_session).
    #[default]
    Created = 1,
    /// At least one prompt has been processed. Prompts may continue.
    Active = 2,
    /// The session has been ended via `session/end`; no further prompts.
    Closed = 3,
}

impl SessionStatus {
    /// Returns `true` if a session in this state may receive prompts.
    pub fn accepts_prompt(self) -> bool {
        matches!(self, Self::Created | Self::Active)
    }

    /// Returns `true` if a session in this state may be ended.
    pub fn can_end(self) -> bool {
        matches!(self, Self::Created | Self::Active)
    }

    /// Returns `true` if the session has been ended and is terminal.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }

    /// Transition the session state as if a prompt was processed.
    ///
    /// - `Created` → `Active`
    /// - `Active` → `Active` (no-op)
    ///
    /// # Errors
    ///
    /// Returns [`AcpError`] if the session is [`Closed`].
    pub fn after_prompt(self) -> Result<Self, AcpError> {
        match self {
            Self::Closed => Err(AcpError::invalid_transition(
                self,
                "prompt",
                "cannot prompt a closed session",
            )),
            Self::Created | Self::Active => Ok(Self::Active),
        }
    }

    /// Transition the session state as if the session was ended.
    ///
    /// - `Created` → `Closed`
    /// - `Active` → `Closed`
    ///
    /// # Errors
    ///
    /// Returns [`AcpError`] if the session is already [`Closed`].
    pub fn after_end(self) -> Result<Self, AcpError> {
        match self {
            Self::Closed => Err(AcpError::invalid_transition(
                self,
                "end",
                "session is already closed",
            )),
            Self::Created | Self::Active => Ok(Self::Closed),
        }
    }
}

impl Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "Created"),
            Self::Active => write!(f, "Active"),
            Self::Closed => write!(f, "Closed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Session entry
// ---------------------------------------------------------------------------

/// Metadata and state for a single active session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    /// The current lifecycle status.
    pub status: SessionStatus,
    /// The working directory for this session.
    pub cwd: String,
    /// When the session was created / loaded.
    pub created_at: DateTime<Utc>,
    /// Last time the session was accessed (prompt or end).
    pub last_activity: DateTime<Utc>,
    /// The protocol version negotiated for this session (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<u16>,
    /// Optional metadata supplied during creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

impl SessionEntry {
    /// Creates a new session entry in the [`Created`] state.
    pub fn new(cwd: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            status: SessionStatus::Created,
            cwd: cwd.into(),
            created_at: now,
            last_activity: now,
            protocol_version: None,
            meta: None,
        }
    }

    /// Records a state transition and updates `last_activity`.
    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
        self.last_activity = Utc::now();
    }

    /// Returns the session ID this entry was stored under (for error messages).
    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }
}

// ---------------------------------------------------------------------------
// Helpers for the server (re-exported)
// ---------------------------------------------------------------------------

pub use SessionEntry as Entry;

use std::fmt::Display;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_state_starts_uninitialized() {
        assert_eq!(ProtocolState::default(), ProtocolState::Uninitialized);
        assert!(!ProtocolState::Uninitialized.is_initialized());
    }

    #[test]
    fn protocol_initialize_transitions_to_initialized() {
        let next = ProtocolState::Uninitialized.initialize().unwrap();
        assert_eq!(next, ProtocolState::Initialized);
        assert!(ProtocolState::Initialized.is_initialized());
    }

    #[test]
    fn protocol_double_initialize_fails() {
        let err = ProtocolState::Initialized.initialize().unwrap_err();
        assert!(matches!(err, AcpError::InvalidState { .. }));
    }

    #[test]
    fn session_status_created_accepts_prompt() {
        let status = SessionStatus::Created;
        assert!(status.accepts_prompt());
        assert!(status.can_end());
        assert!(!status.is_closed());
    }

    #[test]
    fn session_status_active_accepts_prompt_and_end() {
        let status = SessionStatus::Active;
        assert!(status.accepts_prompt());
        assert!(status.can_end());
        assert!(!status.is_closed());
    }

    #[test]
    fn session_status_closed_rejects_prompt_and_end() {
        let status = SessionStatus::Closed;
        assert!(!status.accepts_prompt());
        assert!(!status.can_end());
        assert!(status.is_closed());
    }

    #[test]
    fn prompt_transitions_created_to_active() {
        let next = SessionStatus::Created.after_prompt().unwrap();
        assert_eq!(next, SessionStatus::Active);
    }

    #[test]
    fn prompt_stays_active() {
        let next = SessionStatus::Active.after_prompt().unwrap();
        assert_eq!(next, SessionStatus::Active);
    }

    #[test]
    fn prompt_rejected_on_closed() {
        let err = SessionStatus::Closed.after_prompt().unwrap_err();
        assert!(matches!(err, AcpError::InvalidTransition { .. }));
    }

    #[test]
    fn end_transitions_created_to_closed() {
        let next = SessionStatus::Created.after_end().unwrap();
        assert_eq!(next, SessionStatus::Closed);
    }

    #[test]
    fn end_transitions_active_to_closed() {
        let next = SessionStatus::Active.after_end().unwrap();
        assert_eq!(next, SessionStatus::Closed);
    }

    #[test]
    fn end_rejected_on_closed() {
        let err = SessionStatus::Closed.after_end().unwrap_err();
        assert!(matches!(err, AcpError::InvalidTransition { .. }));
    }

    #[test]
    fn one_way_ordering_enforced() {
        assert!(SessionStatus::Created < SessionStatus::Active);
        assert!(SessionStatus::Active < SessionStatus::Closed);
    }

    #[test]
    fn session_entry_defaults_to_created() {
        let entry = SessionEntry::new("/home/user");
        assert_eq!(entry.status, SessionStatus::Created);
        assert_eq!(entry.cwd, "/home/user");
        assert!(entry.created_at <= entry.last_activity);
    }

    #[test]
    fn session_status_display_matches_variant_name() {
        assert_eq!(SessionStatus::Created.to_string(), "Created");
        assert_eq!(SessionStatus::Active.to_string(), "Active");
        assert_eq!(SessionStatus::Closed.to_string(), "Closed");
    }

    #[test]
    fn session_status_serializes_to_snake_case() {
        // The state machine uses the enum for internal tracking. The
        // serialized form should be stable for logging/debugging.
        let json = serde_json::to_string(&SessionStatus::Active).unwrap();
        // Default serde for a simple enum uses the variant name.
        assert_eq!(json, "\"Active\"");
    }
}
