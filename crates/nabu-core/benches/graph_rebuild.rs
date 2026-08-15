//! Graph rebuild benchmarks for `nabu-core`.
//!
//! Exercises the real graph construction path used by the application after a
//! full vault rescan:
//!
//! - `build_graph_from_objects` — pure parse + serialize (wiki-links, block
//!   references, explicit relations resolved against a `ResolutionIndex`).
//! - `VaultGraph::rebuild_from_objects` — the canonical reindex method: clear,
//!   build resolution index, add nodes, derive explicit-relation edges,
//!   derive content edges (wiki-link/block-ref parsing), rebuild adjacency.
//!   Persist is a no-op here (no persistence handle) so only construction is
//!   timed.
//! - `VaultGraph::rebuild_from_objects` *with* persistence — the full path
//!   including serializing the snapshot to `.nabu/graph/`.
//!
//! The benchmark deliberately re-derives edges from canonical content on every
//! iteration; it never hand-inserts precomputed edges.
//!
//! Budgets (provisional): see `common::budgets::GRAPH_REBUILD_*`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nabu_core::{build_graph_from_objects, VaultGraph};
use tempfile::tempdir;

mod common;
use common::generate_corpus;

pub fn bench_graph_rebuild(c: &mut Criterion) {
    let corpus = generate_corpus();
    let node_count = corpus.len();

    // A vault-on-disk for the persistence variant. Created once; the graph
    // subdirectory is reset per-iteration *outside* the timed region.
    let dir = tempdir().expect("temp vault dir");
    let vault = dir.path().to_path_buf();
    let graph_dir = vault.join(".nabu").join("graph");

    let mut g = c.benchmark_group("graph_rebuild");
    g.throughput(criterion::Throughput::Elements(node_count as u64));
    g.sample_size(50);

    // --- Pure construction: parse wiki-links + build serialized nodes/edges ---
    g.bench_function("build_from_objects", |b| {
        b.iter(|| {
            let (nodes, edges) = build_graph_from_objects(black_box(&corpus));
            (nodes.len(), edges.len())
        })
    });

    // --- VaultGraph rebuild (in-memory; persist is a no-op) ---
    g.bench_function("rebuild", |b| {
        b.iter(|| {
            let graph = VaultGraph::new();
            graph
                .rebuild_from_objects(black_box(&corpus))
                .expect("rebuild graph");
            (graph.node_count(), graph.edge_count())
        })
    });

    // --- VaultGraph rebuild including disk persistence ---
    // Setup (reset graph dir) runs OUTSIDE the timed region via
    // iter_with_setup, so we time only construction + persist writes.
    g.bench_function("rebuild_with_persist", |b| {
        b.iter_with_setup(
            || {
                let _ = std::fs::remove_dir_all(&graph_dir);
            },
            |_| {
                let graph = VaultGraph::with_persistence(None, black_box(vault.clone()))
                    .expect("graph with persistence");
                graph
                    .rebuild_from_objects(black_box(&corpus))
                    .expect("rebuild graph");
                (graph.node_count(), graph.edge_count())
            },
        )
    });

    g.finish();
}

criterion_group!(benches, bench_graph_rebuild);
criterion_main!(benches);
