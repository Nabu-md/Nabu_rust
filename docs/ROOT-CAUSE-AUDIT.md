# Nabu Root-Cause Audit Kit (Tauri 2 + Dioxus 0.6 CSR, macOS)

Use this to **audit, deduce, and REFUTE** the root cause of the two blocking UI
defects. This is an evidence-driven investigation, not a fix list. For every
hypothesis, you must state what evidence would *refute* it and run the experiment
that produces that evidence. Do not modify app code until a hypothesis survives
falsification.

Two *independent* symptoms are in play. Do not conflate them.

---

## Symptom A — "Opening Nabu" spinner never resolves
The app renders a loading screen (`VaultCheckState::Loading` in
`crates/nabu-ui/src/components/app.rs:119-126`) that shows a spinner + "Opening
Nabu..." and **never transitions** to the wizard or workspace. It waits on
`check_vault_exists` IPC (app.rs:79).

## Symptom B — rendered content is a fixed ~1280×800 box that doesn't resize
Whatever renders (loading screen, wizard) appears as a fixed-size dark box. When
the window is enlarged, macOS **native gray** shows around it. The content does
not track the window. The box looks "hardcoded."

**These may have different root causes. Investigate separately.**

---

## Confirmed facts (measured this session — trust these)

1. **App boots fully; main thread is NOT blocked.** Log (captured from a
   terminal launch) shows: `[setup] ApplicationContext ready (async build)`,
   16 lifecycle services start, `VaultGraph loaded from disk`, and
   `[WEBVIEW] label=main finished=true` (page finished loading).
2. **Window config** (`src-tauri/tauri.conf.json`): `width:1280 height:800
   resizable:true transparent:false backgroundColor:#0b1220`. Window is built
   from config only — no manual `WebviewWindowBuilder` in code.
3. **CSS is correct and shipped.** `src/styles/app.css:162` sets
   `html,body{width:100vw;height:100vh}`; `app.css:238` sets
   `.app{width:100vw;height:100vh;overflow:hidden;border-radius:10px;
   background-color:rgb(var(--gray-950))}`. Both confirmed present in the
   SHIPPED `dist/generated/tailwind.css`. So a correct viewport → full-fill.
4. **Dioxus CSR** mounts into `<div id="main">` (index.html template in
   `src-tauri/scripts/build-dioxus.sh`). CSR builds DOM from WASM; it does
   **NOT** hydrate server HTML. The loading screen root uses `h-screen w-screen`
   + `bg-gray-950` (fills viewport).
5. **`--gray-950` palette** is backed by CSS vars defined in `src/styles/app.css`
   `:root`. Verify they resolve (if `rgb(var(--gray-950))` is invalid → the
   `.app` background is transparent and you'd see the native window bg, not
   black). The black you see may come from index.html body inline
   `background-color:#030712` instead.
6. **A JS console hook + poller is installed** in `on_page_load`
   (`src-tauri/src/lib.rs`, `[WEB]` lines) that mirrors console logs and a
   `reportSize()` probe (`innerWidth x innerHeight` + clientWidth/clientHeight)
   to stderr. **In the latest run it produced ZERO `[WEB]` lines** despite
   `finished=true`. This is a red flag: either the eval hook didn't install,
   the poller's `eval_with_callback` isn't returning data, or the JS/wasm never
   actually ran. INVESTIGATE FIRST.
7. A Rust-side `on_window_event` handler logs `[RESIZE] label=... phys=WxH
   scale=...` on every window resize. This measures the WINDOW (not the webview).

---

## How to capture the live log (terminal, not `open`)

```bash
pkill -f "Nabu.app/Contents/MacOS/app"
nohup "/Applications/Nabu.app/Contents/MacOS/app" > /tmp/nabu.log 2>&1 &
# have a human drag the window edge larger/smaller a few times
grep -E "\[RESIZE\]|\[WEBVIEW\]|\[WEB\]|\[setup\]" /tmp/nabu.log
```

CRITICAL BUILD RULE: production UI is built with `cargo tauri build` in
`src-tauri/`. NEVER test with `cargo build` (that binary is dev-mode and hits
`localhost:8080` → white/black screen). After build, install:
`rm -rf /Applications/Nabu.app && cp -R src-tauri/target/release/bundle/macos/Nabu.app /Applications/Nabu.app`.
A stale instance keeps the old binary mapped — always `pkill` first.

---

## Hypotheses (each with its FALSIFICATION experiment)

### A. Window resize
**H1 — The NSWindow is not resizable / does not resize when dragged.**
- Refute: run the capture, have a human drag an edge. If `[RESIZE]` lines appear
  with changing W×H, the window resizes → H1 refuted.
- Corroborate: no `[RESIZE]` on drag, or window snaps back.

