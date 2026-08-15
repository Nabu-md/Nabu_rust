//! Dictation service — delegates to `nabu_core`'s real implementation.
//!
//! Microphone capture (macOS `AVAudioRecorder` driven by `objc2`) and Whisper
//! transcription both live in `nabu-core`, where they are covered by unit
//! tests that run without the Tauri runtime. This module re-exports that
//! service so `commands.rs` keeps its `State<'_, crate::dictation::DictationService>`
//! typing and `lib.rs` keeps its `.manage(...)` registration point.
//!
//! The previous sox-subprocess implementation is intentionally removed: it
//! fabricated a `"Dictation stopped"` result and could not surface real
//! microphone / model errors. The core service instead drives the microphone
//! capture lifecycle directly and returns the real transcription text on stop.

pub use nabu_core::dictation::{DictationError, DictationService, DictationState};
