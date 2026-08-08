# ADR-010 — Capability Platform

**Status:** Accepted
**Date:** 2026-08-08
**Source:** Distilled from AUDIT_0.3 and the Capability Platform Roadmap

**Note:** The roadmap (Capability-Roadmap "Gap Analysis & Implementation
Plan") and this ADR's preceding claims may describe work whose current
state differs from the time the audit ran. Where the implementation has
moved on, the *decisions* below remain the governing contract; the audit
was the review that established them.

---

## Context

Nabu is designed around a **capability platform**: extensible, named
capabilities (`nabu:sync`, `nabu:plugin`, `nabu:ai`, `nabu:embedding`,
…); a `CapabilityRegistry`; a plugin system; and a `ServiceRegistry`
for service instances.

AUDIT-0.3 found the platform exists as a **complete but unregistered
dead architecture** at the time of the audit:

- `CapabilityRegistry::register_builtin()` registers 14 capabilities.
- `ServiceRegistry` holds service instances under string keys.
- Eight system types (EventBus, Storage, Pipeline, JobQueue/Executor,
  WorkerPool, Capture, Indexer, Graph) are all wired in the composition
  root (see ADR-007).
- The `PluginManager` stack was defined but never instantiated in
  production.

The roadmap items — an async `EventBus`, a Tauri event bridge, tokio
process features, and dynamic/plugin capability registration — were
planned and, per roadmap, implemented.

## Decision

Adopt capability registration as the extension contract:

1. **Canonical capability keys**: capabilities are addressed by
   namespaced string keys (`nabu:*`) registered at startup.
2. **PluginManager is the registration path**: plugin-provided
   capabilities register through the `CapabilityRegistry`; if it is not
   wired in production, that is a gap to close, not a choice to accept.
3. **Built-ins are registered at startup** (`register_builtin`); external
   capability manifests extend at runtime.
4. **Subprocess spawning** (for sidecar processing: OCR, Whisper, PDF)
   uses `tokio::::process` after the `process` feature is enabled, managed
   by `ProcessSupervisor` with health checks and restart policy.

## Rationale

- A named capability contract is what makes plugins, AI, sync, and
  embeddings first-class and discoverable.
- One registration path (the `CapabilityRegistry`) avoids ad-hoc feature
  toggles scattered through `lib.rs`.
- Process supervision (ADR-relevant) gives sidecars structured lifecycle
  instead of fire-and-forget spawns.

## Alternatives Considered

- **Features as hard-coded matches in a command handler** — rejected;
  ossuffocates extensibility and mixes unrelated domains.
- **No capability registry (roll services into `ServiceRegistry` only)** —
  rejected; loses the namespaced, discoverable capability surface.

## Consequences

- New capabilities are developed against the `CapabilityRegistry`
  contract.
- `PluginManager` correctness must be verified against current code
  (see verification note above); if still uninstantiated, it must be
  wired or removed — silent dead architecture is not acceptable.
- Sidecar processes follow the supervision lifecycle, not ad-hoc spawns.

## Future Implications

- Dynamic external-capability manifests extend the platform without a
  recompile.
- The capabilities drive both backend commands and (via ADR-008) live
  frontend events.