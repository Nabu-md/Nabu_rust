# Nabu — Ship ASAP Plan

Goal: get Nabu to a **shippable MVP** as fast as possible. The extension-removal purge is complete, verify the whole thing builds and tests green, then polish UI and start testing what matters (Obsidian features, dictation pill, the ACP-powered AI assistant). **ACP is a must-have** (it is how the AI in Nabu runs). **MCP is a nice-to-have** — it improves the ACP agent, and must not block ship.

Owner model: every task below is owned by a **dedicated agent** that lands one clean, reviewed commit. No advisor runs inline work.

---

## Priority 1 — Extension removal purge (COMPLETED)

**Context:** A prior run partially removed the browser-extension subsystem, leaving dangling references in comments and docs. Phase 2 completed the purge end-to-end: all extension source, the host binary, and every capture handler was deleted; the handler count dropped from 11 to 8; all stale references were cleaned; an ADR was written.

**What was removed:**
- `extensions/` directory (entire browser extension tree)
- `src-tauri/src/bin/` host binary and its socket module
- `src-tauri/host-manifest.json.example`
- `src-tauri/scripts/install-host.sh`
- `docs/host.md`
- Browser, Safari-reader, and article capture handlers plus HTML→article extraction helpers
- Handler registrations for `browser` and `safari_reader` in the capture engine
- README "Native Integrations" bullet for the extension host
- Stale references in architecture docs and inline comments

**ADR:** The ADR in `docs/adr/` records the decision, the **exact pre-purge commit**, and the restore procedure (`git show <pre-purge-commit> -- <path>`).

Exit criteria (all met):
- Purge-term search across `*.{rs,sh,json,toml,md}` returns zero source hits
- `cargo check --workspace` green, `cargo test -p nabu-core` green
- ADR committed; single commit

---

## Priority 2 — Verify everything else works (the ship gate)

The app must **build and run before any polish**. Known blockers, in order:

1. **Fix `crates/nabu-ui/src/components/inbox.rs:877`** — pre-existing extra `}` (unclosed delimiter) from commit `7e29423`. **This is the app-blocker.** Fix, then `cargo check` (nabu-ui).
2. **`cargo fmt`** — run across the whole tree before final review.
3. **Full test suite** — `cargo test -p nabu-core`. Run the whole suite after P1+P2.1.
4. **Docs hygiene** — `ARCHITECTURE.md` is stale (still "Electron/React"); rewrite to Tauri+Dioxus or delete.
5. **Stray artifact** — `crates/nabu-core/-` is a ~10 MB tracked binary; `git rm` it and add to `.gitignore`.
6. **Optional, not a blocker:** `cargo clippy`.

Exit criteria: workspace + UI compile with **zero errors**, core tests green, `cargo fmt --check` clean, docs not lying about the stack.

---

## Priority 3 (MUST-HAVE) — Finish ACP. This is how the AI in Nabu actually runs.

**Correction from earlier plan:** ACP is **must-have, not nice-to-have.** It is the protocol that connects Nabu to an external agent (e.g. claude-code) so the chatbot actually produces AI output. MCP is the separate nice-to-have that *improves* ACP (gives the agent tools/context to act inside Nabu) — it is not a substitute.

**Verified current state — the plumbing exists, the protocol + wiring do NOT:**
Built and present:
- **Chat UI** — `crates/nabu-ui/src/components/streaming/` (renders token streams; `ViewMode::Streaming`) ✅
- **Streaming transport** — `crates/nabu-core/src/streaming/` (`StreamingSession` publishes `stream.token` events over the EventBus; `stream_cancel` command exists) ✅
- **Conversation persistence** — `ConversationStore` + `thread_save/load/list/delete/update` IPC ✅
- **ACP-shaped foundation** — `crates/nabu-core/src/rpc/` (transport-independent JSON-RPC 2.0 core), `agent/` + `process_supervisor/` (agent subprocess lifecycle), `tool_calling/` (Tool models) ✅

**Missing (this is what "no ACP yet" means):**
- **No ACP protocol layer** — no `initialize` / `session/new` / `session/load` / `session/prompt` / `session/end`, no session state machine. `rpc/` docs explicitly call ACP a "future higher-layer protocol."
- **No agent is wired in** — `AgentManager`/`ProcessSupervisor` are structurally present (builder + an integration test) but `None` at runtime and never spawn an agent.
- **No way to send a prompt** — there is **no chat-send/agent-send command**. The only stream command is `stream_cancel`. The chat UI is a viewer; nothing calls `publish_token` from a real agent.

**Net:** the chatbot UI is a shell — it renders token events that nothing generates. This is why "the AI" doesn't work today.

To finish ACP (agentclientprotocol.com), as a dedicated agent task:
1. Implement the ACP session protocol on top of `rpc/` — session state machine + `initialize`/`session/new`/`session/load`/`session/prompt`/`session/end` handlers (local agent: JSON-RPC over stdio, per ACP spec)
2. Wire `AgentManager` + `ProcessSupervisor` to actually spawn the configured agent binary (e.g. claude-code) and stream its output into `StreamingSession::publish_token`
3. Add a chat-send Tauri command that the streaming UI calls to submit a prompt and open a stream
4. Expose `tool_calling/` tools as ACP tool definitions mapped to real app actions ("move around the app")
5. Verify end-to-end: type a prompt → agent subprocess runs → tokens stream into the chat

