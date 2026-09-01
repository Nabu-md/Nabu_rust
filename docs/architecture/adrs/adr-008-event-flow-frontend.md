# ADR-008 — Live Event Flow to the Frontend

**Status:** Accepted
**Date:** 2026-08-08
**Source:** Distilled from AUDIT_0.4, AUDIT_0.5, AUDIT_0.7

---

## Context

The backend has an internal `EventBus<PipelineEvent>` that publishes
8 event types and drives the indexer and vault graph via `ITEM_STORED`.
The frontend (`ui-react`) however has **no path to receive these events**:
its `ipc.ts` only wraps `invoke()` — it has no `listen` capability for
backend-originated events. This leaves the UI dependent on polling and
stale-cache problems.

A second gap: `note_save` (and the autosave path) **writes directly to
disk without publishing `ITEM_STORED`**, so edits made in the note editor
never flow to the indexer or graph. Concretely:

- Note edit → search results stale (index never updated)
- Note edit → graph edges stale (`[[wikilink]]` never indexed)
- Note create via editor → invisible to search and graph
- Settings change → frontend signals not propagated (no push model)
- History undo → deleted note still in index and graph

## Decision

Adopt a **unified live event bridge**:

1. **One EventBus → Tauri bridge**: subscribe to internal `EventBus`
   events and forward them to the frontend over a single typed Tauri
   channel (e.g. `nabu-event`) via `app.emit_all()` / `window.emit()`.
   The frontend subscribes once and routes typed events to the relevant
   signals/components.
2. **All writes publish `ITEM_STORED`**: `note_save`, autosave, and note
   creation must route through the canonical pipeline (or at minimum
   emit `ITEM_STORED`) so the indexer and `VaultGraph::add_node` update
   correctly. Remove the temporary direct-disk bypass.
3. **Settings/history side effects emit events** so the frontend updates
   reactively instead of being forced to re-sync local signals.

## Rationale

- Closes the traceability gap: every mutation in the backend reflects to
  search, graph, and UI automatically.
- A single event channel keeps the frontend contract simple and typed,
  avoiding one-off polling code per feature.
- Fixes the root causes AUDIT-0.4 documented rather than their symptoms
  (stale search, stale graph).

## Alternatives Considered

- **Per-command frontend polling** — rejected; duplicates state and
  reintroduces staleness.
- **No push model** — rejected; leaves the UI out of sync with the
  authoritative backend state.
- **Keeping the note_save bypass** — rejected; it is the direct cause of
  the stale index/graph bugs.

## Consequences

- The frontend gains a reactive data flow keyed to backend truth.
- `note_save` behavior changes: it must publish `ITEM_STORED`, which the
  indexer and graph already subscribe to.
- A Tauri event bridge component is added to the backend
  (`src-tauri`) — this is the highest-priority item from AUDIT-0.4/0.7.

## Future Implications

- New backend mutations emit events rather than relying on the UI to
  re-query.
- The single `nabu-event` channel is extensible to all 8 event types;
  new types are added without new IPC channels.