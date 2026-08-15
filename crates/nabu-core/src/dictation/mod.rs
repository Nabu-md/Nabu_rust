//! Dictation subsystem: microphone -> audio -> Whisper -> transcription.
//!
//! See the crate-level [`DictationService`] for the public entry point. The
//! module is organised as:
//!
//! - [`error`]     — [`DictationError`] lifecycle/failure types
//! - [`audio`]     — [`Microphone`] trait + macOS `AVAudioRecorder` capture
//! - [`transcribe`]- [`Transcriber`] trait + the real `WhisperTranscriber`
//! - [`service`]   — [`DictationService`] state machine driving a session
//!
//! Everything that touches the network is avoided: capture is local and
//! transcription uses the bundled `whisper.cpp` engine via
//! [`crate::native::whisper`].

pub mod audio;
pub mod error;
pub mod service;
pub mod transcribe;

pub use error::DictationError;
pub use service::{DictationResult, DictationService, DictationState};

/// Start a recording session against the process-wide dictation service.
///
/// Returns `Ok(())` once the microphone is capturing; the transcription is
/// produced later by [`stop`]. A missing Whisper model or denied microphone
/// permission fails here with an explicit [`DictationError`].
pub fn start() -> Result<(), DictationError> {
    DictationService::global().start()
}

/// Stop the active recording session, transcribe the captured audio, and
/// return the resulting text.
pub async fn stop() -> Result<String, DictationError> {
    DictationService::global().stop().await
}

/// Abort the active session without transcribing.
pub fn cancel() -> Result<(), DictationError> {
    DictationService::global().cancel()
}
