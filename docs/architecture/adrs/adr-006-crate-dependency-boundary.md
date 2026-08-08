# ADR-006 — Crate Dependency Boundary

**Status:** Accepted
**Date:** 2026-08-08
**Source:** Distilled from AUDIT_0.1 (Semantic Architecture Trace)

---

## Context

Nabu is compiled as three independent Rust crates with very different
consumption patterns. Without an explicit statement of the dependency
boundary, contributors will naturally blur lines — importing `nabu-core`
domain types into the UI, coupling the core to Tauri APIs, or sharing
Rust types across the WASM bridge in ways that break compilation and
bloat the frontend bundle.

A semantic dependency trace (AUDIT-0.1) confirmed the intended edges:
`nabu-core` depends only on external crates; `src-tauri` depends on
`nabu-core`; `nabu-ui` depends on neither and talks to the backend only
over the IPC bridge.

## Decision

Enforce a strictly **unidirectional** crate dependency graph:

```
crates/nabu-core  →  (external crates only: chrono, serde, tokio, …)
src-tauri         →  nabu-core
crates/nabu-ui    →  nothing (bridges via wasm_bindgen IPC only)
```

Concretely:

- **`crates/nabu-core`** holds all domain logic with zero knowledge of
  the desktop shell (`src-tauri`) or the frontend (`nabu-ui`).
- **`src-tauri`** owns the Tauri v2 shell and `#[tauri::command]` handlers,
  and depends on `nabu-core` for all domain types and services.
- **`nabu-ui`** never imports Rust types from `nabu-core` into frontend
  code. All type exchange across the WASM boundary is via
  `serde_json::Value` / `serde_wasm_bindgen` over `window.__TAURI__.core.invoke`.

## Rationale

- Keeps the build graph acyclic and each crate independently compilable
  and testable.
- `nabu-ui` builds to a WASM `cdylib`; importing core Rust types there
  would pull backend dependencies into the browser bundle.
- A JSON/serde bridge keeps the frontend boundary a stable, versionable
  contract rather than a Rust type-coupling.

## Alternatives Considered

- **Share a `models` crate with the UI** — rejected; would couple the WASM
  bundle to backend types and complicate the `cdylib` build.
- **Let `nabu-ui` depend on `nabu-core`** — rejected; core pulls in tokio,
  storage, and native dependencies unnecessary for the frontend.

## Consequences

- Architecture validation scans `Cargo.toml` `[dependencies]` for the three
  edges (method used in AUDIT-0.1).
- New domain types exported to the UI must be serializable (serde) and
  documented at the bridge, not imported as Rust types.
- The IPC contract in `src-tauri/src/commands.rs` is the de-facto
  frontend API surface and must stay minimalist.

## Future Implications

- Any new frontend-facing service must expose itself through a
  `#[tauri::command]` returning serde-serializable values.
- If a shared-contracts crate is ever introduced, it must sit **below**
  `nabu-core` (dependency-free), never above it.