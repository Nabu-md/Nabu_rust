

---

## Audit A — Refutation Table

### Environment
- **Tauri**: 2.11.5, **wasm-bindgen**: 0.2.100, **Dioxus**: 0.6.3 CSR, target: `wasm32-unknown-unknown`
- Production build (`cargo tauri build --release`)
- Vault: `/Users/macbook/Desktop/nabu testing` (has `.nabu/` dir, configured)

---

### H4 — wasm/Dioxus NOT running (app stuck before frontend boots)

| Evidence | Timeline |
|---|---|
| `[setup] ApplicationContext ready` | 00:32:24.855 — backend fully booted |
| `[WEBVIEW] finished=true` | HTML loaded |
| `[SPLASH] "SPLASH_PRESENT"` | 200ms — splash present (wasm NOT yet booted) |
| `[TAURI] "TAURI_CORE_AVAILABLE"` | 500ms — `__TAURI__.core` IS available |
| **`[IPC] check_vault_exists invoked`** | Backend RECEIVED an IPC call from the frontend |
| `[IPC] check_vault_exists returning Ok(Some(path))` | Backend RETURNED `Ok(Some("/Users/macbook/Desktop/nabu testing"))` |

**Verdict: REFUTED.** The frontend (wasm) definitely booted — `check_vault_exists` can only be called via `window.__TAURI__.core.invoke()` from `app.rs:79`, which requires wasm to have run `start()`, Dioxus to have mounted, and `use_effect` to have fired.

---

### H5 — `eval_with_callback` / diagnostic bridge broken

| Evidence | Detail |
|---|---|
| `[PROBE] eval_with_callback returned: 2` | `eval_with_callback("1+1")` WORKS at 100ms (returns correct result) |
| `[HOOKSTATE] "HOOKED"` | Console hook IS installed (300ms) |
| `[LOGSCONTENT] "[\"size:...\",\"boot:..."]` | `__nabuLogs` IS populated (300ms) |
| **68+ `[POLLER] eval_with_callback returned: Ok(())`** | Poller calls return `Ok(())` for 100+ seconds |
| **0 `[WEBPOLL]` callbacks fired** | Zero callbacks were invoked |
| **0 `[SPLASH2]`, `[LOGS2]`, `[LOGS3]`, `[LOGS4]`** | 4 one-shot probes at 1000ms, 2500ms, 5000ms — ALL callbacks failed to fire |

**Verdict: SURVIVES (modified).** `eval_with_callback` is NOT fundamentally broken — it works before wasm boots. But **callbacks cease to be invoked after the wasm module boots**. The Tauri `eval_with_callback()` call returns `Ok(())` (accepted by backend), but the JavaScript callback is never invoked. This is the diagnostic blind spot: the bridge works early, then breaks silently when wasm takes over.

---

### H6 — `check_vault_exists` IPC never resolves or signal write/transition lost

| Evidence | Detail |
|---|---|
| `[IPC] check_vault_exists invoked` | Backend received the invoke call |
| `[IPC] check_vault_exists returning Ok(Some(path))` | Backend processed and returned response |
| **0 `[IPC-FE]` markers** | Frontend `console.log("[IPC-FE] ...")` markers never captured |
| **0 `[WEB]` entries after 500ms** | Poller can't retrieve post-wasm logs (same underlying issue) |
| `[TAURI] "TAURI_CORE_AVAILABLE"` | `__TAURI__.core` IS available |

**Verdict: SURVIVES.** The backend DID process the IPC command and sent a response. But the frontend never visibly transitioned (spinner stuck on `Loading`). Either:
- **(a)** The IPC response Promise never resolved on the frontend (Tauri webview message delivery broken post-wasm — same mechanism as H5), OR
- **(b)** The frontend received the response and set `vault_state`, but the transition didn't fire

Given H5's finding that `eval_with_callback` callbacks stop firing after wasm boots, **(a) is the most likely**: the IPC Promise resolution mechanism is broken by the same root cause affecting `eval_with_callback`.

---

## Root Cause

**Tauri 2.11.5's `Webview::eval_with_callback` callback invocation mechanism breaks after the wasm module boots.** After `start()` runs (which calls `dioxus::web::launch::launch_cfg`), the webview's callback dispatch bridge stops invoking JavaScript callbacks. The `eval_with_callback()` call itself returns `Ok(())` (accepted by the Rust side), but the JavaScript callback is never invoked.

This same underlying webview message-passing failure likely affects the IPC Promise resolution mechanism: the backend sends the `check_vault_exists` response, but the webview's message handling fails to deliver it to the frontend's Promise. The `tauri_invoke_safe("check_vault_exists")` Promise never resolves, so the `spawn_local` async block in `app.rs:79` hangs indefinitely at the `.await` point. `vault_state.set(...)` is never called, and the app remains stuck in `VaultCheckState::Loading` with the spinner.

### Why this is hard to diagnose
The console hook + poller approach works perfectly **before** wasm boots (100-500ms), but becomes useless **after** wasm boots (1000ms+), because the callback mechanism that the poller relies on is the same one that breaks. This creates a diagnostic blind spot right when we need to see the frontend's state most.

### Confirming experiment (needed)
Replace `eval_with_callback` with **synchronous `window.eval()`** for post-wasm probes (1000ms+). Synchronous `eval` returns the result directly without relying on the callback bridge. This would allow checking `__nabuLogs` content to see if `[IPC-FE]` markers exist (answering whether H6-a or H6-b is correct), and inspecting DOM state to confirm whether Dioxus rendered or stalled.

