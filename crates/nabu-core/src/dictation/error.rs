//! Error types for the dictation subsystem.
//!
//! All failures are represented as explicit [`DictationError`] variants so the
//! UI can distinguish, for example, a *missing Whisper model* from a
//! *denied microphone permission* — never from a successful transcription.

use thiserror::Error;

/// Lifecycle and failure modes for a dictation session.
///
/// These map 1:1 onto the states the dictation pill renders. Every variant
/// carries an explanation suitable for surfacing to the user; no variant
/// represents a successful transcription (that is [`DictationState::Completed`]).
#[derive(Debug, Clone, Error)]
pub enum DictationError {
    /// The OS does not provide microphone capture in the current build
    /// (i.e. non-macOS). Whisper dictation is a macOS-only feature.
    #[error("microphone capture is unavailable on this platform (requires macOS)")]
    MicrophoneUnavailable,

    /// The user (or parental controls) denied microphone access.
    #[error("microphone access was denied. Grant microphone permission in System Settings → Privacy & Security → Microphone, then restart Nabu.")]
    PermissionDenied,

    /// The audio session / device could not be initialised.
    #[error("microphone could not be initialised: {0}")]
    CaptureInitFailed(String),

    /// The system interrupted the audio capture (e.g. a phone call, or the
    /// audio session was preempted).
    #[error("microphone capture was interrupted")]
    CaptureInterrupted,

    /// The configured Whisper model could not be found on disk.
    #[error("whisper model not found: {0}")]
    ModelNotFound(String),

    /// Whisper produced no usable result (e.g. empty audio, decode failure).
    #[error("audio contained no decodable samples for transcription")]
    EmptyAudio,

    /// Whisper inference ran but failed to produce a transcription.
    #[error("whisper transcription failed: {0}")]
    TranscriptionFailed(String),

    /// The session was cancelled before transcription completed.
    #[error("dictation was cancelled")]
    Cancelled,

    /// `stop` was called with no active recording session.
    #[error("no dictation session is active")]
    NotActive,

    /// `start` was called while a session is already recording.
    #[error("a dictation session is already in progress")]
    AlreadyActive,
}
