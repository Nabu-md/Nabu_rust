//! Dictation session orchestration.
//!
//! [`DictationService`] owns the lifecycle of a single dictation session and
//! enforces a strict state machine:
//!
//! ```text
//!  Idle ──start──► Recording ──stop──► Processing ──┬──► Completed { text }
//!                                                    ├──► Failed { error }
//!                                                    └──(cancel)► Cancelled
//! ```
//!
//! The service never fabricates text: every non-`Completed` terminal state is
//! an explicit error/cancellation, never a "successful" transcription.

use super::audio::{CapturedAudio, Microphone, MicrophoneFactory, SystemMicrophoneFactory};
use super::error::DictationError;
use super::transcribe::{Transcriber, WhisperTranscriber};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// A snapshot of where a dictation session currently is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationState {
    Idle,
    Recording,
    Processing,
    Completed { text: String },
    Failed { error: String },
    Cancelled,
}

/// Outcome of stopping a session. `state` mirrors [`DictationState`]; `text`
/// holds the transcription when state is `Completed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationResult {
    pub state: DictationState,
    pub text: Option<String>,
}

struct ActiveSession {
    mic: Box<dyn Microphone>,
}

/// Real-time dictation pipeline: capture → buffer → transcribe.
///
/// Construct with [`DictationService::new`] (real system microphone + the
/// real Whisper engine) or [`DictationService::with_factory`] (injected
/// microphone/transcriber, used by tests).
pub struct DictationService {
    transcriber: Arc<dyn Transcriber>,
    factory: Arc<dyn MicrophoneFactory>,
    state: Mutex<DictationState>,
    active: Mutex<Option<ActiveSession>>,
}

impl Default for DictationService {
    fn default() -> Self {
        Self::new(Arc::new(WhisperTranscriber::new()))
    }
}

impl DictationService {
    pub fn new(transcriber: Arc<dyn Transcriber>) -> Self {
        Self::with_factory(transcriber, Arc::new(SystemMicrophoneFactory))
    }

    pub fn with_factory(transcriber: Arc<dyn Transcriber>, factory: Arc<dyn MicrophoneFactory>) -> Self {
        Self {
            transcriber,
            factory,
            state: Mutex::new(DictationState::Idle),
            active: Mutex::new(None),
        }
    }