**Blockers checklist before attempting:** all of P1 + P2 green (a broken `inbox.rs` build blocks everything).

---

## Priority 4 (Nice-to-have, AFTER ACP) — MCP

Adds agent capability inside Nabu by exposing app state/actions as MCP resources and tools, so the ACP-connected agent can search, read, and write notes ("move around the app"). This is genuinely optional and sits on top of a working ACP.

---

## Suggested order of agents

| # | Agent task | Depends on |
|---|------------|-----------|
| 2 | Fix `inbox.rs` build + `cargo fmt` + full test suite (P2.1–2.3) | P1 |
| 3 | Docs hygiene + stray binary removal (P2.4–2.5) | P2 |
| 4 | Finish ACP (P3, must-have) | P1+P2 |
| 5 | MCP (P4, nice-to-have) | P3 |
| 6 | Fix frontend hydration / black screen (P5, must-have blocker) | P1+P2 |

---

## Priority 5 (MUST-HAVE, BLOCKER — new, after all original phases are done) — Fix frontend hydration: the app renders a black screen

**Symptom (observed on the running production app):** The app flashes white, a loading spinner appears, the "NABU" boot-splash renders briefly, then the screen turns **black** and stays black. The backend starts fine (log shows `startup complete`, process stays alive). This is the single biggest thing keeping Nabu from being "an app that works properly."

> **Execution plan:** see **`docs/PHASE5-HYDRATION.md`** for the sub-phase breakdown —
> two parallel read-only audits (static/build + runtime/log), a reconcile gate, then one
> sequential remediation agent.

**What the symptom tells us (proven, do not re-litigate):**
- The HTML + boot-splash **loads** (spinner shows) → the shell HTML/CSS is served fine.
- The black screen = `remove_boot_splash()` runs (called from the wasm `start()`), but **Dioxus mounts nothing** → the dark background `#030712` fills the window.
- The production binary loads the frontend at the WebKit layer with **no error** (`didStartProvisionalLoad → didCommitLoad → didFinishLoad`, `didFinishDocumentLoad`). The **window composites on screen** (`visible:true`). So the gap is entirely between "HTML loaded" and "Dioxus actually paints."

**The #1 trap — DO NOT run the wrong binary:**
- `cargo build` (even `--release`) produces a **DEV-MODE** binary that loads the `devUrl` (`http://localhost:8080`), which has no server → WebKit error `-1004` → **white/black screen.** This has caused multiple false "regressions."
- ONLY `cargo tauri build` produces the **production** binary that loads the embedded frontend (`frontendDist: ../dist`). **Always test with the `cargo tauri build` bundle** at `src-tauri/target/release/bundle/macos/Nabu.app`. Re-running `cargo build` over the bundle reintroduces the black screen.

**Diagnostic wiring already in place (use it first):**
- `src-tauri/src/lib.rs` has an `on_page_load` hook that (a) installs a JS hook accumulating `console.*` output + uncaught errors + unhandled rejections into `window.__nabuLogs`, and (b) polls it back to stderr every ~1.5s as `[WEB] ...` lines via `WebviewWindow::eval_with_callback`.
- **Run:** `src-tauri/target/release/bundle/macos/Nabu.app/Contents/MacOS/app > /tmp/nabu.log 2>&1`, wait ~20s, then `grep '\[WEB\]' /tmp/nabu.log`. If the wasm fetch/instantiation fails or `start()`/`App` panics, the `[WEB]` lines will show it. If there are **no** `[WEB]` lines at all, the JS module/wasm never executed (asset serving problem).

**Likely failure surface (investigate in this order):**
1. **Asset serving** — confirm `nabu_ui.js`, `nabu_ui_bg.wasm`, and the `snippets/` dir are actually embedded in the bundle (check `dist/`, and that `frontendDist` points at the real build output). A 404 on `nabu_ui_bg.wasm` means `init()` never runs → black screen, no `[WEB]` lines.
2. **Wasm boot** — `crates/nabu-ui/src/lib.rs::start()` (`#[wasm_bindgen(start)]`) calls `remove_boot_splash()` then `dioxus::web::launch::launch_cfg(App, ...)`. If it errors before/at launch, the splash is removed but nothing renders. `console_error_panic_hook` should surface the panic via `[WEB]`.
3. **App component / IPC on mount** — `provide_theme`/`App` calls IPC `settings_get` at mount. If it blocks/panics on the wasm→Tauri bridge, the root doesn't render. Verify `window.__TAURI__.core.invoke` is available (needs `withGlobalTauri` in config) and that IPC calls resolve.
4. **Dioxus mount root** — confirm the dioxus root id in `index.html` matches what `launch_cfg` targets, and that the dioxus renderer actually paints (a `dioxus::web::Config`/root mismatch yields a blank viewport).

**Exit criteria:** run the production bundle → `grep '[WEB]'` shows successful wasm boot (no panic/404), and the Nabu UI (Inbox/Settings/etc.) is visibly interactive, not a black screen. Commit the fix; do not close this until the rendered app is confirmed on screen.
