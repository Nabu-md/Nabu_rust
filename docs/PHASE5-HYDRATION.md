# Phase 5 — Frontend Hydration: Sub-Phase Execution Plan

This is the execution plan for **Priority 5** of `SHIP-ASAP.md` (the black-screen /
hydration blocker). It breaks the work into **parallel audits** (read-only) and a
**sequential remediation**, so multiple agents can investigate simultaneously without
stepping on each other.

## Workflow

```
        ┌──────────────────────────────┐
        │  5.1 AUDIT-A  (static/build) │──┐  parallel
        └──────────────────────────────┘  │
        ┌──────────────────────────────┐  │
        │  5.2 AUDIT-B  (runtime/log)  │──┘
        └──────────────────────────────┘
                        │  BOTH DONE → hand off findings
                        ▼
        ┌──────────────────────────────┐
        │  5.3 RECONCILE (gate)        │  sequential, small
        └──────────────────────────────┘
                        ▼
        ┌──────────────────────────────┐
        │  5.4 REMEDIATE (writes code) │  sequential
        └──────────────────────────────┘
                        ▼
              EXIT: rendered UI on screen
```

**Answer to the question directly:** launch **5.1 and 5.2 in parallel** (both are
read-only audits → no conflicts). When **both** have reported, run **5.3 (reconcile)**,
then **5.4 (remediate)**. Remediation is strictly sequential after the audits, because
it edits source and must act on confirmed findings, not guesses.

## Global constraints (applies to every sub-phase)

- **ONLY test with the production bundle** from `cargo tauri build`:
  `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app`.
- **NEVER overwrite the bundle with a `cargo build` binary** — that produces a DEV-MODE
  binary that loads `http://localhost:8080` (no server → WebKit `-1004` → black screen).
  This caused every false "regression." If you must rebuild, use `cargo tauri build`.
- **Audit agents are READ-ONLY.** They may read files, run the app, and write their
  findings file — they may NOT edit `src-tauri/` or `crates/nabu-ui/` source.
- Hand off by writing findings to `docs/phase5/audit-a.md` and `docs/phase5/audit-b.md`.
  Never destroy the other agent's file.

---

## 5.1 — AUDIT-A: Static build & asset-serve chain (read-only)

**Owner:** one agent. **Runs in parallel with 5.2.** **Do not edit source.**

Investigate whether the frontend assets are built and embedded so the page can load at all:

1. **Asset presence in `dist/`** — confirm `nabu_ui.js`, `nabu_ui_bg.wasm`, and
   `snippets/` (if present) exist and are non-empty. Note timestamps vs. last `dist` build.
2. **Asset embedding** — confirm the production bundle embeds `dist/`. Check the
   `frontendDist` value in `src-tauri/tauri.conf.json` and that the packaged app
   (`Nabu.app/Contents/Resources/`) actually contains `nabu_ui.js` + `nabu_ui_bg.wasm`.
3. **Wasm boot code** — read `crates/nabu-ui/src/lib.rs::start()` (`#[wasm_bindgen(start)]`):
   order of `remove_boot_splash()` vs `dioxus::web::launch::launch_cfg(App, ...)`, and whether
   anything there can fail silently before launch.
4. **Dioxus mount root** — read `dist/index.html` and confirm the dioxus root id matches what
   `launch_cfg` targets; confirm `nabu_ui.js` is loaded via `<script type="module">` + `init()`.
5. **Config** — check `withGlobalTauri` in `tauri.conf.json` (needed for `window.__TAURI__`).

**Deliverable:** `docs/phase5/audit-a.md` — a checklist of pass/fail + evidence + your
failure hypothesis (or "assets OK, no problem here"). If you find a definitive blocker
(e.g. wasm missing from bundle), call it out loudly as the root cause.

---

## 5.2 — AUDIT-B: Runtime boot, IPC, and Dioxus paint (read-only)

**Owner:** one agent. **Runs in parallel with 5.1.** **Do not edit source.**

Investigate at runtime whether the wasm executes, Dioxus mounts, and the app paints:

1. **Capture `[WEB]` console** — the production bundle already wires console + uncaught
   errors back to stderr. Run:
   `.../Nabu.app/Contents/MacOS/app > /tmp/nabu.log 2>&1`, wait ~20s, `grep '[WEB]' /tmp/nabu.log`.
   - If `[WEB]` shows a wasm fetch/instantiation error or a panic → that's the root cause.
   - If there are **zero** `[WEB]` lines → the JS module/wasm never executed (asset serving;
     hand to 5.4 with that signal, cross-reference 5.1).
2. **App component & IPC on mount** — read `crates/nabu-ui/src/components/app.rs` and
   `provide_theme`; trace any IPC (`settings_get`) called at mount. Verify it can't block or
   panic the root render. Confirm `window.__TAURI__.core.invoke` is reachable.
3. **Window/visibility** — confirm the production window composites on screen
   (`visible:true` already set) and there's no remaining `transparent:true` quirk.
4. **Distinguish cases** — "splash stays" (wasm never boots) vs "splash removed, black"
   (wasm boots, Dioxus doesn't paint). Report which one is actually happening.

**Deliverable:** `docs/phase5/audit-b.md` — the actual `[WEB]` log excerpt, pass/fail, and
your failure hypothesis. If you capture a panic/error, that IS the root cause.

---

## 5.3 — RECONCILE (gate)

**Owner:** one agent (or the orchestrator). **Sequential — after BOTH 5.1 and 5.2 land.**

- Read `audit-a.md` and `audit-b.md`.
- Cross-reference: does Audit-A's asset/build finding match Audit-B's runtime signal?
  (e.g. "wasm missing from bundle" (A) + "zero [WEB] lines" (B) = confirmed root cause.)
- Produce `docs/phase5/report.md`: a single root-cause statement + remediation order.
- If the two audits contradict, re-run only the disputed check (do NOT start 5.4 on a guess).

**Deliverable:** `docs/phase5/report.md` — confirmed root cause(s), in priority order.

---

## 5.4 — REMEDIATE (writes code)

**Owner:** one agent. **Sequential — only after 5.3 confirms the root cause.**

- Fix the confirmed root cause(s) from `report.md` (asset serving, wasm boot, IPC on mount,
  or dioxus mount root). Smallest change that makes the app paint.
- Rebuild ONLY via `cargo tauri build`. Do **not** reintroduce a dev-mode binary.
- Run the production bundle and verify per the exit criteria below.

**Deliverable:** one clean commit + a note in `report.md` of what changed and how it was verified.

---

## Exit criteria (DoD)

- Production bundle: `grep '[WEB]'` shows successful wasm boot (no panic, no 404).
- The Nabu UI (Inbox / Settings / etc.) is **visibly interactive**, not a black screen.
- A commit landed; `docs/phase5/report.md` records root cause + fix + verification.
