//! Core-initialization (startup) benchmarks for `nabu-core`.
//!
//! ## What "startup" means here
//!
//! This benchmark measures the **core initialization work that contributes to
//! application startup** and is reliably measurable inside `nabu-core`:
//!
//! 1. `Indexer::load()` — deserialize the persisted inverted index JSON into an
//!    in-memory `HashMap`.
//! 2. `StorageManager::reload_from_disk()` — scan `.nabu/*.json` sidecars and
//!    their content files, reconstructing the in-memory object cache.
//! 3. `VaultGraph::with_persistence(...)` — run the full recovery path: load +
//!    quick-check + version compatibility + full integrity validation of the
//!    persisted graph snapshot.
//!
//! ## What this is NOT
//!
//! This is **not** the full desktop process cold-start. It does not measure
//! OS process launch, dynamic-library loading, the Tauri/Svelte shell, GPU
//! compositing, or window creation — none of which are reliably measurable
//! from a `nabu-core` micro-benchmark. Those contribute the bulk of a true
//! "first paint" and are out of scope for this benchmark (see BENCHMARKS.md).
//!
//! The OS page cache is expected to be warm after the first iteration; that is
//! intentional. We measure *application initialization work* (parsing,
//! structure-building), not OS-level disk I/O timing, per the phase
//! requirements.
//!
//! Budgets (provisional): see `common::budgets::STARTUP_*`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nabu_core::{Indexer, StorageManager, VaultGraph};
use tempfile::tempdir;

mod common;
use common::{generate_corpus, stage_full_vault};

pub fn bench_startup(c: &mut Criterion) {
    let corpus = generate_corpus();

    // One-time vault staging (outside all timed regions).
    let dir = tempdir().expect("temp vault dir");
    let vault = dir.path().to_path_buf();
    stage_full_vault(&vault, &corpus);

    let mut g = c.benchmark_group("startup");
    g.sample_size(30);

    // --- 1. Search index load (deserialize persisted inverted index JSON) ---
    g.bench_function("index_load", |b| {
        b.iter(|| {
            let indexer = Indexer::with_vault_path(black_box(vault.clone()));
            black_box(indexer.load()).expect("load index");
            indexer
        });
    });

    // --- 2. Storage cache reload (sidecars + content files from disk) ---
    g.bench_function("storage_reload", |b| {
        b.iter(|| {
            let storage = StorageManager::new(black_box(vault.clone()));
            let count = black_box(storage.reload_from_disk()).expect("reload from disk");
            (storage, count)
        });
    });

    // --- 3. Graph load + integrity validation ---
    g.bench_function("graph_load", |b| {
        b.iter(|| {
            let graph = VaultGraph::with_persistence(None, black_box(vault.clone()))
                .expect("load graph from disk");
            let nodes = graph.node_count();
            let edges = graph.edge_count();
            (graph, nodes, edges)
        });
    });

    // --- 4. Aggregate core initialization ---
    g.bench_function("core_init", |b| {
        b.iter(|| {
            let indexer = Indexer::with_vault_path(vault.clone());
            let storage = StorageManager::new(vault.clone());
            let graph = VaultGraph::with_persistence(None, vault.clone()).expect("graph load");

            let idx_ok = indexer.load().is_ok();
            let sto_count = storage.reload_from_disk().expect("reload");
            let node_count = graph.node_count();
            let edge_count = graph.edge_count();

            (idx_ok, sto_count, node_count, edge_count, indexer, storage, graph)
        });
    });

    g.finish();
}

criterion_group!(benches, bench_startup);
criterion_main!(benches);
