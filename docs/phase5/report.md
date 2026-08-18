# Phase 5 — Frontend Hydration Root-Cause Report

## Status

CONFIRMED ROOT CAUSE

---

## 1. Confirmed root cause

The production Tauri bundle
(`src-tauri/target/release/bundle/macos/Nabu.app`) was emitted without the
`frontendDist` contents. `Nabu.app/Contents/Resources/` contains only
`icon.icns`; `index.html`, `nabu_ui.js`, `nabu_ui_bg.wasm`, `snippets/`, and
`generated/tailwind.css` are all absent. The repo-root `dist/` directory is
fully and correctly populated and `tauri.conf.json` `build.frontendDist` is
correctly set to `"../dist"`, which resolves to that populated `dist/`.
Consequently the Tauri webview has no `index.html` to load at startup: the
`<script type="module">` bootstrap never runs, the wasm-bindgen `init()` never
executes, the `#[wasm_bindgen(start)] start()` (Dioxus `launch_cfg`) never
runs, no first paint occurs, and the user sees a black/blank window. The
backend services all initialize normally (ApplicationContext ready, 16
services running), but the webview console hook installed in `on_page_load`
never fires, producing zero `[WEB]` output.

---

## 2. Evidence

### From Audit-A (static / build / packaging)

- `dist/` is located at repo root `/Users/macbook/github code/Nabu/dist` and
  is the target of `frontendDist: "../dist"` in `tauri.conf.json` (line 16).
- `dist/` is fully populated and non-empty (timestamps Aug 17–18 11:30):
  `index.html` (1,526 B), `nabu_ui.js` (73,624 B), `nabu_ui_bg.wasm`
  (6,436,561 B), `snippets/` (3 subdirs), `generated/tailwind.css`
  (88,592 B). The build output itself is correct.
- Inspected packaged bundle
  `src-tauri/target/release/bundle/macos/Nabu.app/Contents/Resources/`:
  contains only `icon.icns` (1,556,143 B, stamped Aug 3 — stale). All five
  expected frontend assets are **absent**.
- The `app` binary at
  `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app`
  exists (31,154,204 B, stamped Aug 18 11:44) and the bundle directory itself
  is stamped Aug 18 11:44 — ~14 min **after** `dist/` (Aug 18 11:30). This
  indicates the bundle was constructed after a valid `dist/` existed, yet the
  `frontendDist` contents were still not copied into `Resources/`.
- `tauri.conf.json` config is correct: `frontendDist: "../dist"`,
  `withGlobalTauri: true`, `bundle.active: true`, `bundle.resources: []`
  (normal; frontend is pulled from `frontendDist`, not explicit globs).
- The `beforeBuildCommand` (`src-tauri/scripts/build-dioxus.sh`) is sound:
  it builds Tailwind, compiles `nabu-ui` to `wasm32-unknown-unknown`, runs
  `wasm-bindgen --out-dir ../dist --target web`, copies `tailwind.css`, and
  writes a correct `index.html` that loads `./nabu_ui.js` as an ES module.
- Boot sequence (`crates/nabu-ui/src/lib.rs` `start()`):
  `panic hook → remove_boot_splash() → launch_cfg(body mount)`. The
  splash-removal-then-launch order means asset absence manifests as a black
  window (splash already gone, no app to mount).
- The generated `dist/index.html` is correct and production-appropriate:
  `<script type="module">` importing `./nabu_ui.js`, no `localhost:8080`
  dev-URL leakage. The HTML itself is valid; it is simply **not inside the
  shipped `.app`**.

### From Audit-B (runtime)

- Launched the existing `.app` binary directly with stdout/stderr redirected
  to `/tmp/nabu.log`; allowed 20 s to initialize.
- The backend fully initialized successfully: Tracing, PluginManager,
  VaultGraph, watcher, EventBus, and all 16 lifecycle services (StorageManager,
  ConversationStore, WorkerPool, Indexer, VaultGraph, PluginManager) started.
  `ApplicationContext ready — startup complete stage=Running` logged at
  12:08:56.
- `/tmp/nabu.log` contains 53 lines and **zero** lines matching `[WEB]`.
  The webview console hook installed in `on_page_load` (lib.rs) never ran —
  i.e. the frontend JavaScript/WASM path was never reached.
- No evidence of WASM execution: no fetch, instantiation, or execution of
  `nabu_ui_bg.wasm` observed. The `on_page_load` hook that installs the
  `[WEB]` console bridge never fired, so nothing downstream of the webview
  load could execute.
- No evidence Dioxus mounted: `AppRouter` calls
  `tauri_invoke_safe("check_vault_exists", ...)` in a `use_effect` spawned via
  `spawn_local`, but this code cannot run until `nabu_ui.js → init() →
  start() → launch_cfg` runs, which requires a loadable `index.html`.
