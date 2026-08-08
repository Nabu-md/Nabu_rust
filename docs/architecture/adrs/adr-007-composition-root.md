# ADR-007 — Composition Root & Service Wiring

**Status:** Accepted
**Date:** 2026-08-08
**Source:** Distilled from AUDIT_0.2, AUDIT_0.3

---

## Context

The application needs a single, authoritative composition root that
constructs and wires the ~12 core services (event bus, capability
registry, storage, pipeline, job queue, worker pool, capture engine,
indexer, vault graph) and registers them in a `ServiceRegistry`.

AUDIT-0.2/0.3 found **two competing composition roots**:
`ApplicationBuilder` (a builder pattern, never used in production) and
`build_application_context` in `src-tauri/src/lib.rs` (the code that
actually runs). This split is a confusion trap: `nabu-core` still
exports `ApplicationBuilder` as dead public API.

## Decision

Standardize on **`build_application_context`** in `src-tauri/src/lib.rs`
as the sole composition root. Treat `ApplicationBuilder` as deprecated:

- The composition root manually constructs, in dependency order:
  `EventBus`, `ServiceRegistry`, `CapabilityRegistry`, `StorageManager`,
  `ProcessingPipeline`, `DurableJobQueue`, `PipelineExecutor`,
  `WorkerPool`, `CaptureEngine`, `Indexer`, `VaultGraph`.
- Each service registers itself in the `ServiceRegistry` under a
  canonical string key.
- All service construction and wiring happens in exactly one place.

## Decision (service lifecycle)

Service startup follows the manual `lib.rs::run()` ordering. The
`Lifecycle` trait and `LifecycleManager` abstractions are **not** used
for actual startup — services are initialized explicitly in order, with
failure handled (and currently still panicking on queue/graph creation —
see Consequences).

## Rationale

- A single visible wiring point makes startup ordering obvious and
  debuggable.
- Two roots caused exactly the class of confusion AUDIT-0.3 documented
  ("AUDIT-0.1 suggested a builder that no longer exists").
- Manual ordered wiring is simpler than a lifecycle abstraction that
  added no runtime benefit.

## Alternatives Considered

- **Adopt `ApplicationBuilder` as the canonical root** — rejected; it is
  unused in production and `build_application_context` already works.
- **A full lifecycle-manager startup** — rejected; adds indirection
  without benefit when services are few and startup is linear.

## Consequences

- There is exactly one place to find "how is the app assembled".
- `ApplicationBuilder` and any `build_standard_application_context`
  dual API in `nabu-core` should be removed to end the duplication.
- Startup failures currently panic; graceful degradation for critical
  services remains open (see ADR-009).

## Future Implications

- New services register themselves in the composition root, never in a
  parallel builder.
- If service startup becomes non-linear (async registration), revisit a
  lifecycle manager — but only on demonstrated need.