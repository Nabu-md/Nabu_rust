//! Native macOS screen capture.
//!
//! On macOS this uses the `screencapture` CLI tool (bundled with the OS) to
//! capture the full screen or a selection, returning PNG bytes. Other platforms
//! return [`NativeError::UnsupportedPlatform`].
//!
//! The capture function is invoked by the [`ScreenshotHandler`](crate::capture::handler::ScreenshotHandler)
//! when a `CaptureRequest` with `CaptureData::ScreenCapture` is received,
//! keeping the actual screen-grabbing FFI isolated inside `native/` — all
//! Objective-C / shell interaction stays within this module.

use super::error::NativeError;

/// Options controlling how a screen capture is performed.
#[derive(Debug, Clone)]
pub struct ScreenCaptureOptions {
    /// Capture a specific selection (x, y, width, height) in screen coordinates.
    /// When `None`, the full primary display is captured.
    pub selection: Option<(i32, i32, u32, u32)>,
    /// Include the cursor in the capture.
    pub show_cursor: bool,
}

impl Default for ScreenCaptureOptions {
    fn default() -> Self {
        Self {
            selection: None,
            show_cursor: true,
        }
    }
}

/// Capture the screen and return PNG image bytes.
///
/// On macOS this shells out to the system `screencapture` utility.
/// On other platforms it returns an error.
pub fn capture_screen(options: &ScreenCaptureOptions) -> Result<Vec<u8>, NativeError> {
    imp::capture_screen(options)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub fn capture_screen(options: &ScreenCaptureOptions) -> Result<Vec<u8>, NativeError> {
        use std::process::Command;

        let mut args: Vec<String> = Vec::new();

        if let Some((x, y, w, h)) = &options.selection {
            args.push(format!("-R{},{},{},{}", x, y, w, h));
        }

        if options.show_cursor {
            args.push("-C".to_string());
        }

        if !options.selection.is_some() {
            args.push("-x".to_string()); // silent
        }

        args.push("-t".to_string());
        args.push("png".to_string());

        // macOS `screencapture` cannot write PNG bytes to stdout: a trailing "-"
        // is interpreted as a literal output *filename*, silently producing a
        // stray file named "-" in the caller's working directory (this
        // previously created the repeatedly-committed `crates/nabu-core/-`
        // artifact). Capture to a private temp file and read the bytes back.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NativeError::CallFailed("system clock unavailable".to_string()))?
            .as_nanos();
        let tmp = std::env::temp_dir().join(format!(
            "nabu-screencapture-{}-{}.png",
            std::process::id(),
            nanos
        ));
        args.push(tmp.to_string_lossy().into_owned());

        let output = Command::new("screencapture")
            .args(&args)
            .output()
            .map_err(|e| NativeError::CallFailed(format!("screencapture not found: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = std::fs::remove_file(&tmp);
            return Err(NativeError::CallFailed(format!(
                "screencapture failed: {}",
                stderr.trim()
            )));
        }

        let png = std::fs::read(&tmp)
            .map_err(|e| NativeError::CallFailed(format!("reading capture output: {}", e)))?;
        let _ = std::fs::remove_file(&tmp);

        if png.is_empty() {
            return Err(NativeError::CallFailed(
                "screencapture returned empty output".to_string(),
            ));
        }

        Ok(png)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    pub fn capture_screen(_options: &ScreenCaptureOptions) -> Result<Vec<u8>, NativeError> {
        Err(NativeError::UnsupportedPlatform)
    }
}
