# Phase 5.1 — Audit-A

> READ-ONLY static / build / asset-serving investigation of the Priority 5
> frontend hydration / black-screen blocker.
> No source, config, binary, or bundle was modified.

## Executive result

**BLOCKER FOUND**

The production Tauri bundle at
`src-tauri/target/release/bundle/macos/Nabu.app/Contents/Resources/`
contains **no frontend assets at all** — only `icon.icns` is present.
The expected `index.html`, `nabu_ui.js`, `nabu_ui_bg.wasm`, `snippets/`, and
`generated/` are absent from the packaged `.app`. The `dist/` source tree is
fully and correctly populated, so this is a **packaging / embedding failure**,
not a build failure.

When Tauri launches the window it serves from an empty `Resources/` directory,
so the webview has no `index.html` to load → WebKit cannot boot the WASM → the
window stays black (or shows only a blank/transparent body).

---

## 1. `dist/` assets

`dist/` is located at repo root `/Users/macbook/github code/Nabu/dist`
(NOT `crates/nabu-ui/dist`). This matches `frontendDist: "../dist"` in
`tauri.conf.json` (relative to `src-tauri/`).

| asset | exists? | non-empty? | size | timestamp (2026) | result |
|---|---|---|---|---|---|
| `index.html` | yes | yes | 1,526 B | Aug 18 11:30:20 | OK |
| `nabu_ui.js` | yes | yes | 73,624 B | Aug 18 11:30:20 | OK |
| `nabu_ui_bg.wasm` | yes | yes | 6,436,561 B | Aug 18 11:30:20 | OK |
| `nabu_ui.d.ts` | yes | yes | 3,478 B | Aug 18 11:30:20 | OK (typings) |
| `nabu_ui_bg.wasm.d.ts` | yes | yes | 2,130 B | Aug 18 11:30:20 | OK (typings) |
| `snippets/` | yes | yes (3 subdirs) | — | Aug 17 00:09 | OK |
| `snippets/dioxus-cli-config-...` | yes | yes | — | Aug 17 00:09 | OK |
| `snippets/dioxus-interpreter-js-...` | yes | yes | — | Aug 17 00:09 | OK |
| `snippets/dioxus-web-...` | yes | yes | — | Aug 17 00:09 | OK |
| `generated/tailwind.css` | yes | yes | 88,592 B | Aug 18 11:30:20 | OK |

**Conclusion:** `dist/` is a valid, complete production build output. All
expected artifacts are present, non-empty, and timestamped together
(Aug 18 11:30), consistent with a single `build-dioxus.sh` run. The build
itself is NOT the problem.

---

## 2. Tauri packaging configuration

File: `src-tauri/tauri.conf.json`

| setting | value | evidence |
|---|---|---|
| `build.frontendDist` | `"../dist"` | line 16 |
| `build.beforeBuildCommand` | `src-tauri/scripts/build-dioxus.sh` (cwd `..`) | lines 11–14 |
| `build.devUrl` | `http://localhost:8080` | line 15 |
| `app.withGlobalTauri` | **`true`** | line 19 |
| `bundle.active` | `true` | line 34 |
| `bundle.resources` | `[]` | line 43 |
| `bundle.targets` | `"all"` | line 35 |

- `frontendDist: "../dist"` resolves to the repo-root `dist/` that is correctly
  populated (see §1). The configuration value itself is **correct**.
- `withGlobalTauri: true` — `window.__TAURI__` will be exposed by the Tauri
  runtime (good; the app relies on it). No change required.
- `bundle.resources: []` is normal for this project; frontend assets are pulled
  from `frontendDist`, not via explicit resource globs.

**Conclusion:** Tauri is *instructed* to package `../dist`, which is valid.
The configuration is not the defect — the packaging step did not actually copy
the `frontendDist` contents into the bundle.

---

## 3. Packaged production bundle

Inspected:
`src-tauri/target/release/bundle/macos/Nabu.app/Contents/Resources/`

Full tree (depth-limited, complete):

```
Nabu.app/Contents/Resources/
└── icon.icns     (1,556,143 B, Aug 3 18:32)
```

| expected asset | present in `.app/Contents/Resources/`? |
|---|---|
| `index.html` | **NO** |
| `nabu_ui.js` | **NO** |
| `nabu_ui_bg.wasm` | **NO** |
| `snippets/` | **NO** |
| `generated/` | **NO** |

- The binary `Nabu.app/Contents/MacOS/app` exists (31,154,204 B, built Aug 18 11:44).
- The bundle directory itself was stamped Aug 18 11:44, ~14 minutes **after**
  the `dist/` assets (Aug 18 11:30). So the bundle was built *after* a valid
  `dist/` existed, yet still shipped with an empty `Resources/`.
- `dist/` assets are present and correct; the packaged application does **NOT**
  contain them. This is the strongest available evidence for the black-screen
  root cause.

**ROOT CAUSE — DEFINITIVE (packaging/embedding failure):** The production
`.app` was built without copying the `frontendDist` contents into
`Contents/Resources/`. The webview is served from a directory that contains no
`index.html`, so the WASM frontend can never load → black screen.

---

## 4. WASM boot path

