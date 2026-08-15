# nabu-core Performance Benchmarks

This directory (`crates/nabu-core/benches/`) houses the core performance
benchmark suite for Nabu. Benchmarks measure the **real** indexer, storage, and
graph paths — not trivial microbenchmarks.

## Running

```bash
# Optimized release-mode benchmarks (default `cargo bench` profile).
cargo bench -p nabu-core

# Run a single benchmark family.
cargo bench -p nabu-core --bench search
cargo bench -p nabu-core --bench startup
cargo bench -p nabu-core --bench graph_rebuild

# Reduced sample count for a quick smoke test (compilation + sanity only).
cargo bench -p nabu-core -- --sample-size 10
```

## Workloads

The shared corpus generator (`common/mod.rs`) builds a **deterministic**
500-note Markdown corpus entirely in-process from a fixed-seed internal LCG
(no `rand` dependency, no real vault, no network, no wall-clock dependency):

| property                  | value                              |
|---------------------------|------------------------------------|
| note count                | 500                                |
| approx. body size         | ~2 KB per note (~1 MB total)       |
| content type              | `Markdown` (real `KnowledgeObject`)|
| title                     | `Note 0000` … `Note 0499`          |
| tags                      | 2–4 tags drawn from a shared pool  |
| common terms              | 12 sentence templates, recurring   |
| distinctive terms         | `alpha0000` … `alpha0499` (one per note, **body-only**) |
| wiki-links                | `[[Note N]]`, varying density (0–3 per note) |
| block references          | `((Note N))` on a subset of notes  |
| explicit relations        | sparse `References` edges to hub node 0 |
| object IDs                | deterministic (`Uuid::from_u128`)  |
| timestamps                | fixed `2024-01-01T00:00:00Z`       |

### Graph workload shape

- 10 **hub** notes (`Note 0000`…`Note 0009`) form a densely interlinked cluster.
- ~20% of the remaining notes are **lone** (no outgoing wiki-links).
- The rest carry 1–3 links, always referencing at least one hub node so high-in-degree
  nodes exist (multiple references to common nodes).
- Block references appear on every 4th linked note.

This satisfies: many notes, wiki-links, varying link density, notes with no
links, and multiple references to common nodes.

## Benchmarks

### Search (`search.rs`)

Measures `Indexer::search()` — the real query path (lowercase tokenization,
posting-list accumulation, `Vec` sort + dedup). Corpus is indexed once in setup
(outside the timed region).

| benchmark               | query                         | matches | exercises                          |
|-------------------------|-------------------------------|---------|------------------------------------|
| `distinctive_body_token`| `alpha0499`                   | ~1      | body-text indexing (not title-only)|
| `common_term`           | `nabu`                        | ~most   | broad posting-list walk            |
| `multi_token_or`        | `alpha0499 graph`             | ~most   | posting-list union (OR semantics)  |
| `miss_query`            | `zzz_no_such_token_xyz`       | 0       | empty-list accumulation            |

### Startup (`startup.rs`)

> **"Startup" = core initialization work measurable inside `nabu-core`.**

This is explicitly **not** the full desktop process cold-start. It does **not**
measure OS process launch, dynamic-library loading, the Tauri/Svelte shell, GPU
compositing, or window creation. Those are out of scope for a `nabu-core`
benchmark and are not reliably measurable from here.

What it measures (vault staged to a `tempfile` dir once, then reloaded per
iteration):

| benchmark       | operation                                          |
|-----------------|----------------------------------------------------|
| `index_load`    | `Indexer::with_vault_path().load()` — deserialize persisted inverted-index JSON |
| `storage_reload`| `StorageManager::reload_from_disk()` — read all `.nabu/*.json` sidecars + content files |
| `graph_load`    | `VaultGraph::with_persistence()` — full recovery: load + quick-check + version check + integrity validation |
| `core_init`     | aggregate: index load + storage reload + graph load |

