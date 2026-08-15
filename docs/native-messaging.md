# Native Messaging Integration

This document describes the generic native messaging installation procedure for
browsers that support the native messaging protocol — **Chrome/Chromium**,
**Firefox**, **Edge**, and **Brave**.

The integration is browser-agnostic: the native messaging host binary is
bundled inside the Nabu application, and a small JSON manifest file (one per
browser) tells the browser where to find it.

## Architecture

```
Browser Extension
    ↓ (native messaging protocol — length-prefixed JSON over stdio)
native-messaging-host  (bundled in the Nabu application)
    ↓ (Unix socket: /tmp/nabu-native-messaging.sock)
Nabu Tauri App
    ↓ (socket server → CaptureEngine::ingest)
CaptureEngine → ProcessingPipeline → StorageManager
```

## Wire protocol

Messages are standard native-messaging length-prefixed JSON:

1.  **4-byte big-endian length prefix** (u32)
2.  **JSON body** (UTF-8)

The canonical message schema uses **camelCase** field names:

### Request (browser → host)

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

### Response (host → browser)

```json
{
  "requestId": 1,
  "command": "capture",
  "captureType": "bookmark",
  "success": true,
  "result": {
    "object_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

### Field mapping (wire ↔ Rust)

| Wire field (camelCase) | Rust field (snake_case) |
|---|---|
| `requestId` | `request_id` |
| `captureType` | `capture_type` |
| `command` | `command` |
| `payload` | `payload` |
| `success` | `success` |
| `error` | `error` |
| `result` | `result` |

The `#[serde(rename_all = "camelCase")]` attribute on the Rust `Message`
struct handles this mapping at the serialization boundary.

## Supported capture types

| `captureType` | Description |
|---|---|
| `bookmark` | Save a URL as a bookmark |
| `note` | Save selected text as a note |
| `document` | Save full page HTML as a document |
| `reader_mode` | Save reader-mode extracted HTML as an article |
| `safari_reader` | Alias for `reader_mode` |
| `clipboard` | Capture from clipboard contents |
| `screenshot` | Capture a screenshot |
| `screen_capture` | Capture a screen region |
| `file_drop` | Capture a dropped file |
| `watch_folder` | Capture from a watched folder |
| `youtube` | Capture a YouTube URL |
| `github` | Capture a GitHub repository URL |
| `email` | Capture an email (.eml) |
| `article` | Capture article text/HTML |
| `browser` | Generic browser capture (routes by URL) |

## Installation

### 1. Install Nabu

Install Nabu using the standard Tauri app bundle (`.app` on macOS, `.deb`/`.rpm`
on Linux, `.msi`/`.exe` on Windows). The native messaging host binary is
**automatically included** in the application bundle — no separate binary
download or copy is required.

### 2. Locate the bundled host binary

After installation, the binary lives inside the application bundle:

| Platform | Default install path |
|---|---|
| **macOS** | `/Applications/Nabu.app/Contents/Resources/native-messaging-host` |
| **Linux** | `/usr/lib/nabu/native-messaging-host` (or alongside the app binary) |
| **Windows** | `C:\Program Files\Nabu\native-messaging-host.exe` |

> The browser-agnostic manifest below uses the macOS path as an example.
> Adjust the `path` field for your platform.

### 3. Register the native messaging host manifest

Create a manifest file named `com.nabu.capture.host.json` in the browser's
native messaging hosts directory. The manifest tells the browser where the
host binary lives and which extensions are allowed to communicate with it.

#### Chrome / Chromium / Edge / Brave

**macOS:**
```
~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nabu.capture.host.json
```

**Linux:**
```
~/.config/google-chrome/NativeMessagingHosts/com.nabu.capture.host.json
```
(Use `chromium` instead of `google-chrome` for Chromium-based browsers.)

**Windows:**
```
%LOCALAPPDATA%\Google\Chrome\User Data\NativeMessagingHosts\com.nabu.capture.host.json
```

#### Firefox

**macOS:**
```
~/Library/Application Support/Mozilla/NativeMessagingHosts/com.nabu.capture.host.json
```

**Linux:**
```
~/.mozilla/native-messaging-hosts/com.nabu.capture.host.json
```

**Windows:**
```
%APPDATA%\Mozilla\NativeMessagingHosts\com.nabu.capture.host.json
```

### Manifest file format

Chrome / Chromium / Edge / Brave use `allowed_origins`:

```json
{
  "name": "com.nabu.capture.host",
  "description": "Nabu native messaging host for browser capture",
  "path": "/Applications/Nabu.app/Contents/Resources/native-messaging-host",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://YOUR_EXTENSION_ID_HERE/"
  ]
}
```

Firefox uses `allowed_extensions` instead of `allowed_origins`:

```json
{
  "name": "com.nabu.capture.host",
  "description": "Nabu native messaging host for browser capture",
  "path": "/Applications/Nabu.app/Contents/Resources/native-messaging-host",
  "type": "stdio",
  "allowed_extensions": [
    "nabu-capture@proton.me"
  ]
}
```

> **Finding your extension ID:**
> - Chrome/Chromium: Go to `chrome://extensions/`, enable "Developer mode", and
>   the ID is displayed under each extension card.
> - Firefox: Go to `about:debugging#/runtime/this-firefox`, find your extension,
>   and its ID is listed there (for temporary add-ons it ends with `@test-mechanic`).

### 4. Install the browser extension

Load the unpacked extension from `extensions/browser/`:

1. Open the browser's extensions management page:
   - Chrome/Chromium/Edge/Brave: `chrome://extensions/`
   - Firefox: `about:debugging#/runtime/this-firefox`
2. Enable "Developer mode" (Chrome) or "Debug mode" (Firefox).
3. Click "Load unpacked" (Chrome) or "Load Temporary Add-on" (Firefox) and
   select the `extensions/browser` folder.
4. Enable the extension.

### 5. Verify the connection

1. Launch Nabu and open your vault.
2. In the browser, open the Nabu extension popup.
3. Click any capture button (e.g., "Capture Page").
4. The extension should show "Captured!" and the capture should appear in your
   Nabu vault.

If the connection fails, check the following:

- **Manifest path is correct:** Ensure the `path` in the manifest JSON points
  to the actual location of the bundled `native-messaging-host` binary.
- **Browser restarted:** Some browsers require a restart after installing
  the native messaging manifest.
- **Nabu is running:** The socket server starts when Nabu launches; the
  native messaging host cannot connect if the app is not running.
- **Socket file:** The Unix socket lives at `/tmp/nabu-native-messaging.sock`
  (macOS/Linux) or a named pipe on Windows. It is created with `0600`
  permissions.
- **Browser console:** Check the browser's developer console for
  `browser.runtime.lastError` messages describing native messaging failures.

## Error handling

The native messaging host and socket server produce explicit errors for:

| Condition | Error |
|---|---|
| Malformed JSON in native message | `DeserializationError` |
| Missing `captureType` field | `ValidationError: "Capture type is required for capture command"` |
| Invalid capture type value | `ValidationError: "Invalid capture type: …"` |
| Malformed native messaging framing | `IoError` (stdin read failure) |
| Socket connection failure | `SocketError: "Failed to connect to Nabu socket at …"` |
| Host startup failure | `process::exit(1)` with error on stderr |
| Packaged binary not found | Browser reports "native messaging host not found" |

Malformed requests are never silently treated as successful captures. The
host responds with a JSON error message containing `success: false` and a
descriptive `error` field.