File: `crates/nabu-ui/src/lib.rs`

`#[wasm_bindgen(start)] pub fn start()` (lines 106–114):

```rust
console_error_panic_hook::set_once();
remove_boot_splash();
dioxus::web::launch::launch_cfg(
    components::app::App,
    dioxus::web::Config::default(),
);
```

Findings:

- Order: panic hook → `remove_boot_splash()` → `dioxus::web::launch::launch_cfg`.
- `remove_boot_splash()` (lines 119–127) removes the `#boot-splash` element by
  ID. **Important interaction:** the boot splash (dark `#030712` background +
  spinner) is the *only* thing that paints instantly. `remove_boot_splash()` is
  called **before** `launch_cfg`. If Dioxus fails to mount (or never runs
  because the WASM was never loaded into the webview), the splash is already
  gone and the window shows the body's `background-color: #030712` — i.e. a
  **black screen**. This is fully consistent with the missing-asset scenario.
- `launch_cfg` uses `Config::default()`, which mounts Dioxus to the document
  body (no custom root element id configured). The `index.html` body is empty
  (`<body>` contains only `#boot-splash` + the module script), so the default
  body mount is correct.
- Nothing before launch can "fail silently" except a JS/wasm load failure
  upstream — which is exactly what happens when `index.html`/`nabu_ui.js` are
  absent. `console_error_panic_hook::set_once()` would surface a Rust panic to
  the console, but with no module script loaded at all, `start()` never runs.
- The WASM startup code is structurally valid for a correctly-served bundle.

**Conclusion:** The boot sequence is sound *if the assets are present*. The
code's splash-removal-then-launch order means that absence of assets manifests
as a black window (no splash, no app).

---

## 5. Dioxus mount / `index.html`

File: `dist/index.html` (matches `build-dioxus.sh` generated template, lines 37–97)

1. **Dioxus mount/root element ID:** None explicitly. `launch_cfg` uses
   `Config::default()`, which mounts to `<body>`. The body has no id but is the
   default mount target — correct.
2. **Launch target correspondence:** Default body mount ↔ empty `<body>` in
   `index.html` → consistent.
3. **`nabu_ui.js` loaded?** Yes.
4. **Loaded as a module?** Yes — `<script type="module">` with
   `import init from "./nabu_ui.js"; init();`.
5. **WASM initialization:** The generated `nabu_ui.js` exports `init`, which
   loads `nabu_ui_bg.wasm` and runs the `#[wasm_bindgen(start)]` `start()`.
   Confirmed `index.html` calls `init()` directly; the JS references
   `getElementById`/`querySelector`/`body` (lines 174, 422, 943) exactly as the
   Rust boot path expects.
6. **Suspicious references:** None. No hardcoded `localhost` / `:8080` in
   `index.html` (the `devUrl` `localhost:8080` lives only in `tauri.conf.json`,
   used by `beforeDevCommand`, not in the production HTML). No dev URLs in the
   generated `index.html`.

**Conclusion:** The generated HTML/module bootstrap is correct and
production-appropriate. The missing piece is that this HTML is not actually
inside the shipped `.app`.

---

## 6. Root-cause hypothesis

**ROOT CAUSE — DEFINITIVE**

The production Tauri bundle was emitted without the `frontendDist` contents.
`Nabu.app/Contents/Resources/` contains only `icon.icns`; `index.html`,
`nabu_ui.js`, `nabu_ui_bg.wasm`, `snippets/`, and `generated/` are all
missing. The `dist/` source is fully populated and correct, and
`frontendDist: "../dist"` is correctly configured, so this is a
packaging/embedding defect (the frontend assets were not copied into the
bundle at `cargo tauri build` time), not a build or configuration error.

Consequence chain:
`Resources/` has no `index.html` → Tauri webview serves an empty/non-existent
document → the `<script type="module">` bootstrap never loads `nabu_ui.js` →
WASM `start()` never executes → `remove_boot_splash()` already removed the
splash during any prior paint path / or no paint occurs → window renders the
body background `#030712` → **black screen**.

This is **Case A — Assets/build/package failure** (sub-case: assets exist in
`dist/` but NOT in the packaged `.app`).

---

## 7. Handoff to Audit-B

Audit-B should answer the **runtime** question that remains after this asset
investigation:

> When the existing production binary
> `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app`
> is launched, what does the WebKit webview actually load, and what errors are
> emitted?

Specifically, Audit-B should:
1. Launch the existing `.app` (do NOT rebuild, do NOT replace the binary) and
   capture the webview console / network errors (expect: failed to load
   `index.html` / `file not found` for the frontend resources, WebKit
   `-1004`-class errors or `ERR_FILE_NOT_FOUND`).
2. Confirm the webview's effective document URL and whether any `index.html`
   is served from `Resources/`.
3. Verify whether `window.__TAURI__` is reachable (informational; isolated
   from the asset gap).

The asset/packaging defect is already established here as definitive; Audit-B's
runtime confirmation is corroborating evidence only and should not be used to
attempt a rebuild. If a rebuild is required to fully resolve, the remediation
agent must run `cargo tauri build` (NOT `cargo build`) to regenerate the bundle
with the `frontendDist` contents correctly embedded.
