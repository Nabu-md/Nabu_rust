//! Native messaging host for browser extension communication.
//!
//! This binary implements the native messaging protocol:
//! - Reads length-prefixed JSON messages from stdin (browser → host)
//! - Validates and forwards them to the Nabu Tauri application via Unix socket
//! - Reads responses from the Tauri application
//! - Writes length-prefixed JSON responses to stdout (host → browser)
//!
//! The native messaging protocol is the same for all browsers that support it
//! (Chrome/Chromium, Firefox, Edge, Brave).  The browser launches this binary
//! as a subprocess and communicates over stdin/stdout using the standard
//! length-prefixed JSON framing.
//!
//! ## Registration
//!
//! The browser discovers this binary via a native messaging manifest installed
//! in the browser's native messaging hosts directory.  The manifest specifies
//! the path to this executable.  See `docs/native-messaging.md` for
//! per-platform installation instructions.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

// The native messaging protocol types live in the app library crate.
// The socket path is shared with the socket server module so both sides
// always agree on the IPC endpoint location.
use app_lib::native_messaging::{Message, NativeMessagingError, NativeMessagingHost};
use app_lib::native_messaging_socket::SOCKET_PATH;

fn main() {
    if let Err(e) = run() {
        eprintln!("Native messaging host error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), NativeMessagingError> {
    let mut host = NativeMessagingHost::new();

    loop {
        // Read message from browser (stdin)
        let message = host.read_message()?;

        // Validate the message
        let validated = host.validate_message(&message)?;

        // Forward to Nabu Tauri app via Unix socket
        let response = forward_to_tauri(&validated)?;

        // Write response to browser (stdout)
        host.write_message(&response)?;
    }
}

/// Forwards a validated native-messaging message to the Nabu application via
/// Unix socket and returns the application's response.
///
/// The socket server runs inside the Tauri application process.  If the
/// Nabu application is not running or the socket is unavailable, an
/// explicit `SocketError` is returned — the browser receives a JSON
/// error response rather than silently dropping the capture.
fn forward_to_tauri(message: &Message) -> Result<Message, NativeMessagingError> {
    let socket_path = PathBuf::from(SOCKET_PATH);

    // Connect to Nabu app's Unix socket
    let mut stream = UnixStream::connect(&socket_path).map_err(|e| {
        NativeMessagingError::SocketError(format!(
            "Failed to connect to Nabu socket at {}: {}",
            socket_path.display(),
            e
        ))
    })?;

    // Serialize the message
    let message_json = serde_json::to_vec(message)
        .map_err(|e| NativeMessagingError::SerializationError(e.to_string()))?;

    // Write length-prefixed message (4-byte big-endian length prefix
    // + JSON body, per the native messaging protocol)
    let length = (message_json.len() as u32).to_be_bytes();
    stream
        .write_all(&length)
        .map_err(|e| NativeMessagingError::SocketError(format!("Failed to write length: {}", e)))?;
    stream.write_all(&message_json).map_err(|e| {
        NativeMessagingError::SocketError(format!("Failed to write message: {}", e))
    })?;
    stream
        .flush()
        .map_err(|e| NativeMessagingError::SocketError(format!("Failed to flush: {}", e)))?;

    // Read response length
    let mut length_bytes = [0u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .map_err(|e| {
            NativeMessagingError::SocketError(format!("Failed to read response length: {}", e))
        })?;
    let length = u32::from_be_bytes(length_bytes) as usize;

    // Read response body
    let mut response_bytes = vec![0u8; length];
    stream.read_exact(&mut response_bytes).map_err(|e| {
        NativeMessagingError::SocketError(format!("Failed to read response body: {}", e))
    })?;

    // Deserialize response
    let response: Message = serde_json::from_slice(&response_bytes)
        .map_err(|e| NativeMessagingError::SerializationError(e.to_string()))?;

    Ok(response)
}