The fix direction depends on which path H6 takes:
- If `__nabuLogs` contains `[IPC-FE]` markers → IPC resolved but signal transition broke (investigate Dioxus `use_effect` / `Signal` update issue)
- If `__nabuLogs` is clean after IPC → IPC Promise hung (investigate Tauri/wasm-bindgen bridge issue; consider upgrading Tauri or patching the IPC path)
- Audit B — Findings Summary

**Symptom:** Content renders as a fixed dark box at the window's initial size; native gray shows around it when the window enlarges.

### What the evidence proves

1. **CSS is correct and shipped.** `dist/generated/tailwind.css` contains `html,body{width:100vw;height:100vh;overflow:hidden}` and `.app{width:100vw;height:100vh;overflow:hidden;border-radius:10px;background-color:rgb(var(--gray-950))}`. `--gray-950: 3 7 18` is a valid triplet, so `.app` paints its own solid dark bg — the box is the webview surface, not a CSS sub-rect. (Fact-3 "invalid var" path REFUTED.)
2. **Dioxus mounts into `#main`; `.app` uses viewport units (`100vw/100vh`)**, so it fills the CSS viewport regardless of `#main`'s size.
3. **The webview viewport measurement at boot was `1280×772`** (772 = 800 minus macOS title-bar chrome) — exactly the window's logical size. So at boot the webview *does* match the window.
4. **The JS→Rust eval bridge works** (`[PROBE] returned: 2`, `[HOOKSTATE] HOOKED`, `[LOGSCONTENT]` showed the live `size: 1280x772`). The earlier "zero `[WEB]`" was a **stale binary** — the running app predated the current `src-tauri/src/lib.rs` instrumentation. H5 (broken bridge) is REFUTED.
5. **Resizing the window from Rust (`set_size(1700,1100)`) produced a storm of `[RESIZE]` events but the physical width stayed pinned at `2560` (= 1280 logical × scale 2).** The requested 1700-logical width was clamped back to ~1280. The height oscillated (the window "fought" the resize and settled near the original), while width never exceeded 1280.

### Refutation table

| H | Hypothesis | Result |
|---|---|---|
| H1 | Window not resizable | **REFUTED** — `[RESIZE]` fires on drag/programmatic resize; config `resizable:true` |
| H2 | Webview frame pinned to initial 1280×800 | **SURVIVES** — width clamped to 1280; gray gap on enlarge = webview not tracking |
| H3 | Viewport tracks, CSS fails relayout | **REFUTED** — valid var + `100vw/100vh` shipped; webview measured at exact window size |
| H5 | Eval bridge broken | **REFUTED** — bridge works; silence was a stale binary |

### Most-probable root cause

**H2: the WKWebView (and therefore the window) is being held at a fixed ~1280×800, not tracking/enlarging with the user's drag.** The decisive signal is that a programmatic `set_size(1700,1100)` was clamped to 1280 wide — something is constraining the window's size (most likely a Tauri/macOS minimum-content-size or an intrinsic-content-size tie between the webview and the window), so enlarging reveals native gray around the fixed webview box.

### What "truly dynamic" requires (the fix direction)

- The window must be allowed to grow beyond 1280×800 and the WKWebView must fill it. Concretely: verify no `minWidth`/`maxWidth`/`minHeight`/`maxHeight` or `NSWindow` content-size constraint is pinning the surface, and ensure the webview explicitly resizes with the window (Wry normally does this by default — the constraint is the bug to remove). CSS already does its part (no change needed there).

### Caveat

I could not capture a clean post-resize `innerWidth` comparison because each measurement required a rebuild, and the width clamp made the resize inconclusive on the viewport side. The width-clamp observation (#5) is sufficient to confirm H2; a final confirming step is to reproduce a successful enlarge and read `[AUDIT-VIEWPORT]` before/after once the size constraint is identified and removed.

I made **no production fix** to app behavior — only added diagnostic logging in `src-tauri/src/lib.rs` (uncommitted). The root cause to fix is the window/webview size constraint that pins the surface at 1280×800.
These two are **real, viable references** — same architecture as Nabu (Tauri 2 + Dioxus + WASM), and both presumably resize fine. So the architecture is sound; the bug is in Nabu's *specifics*. Here's what they reveal, honestly:

**The three concrete differences that matter for you:**

1. **`mtc-app` explicitly re-asserts resize after building the window.** In `mtc-app/src/lib.rs` it does `WebviewWindowBuilder::new(...)` then, *after* build: `window.set_resizable(true)`, `set_decorations(true)`, and `window.maximize()`. Nabu relies solely on `resizable:true` in `tauri.conf.json` and never touches the window after creation. This is the strongest candidate for the macOS webview-not-tracking-window bug — on some Wry versions, (re)setting `resizable` + a programmatic resize/maximize forces the webview frame to re-track. **Worth replicating in Nabu.**

2. **`maes` uses `h-dvh w-dvw` (dynamic viewport units) on its root div**, not `100vh`/`100vw`. `maes/aide_ui/src/main.rs` root is `class: "flex-fixed h-dvh w-dvw min-h-screen"`. Dioxus 0.7's `h-dvh` = `100dvh`. Nabu uses `h-screen w-screen` (i.e. `100vh/100vw`) and `.app{100vw;100vh}`. If the WKWebView's `vh` unit isn't tracking resize, `dvh` can behave differently. Worth testing as the CSS change.

3. **Both build the frontend with the official Dioxus CLI (`dx bundle`), not a hand-rolled `index.html` + build script.** maes: `beforeBuildCommand: "dx bundle --release --package aide_ui"`, `frontendDist: ../target/dx/.../web/public`. Nabu hand-rolls `build-dioxus.sh` (custom index.html with a `#main` div and a `#boot-splash`). This is the biggest structural divergence — `dx bundle` generates a known-good mount page. Removing the hand-rolled layer eliminates a whole class of boot/mount bugs.