    /// Process-wide instance backing the Tauri IPC commands.
    pub fn global() -> &'static DictationService {
        static INSTANCE: OnceLock<DictationService> = OnceLock::new();
        INSTANCE.get_or_init(|| DictationService::default())
    }

    pub fn status(&self) -> DictationState {
        self.state.lock().unwrap().clone()
    }

    fn set_state(&self, state: DictationState) {
        *self.state.lock().unwrap() = state;
    }

    fn fail(&self, error: String) {
        self.set_state(DictationState::Failed { error });
    }

    /// Pre-flight check + begin real-time capture.
    pub fn start(&self) -> Result<DictationState, DictationError> {
        let current = self.status();
        if matches!(current, DictationState::Recording | DictationState::Processing) {
            return Err(DictationError::AlreadyActive);
        }

        if !self.transcriber.model_available() {
            let path = self
                .transcriber
                .model_path()
                .unwrap_or_else(|| PathBuf::from("resources/whisper-models/ggml-base.en.bin"));
            let msg = path.display().to_string();
            self.fail(format!("whisper model not found: {msg}"));
            return Err(DictationError::ModelNotFound(msg));
        }

        let mut mic = self.factory.create()?;
        if !mic.permission_granted() {
            self.fail(DictationError::PermissionDenied.to_string());
            return Err(DictationError::PermissionDenied);
        }
        mic.open()?;
        mic.start()?;

        *self.active.lock().unwrap() = Some(ActiveSession { mic });
        self.set_state(DictationState::Recording);
        Ok(DictationState::Recording)
    }

    /// Stop capture and run Whisper on the accumulated audio, returning the
    /// transcription text on success. Whisper inference runs on a blocking
    /// thread so neither the audio path nor the UI is blocked.
    pub async fn stop(&self) -> Result<String, DictationError> {
        let session = self.active.lock().unwrap().take();
        let mut session = match session {
            Some(s) => s,
            None => {
                self.fail("recording has not been started".to_string());
                return Err(DictationError::NotActive);
            }
        };
        self.set_state(DictationState::Processing);

        let transcriber = Arc::clone(&self.transcriber);
        let outcome = tokio::task::spawn_blocking(move || {
            let audio = session.mic.stop()?;
            if audio.wav_bytes.is_empty() {
                return Err(DictationError::EmptyAudio);
            }
            transcriber.transcribe(&audio)
        })
        .await
        .expect("dictation transcription task panicked");

        let text = match outcome {
            Ok(t) => t,
            Err(ref e) => {
                self.fail(e.to_string());
                return Err(e.clone());
            }
        };
        self.set_state(DictationState::Completed { text: text.clone() });
        Ok(text)
    }

    /// Abort the current session without transcribing. Resources are released
    /// and no transcription (real or partial) is inserted as a success.
    pub fn cancel(&self) -> Result<(), DictationError> {
        let session = self.active.lock().unwrap().take();
        let mut session = match session {
            Some(s) => s,
            None => {
                if matches!(self.status(), DictationState::Idle) {
                    return Ok(());
                }
                self.fail("no active recording session".to_string());
                return Err(DictationError::NotActive);
            }
        };
        session.mic.cancel();
        self.set_state(DictationState::Cancelled);
        Ok(())
    }

    /// Convenience wrapper used by the IPC layer: returns a [`DictationResult`]
    /// for `stop` and `start` flows alike.
    pub async fn stop_result(&self) -> Result<DictationResult, DictationError> {
        match self.stop().await {
            Ok(text) => Ok(DictationResult {
                state: self.status(),
                text: Some(text),
            }),
            Err(e) => {
                self.fail(e.to_string());
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Mock microphone that hands back a pre-canned `CapturedAudio` (or an
    /// error) so capture can be tested without a physical microphone.
    struct MockMicrophone {
        permission: bool,
        open_ok: bool,
        start_ok: bool,
        stop_result: Result<CapturedAudio, DictationError>,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Microphone for MockMicrophone {
        fn permission_granted(&self) -> bool {
            self.permission
        }
        fn open(&mut self) -> Result<(), DictationError> {
            self.calls.lock().unwrap().push("open");
            if self.open_ok {
                Ok(())
            } else {
                Err(DictationError::CaptureInitFailed("open failed".into()))
            }
        }
        fn start(&mut self) -> Result<(), DictationError> {
            self.calls.lock().unwrap().push("start");
            if self.start_ok {
                Ok(())
            } else {
                Err(DictationError::CaptureInitFailed("start failed".into()))
            }
        }
        fn stop(&mut self) -> Result<CapturedAudio, DictationError> {
            self.calls.lock().unwrap().push("stop");
            self.stop_result.clone()
        }
        fn cancel(&mut self) {
            self.calls.lock().unwrap().push("cancel");
        }
    }

    /// Mock factory that returns a fixed microphone instance.
    struct MockFactory {
        mic: Option<MockMicrophone>,
    }
    impl MicrophoneFactory for MockFactory {
        fn create(&self) -> Result<Box<dyn Microphone>, DictationError> {
            // The factory is stateless here; rebuild a fresh mock each call by
            // cloning the shared call log so tests can inspect invocations.
            Ok(Box::new(MockMicrophone {
                permission: true,
                open_ok: true,
                start_ok: true,
                stop_result: Ok(CapturedAudio {
                    wav_bytes: b"\x00".repeat(48),
                    sample_rate: 16000,
                    channels: 1,
                }),
                calls: self.mic.as_ref().unwrap().calls.clone(),
            }))
        }
    }

    /// Mock transcriber that records received audio and returns a canned text.
    struct MockTranscriber {
        available: bool,
        path: Option<PathBuf>,
        text: Result<String, DictationError>,
        received: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl Transcriber for MockTranscriber {
        fn model_available(&self) -> bool {
            self.available
        }
        fn model_path(&self) -> Option<PathBuf> {
            self.path.clone()
        }
        fn transcribe(&self, audio: &CapturedAudio) -> Result<String, DictationError> {
            self.received.lock().unwrap().push(audio.wav_bytes.clone());
            self.text.clone()
        }
    }

    fn svc_with(mic: MockMicrophone, transcriber: Arc<MockTranscriber>) -> DictationService {
        let factory = Arc::new(MockFactory { mic: Some(mic) });
        DictationService::with_factory(transcriber, factory)
    }

    async fn assert_completed_text(svc: &DictationService, expected: &str) {
        let text = svc.stop().await.expect("stop should succeed");
        assert_eq!(text, expected);
        assert_eq!(
            svc.status(),
            DictationState::Completed { text: expected.to_string() }
        );
    }

    // Test 1 — state lifecycle: Idle -> Recording -> Processing -> Completed
    #[tokio::test]
    async fn state_lifecycle_idle_to_completed() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok("hello world".into()),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: b"\x00".repeat(48),
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber.clone());

        assert_eq!(svc.status(), DictationState::Idle);
        svc.start().expect("start ok");
        assert_eq!(svc.status(), DictationState::Recording);
        let text = svc.stop().await.expect("stop ok");
        assert_eq!(svc.status(), DictationState::Completed { text: "hello world".into() });
        assert_eq!(text, "hello world");
        let received = transcriber.received.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert!(!received[0].is_empty());
    }

    // Test 2 — the Whisper processor receives real captured audio.
    #[tokio::test]
    async fn audio_reaches_transcriber() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok("sampled words".into()),
            received: received.clone(),
        });
        let payload = vec![0x01u8, 0x02, 0x03, 0x04, 0x05];
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: payload.clone(),
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        svc.start().unwrap();
        svc.stop().await.unwrap();
        let got = received.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], payload, "transcriber must receive the exact captured bytes");
    }

    // Test 3 — missing Whisper model fails explicitly, no fake transcription.
    #[tokio::test]
    async fn missing_model_fails_at_start() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: false,
            path: Some(PathBuf::from("/no/such/model.bin")),
            text: Ok(String::new()),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0; 48],
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        let err = svc.start().unwrap_err();
        assert!(matches!(err, DictationError::ModelNotFound(_)), "got {err:?}");
        assert_eq!(svc.status(), DictationState::Failed { .. });
        assert!(svc.stop().await.is_err(), "no transcription should be produced");
        assert!(svc.active.lock().unwrap().is_none());
    }

    // Test 3b — missing model surfaces as an error at transcribe time too.
    #[tokio::test]
    async fn missing_model_fails_at_transcribe() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("/no/such/model.bin")),
            text: Err(DictationError::ModelNotFound("/no/such/model.bin".into())),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0; 48],
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        svc.start().unwrap();
        let err = svc.stop().await.unwrap_err();
        assert!(matches!(err, DictationError::ModelNotFound(_)), "got {err:?}");
        assert!(matches!(svc.status(), DictationState::Failed { .. }));
    }

    // Test 4 — capture (start) failure enters an error state without crashing.
    #[tokio::test]
    async fn capture_failure_enters_error_state() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok(String::new()),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: false, // open() fails
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0; 48],
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        let err = svc.start().unwrap_err();
        assert!(matches!(err, DictationError::CaptureInitFailed(_)), "got {err:?}");
        assert!(matches!(svc.status(), DictationState::Failed { .. }));
        assert!(svc.active.lock().unwrap().is_none(), "no session should be retained");
    }

    // Test 4b — microphone permission denied is reported distinctly.
    #[tokio::test]
    async fn permission_denied_is_reported() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok(String::new()),
            received,
        });
        let mic = MockMicrophone {
            permission: false,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0; 48],
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        let err = svc.start().unwrap_err();
        assert!(matches!(err, DictationError::PermissionDenied), "got {err:?}");
    }

    // Test 5 — successful transcription reaches the result boundary.
    #[tokio::test]
    async fn successful_transcription_is_returned() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok("Nabu records audio".into()),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0u8; 192000], // ~6s of 16kHz mono 16-bit
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        svc.start().unwrap();
        let text = svc.stop().await.unwrap();
        assert_eq!(text, "Nabu records audio");
        assert_eq!(svc.status(), DictationState::Completed { text: "Nabu records audio".into() });
    }

    // Test 6 — cancellation releases the session and inserts nothing.
    #[tokio::test]
    async fn cancellation_cleans_up_and_inserts_nothing() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok(String::new()),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0; 48],
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber.clone());
        svc.start().unwrap();
        assert_eq!(svc.status(), DictationState::Recording);

        svc.cancel().unwrap();
        assert_eq!(svc.status(), DictationState::Cancelled);
        assert!(svc.active.lock().unwrap().is_none(), "session must be released");

        // No transcription must have been produced.
        assert!(received.lock().unwrap().is_empty());
        // Stopping after cancel is an error, not a fake success.
        assert!(svc.stop().await.is_err());
    }

    // Test 7 — double-start is rejected.
    #[tokio::test]
    async fn double_start_is_rejected() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let transcriber = Arc::new(MockTranscriber {
            available: true,
            path: Some(PathBuf::from("model.bin")),
            text: Ok(String::new()),
            received,
        });
        let mic = MockMicrophone {
            permission: true,
            open_ok: true,
            start_ok: true,
            stop_result: Ok(CapturedAudio {
                wav_bytes: vec![0; 48],
                sample_rate: 16000,
                channels: 1,
            }),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let svc = svc_with(mic, transcriber);
        svc.start().unwrap();
        let err = svc.start().unwrap_err();
        assert!(matches!(err, DictationError::AlreadyActive), "got {err:?}");
        svc.cancel().unwrap();
    }
}