- Window configuration is healthy: `tauri.conf.json` sets `visible: true`,
  `transparent: false`, `backgroundColor: "#0b1220"`; `lib.rs` shows the
  window is shown on `PageLoadEvent::Finished` with an 8 s safety-net
  `show()` fallback. The native window is capable of displaying content.
- Observed visual state: splash remains / webview is blank, consistent with
  the frontend bundle never loading.

---

## 3. Contributing causes

- **(Secondary) Stale `icon.icns`.** The lone packaged resource carries an
  Aug 3 timestamp, suggesting the bundle's `Resources/` staging area was not
  fully refreshed. This is a symptom of the embedding failure, not an
  independent cause, but it indicates the bundler did not stage a fresh
  resource set for this `.app`.
- **(Secondary) Ambiguous build provenance.** Audit-B raises the legitimate
  question of whether the `app` binary was produced by `cargo tauri build`
  (which runs `beforeBuildCommand` then stages `frontendDist`) versus a plain
  `cargo build` with the binary copied into a hand-made `.app` skeleton. The
  Aug 18 11:30 → 11:44 timestamp gap and the populated `dist/` are consistent
  with a real `cargo tauri build`, but the absence of a captured
  `beforeBuildCommand` exit log and the absence of any Tauri bundler staging
  output in `target/release/build/` build-* dirs (no per-build `output`/
  `stderr` files under the `tauri-*` fingerprint dir) means the exact
  bundling invocation cannot be positively confirmed from artifacts alone.
  This is the one piece of process-level uncertainty that the evidence
  cannot fully close — but it does **not** change the conclusion, because
  regardless of which command produced the binary, the fix is to rebuild via
  `cargo tauri build` and confirm the assets land in `Resources/`.
- **(Secondary) Boot-splash ordering.** The `remove_boot_splash()` call
  preceding `launch_cfg` means that if assets fail to load, the user sees a
  black body (`#030712`) rather than a stuck spinner. This is a UX-masking
  factor that makes the missing-asset failure look like a total blank rather
  than a stalled load; it is not the root defect but it obscures diagnosis.

---

## 4. Ruled-out causes

- **Dioxus mount root / `index.html` structure.** `dist/index.html` uses the
  default `<body>` mount target, `launch_cfg` uses `Config::default()`
  (body mount), and the body contains no conflicting id. The HTML, module
  script, and WASM init path are all correct and production-appropriate. The
  mount root is sound *when the assets are present*.
- **`withGlobalTauri` / IPC wiring.** `tauri.conf.json` sets
  `app.withGlobalTauri: true` (correct for this app). `lib.rs` registers
  `window.__TAURI__`-based IPC and the `[WEB]` console hook. No IPC
  misconfiguration is implicated; the IPC path (`check_vault_exists` at
  `commands.rs:476`, `tauri_invoke_safe` in `ipc.rs:9-10`) is never reached
  only because the frontend never loads, not because the wiring is broken.
- **WASM bootstrap code.** The `#[wasm_bindgen(start)] start()` sequence
  (panic hook → splash removal → `launch_cfg`) is structurally valid. There
  is no Rust-side startup code defect; `console_error_panic_hook::set_once()`
  would surface a panic, but with no module script loaded at all, `start()`
  never runs — so no panic/log can appear.
- **Dev-URL / `localhost:8080` leakage in production HTML.** Confirmed
  absent: `devUrl` lives only in `tauri.conf.json` (used by
  `beforeDevCommand`), never in the generated `index.html`. Production HTML
  references only `./nabu_ui.js` and `generated/tailwind.css` (relative).
- **Window/compositing settings.** `transparent: false`, `visible: true`, no
  special layering — the native window renders normally; the blankness is
  purely the empty webview document, not a compositing or transparency bug.
- **`bundle.resources: []`.** For this project this is normal and does not
  exclude `frontendDist` contents; Tauri stages `frontendDist` into `Resources/`
  independently of `bundle.resources`, so the empty array is not the defect.
- **Stale `dist/` vs stale `.app`.** `dist/` is fresh (Aug 18 11:30) and
  complete; only the `.app`'s `Resources/` is stale/stripped. The mismatch is
  the defect, not a stale `dist/`.

---

## 5. Audit reconciliation

Audit-A and Audit-B **agree** — they are mutually reinforcing, not
contradictory.

- Audit-A establishes the **static fact**: `dist/` is complete and correctly
  configured, yet the packaged `.app/Contents/Resources/` is empty (only
  `icon.icns`).
- Audit-B establishes the **runtime consequence of that fact**: the backend
  boots cleanly (all 16 services, ApplicationContext ready) but the webview
  produces zero `[WEB]` output because `on_page_load` never fires — i.e. the
  frontend bundle never loads. A healthy, full backend with a dead webview is
  exactly the signature of a missing `index.html`.

