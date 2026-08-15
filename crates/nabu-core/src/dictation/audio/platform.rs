//! Platform-specific microphone capture for dictation.
//!
//! On macOS this drives `AVAudioRecorder` through `objc2`'s `msg_send!`
//! macro. The `AVAudioRecorder` NSObject is allocated, started and stopped
//! entirely within a private worker thread, because `objc2`'s `Retained`
//! handles are intentionally `!Send` — confining them to one thread is what
//! keeps the capture path free of data races. The `PlatformMicrophone` handle
//! that the service holds only owns an `mpsc` sender and a `JoinHandle`, both
//! of which are `Send`.

use super::{CapturedAudio, DictationError, Microphone};

#[cfg(target_os = "macos")]
mod mac;

#[cfg(not(target_os = "macos"))]
mod fallback;

pub use mac::PlatformMicrophone;
