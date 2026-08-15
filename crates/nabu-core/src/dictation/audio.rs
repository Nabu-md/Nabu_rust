//! Microphone capture abstraction for dictation.
//!
//! Capture is abstracted behind the [`Microphone`] trait so the
//! [`DictationService`](super::service::DictationService) is fully testable
//! without a physical microphone. A [`MicrophoneFactory`] defers construction
//! until a session starts, so platforms that cannot capture (non-macOS) fail
//! eagerly instead of pretending to record.

use super::error::DictationError;

/// Captured audio normalised to the WAV container.
///
/// `wav_bytes` is RIFF/WAVE data; the Whisper decoder re-samples to 16 kHz /
/// mono during decoding, so callers may record at whatever native rate the
/// device supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedAudio {
    pub wav_bytes: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Real microphone capture (or a mock, in tests).
///
/// A microphone is single-use per session: `open` -> `start` -> `stop`/`cancel`.
/// Implementations MUST NOT block on a real-time audio callback and MUST
/// release all native capture resources on `stop` or `cancel`.
pub trait Microphone: Send {
    /// Whether the OS has granted (or will prompt for) microphone access.
    fn permission_granted(&self) -> bool;

    /// Configure the audio session / device for recording.
    fn open(&mut self) -> Result<(), DictationError>;

    /// Begin capturing. Audio arrives asynchronously until [`stop`](Self::stop)
    /// or [`cancel`](Self::cancel).
    fn start(&mut self) -> Result<(), DictationError>;

    /// Stop capturing and return the accumulated audio.
    fn stop(&mut self) -> Result<CapturedAudio, DictationError>;

    /// Abort capture, discard audio, and release resources. No transcription
    /// is produced and nothing is inserted as a "successful" result.
    fn cancel(&mut self);
}

/// Builds a fresh [`Microphone`] for a new session.
pub trait MicrophoneFactory: Send + Sync {
    fn create(&self) -> Result<Box<dyn Microphone>, DictationError>;
}

/// Real capture on macOS; an unavailable placeholder on other platforms.
pub struct SystemMicrophoneFactory;

impl MicrophoneFactory for SystemMicrophoneFactory {
    fn create(&self) -> Result<Box<dyn Microphone>, DictationError> {
        platform::PlatformMicrophone::new().map(|m| Box::new(m) as Box<dyn Microphone>)
    }
}

/// Platform-specific capture implementation.
///
/// On macOS this drives `AVAudioRecorder` via `objc2` message sending; on
/// every other target it is an unavailable placeholder so the crate still
/// compiles (dictation is a macOS-only feature).
pub mod platform;
