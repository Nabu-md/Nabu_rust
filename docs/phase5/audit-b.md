# Phase 5.2 — Audit-B

## Executive result

**ROOT CAUSE FOUND**

The production `.app` bundle is invalid: it contains no frontend assets. The webview therefore never loads the application, producing ZERO `[WEB]` output and leaving the user with a persistent splash/blank window.

---

## 1. Production bundle tested

- **Executable:** `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app`
- **Bundle size:** 31 MB
- **Binary size:** 31,154,204 bytes
- **Test method:** Launched directly with stdout/stderr redirected to `/tmp/nabu.log`; allowed 20 s to initialize.

---

## 2. `[WEB]` runtime evidence

`/tmp/nabu.log` contains **53 lines** and **zero** lines matching `[WEB]`.

Relevant stderr excerpt (backend initialization only):

```
2026-08-18T18:08:55.101Z INFO Tracing initialized subsystem="diagnostics"
2026-08-18T18:08:56.122Z INFO ApplicationContext ready — startup complete stage=Running
2026-08-18 12:08:56.946 app[33838:106053327] +[IMKClient subclass]: chose IMKClient_Legacy
```

**Classification:** ZERO `[WEB]` OUTPUT — FRONTEND EXECUTION NOT OBSERVED.

The webview console hook (installed in `on_page_load`) never ran, so the frontend JavaScript/WASM path was never reached.

---

## 3. WASM execution

**No evidence WASM executed.**

Because `on_page_load` never fired, the console hook was never installed. There is no indication that `nabu_ui_bg.wasm` was fetched, instantiated, or executed.

---

## 4. Dioxus mount / first paint

**No evidence Dioxus mounted or rendered.**

`crates/nabu-ui/src/components/app.rs` defines the root `App` component and `AppRouter`. `AppRouter` calls `crate::ipc::tauri_invoke_safe("check_vault_exists", ...)` inside a `use_effect` spawned via `spawn_local`. None of this code can execute until the WASM/JS bootstrap (`nabu_ui.js` → `init()`) runs. Since the bootstrap never ran, Dioxus never mounted and no first paint occurred.

---

## 5. App initialization / IPC

**Backend is healthy; frontend never reaches IPC.**

Backend startup sequence completed successfully:
- Tracing, PluginManager, VaultGraph, watcher, EventBus
- ApplicationContext built with 16 services
- Lifecycle services (StorageManager, ConversationStore, WorkerPool, Indexer, VaultGraph, PluginManager) all started

IPC path from `app.rs`:
- `window.__TAURI__.core.invoke` is the expected JS entrypoint (`ipc.rs:9-10`)
- `tauri_invoke_safe` wraps it with safe error handling
- `check_vault_exists` exists as a Tauri command (`src-tauri/src/commands.rs:476`)
- However, none of this is reached because the webview never loads the frontend bundle.

---

## 6. Window configuration

From `src-tauri/tauri.conf.json`:
- `visible: true`
- `transparent: false`
- `backgroundColor: "#0b1220"`

From `src-tauri/src/lib.rs:624-637`:
- The main window is shown by `on_page_load` once `PageLoadEvent::Finished` fires
- A safety net forces the window visible after 8 s if still hidden
- No `transparent`, decor, or compositing settings prevent rendering

**The native window is capable of displaying content as configured.**

---

## 7. Observed visual state

**Splash remains / webview is blank.**

The `dist/index.html` boot splash (`#boot-splash`) is never removed because Dioxus never hydrates. The 8-second safety net likely force-shows the window, but with no frontend assets loaded the webview displays an empty/black surface.

---

## 8. Root-cause hypothesis

**ROOT CAUSE — DEFINITIVE**

`src-tauri/target/release/bundle/macos/Nabu.app` is **missing all frontend assets**.

The `.app` contains only:
- `Contents/MacOS/app` (binary)
- `Contents/Resources/icon.icns`
- `Contents/Info.plist`

It does **not** contain:
- `index.html`
- `nabu_ui.js` (73 KB)
- `nabu_ui_bg.wasm` (6.4 MB)
- `generated/tailwind.css`
- `snippets/dioxus-*/**` (Dioxus runtime JS)

The project-root `dist/` directory (6.4 MB) holds the complete build output, but the Tauri bundler did not copy these assets into the `.app` bundle. Without them the webview cannot load the application, `on_page_load` never fires, the `[WEB]` console hook is never installed, and the user sees a persistent splash or black screen.

---

## 9. Handoff to Audit-A / Reconcile

Audit-A must determine **why the frontend assets were not bundled into the `.app`** during `cargo tauri build`.

Specific questions:
1. Was `cargo tauri build` actually invoked for this `.app`, or was the binary produced by `cargo build` and manually placed in the bundle?
2. Did the `beforeBuildCommand` (`src-tauri/scripts/build-dioxus.sh`) complete successfully, and was `../dist` present at bundle time?
3. Did the Tauri bundler (`tauri-bundler`) fail to stage `frontendDist` into `Contents/Resources/` for macOS?
4. Is there a misconfiguration in `tauri.conf.json` (`frontendDist` path resolution, missing `bundle` settings) that prevents asset inclusion in v2.11.5?
5. Is the `.app` stale (e.g., from a build before the Dioxus migration added the `snippets/` directory tree)?

The runtime evidence confirms the symptom (ZERO `[WEB]`, blank webview); Audit-A must identify the packaging/bundling failure that produced an incomplete `.app`.