### B. Webview viewport vs window
**H2 — The window resizes but the WKWebView viewport stays at the initial
1280×800**, so gray window background shows around a fixed webview.
- Refute: `[RESIZE]` grows, AND the `[WEB] size=` probe also grows. If both grow,
  the webview tracks the window → H2 refuted (webview fine).
- Corroborate: `[RESIZE]` grows but `[WEB] size=` stays 1280×800.
- Note: if the `[WEB]` probe is dead (fact 6), you cannot measure the CSS
  viewport. FIX THE PROBE FIRST (see D).

**H3 — Webview viewport tracks the window, but Dioxus/.app CSS does not
relayout** (e.g. undefined `--gray-*` var, stale `100vh`).
- Refute: `[WEB] size=` grows AND the box visibly grows → refuted.
- Corroborate: `[WEB] size=` grows but the box does not.

### C. JS / WASM not actually running
**H4 — The wasm/Dioxus is not executing in the shipped build**, so the "Opening
Nabu" you see is a partial/stale render and the JS console hook (fact 6) never
runs.
- Refute: `[WEB]` lines appear (hook pushed `boot: console hook installed` and a
  `size:` entry). If they appear, JS runs → H4 refuted.
- Corroborate: zero `[WEB]` despite `finished=true`. Also check for a wasm
  boot error / panic in the log, and whether the spinner actually animates.
- If the boot-splash (`#boot-splash`, `position:fixed; inset:0` in index.html)
  is visible instead of the loading screen, then `remove_boot_splash()`
  (`crates/nabu-ui/src/lib.rs:119`) did not run → Dioxus start() did not run.

### D. The instrumentation itself
**H5 — The eval/eval_with_callback diagnostics are broken**, not the app.
- Test in isolation: after `finished=true`, from Rust call
  `window.eval_with_callback("1+1", |r| eprintln!("[PROBE] {}", r))`. If you get
  `[PROBE] 2`, eval works → the zero-`[WEB]` is an app-side JS problem (C).
  If you get nothing/error, the webview JS bridge itself is the problem → strong
  evidence for a broken/stalled webview.

### E. Loading-state IPC
**H6 — `check_vault_exists` IPC resolves but the frontend doesn't transition**
(loss of signal write), OR **the IPC never resolves**.
- Refute: add a `console.log` / `[WEB]` after each branch of the
  `tauri_invoke_safe("check_vault_exists")` await (app.rs:79-113). If you see
  the result logged, the IPC resolves → refute "never resolves."
- Corroborate: no log at all → IPC awaits forever.

---

## Prior conclusions already REFUTED (do not re-litigate)

- **Hydration mismatch (React-SSR style).** N/A — Dioxus 0.6 CSR does not
  hydrate; there is no pre-rendered HTML to reconcile. The pasted advice
  (`dangerous_inner_html`, dioxus-markdown, SSR normalization) does not apply.
- **Main-thread `block_on` deadlock in `.setup()`.** WAS the cause of a separate
  freeze; already fixed by spawning the context build
  (`tauri::async_runtime::spawn` in `src-tauri/src/lib.rs`). Symptom A/B
  persist after this fix, so this is NOT the root cause of the fixed box.
- **Reference project `/Users/macbook/github code/Nabu reference/spacedrive-main`**
  (Tauri 1 + React/TS). Different webview pipeline and no Dioxus; treat as NOT
  useful unless a concrete mechanism is shown to transfer.
- **Hydration of markdown rendering.** The stuck screen contains no markdown.

---

## Key files / line refs

- `src-tauri/tauri.conf.json` — window (1280×800, resizable).
- `src-tauri/src/lib.rs` — `setup()` (async context build), `on_page_load`
  (`[WEBVIEW]`/`[WEB]`), `on_window_event` (`[RESIZE]`), `run()`.
- `src-tauri/src/commands.rs` — `check_vault_exists` (:476), `check_vault_exists_impl`
  (:480), `select_vault_dialog`/`create_vault_dialog` (:504+), `complete_setup`.
- `src-tauri/scripts/build-dioxus.sh` — index.html template (`#main`, boot-splash).
- `crates/nabu-ui/src/components/app.rs` — `App` (:30), `AppRouter` (:71),
  Loading state (:119), `ViewContent` (:156).
- `crates/nabu-ui/src/lib.rs` — wasm `start()` (:106), `remove_boot_splash` (:119).
- `crates/nabu-ui/src/components/vault_setup_wizard.rs` — wizard (:24).
- `src/styles/app.css` — `:root` vars (:24), `html,body` (:162), `.app` (:238).
- `dist/generated/tailwind.css` — shipped CSS (verify `.app` + `100vh` present).

## Deliverable expected from the audit
A short REFUTATION TABLE: for each hypothesis H1–H6, the experiment run, the
observed evidence, and whether it is REFUTED or SURVIVES. Only survivors are
root-cause candidates. End with a single most-probable root cause per symptom
(A and B) plus the minimal experiment to confirm it.