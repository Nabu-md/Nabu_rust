//! Fallback for platforms without dictation support.
//!
//! Dictation is a macOS-only feature. This stub keeps the crate compiling on
//! every platform and reports every operation as unavailable so the product
//! never pretends Whisper works where the build does not support it.

use super::super::{CapturedAudio, DictationError, Microphone};

pub struct PlatformMicrophone;

impl PlatformMicrophone {
    pub fn new() -> Result<Self, DictationError> {
        Err(DictationError::MicrophoneUnavailable)
    }
}

impl Microphone for PlatformMicrophone {
    fn permission_granted(&self) -> bool {
        false
    }

    fn open(&mut self) -> Result<(), DictationError> {
        Err(DictationError::MicrophoneUnavailable)
    }

    fn start(&mut self) -> Result<(), DictationError> {
        Err(DictationError::MicrophoneUnavailable)
    }

    fn stop(&mut self) -> Result<CapturedAudio, DictationError> {
        Err(DictationError::MicrophoneUnavailable)
    }

    fn cancel(&mut self) {}
}

impl Default for PlatformMicrophone {
    fn default() -> Self {
        Self
    }
}
