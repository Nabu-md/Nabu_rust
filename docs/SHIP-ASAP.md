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
