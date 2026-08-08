# Changelog

All notable changes to Nabu are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Note:** The `1.0.0` entry below documents the **legacy pre-migration release**
> (Electron/React architecture). It was superseded by the pure-Rust rewrite
> (Tauri v2 + Dioxus 0.6 + `nabu-core`), whose features are recorded under
> `[Unreleased]`. History and migration records were consolidated into
> the Architecture Decision Records in [`docs/architecture/adrs/`](docs/architecture/adrs/).

## [Unreleased] — 2026-08-08

### Added

#### Capability Platform Foundation
- **Process Supervision** — managed subprocess lifecycle (ProcessSupervisor) with health checks and restart policies
- **Synchronization** — sync folder/status/conflict models with provider-agnostic scheduling
- **Diagnostics Pipeline** — streaming diagnostics with provider contracts and editor bridge (`diagnostic_requested` IPC)
- **Live Event Bridge** — unified EventBus published to frontend via single `nabu-event` channel with typed subscriptions
- **Capability Registry** — runtime enable/disable/list of platform capabilities over IPC
- **Plugin Foundation** — plugin manager with manifests, permissions, dependency & feature registries, provider + event contracts
- **Health & Metrics** — live service health reporting and runtime metrics over IPC (`health_check`, `metrics`)
- **Graceful Shutdown** — coordinated teardown via `on_exit` handler with socket server and ApplicationContext shutdown

#### Dioxus UI Migration (Phase 0 Complete)
- **Settings panel** — 15-tab configuration with `AppSettings` struct and IPC persistence
- **Inbox** — batch handlers, drag/drop, keyboard shortcuts
- **Dictation pill** — opacity loading, clipboard cache, drop-zone IPC, mode switching
- **Version history & recovery** — snapshot browsing, diff view, restore, duplicate, session recovery
- **Statistics** — vault metrics, growth histograms, tag analytics, recently modified/created
- **Template editor & picker** — backend-wired template management with save/delete/duplicate/favourite

#### Diagnostic Platform
- **Diagnostic Provider trait** — registered with DiagnosticPlatform for on-demand analysis
- **Diagnostic response** — batch with `DiagnosticStyleMap` for editor rendering
- **PerformanceMonitor** — runtime metrics aggregation with tracing integration

#### Graph View Modes
- **Tag View mode** — visualize tag co-occurrence as nodes in the graph with color-coded tags
- **Blocks View mode** — visualize block references (`^id` definitions linked from `[[Note#^id]]` links) as a graph of notes and their blocks

#### macOS Vision OCR Pipeline
- **Automatic OCR** — extract text from images using macOS Vision framework on file add
- **Companion Notes** — OCR text saved as `.ocr.md` notes linked to source images

#### PDF Annotation
- **PDF Viewer** — dedicated pane for viewing PDF documents with navigation and zoom
- **Highlight annotations** — select and annotate text in PDFs with yellow overlay
- **Note cards** — convert annotations to linked notes with source frontmatter

#### Audio Dictation
- **Fn-key dictation activation** — hold `fn` key to start voice-to-text dictation
- **Whisper.cpp integration** — local speech-to-text with Base model (~140MB)
- **Large-V3 Turbo model** — optional download (~550MB) for higher accuracy
- **Always-on-top widget** — transparent widget shows waveform animation during dictation

### Fixed

- Security review confirms `contextIsolation` remains enabled
- No `nodeIntegration` or `allow-same-origin` in any webview or widget
- All audio captured in memory only, never written to disk as raw PCM
- Socket server handle now registered via `app.manage()` so graceful shutdown works
- `versions_restore` routes through `StorageManager::save_note_content()` instead of raw `std::fs::write`, keeping Indexer and VaultGraph in sync

### Security

- OCR Swift helper restricted to provided image path only
- Whisper process has no network access beyond model download (user-initiated)
- PDF viewer pane maintains same security posture as main window

## [1.0.0] — 2026-07-04

### Added

- **Setup wizard** — first-launch flow to create or open a vault
- **File tree** — recursive tree with folder/note creation, rename, delete via context menu
- **Inline editing** — `Cmd+E` to edit, 1-second auto-save debounce, `Cmd+S` to save
- **Graph view** — d3-force directed graph of `[[wiki-link]]` relationships with drag, pan, zoom
- **Full-text search** — every word indexed, results sorted by relevance
- **Tag filtering** — YAML frontmatter `tags:` → tag panel → filter file tree
- **Backlinks** — every note shows which other notes link to it with a snippet
- **Templates** — create notes from templates stored in `_templates/` (Meeting Note, Bug Report, Project Brief)
- **Themes** — dark, light, system — follows macOS preference
- **Export** — export any note as HTML or print to PDF
- **External edit detection** — chokidar watches for external file changes, hot-reloads the visual canvas
- **HTML-native app blocks** — embed raw HTML, CSS, and JavaScript inside notes via sandboxed iframe
- **Custom remark plugins** — wiki-links, toggle blocks, task lists all parsed from standard markdown
- **Vector index** — ONNX-based semantic search using bundled bge-micro-v2 model
- **Property-based tests** — 278+ tests including fast-check invariants for graph, indexing, and templates
- **Unsigned DMG distribution** — universal binary for Intel + Apple Silicon, no Apple Developer account required

### Architecture

- Three-tier Electron architecture: main process ↔ preload bridge ↔ React renderer
- Zod v4 schemas for bidirectional IPC validation (14+ channels)
- CRDT-ready sync foundation in the private nabu-cloud monorepo