The one process-level question Audit-B raises — "was this produced by
`cargo tauri build` or by `cargo build` with a hand-copied binary?" — is
investigated by filesystem stamps: `dist/` is Aug 18 11:30, the `.app`
binary + bundle is Aug 18 11:44, and the `tauri-build` fingerprint dir
`tauri-fdf1e8da9f01fd68` also carries Aug 18 11:30, consistent with a single
`cargo tauri build` invocation that ran `beforeBuildCommand` (regenerating
`dist` at 11:30) and then bundled (stamping the app at 11:44). The absence of
a captured bundler staging log does not overturn this; the decisive physical
fact — `dist/` present and correct, `Resources/` empty — is independent of
how the binary was produced. **No contradiction remains that affects the
root cause.** The evidence from both audits converges on the same defect:
the `frontendDist` contents are not embedded in the shipped `.app`.

---

## 6. Remediation order

1. **Rebuild the production bundle** from `src-tauri/` with:
   ```
   cargo tauri build
   ```
   This runs `beforeBuildCommand` (`src-tauri/scripts/build-dioxus.sh`,
   regenerating `../dist`) and then the Tauri bundler, which must stage
   `frontendDist` (`../dist`) into
   `Nabu.app/Contents/Resources/`.
2. **Verify asset embedding** — confirm
   `src-tauri/target/release/bundle/macos/Nabu.app/Contents/Resources/`
   now contains `index.html`, `nabu_ui.js`, `nabu_ui_bg.wasm`,
   `snapshots/`, and `generated/tailwind.css`.
3. **Run the production executable** — launch
   `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app`
   (do NOT copy a dev binary or `cargo build` artifact into the bundle).
4. **Verify frontend bootstrap** — confirm `[WEB]` output appears in the
   app log (the `on_page_load` console bridge now fires), confirming the
   webview loaded `index.html` and the wasm `init()` ran.
5. **Verify visible UI** — confirm the webview renders interactive Dioxus
   content (first paint) rather than remaining on the boot splash /
   black screen.

---

## 7. 5.4 handoff

**What must be changed**

- Regenerate the `.app` bundle via `cargo tauri build` so the Tauri bundler
  stages the **`frontendDist` contents** (`../dist`) into
  `Nabu.app/Contents/Resources/`. No source code, `tauri.conf.json`, or
  build configuration needs to change — `frontendDist: "../dist"` is already
  correct and `dist/` is already correctly populated by
  `build-dioxus.sh`. The defect is purely that the bundle was not rebuilt /
  not re-staged after `dist/` was produced.
- After a successful `cargo tauri build`, verify
  `Contents/Resources/` contains `index.html`, `nabu_ui.js`,
  `nabu_ui_bg.wasm`, `snapshots/`, and `generated/tailwind.css`.

**What must NOT be changed**

- Do **not** edit `src-tauri/tauri.conf.json` `build.frontendDist` — it is
  correct (`"../dist"`).
- Do **not** edit `src-tauri/scripts/build-dioxus.sh` — it correctly emits a
  complete `dist/`.
- Do **not** edit `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`,
  `src-tauri/src/diagnostics.rs`, or any window/console-hook wiring — the
  runtime bootstrap and `[WEB]` hook are sound; they simply never ran because
  the HTML was missing.
- Do **not** edit any source under `crates/nabu-ui/src/` — the WASM boot
  sequence (`lib.rs` `start()`, `launch_cfg`, `remove_boot_splash`) is
  structurally valid; it works once the assets are present.
- Do **not** touch `dist/` — it is the correct input; the fix is to make the
  bundler consume it, not to regenerate it by hand.

**What production command must be used**

- MUST: `cargo tauri build`
- NEVER: `cargo build` (produces a raw binary with no frontend staging)
- NEVER: copy a `cargo build` / dev binary into `Nabu.app/Contents/MacOS/`
  (produces an app with no embedded frontend)
- NEVER: install a hand-built `dist` into a pre-existing stale `.app`

**Runtime verification that must pass**

- `src-tauri/target/release/bundle/macos/Nabu.app/Contents/Resources/`
  contains `index.html`, `nabu_ui.js`, `nabu_ui_bg.wasm`, `snapshots/`,
  `generated/tailwind.css` (non-empty).
- Launching
  `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app`
  emits `[WEB]` output in the app log (the `on_page_load` console bridge
  fires), confirming the webview loaded the frontend.
- The application renders visible, interactive Dioxus UI (first paint)
  instead of remaining on the boot splash / black screen.
- The webview's effective document URL resolves to the embedded
  `index.html` (not a `file:///` error / missing-resource state).
