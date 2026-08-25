# ADR 001: Remove the browser extension subsystem

**Status:** Accepted — feature removed from the MVP
**Date:** 2026-08-15
**Deciders:** Nabu Labs

## Context

Nabu is intentionally removing the built-in browser-extension subsystem from the MVP. The subsystem consisted of:

- A browser extension (source tree under `extensions/`)
- An extension host binary and its socket server module under `src-tauri/src/`
- A manifest example and install script under `src-tauri/`
- Browser, Safari-reader, and article capture handlers in `nabu-core/src/capture/handler.rs`
- Handler registrations for browser and safari-reader capture in the capture engine
- HTML-to-article extraction helpers
- Associated build, staging, and resource wiring in Tauri configuration and scripts
- Documentation references in README and architecture docs

Removing this subsystem simplifies the codebase, eliminates an attack surface, and reduces the Tauri packaging surface area. The capture engine continues to support all non-extension capture sources (clipboard, screenshot, file drop, folder watch, YouTube, GitHub, email, bookmarks).

## Decision

Nabu no longer ships the browser-extension / extension-host subsystem. All implementation code, host infrastructure, build wiring, scripts, documentation, and orphaned helpers associated with this feature have been removed.

Removed components:

| Component | Location |
|---|---|
| Browser extension source tree | `extensions/` |
| Browser, safari-reader, and article capture handlers | `nabu-core/src/capture/handler.rs` |
| Browser and safari-reader handler registrations | `nabu-core/src/capture/engine.rs` |
| Build/packaging wiring | `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json` |
| Build scripts | `src-tauri/scripts/` |
| Host module declarations and socket wiring | `src-tauri/src/lib.rs` |
| Documentation references | `README.md`, `docs/architecture/architecture.md` |

The capture handler count decreased from 11 to 8 (clipboard, screenshot, file drop, watch folder, YouTube, GitHub, email, bookmark).

## Pre-Purge Commit

Pre-purge commit: `ea9cc8427eff0767c25b3594e1a6d01383d48a31`

> Note: This commit was a halfway cleanup that had already deleted many implementation files. The complete pre-purge implementation (including already-deleted files) is preserved in the parent commit `7e29423cc63f7807a8833acefcf4b20e4ba8d034`. For files already deleted at the pre-purge commit, use the parent commit to inspect their full content.

## Restore Procedure

The removed implementation can be reconstructed from the pre-purge commit. Use:

```bash
git show ea9cc8427eff0767c25b3594e1a6d01383d48a31 -- <path>
```

For files deleted in the halfway purge (extension source, host binary modules, etc.), use the parent commit:

```bash
git show 7e29423cc63f7807a8833acefcf4b20e4ba8d034 -- <path>
```

To inspect the complete list of removed files:

```bash
git show ea9cc8427eff0767c25b3594e1a6d01383d48a31 --stat
```

Key paths to inspect:

- `extensions/` — browser extension source (manifest, background, content scripts, popup, icons)
- `nabu-core/src/capture/handler.rs` — handler implementations (browser, safari-reader, and article handlers; HTML-to-article extraction helpers)
- `nabu-core/src/capture/engine.rs` — handler registrations (browser and safari-reader)
- `nabu-core/src/models/knowledge_object.rs` — `CaptureSource` variants for browser, safari-reader, and article
- `src-tauri/src/lib.rs` — host socket wiring and module declarations
- `src-tauri/Cargo.toml` — `[[bin]]` entry for the host binary
- `src-tauri/build.rs` — host staging logic
- `src-tauri/tauri.conf.json` — resource entries
- `src-tauri/scripts/run-dioxus.sh`, `src-tauri/scripts/build-dioxus.sh` — host build steps
- `README.md` — "Native Integrations" bullet and architecture diagram entries

Restoring the feature requires reviewing the complete dependency and build-wiring set, not just recovering one file. The handler types, the engine registrations, the model enum variants, the Tauri build wiring, the host modules, and the browser extension source are all interdependent.

## Consequences

- **Smaller codebase:** the extension source tree, host binary, socket server, and extraction helpers are gone.
- **No browser extension:** Nabu no longer ships a browser extension for capturing web content.
- **No extension host:** the host binary and socket server are removed; Tauri packaging is simplified.
- **Simpler build:** no host binary compilation, staging, or resource bundling in the Tauri build.
- **Capture engine streamlined:** 8 handlers (down from 11). The browser, safari-reader, and article capture paths are no longer available.
- **Recoverable:** the complete implementation remains in Git history at the pre-purge commit and its parent.
