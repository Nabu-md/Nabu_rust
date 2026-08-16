# Nabu — Ship ASAP Plan

Goal: get Nabu to a **shippable MVP** as fast as possible. Finish the webclipper purge, verify the whole thing builds and tests green, then polish UI and start testing what matters (Obsidian features, dictation pill, the ACP-powered AI assistant). **ACP is a must-have** (it is how the AI in Nabu runs). **MCP is a nice-to-have** — it improves the ACP agent, and must not block ship.

Owner model: every task below is owned by a **dedicated agent** that lands one clean, reviewed commit. No advisor runs inline work.

---

## Priority 1 — Finish the webclipper purge (unblocked, do first)

**Context:** An earlier run partially removed webclipper but left the work **uncommitted, no ADR, dirty tree** (and verified `nabu-core` compiles). The UI build is separately broken (see P2). Decide once: keep-and-review the current working tree, or `git reset --hard HEAD` and have the agent redo it cleanly. **Recommendation: keep the reviewed diff** — it's sound and verified — and have the agent finish the remaining pieces (ADR + commit + final green check). If you distrust the diff's provenance, reset and redo for process hygiene.

**One agent owns the whole purge end-to-end** so it lands as a single reviewable commit.

Scope to remove (exact list):
- `extensions/browser/**` — browser extension source
- `src-tauri/src/native_messaging.rs`, `native_messaging_socket.rs`, `bin/native_messaging_host.rs`
- `src-tauri/native-messaging-manifest.json.example`
- `src-tauri/scripts/install-native-messaging.sh`
- `docs/native-messaging.md`
- `nabu-core/src/capture/handler.rs`: `BrowserCaptureHandler`, `SafariReaderHandler`, `ArticleCaptureHandler`, the HTML→article extraction subsystem (`html_to_article` + `extract_tag_content`, `extract_meta_property`, `extract_meta_name_content`, `extract_meta_name`, `strip_boilerplate_tags`, `html_block_to_markdown`), and now-orphaned helpers (`extract_domain`, `is_youtube_url`, `is_github_url`, `regex_lazy!`) — remove each only if unused after the others go (verify with `rg`)
- `nabu-core/src/capture/engine.rs`: remove `browser` + `safari_reader` registrations; update handler-count tests (`11` → `8`) and drop the browser-routing test
- `src-tauri/Cargo.toml` (`[[bin]] native-messaging-host`), `build.rs` (host staging), `tauri.conf.json` (`resources` → `[]`), `scripts/run-dioxus.sh` + `scripts/build-dioxus.sh` (host build steps)
- `README.md`: drop the "Browser extension" bullet

**Write the ADR (required before commit):** create `docs/adr/` and record the decision to remove webclipper, the **exact pre-purge commit to restore from**, and the restore procedure (`git show <pre-purge-commit> -- <path>` / `git restore`). This is the user's stated reason for keeping history: *"in case I ever want to bring it back I can recreate it from the commit before the purge."*

Exit criteria:
- `rg -rni "native.messaging|native_messaging|webclip|browsercapture|articlecapture|safarireader|html_to_article" --glob '*.{rs,sh,json,toml,md}'` → zero source hits
- `cargo check --workspace` green, `cargo test -p nabu-core` green
- ADR committed; single commit

---

## Priority 2 — Verify everything else works (the ship gate)

The app must **build and run before any polish**. Known blockers, in order:

1. **Fix `crates/nabu-ui/src/components/inbox.rs:877`** — pre-existing extra `}` (unclosed delimiter) from commit `7e29423`. **This is the app-blocker.** Fix, then `cargo check` (nabu-ui).
2. **`cargo fmt`** — 825 diffs across the repo. One pass normalizes everything; run before final review.
3. **Full test suite** — `cargo test -p nabu-core` (one failing handler-count test is fixed in P1). Run the whole suite after P1+P2.1.
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

Adds agent capability inside Nabu by exposing app state/actions as MCP resources and tools, so the ACP-connected agent can search, read, and edit notes ("move around the app"). This is genuinely optional and sits on top of a working ACP.

---

## Suggested order of agents

| # | Agent task | Depends on |
|---|------------|-----------|
| 1 | Webclipper purge + ADR + commit (P1) | — |
| 2 | Fix `inbox.rs` build + `cargo fmt` + full test suite (P2.1–2.3) | P1 |
| 3 | Docs hygiene + stray binary removal (P2.4–2.5) | P2 |
| 4 | Finish ACP (P3, must-have) | P1+P2 |
| 5 | MCP (P4, nice-to-have) | P4 → P3 |