The OS page cache is warm after the first iteration; this is intentional. We
measure application initialization work (parsing, deserialization,
structure-building), not OS-level disk I/O timing.

### Graph rebuild (`graph_rebuild.rs`)

Measures re-deriving the graph from canonical content. No precomputed edges are
inserted — wiki-links and block references are re-parsed each iteration.

| benchmark                | operation                                              |
|--------------------------|-------------------------------------------------------|
| `build_from_objects`     | `build_graph_from_objects()` — pure parse + serialize |
| `rebuild`                | `VaultGraph::new()` + `rebuild_from_objects()` — in-memory construction (persist is a no-op) |
| `rebuild_with_persist`   | `VaultGraph::with_persistence()` + `rebuild_from_objects()` — full path incl. disk persist (graph dir reset *outside* the timed region via `iter_with_setup`) |

## Performance budgets (provisional)

Nabu has **not yet** established product-level performance requirements. The
thresholds below are **provisional** and are encoded as documented constants in
`common::budgets`. Treat them as regression-detection signposts, not hard
product SLAs.

| budget                           | value (ms) | workload                                            |
|----------------------------------|------------|-----------------------------------------------------|
| `SEARCH_DISTINCTIVE_BUDGET_MS`   | 5          | 500-note corpus, single distinctive body token      |
| `SEARCH_COMMON_BUDGET_MS`        | 15         | 500-note corpus, common term (broad posting list)   |
| `SEARCH_MULTI_TOKEN_BUDGET_MS`   | 25         | 500-note corpus, multi-token OR query               |
| `SEARCH_MISS_BUDGET_MS`          | 3          | 500-note corpus, empty-result query                 |
| `STARTUP_INDEX_LOAD_BUDGET_MS`   | 30         | deserialize persisted inverted-index JSON           |
| `STARTUP_STORAGE_RELOAD_BUDGET_MS`| 80         | reload 500 object cache from sidecars + content     |
| `STARTUP_GRAPH_LOAD_BUDGET_MS`    | 50         | load + validate persisted graph snapshot            |
| `STARTUP_CORE_INIT_BUDGET_MS`     | 150        | index + storage + graph load combined               |
| `GRAPH_REBUILD_INMEMORY_BUDGET_MS`| 60         | 500-note, ~1 000 wiki-link edges, in-memory         |
| `GRAPH_REBUILD_FULL_BUDGET_MS`    | 120        | 500-note rebuild including disk persist              |

These budgets are **approximate** and **machine-dependent** (CPU, disk, OS page
cache). They are meant to flag order-of-magnitude regressions, not to gate CI on
sub-threshold noise.

## Regression detection

The suite uses [criterion](https://docs.rs/criterion), which is statistics-aware:

- Each benchmark runs many samples with warmup.
- criterion reports mean, median, and a confidence interval; by default it **already
  aborts a run if the new measurement is statistically indistinguishable from the
  previous one** — i.e. tiny noise is *not* treated as a regression.
- To compare against a stored baseline:

  ```bash
  cargo bench -p nabu-core -- --save-baseline main
  # ... make a change ...
  cargo bench -p nabu-core -- --baseline main
  ```

  criterion prints a per-benchmark `change: [X%, Y%]` with confidence. A change is
  only actionable when statistically significant (criterion's default window) **and**
  large enough to matter (rule of thumb: >10%).

- CI policy: **do not** fail the build on sub-threshold timing fluctuations. Review
  criterion's significance report and the magnitude together. If automated gating is
  desired later, prefer a relative-threshold comparison against a committed baseline
  rather than absolute hard cutoffs (absolute budgets drift machine-to-machine).

## Reporting

Each ran benchmark prints (via criterion): operation name, workload size
(`Throughput`), iterations, ns/iter (mean ± std), and change vs. baseline when
applicable. The budgets above are also printed by the suite when run with a
baseline. `BENCHMARKS.md` captures the methodology so the results are
self-describing: operation measured, workload size, latency/time, budget, and
in-budget status.
