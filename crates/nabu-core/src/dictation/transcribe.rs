//! Transcription backend for dictation.
//!
//! The dictation service never talks to `whisper-rs` directly; it goes through
//! the [`Transcriber`] abstraction so that (a) the real Whisper engine
//! provided by [`crate::native::whisper`] is reused rather than duplicated,
//! and (b) tests can inject a deterministic fake.
//!
//! Model discovery follows the existing convention used by the
//! `whisper_processor`: the `NABU_WHISPER_MODEL` environment variable points
//! at a model file, otherwise `resources/whisper-models/ggml-base.en.bin`.

use super::audio::CapturedAudio;
use super::error::DictationError;
use crate::native::whisper as native_whisper;
use std::path::{Path, PathBuf};

/// A type that turns captured audio into a transcription string.
///
/// Implementors MUST NOT fabricate text: a missing/empty result must surface as
/// an error rather than a plausible-looking string.
pub trait Transcriber: Send + Sync {
    /// Whether a usable model is available right now. Used for the pre-flight
    /// check in [`DictationService::start`](super::service::DictationService).
    fn model_available(&self) -> bool;

    /// Transcribe captured audio and return the text, or an explicit error.
    fn transcribe(&self, audio: &CapturedAudio) -> Result<String, DictationError>;
}

/// Path resolution mirroring `whisper_processor::resolve_model_path`.
///
/// Kept local so that the dictation subsystem owns its configuration without
/// depending on the private helper inside the whisper processor.
fn resolve_model_path() -> PathBuf {
    std::env::var("NABU_WHISPER_MODEL")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("resources/whisper-models/ggml-base.en.bin"))
}

/// Real transcription backend: delegates to the existing
/// [`crate::native::whisper`] engine (the *same* function the
/// `whisper_processor` calls), so there is exactly one Whisper implementation
/// in the codebase.
pub struct WhisperTranscriber {
    model_path: PathBuf,
}

impl WhisperTranscriber {
    pub fn new() -> Self {
        Self {
            model_path: resolve_model_path(),
        }
    }

    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self { model_path }
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn default_model_path() -> PathBuf {
        resolve_model_path()
    }
}

impl Default for WhisperTranscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl Transcriber for WhisperTranscriber {
    fn model_available(&self) -> bool {
        self.model_path.exists()
    }

    fn transcribe(&self, audio: &CapturedAudio) -> Result<String, DictationError> {
        if !self.model_path.exists() {
            return Err(DictationError::ModelNotFound(
                self.model_path.display().to_string(),
            ));
        }

        match native_whisper::transcribe(&self.model_path, &audio.wav_bytes) {
            Ok(transcription) => {
                if transcription.text.trim().is_empty() {
                    // Honest: the model returned nothing. Do not fabricate.
                    return Err(DictationError::EmptyAudio);
                }
                Ok(transcription.text)
            }
            Err(crate::native::error::NativeError::ModelNotFound(p)) => {
                Err(DictationError::ModelNotFound(p))
            }
            Err(crate::native::error::NativeError::UnsupportedPlatform) => {
                Err(DictationError::TranscriptionFailed(
                    "whisper engine unavailable on this platform".into(),
                ))
            }
            Err(crate::native::error::NativeError::UnsupportedAudio(msg)) => {
                Err(DictationError::CaptureInitFailed(msg))
            }
            Err(crate::native::error::NativeError::InvalidData(msg))
            | Err(crate::native::error::NativeError::CallFailed(msg)) => {
                Err(DictationError::TranscriptionFailed(msg))
            }
        }
    }
}
