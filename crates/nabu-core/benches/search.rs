//! Search latency benchmarks for `nabu-core`.
//!
//! Measures the real `Indexer::search` query path (lower-cased tokenization,
//! posting-list accumulation, sort + dedup) over a realistic 500-note corpus
//! with body-text indexing.
//!
//! Budget (provisional): see `common::budgets::SEARCH_*_BUDGET_MS`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nabu_core::Indexer;

mod common;
use common::{budgets, distinctive_token, generate_corpus};

pub fn bench_search(c: &mut Criterion) {
    let corpus = generate_corpus();

    // Index setup happens ONCE, outside any timed region.
    let indexer = Indexer::new();
    indexer.reindex(&corpus).expect("reindex corpus");

    let distinctive = distinctive_token(budgets::SEARCH_NOTE_COUNT - 1);

    let mut g = c.benchmark_group("search");
    g.sample_size(100);

    // A distinctive body-only token -> ~1 result. Exercises body-text indexing
    // (the token lives only in note bodies, never title/tags/type).
    g.bench_function("distinctive_body_token", |b| {
        b.iter(|| black_box(indexer.search(&distinctive).len()))
    });

    // A common term present in most bodies -> large posting-list walk.
    g.bench_function("common_term", |b| {
        b.iter(|| black_box(indexer.search("nabu").len()))
    });

    // Multi-token OR query (distinctive + common) -> posting-list union.
    let multi = format!("{} graph", distinctive);
    g.bench_function("multi_token_or", |b| {
        b.iter(|| black_box(indexer.search(&multi).len()))
    });

    // Miss query -> empty posting-list walk, sort, dedup.
    g.bench_function("miss_query", |b| {
        b.iter(|| black_box(indexer.search("zzz_no_such_token_xyz").len()))
    });

    g.finish();
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
