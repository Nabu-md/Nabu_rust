//! Dictation service — captures microphone audio on macOS and transcribes it
//! using the local Whisper engine.
//!
//! On macOS: uses the `sox` binary to record 16 kHz mono WAV audio to a temp
//! file. On stop, sends SIGINT to sox, reads the WAV, and calls
//! `nabu_core::native::whisper::transcribe`.
//!
//! On non-macOS: `start()` returns `UnsupportedPlatform`; dictation is a
//! macOS-only feature.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::SystemTime;
use thiserror::Error;

/// Raw captured audio bytes (WAV, 16 kHz mono PCM).
pub struct CapturedAudio {
    pub wav_bytes: Vec<u8>,
}

/// Errors returned by the dictation service.
#[derive(Debug, Error)]
pub enum DictationError {
    #[error("a dictation session is already in progress")]
    AlreadyRecording,

    #[error("no dictation session is active")]
    NotRecording,

    #[error("microphone access was denied. Grant microphone permission in System Settings → Privacy & Security → Microphone, then restart Nabu.")]
    PermissionDenied,

    #[error("microphone could not be initialised: {0}")]
    MicInitFailed(String),

    #[error("audio capture failed: {0}")]
    CaptureFailed(String),

    #[error("whisper model not found: {0}")]
    ModelNotFound(String),

    #[error("whisper transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("microphone capture is unavailable on this platform (requires macOS)")]
    UnsupportedPlatform,
}

/// Internal mutable state of [`DictationService`].
struct DictationInner {
    /// The running `sox` subprocess, if recording.
    process: Option<Child>,
    /// Path to the temp WAV file being written by `sox`.
    temp_path: Option<PathBuf>,
    /// Current lifecycle state.
    state: DictationState,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DictationState {
    Idle,
    Recording,
    Processing,
}

/// Thread-safe dictation controller. Registered as Tauri managed state so
/// commands can call `start()` and `stop()` through `State<'_, DictationService>`.
pub struct DictationService {
    inner: Mutex<DictationInner>,
}

impl DictationService {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(DictationInner {
                process: None,
                temp_path: None,
                state: DictationState::Idle,
            }),
        }
    }

    /// Start recording microphone audio.
    ///
    /// On macOS, spawns a `sox` subprocess that records 16 kHz mono WAV to a
    /// temp file. Returns an error if a session is already active.
    ///
    /// On non-macOS, returns [`DictationError::UnsupportedPlatform`].
    pub fn start(&self) -> Result<(), DictationError> {
        #[cfg(target_os = "macos")]
        {
            let mut guard = self.inner.lock().unwrap();
            if guard.state != DictationState::Idle {
                return Err(DictationError::AlreadyRecording);
            }

            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let temp_path =
                std::env::temp_dir().join(format!("nabu_dictation_{}.wav", timestamp));

            let child = Command::new("sox")
                .arg("-d")
                .arg("-r")
                .arg("16000")
                .arg("-c")
                .arg("1")
                .arg("-t")
                .arg("wav")
                .arg(&temp_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    let msg = format!("failed to spawn sox: {}", e);
                    let _ = std::fs::remove_file(&temp_path);
                    DictationError::MicInitFailed(msg)
                })?;

            guard.process = Some(child);
            guard.temp_path = Some(temp_path);
            guard.state = DictationState::Recording;
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(DictationError::UnsupportedPlatform)
        }
    }

    /// Stop recording, transcribe the captured audio, and return the text.
    ///
    /// On macOS: sends SIGINT to the `sox` subprocess (so it finalises the WAV),
    /// waits for it to exit, reads the temp file, and transcribes with Whisper.
    ///
    /// On non-macOS, returns [`DictationError::UnsupportedPlatform`].
    pub async fn stop(&self) -> Result<String, DictationError> {
        #[cfg(target_os = "macos")]
        {
            let (child, temp_path) = {
                let mut guard = self.inner.lock().unwrap();
                if guard.state == DictationState::Idle {
                    return Err(DictationError::NotRecording);
                }
                guard.state = DictationState::Processing;
                let child = guard.process.take();
                let temp_path = guard.temp_path.take();
                (child, temp_path)
            };

            let mut child = match child {
                Some(c) => c,
                None => {
                    self.reset_idle();
                    return Err(DictationError::NotRecording);
                }
            };

            let temp_path = match temp_path {
                Some(p) => p,
                None => {
                    self.reset_idle();
                    return Err(DictationError::NotRecording);
                }
            };

            // SIGINT tells sox to flush and exit cleanly, which finalises the WAV header.
            let _ = child.kill();
            let _ = Command::new("kill")
                .arg("-INT")
                .arg(child.id().to_string())
                .status();

            let _ = child.wait();

            // Read the captured WAV bytes.
            let wav_bytes = match std::fs::read(&temp_path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = std::fs::remove_file(&temp_path);
                    self.reset_idle();
                    return Err(DictationError::CaptureFailed(format!(
                        "failed to read captured audio: {}",
                        e
                    )));
                }
            };

            // Clean up temp file.
            let _ = std::fs::remove_file(&temp_path);

            // Resolve the Whisper model path.
            let model_path = std::env::var("NABU_WHISPER_MODEL")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("resources/whisper-models/ggml-base.en.bin"));

            if !model_path.exists() {
                self.reset_idle();
                return Err(DictationError::ModelNotFound(model_path.display().to_string()));
            }

            // Run transcription on a blocking thread to avoid stalling the async runtime.
            let model_path_clone = model_path.clone();
            let wav_bytes_clone = wav_bytes.clone();

            let result = tauri::async_runtime::spawn_blocking(move || {
                nabu_core::native::whisper::transcribe(&model_path_clone, &wav_bytes_clone)
            })
            .await
            .map_err(|e| DictationError::TranscriptionFailed(e.to_string()))?
            .map(|transcription| transcription.text)
            .map_err(|e| match e {
                nabu_core::native::error::NativeError::ModelNotFound(p) => {
                    DictationError::ModelNotFound(p)
                }
                nabu_core::native::error::NativeError::UnsupportedAudio(msg) => {
                    DictationError::MicInitFailed(msg)
                }
                nabu_core::native::error::NativeError::InvalidData(msg)
                | nabu_core::native::error::NativeError::CallFailed(msg) => {
                    DictationError::TranscriptionFailed(msg)
                }
                nabu_core::native::error::NativeError::UnsupportedPlatform => {
                    DictationError::UnsupportedPlatform
                }
            });

            self.reset_idle();
            result
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(DictationError::UnsupportedPlatform)
        }
    }

    /// Reset the service to idle state.
    fn reset_idle(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.state = DictationState::Idle;
    }
}

impl Default for DictationService {
    fn default() -> Self {
        Self::new()
    }
}
