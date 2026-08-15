# Nabu Browser Extension (Native Messaging Host)

A generic Manifest-V2 web extension for capturing web content directly into
Nabu, communicating with the Nabu native messaging host over standard native
messaging.

## Architecture

```
Browser Extension
    ↓ (native messaging)
Native Messaging Host
    ↓ (Unix socket)
Tauri App (CaptureEngine)
    ↓
ProcessingPipeline → StorageManager
```

The extension is a standard web extension (Manifest V2). It talks to the
browser's native messaging API, which launches the standalone
`native-messaging-host` binary and exchanges length-prefixed JSON messages.
The host forwards validated captures to the Nabu Tauri app over a Unix socket.

## Components

### Browser Extension

The browser extension provides several capture modes:

- **Capture Page**: Captures the current page URL, title, and favicon as a Bookmark
- **Capture Selection**: Captures selected text with source URL and title as a Note
- **Capture Full Page**: Captures the complete HTML content as a Document

### Native Messaging Host

A separate binary (`native-messaging-host`) that:

1. Reads length-prefixed JSON messages from the browser via stdin
2. Validates messages (command, payload size, capture type)
3. Forwards validated messages to the Tauri app via Unix socket
4. Reads responses from the Tauri app
5. Writes length-prefixed JSON responses to the browser via stdout

### Tauri App Integration

The Tauri app:

1. Registers `BrowserCaptureHandler` with the `CaptureEngine`
2. Starts a Unix socket server at `/tmp/nabu-native-messaging.sock`
3. Receives capture requests from the native messaging host
4. Dispatches requests to the `CaptureEngine`
5. Returns `CaptureResult` responses

## Installation

### 1. Build the Native Messaging Host

```bash
cargo build --bin native-messaging-host --release
```

### 2. Install the Native Messaging Host

Copy the binary to a permanent location:

```bash
cp target/release/native-messaging-host /usr/local/bin/nabu-native-messaging-host
chmod +x /usr/local/bin/nabu-native-messaging-host
```

### 3. Register the Native Messaging Host

The browser launches the native messaging host via a host registration
manifest named `com.nabu.capture.host`. Register it in your browser's native
messaging hosts directory (e.g. `~/.config/google-chrome/NativeMessagingHosts/`
for Chrome/Chromium, `~/.mozilla/native-messaging-hosts/` for Firefox).

The canonical per-platform registration is part of the native host setup; see
the native messaging host implementation for the exact manifest expected by
Nabu.

### 4. Load the Browser Extension

1. Open the browser's extension management page (`chrome://extensions/` for
   Chrome/Chromium, `edge://extensions/` for Edge, `about:addons` → "Debug
   Add-ons" → "Load Temporary Add-on" for Firefox).
2. Enable "Developer mode".
3. Click "Load unpacked" and select the `extensions/browser` folder.
4. Enable the extension.

## Security

The native messaging host implements several security measures:

- **Command allow-listing**: Only `capture` command is accepted
- **Payload size limit**: Maximum 1MB payload size
- **Capture type validation**: Only `bookmark`, `note`, and `document` are accepted
- **Unix socket permissions**: Socket is created with appropriate permissions

## Message Protocol

### Request Format

```json
{
  "requestId": 1,
  "command": "capture",
  "captureType": "bookmark",
  "payload": {
    "url": "https://example.com",
    "title": "Example Domain",
    "favicon": "https://example.com/favicon.ico"
  }
}
```

### Response Format

```json
{
  "requestId": 1,
  "success": true,
  "result": {
    "success": true,
    "knowledge_object_id": "uuid",
    "message": "Browser bookmark captured successfully"
  }
}
```

## Capture Types

### Bookmark

Captures:
- URL
- Page title
- Favicon (when available)

Creates a `KnowledgeObject` of type `Bookmark`.

### Note

Captures:
- Selected text
- Source URL
- Page title

Creates a `KnowledgeObject` of type `Note`.

### Document

Captures:
- Complete HTML
- Page URL
- Title

Creates a `KnowledgeObject` of type `Document`.

## Development

### Running Tests

```bash
# Run nabu-core tests
cargo test -p nabu-core

# Run native messaging host tests
cargo test --bin native-messaging-host
```

### Debugging

Enable debug logging in the Tauri app to see capture requests and responses.

## Troubleshooting

### Native messaging host not found

Ensure the host registration manifest `com.nabu.capture.host` is installed in
your browser's native messaging hosts directory. The browser must be restarted
after installing the manifest.

### Socket connection refused

Ensure the Tauri app is running before attempting to capture. The socket server
starts when the app launches.

### Permission denied

Ensure the native messaging host binary is executable:
```bash
chmod +x /usr/local/bin/nabu-native-messaging-host
```
