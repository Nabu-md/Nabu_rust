# Architecture

Nabu is a **Tauri v2** desktop application built on a **Rust core**
(`crates/nabu-core`) with a **Dioxus/WASM** user interface (`crates/nabu-ui`)
rendered inside a Tauri WebView. Content flows through the capture →
processing → storage → index pipeline.

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              Tauri v2 Host (Rust)                           │
│                                                                            │
│  ┌──────────────────┐   ┌────────────────────┐   ┌──────────────────┐      │
│  │  src-tauri/src   │   │  crates/nabu-core  │   │  native/platform │      │
│  │  tauri commands  │   │  models, storage,  │   │  macOS: Vision,  │      │
│  │  settings, main  │   │  indexer, capture  │   │  screencapture, │      │
│  │  (IPC bridge)    │   │  ACP/MCP servers   │   │  whisper        │      │
│  └────────┬─────────┘   └────────┬───────────┘   └──────────────────┘      │
│           │                      │                                          │
│           │  Tauri webview (wasm-bindgen)                                  │
│           ▼                                                                  │
│  ┌──────────────────────────────────────────────────────────────────┐      │
│  │              crates/nabu-ui (Dioxus 0.6.3, WASM)                   │      │
│  │  App, NoteView, Blocks, Graph, FileTree, Sidebar, Settings       │      │
│  │  (built by `cargo dioxus build`, served via index.html)           │      │
│  └──────────────────────────────────────────────────────────────────┘      │
└────────────────────────────────────────────────────────────────────────────┘
```

## Data Flow

1. **Capture → Core:** Content is captured from the clipboard, screen, file
   drops, watch folders, URLs, emails, GitHub/YouTube links, and native
   messaging hosts. Capture handlers produce [`KnowledgeObject`](crates/nabu-core/src/models)
   values and enqueue them on the job queue.

2. **Pipeline → Indexer:** The processing pipeline
   (`crates/nabu-core/src/processing`) runs OCR (macOS Vision), generates text
   embeddings, transcribes audio (whisper), and parses markdown into a graph.
   The indexer maintains the full-text, tag, and vector indexes. A write lock
   prevents races between file writes and edits.

3. **Core → Tauri → UI:** State changes are surfaced to the Dioxus UI through
   typed Tauri commands (IPC). The webview layer re-renders from the resulting
   state.

4. **UI → Tauri → Core:** User actions (edit note, toggle task, rename file,
   run a tool) go through named `#[tauri::command]` handlers →
   `nabu-core` → file system.

## Key Modules

### Tauri Host (`src-tauri/`)

- `Cargo.toml`, `build.rs` (`tauri_build::build()`), `tauri.conf.json` — shell
  configuration, window definition, bundle/icons, and the beforeDev/beforeBuild
  hooks that build the Dioxus frontend.
- `src/main.rs` / `src/lib.rs` — Tauri application entry point and plugin setup.
- `src/commands.rs` — `#[tauri::command]` handlers exposing `nabu-core` to the
  UI (inbox, templates, history, recovery, statistics, settings, dictation, IPC).
- `src/settings.rs` — settings persistence via Tauri's settings layer.
- `src/dictation.rs` — dictation IPC wiring (clipboard cache, file-drop capture).
- `src/history.rs`, `src/recovery.rs`, `src/diagnostics.rs`, `src/event_bridge.rs`
  — history/recovery session management, diagnostics, and native-event bridging.
- `scripts/` — build/dev scripts (`build-dioxus.sh`, `run-dioxus.sh`,
  `gen-icons.sh`).

### Rust Core (`crates/nabu-core/`)

- `src/models/` — `KnowledgeObject`, content types, capture sources, metadata.
- `src/storage/` — vault persistence via the `StorageManager` and sidecar stores.
- `src/indexer.rs` — full-text, tag, and graph indexes; `KnowledgeGraph`.
- `src/capture/` — capture handlers (clipboard, screenshot, file drop, watch
  folder, URL, email, GitHub, YouTube) producing `KnowledgeObject`s.
- `src/processing/` — processing pipelines, OCR (Vision), embeddings,
  transcription (whisper).
- `src/native/` — macOS FFI: `screenshot` (`screencapture`), `vision` (OCR),
  `whisper` (transcription), `pdfkit`.
- `src/acp/` — ACP (Agent Communication Protocol) server and client over stdio.
- `src/mcp/` — MCP (Model Context Protocol) server.
- `src/rpc/` — JSON-RPC types shared by ACP/MCP.
- `src/bin/` — `nabu-mcp-server` and `acp-test-agent` binaries.

### UI (`crates/nabu-ui/`)

A standalone Dioxus 0.6.3 workspace (`crate-type = ["cdylib", "rlib"]`),
compiled to `wasm32-unknown-unknown` and bundled into the Tauri WebView.

- `src/lib.rs` — Dioxus app entry: the `App` root component and view routing.
- `src/components/` — view-mode components (settings panel, inbox, templates,
  history, recovery, statistics, dictation pill).
- `src/ipc.rs` — typed IPC clients that invoke the `#[tauri::command]`
  handlers in `src-tauri/src/commands.rs`.
- `index.html` + Tailwind (`npm run css:build` → `generated/tailwind.css`) —
  boot splash and stylesheet consumed by the build/dev hooks.

## Security Architecture

The UI runs as a **sandboxed WebView** rather than a Node-integrated renderer:

- The Dioxus/WASM bundle has no Node.js access and no direct filesystem access.
- All privileged operations are gated behind explicitly-declared Tauri commands
  (`src-tauri/src/commands.rs`). The webview invokes each command by name; there
  is no dynamic channel routing.
- IPC arguments are typed at the Rust call sites via serde across the Tauri
  bridge, and sensitive operations (vault path, file writes) are validated
  server-side.

## Tech Decisions

- **Tauri v2** over Electron — a Rust core running in a single native WebView,
  yielding a far smaller bundle size and attack surface than an Electron/Node
  runtime.
- **Dioxus 0.6 (WASM)** over React — a native-Rust UI compiled to WebAssembly
  that shares types and logic directly with `nabu-core`.
- **Tailwind CSS** for styling — the stylesheet is built at compile time
  (`npm run css:build`) rather than shipped as a runtime CSS-in-JS dependency.
- **ACP + MCP** for agent/tool integration — both protocols are implemented
  natively in Rust within `crates/nabu-core` (`src/acp/`, `src/mcp/`).
- **useReducer + Context** over Redux — retained for the Dioxus app state; the
  state shape is not complex enough to warrant a store library.
