# Investigation — Black screen on launch (Nabu, Tauri 2 + Dioxus 0.6.3 CSR)

Date: 2026-08-22 · Status: root cause NOT yet confirmed; evidence collected below.
Launch mode under investigation: **installed release app** (`/Applications/Nabu.app`,
built Aug 18 22:33 from `src-tauri/target/release/app`, built 22:29).

## Symptom

Window opens (dark `#0b1220` background) and shows nothing — no spinner, no
"Opening Nabu...", no UI. Persists across ~15 commits of attempted fixes.

## What the boot chain SHOULD be

1. Tauri creates window `main` (1280x800), loads embedded assets from
   `frontendDist = ../crates/nabu-ui/web/public` via `tauri://localhost`.
2. Embedded `index.html` (dx-bundle output) has `<div id="main">` + a script:
   `import("/./assets/nabu_ui-09dde5fcac476abd.js").then(init → wasm)`.
3. wasm `#[wasm_bindgen(start)] start()` (`crates/nabu-ui/src/lib.rs:106`)
   removes boot splash, calls `dioxus::web::launch::launch_cfg(App, ...)`.
4. `App` → providers → `AppRouter` renders a **Loading** state immediately
   ("Opening Nabu..." spinner, light-gray text on gray-950) and invokes IPC
   `check_vault_exists`.

## Evidence collected

### E1 — Installed binary embeds the correct frontend (verified)
- `/Applications/Nabu.app` mtime Aug 18 22:33; binary embeds asset references
  `/assets/nabu_ui-09dde5fcac476abd.js` and `/assets/nabu_ui_bg-2e4ee086c2acf4c8.wasm`
  — both files exist in `crates/nabu-ui/web/public/assets/`.
- The binary does **not** contain the repo-root index.html's "boot-splash"
  markup (only Rust-injected eval strings mentioning it) → embedded page is the
  dx-bundle one. Stale-asset hypothesis REFUTED for this build.

### E2 — dx-bundle output is internally consistent (verified)
- `web/public/index.html` preloads + imports exactly the hashed files present
  in `web/public/assets/`. Tailwind CSS (88 KB) exists at
  `web/public/generated/tailwind.css` and is linked.

### E3 — If Dioxus mounts AT ALL, something must be visible
`AppRouter`'s Loading state renders visible text ("Opening Nabu...") with
`text-gray-100` on `bg-gray-950`; Error state renders red text. A pure black
window means either:
- (a) the page/JS/wasm never executed, or
- (b) wasm started (splash removal is a no-op here — no splash in dx html)
  but the very first render produced nothing / panicked before paint.

Note: the Loading/Error screens use Tailwind classes; if the stylesheet failed,
text would still render in default color (black text on near-black bg could
*look* like a black screen). CSS presence verified on disk (E2), but not that
the webview actually loaded it.

### E4 — Runtime logs are nearly empty
`.nabu/logs/nabu.2026-08-19.log` (+18 log): only repeated
`Tracing initialized` lines per launch — nothing else. All boot diagnostics
(`[setup] ...`, `[WEBVIEW] ...`, `[DIAG] ...` from `lib.rs`) go to **stderr**,
which is invisible when launching from Finder. We have NEVER seen the frontend's
own diagnostics from an installed-app launch.

### E5 — Window config mismatch (minor)
`tauri.conf.json` sets `"visible": true`, but `lib.rs` setup comments say the
window starts hidden and is force-shown after 8 s + on page load. Not fatal
(there are two show paths) but indicates config drift.

### E6 — Capabilities file is EMPTY
`src-tauri/capabilities/default.json`: `"permissions": []`.
- App-defined commands (invoke) do NOT need ACL entries → `check_vault_exists`
  should work.
- BUT `window.__TAURI__.event.listen` requires `core:event:allow-listen`.
  With zero permissions, `EventServiceProvider`'s listener install will reject.
  Need to verify provider catches this; if it throws during mount it could
  abort first render → matches symptom (b).

### E7 — Three build pipelines coexist (context, not necessarily cause)
| Script | Tool | Output | Served by |
|---|---|---|---|
| `build-dioxus.sh` | cargo+wasm-bindgen manual | `dist/` | nothing currently |
| `dx-bundle.sh` | dx bundle | `crates/nabu-ui/web/public` | tauri.conf `frontendDist` ✔ |
| `scripts/build.sh` | trunk | — | BROKEN: `Trunk.toml` missing |
| root `dioxus.toml` | dx serve (dev) | `dist/` | devUrl :8080 |

Dev-mode trap (separate bug): `run-dioxus.sh` never `cd`s to `crates/nabu-ui`,
so `cargo tauri dev` serves the ROOT dioxus project whose `index.html` has NO
wasm bootstrap script → guaranteed black/splash-only screen in dev mode.
Release mode is unaffected by this.

### E8 — Frontend IPC layer is defensive (read)
`ipc.rs` never panics on invoke failure; failures become `Err(IpcError)` →
AppRouter shows Error text. So backend IPC rejection alone cannot produce a
blank window.

## Hypotheses (ranked)

H1 (HIGH): **JS module load failure in WKWebView under `tauri://` scheme** —
the dx-bundle bootstrap uses dynamic `import()` of an ES module. If the module
or wasm fetch fails (scheme/MIME/path issue), nothing ever executes → pure
black window (matches E3a). Untested.

H2 (MEDIUM): **First-render panic or unhandled rejection during mount** — e.g.
`EventServiceProvider` calling `__TAURI__.event.listen` with empty capabilities
(E6) rejecting uncaught, aborting render → splash-less black window (E3b).

H3 (LOW-MED): **CSS-only illusion** — everything mounts but stylesheet missing
in webview so text is invisible (black-on-dark). Refuted on disk (E2) but not
refuted inside the running webview.

H4 (LOW): stale/mismatched installed app vs current source. Refuted for the
Aug 18 build (E1) but user may be testing later source changes without
rebuilding/reinstalling — rebuild+reinstall step must be part of any test.

## Discriminating experiments (proposed, none run yet)

X1. Run the installed binary from a terminal to capture stderr:
    `/Applications/Nabu.app/Contents/MacOS/app 2>&1 | tee /tmp/nabu-boot.log`
    Look for `[WEBVIEW] label=main finished=...`, `[HOOK]`, `[DIAG] FE ...`
    (frontend posts DOM state via `diag_report`). One run answers H1 vs H2:
    - `finished=true` + `[DIAG] FE main_len>0` → mounted (H3/H2 territory)
    - `finished=true` + no `[DIAG]` → JS never ran (H1)
    - no `[WEBVIEW] finished=true` at all → page load itself failed (H1 variant)

X2. Add `core:default` (at minimum `core:event:default`) to
    `capabilities/default.json` and retest — directly tests H2/E6.

X3. In the console-hook eval (lib.rs), also report
    `document.getElementById('main')?.childElementCount` and whether the
    imported module resolved — distinguishes "script ran" from "module failed".

X4. Temporarily inline a visible fallback ("If you can read this, HTML loaded")
    into the body of `index.html` template input — separates HTML-load failure
    from wasm failure without needing console access.

## Open questions for the user

1. Confirm the exact launch artifact tested each time (installed .app? rebuilt
   first?). H4 depends on it.
2. Was there EVER a version where UI showed? (bisect anchor)
