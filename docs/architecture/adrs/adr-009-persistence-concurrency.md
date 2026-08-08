# ADR-009 — Persistence & Concurrency Model

**Status:** Accepted
**Date:** 2026-08-08
**Source:** Distilled from AUDIT_0.5

---

## Context

Three backend subsystems persist state to `.nabu/` on every mutation,
without batching or write coordination:

- **`StorageManager`** writes a JSON sidecar alongside every markdown
  object on save.
- **`Indexer`** persists to `.nabu/search_index.json` on every
  `index_object` call.
- **`VaultGraph`** persists to `.nabu/graph/` on every `add_node` call.

Because the live event flow (ADR-008) routes `ITEM_STORED` to the
indexer and graph, a single save can trigger multiple concurrent JSON
writes to overlapping files. JSON is **not safe for concurrent
writers**: races corrupt state and block the file.

Additionally, concurrent captures of two files with the same `title-slug`
within the same second collide on generated paths.

## Decision

Adopt an explicit persistence & concurrency model:

1. **Journalized/batch persistence for hot paths**: batch JSON index and
   graph persistence rather than persisting on every event. Persist to a
   temp file and atomically rename on commit.
2. **Writes are coordinated**: serialization writes (single writer
   ownership per file) instead of unlocked concurrent appends. Use
   per-file locks / a write queue where multiple services could touch
   the same artifact.
3. **Collision-proof identifiers**: make generated slugs unique under
   concurrent captures (e.g. append a timestamp/nonce when a slug
   already exists in the same second).
4. **Graceful degradation on service failure**: replace bare panics on
   queue/graph creation failure with recovery paths (from AUDIT-0.7) so a
   single failed service does not take down the app.

## Decision summary

Treat JSON sidecars, the search index, and the graph as **atomic,
serialized** artifacts — never raw concurrent appends. Prefer debounced,
batched, temp-then-rename writes coordinated through one ownership
path.

## Rationale

- JSON has no in-place concurrency story; the current on-every-event
  writes are the highest-leverage corruption risk in the app.
- Batched writes reduce disk churn and latency on capture bursts.
- Atomic rename guarantees readers never observe a partially-written file.

## Alternatives Considered

- **Per-file locks around every write** — sufficient for mutual exclusion
  but does not reduce churn; combine with batching rather than alone.
- **Keep write-on-every-event** — rejected; guarantees contention in the
  live-event design and risks corruption.
- **Panic on startup failure** — rejected; hostile to long-running
  operation and to the recovery/session features.

## Consequences

- Write paths in `StorageManager`, `Indexer`, and `VaultGraph` are
  revisited to batch-and-commit.
- Tests should add concurrency stress for simultaneous captures and
  simultaneous `ITEM_STORED` bursts.
- Startup becomes resilient; failing subsystems report and degrade
  instead of aborting.

## Future Implications

- Any new persistent subsystem must adopt the same serialize-and-rename
  contract.
- The collision-safe slug generator is a reusable utility for all
  object creation.