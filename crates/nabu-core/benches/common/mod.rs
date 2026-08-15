//! Shared deterministic fixtures and performance budgets for the `nabu-core`
//! criterion benchmark suite.
//!
//! ## Determinism policy
//!
//! No benchmark in this suite depends on:
//! - the user's real vault (corpus is generated in-memory + staged to a
//!   `tempfile` directory inside the benchmark process),
//! - network access (no I/O to the internet),
//! - the current wall-clock time (objects use a fixed `DateTime`),
//! - unseeded randomness (a small internal LCG is seeded with a fixed constant).
//!
//! All outputs are reproducible run-to-run so criterion baselines remain
//! comparable across machines and commits.

// This module is shared across multiple `[[bench]]` binaries via
// `mod common;`. Individual helpers are only used by some benches, so silence
// the cross-binary dead-code warnings for the shared helper module.
#![allow(dead_code)]

use nabu_core::models::{KnowledgeObject, ObjectContent, ObjectMetadata, ObjectType, ProcessingState};
use std::collections::HashMap;
use uuid::Uuid;

use chrono::TimeZone;

/// Fixed timestamp used for every generated object so fixtures are independent
/// of the wall clock.
pub fn fixed_time() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
        .unwrap()
}

/// Deterministic, seedable linear-congruential PRNG.
///
/// Avoids pulling in `rand` as a dev-dependency; only `u128`-from-`usize`
/// determinism is required.
pub struct Lcg {
    state: u128,
}

impl Lcg {
    pub fn new(seed: u128) -> Self {
        Self {
            state: seed.wrapping_mul(6364136223846793005).wrapping_add(1),
        }
    }

    pub fn next_u128(&mut self) -> u128 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    pub fn next_range(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u128() % (n as u128)) as usize
    }

    pub fn next_f64(&mut self) -> f64 {
        let x = self.next_u128() >> 11;
        (x as f64) / ((1u128 << 53) as f64)
    }
}

/// Deterministic UUID from an index.
pub fn deterministic_id(i: usize) -> Uuid {
    Uuid::from_u128(i as u128)
}

// ---------------------------------------------------------------------------
// Corpus parameters
// ---------------------------------------------------------------------------

/// Number of notes in the standard benchmark corpus.
///
/// Chosen to be a "meaningful number" of Markdown notes for a small vault:
/// large enough to make index posting lists and graph adjacency maps
/// non-trivial, small enough that the repo stays lean (fixtures are generated
/// at runtime, never committed).
pub const CORPUS_NOTE_COUNT: usize = 500;

/// Approx total body bytes the corpus occupies (~2 KB * 500 ~= 1 MB).
pub const CORPUS_APPROX_BYTES: usize = 1_000_000;

/// Distinctive token namespace. Each note carries exactly one body-only
/// token of the form `alphaNNNN` which is findable through a *body-text*
/// search, proving the index is not a title-only lookup.
pub fn distinctive_token(note_index: usize) -> String {
    format!("alpha{:04}", note_index)
}

// ---------------------------------------------------------------------------
// Vocabulary pools (realistic, repeat across the corpus)
// ---------------------------------------------------------------------------

/// Terms that recur across many notes so posting lists grow realistically.
const COMMON_SENTENCES: &[&str] = &[
    "Nabu is a knowledge management platform for connected thinking.",
    "The graph engine rebuilds relationships from wiki-links and block references.",
    "Search indexing tokenizes titles, tags, and body text into lowercased terms.",
    "A vault stores Markdown notes alongside generated sidecar metadata.",
    "Incremental updates only recalculate the affected regions of the graph.",
    "The inverted index persists under .nabu/search_index.json between sessions.",
    "Recovery rebuilds the graph from canonical Markdown when needed.",
    "Wiki-links resolve by title or by file stem through the resolution index.",
    "Block references transclude content from other notes via double parentheses.",
    "The lifecycle manager drives services through created, initialized, running, and shutdown.",
    "Each KnowledgeObject flows through capture, storage, indexing, and graph update.",
    "Durable persistence keeps object metadata so vaults survive application restarts.",
];

const COMMON_TAGS: &[&str] = &[
    "concept", "reference", "project", "index", "graph", "search",
    "vault", "metadata", "archive", "draft",
];

/// Hub notes (indices 0..HUB_COUNT) are referenced by many other notes so the
/// graph contains high-in-degree nodes alongside leaf nodes with no links.
pub const HUB_COUNT: usize = 10;

// ---------------------------------------------------------------------------
// Performance budgets (provisional)
// ---------------------------------------------------------------------------

/// PROVISIONAL performance budgets for the `nabu-core` benchmark suite.
///
/// Nabu has **not yet** established hard product-level performance
/// requirements. These thresholds are intentionally provisional: they exist to
/// detect *meaningful* regressions through criterion's statistical comparison
/// (`cargo bench -- --baseline <name>` / `criterion compare`).
///
/// CI policy: do **not** fail on sub-threshold timing noise. A regression is
/// only actionable when criterion reports a statistically significant change
/// (default ±3.8% confidence window) *and* the magnitude exceeds ~10%.
///
/// These constants are reference budgets (not asserted in code, to avoid
/// gating CI on machine-dependent noise). They are documented exhaustively in
/// `BENCHMARKS.md` and may be wired into a statistical regression harness later.
#[allow(dead_code)]
pub mod budgets {
    /// Search corpus: 500 notes, ~2 KB body each (~1 MB total), body-text
    /// indexing enabled (titles + tags + bodies tokenized together).
    pub const SEARCH_NOTE_COUNT: usize = 500;

    /// Search: a single distinctive body-only token returning ~1 result.
    pub const SEARCH_DISTINCTIVE_BUDGET_MS: f64 = 5.0;

    /// Search: a common token returning the majority of the corpus (broad
    /// posting list walk + sort + dedup).
    pub const SEARCH_COMMON_BUDGET_MS: f64 = 15.0;

    /// Search: multi-token OR query mixing a distinctive term and a common
    /// term — exercises posting-list merge across disparate list sizes.
    pub const SEARCH_MULTI_TOKEN_BUDGET_MS: f64 = 25.0;

    /// Search: query that matches nothing (empty posting-list accumulation).
    pub const SEARCH_MISS_BUDGET_MS: f64 = 3.0;

    /// Startup (core init, warm OS page cache): deserialize the persisted
    /// inverted-index JSON into memory.
    pub const STARTUP_INDEX_LOAD_BUDGET_MS: f64 = 30.0;

    /// Startup: reload the in-memory object cache from `.nabu/*.json` sidecars
    /// and their content files on disk.
    pub const STARTUP_STORAGE_RELOAD_BUDGET_MS: f64 = 80.0;

    /// Startup: load + structure-validate the persisted graph snapshot.
    pub const STARTUP_GRAPH_LOAD_BUDGET_MS: f64 = 50.0;

    /// Startup: aggregate core initialization = index load + storage reload +
    /// graph load. This is the "core initialization benchmark" and is
    /// distinct from the full desktop process cold-start (documented in
    /// BENCHMARKS.md).
    pub const STARTUP_CORE_INIT_BUDGET_MS: f64 = 150.0;

    /// Graph rebuild (in-memory construction only, 500 notes, ~1 000+ wiki-link
    /// edges): the pure parsing + adjacency-build path.
    pub const GRAPH_REBUILD_INMEMORY_BUDGET_MS: f64 = 60.0;

    /// Graph rebuild (full path incl. disk persist to `.nabu/graph/`).
    pub const GRAPH_REBUILD_FULL_BUDGET_MS: f64 = 120.0;
}

// ---------------------------------------------------------------------------
// Corpus generation
// ---------------------------------------------------------------------------

/// Return deterministic outgoing wiki-link targets for note `i`.
///
/// Link density is intentionally varied:
/// - the first `HUB_COUNT` notes form a densely interlinked hub cluster,
/// - 20% of remaining notes are "lone" (no outgoing wiki-links),
/// - the rest carry 1–3 links, always including a reference back to a hub node
///   so high-in-degree nodes exist (multi-reference to common nodes).
fn wiki_link_targets(i: usize, n: usize) -> Vec<String> {
    let mut targets = Vec::new();
    if i < HUB_COUNT {
        for k in 1..=3 {
            let t = (i + k) % HUB_COUNT;
            if t != i {
                targets.push(title_for(t));
            }
        }
    } else if i % 5 == 4 {
        // lone note: no outgoing wiki-links
    } else {
        targets.push(title_for(i % HUB_COUNT)); // reference a hub (common node)
        targets.push(title_for((i + 7) % n)); // a distributed target
        if i % 3 == 0 {
            targets.push(title_for((i + 19) % n)); // extra link for a third of notes
        }
    }
    targets
}

/// Title for note index `i`. Titles are also the wiki-link resolution key.
fn title_for(i: usize) -> String {
    format!("Note {:04}", i)
}

/// Vault-relative path for note index `i`.
fn vault_path_for(i: usize) -> String {
    format!("notes/note-{:04}.md", i)
}

/// Build a realistic Markdown body for note `i`.
///
/// The body contains: a heading, several common sentences drawn deterministically
/// (so common terms recur across notes), a unique body-only distinctive token
/// (`alphaNNNN`, not present in title/tags/type), inline wiki-links, and a
/// block reference. This exercises the real tokenizer and wiki-link parser.
fn build_body(rng: &mut Lcg, i: usize, n: usize) -> String {
    let mut s = String::with_capacity(2048);
    let distinctive = distinctive_token(i);

    s.push_str(&format!("# {}\n\n", title_for(i)));

    // 3-5 common sentences (deterministic selection) -> common terms recur.
    let n_sentences = 3 + rng.next_range(3);
    for _ in 0..n_sentences {
        let idx = rng.next_range(COMMON_SENTENCES.len());
        s.push_str(COMMON_SENTENCES[idx]);
        s.push('\n');
    }

    s.push_str("\n## Summary\n\n");

    // Distinctive body-only sentence -> searchable only via body text.
    s.push_str(&format!(
        "The distinctive identifier {} marks this note uniquely in the vault. ",
        distinctive
    ));
    s.push_str(&format!(
        "Indexing this {} must return exactly this one object. ",
        distinctive
    ));

    s.push_str("\n## Backlinks\n\n");
    // Inline wiki-links.
    let links = wiki_link_targets(i, n);
    for link in &links {
        s.push_str(&format!("Related reading: [[{}]].\n", link));
    }

    // A block reference on some notes (exercises the block-ref parser path).
    if i % 4 == 0 && !links.is_empty() {
        s.push_str(&format!("\nTransclude detail (( {} )) here.\n", links[0]));
    }

    s.push_str("\n## Closing\n\n");
    s.push_str("This note contributes to the connected knowledge graph in Nabu.\n");

    s
}

/// Construct a single deterministic `KnowledgeObject` for index `i`.
pub fn make_note(i: usize, rng: &mut Lcg, n: usize) -> KnowledgeObject {
    let body = build_body(rng, i, n);

    // 2-4 tags from the common pool, deterministic selection.
    let n_tags = 2 + rng.next_range(3);
    let mut tags: Vec<String> = Vec::with_capacity(n_tags);
    let mut used = std::collections::HashSet::new();
    while tags.len() < n_tags {
        let t = rng.next_range(COMMON_TAGS.len());
        if used.insert(t) {
            tags.push(COMMON_TAGS[t].to_string());
        }
    }

    let metadata = ObjectMetadata {
        title: Some(title_for(i)),
        vault_path: Some(vault_path_for(i)),
        description: Some(format!("Benchmark note {}", i)),
        ..Default::default()
    };

    let id = deterministic_id(i);
    let now = fixed_time();

    let mut obj = KnowledgeObject {
        id,
        object_type: ObjectType::Note,
        content: ObjectContent::Markdown(body),
        metadata,
        custom_properties: HashMap::new(),
        tags,
        relations: Vec::new(),
        processing_state: ProcessingState::Completed,
        content_hash: None,
        created_at: now,
        updated_at: now,
    };

    // Sparse explicit relations: every 9th note references hub 0 by id. This
    // exercises the explicit-relation edge path in rebuild_from_objects.
    if i % 9 == 0 && i != 0 {
        obj.relations.push(nabu_core::models::ObjectRelation {
            target_id: deterministic_id(0),
            relation_type: nabu_core::models::RelationType::References,
            label: Some("related".to_string()),
        });
    }

    obj
}

/// Generate the full deterministic search/graph corpus (500 notes by default).
pub fn generate_corpus() -> Vec<KnowledgeObject> {
    generate_corpus_count(CORPUS_NOTE_COUNT)
}

/// Generate a deterministic corpus of exactly `n` notes.
pub fn generate_corpus_count(n: usize) -> Vec<KnowledgeObject> {
    let mut rng = Lcg::new(0x5C4E_u128);
    let mut notes = Vec::with_capacity(n);
    for i in 0..n {
        notes.push(make_note(i, &mut rng, n));
    }
    notes
}

/// Write the corpus to a vault-on-disk layout using `StorageManager`, returning
/// the vault root path. Used by startup + graph-persist benchmarks so that load
/// paths read real on-disk state rather than reconstructed fixtures.
pub fn stage_vault(vault_path: &std::path::Path, corpus: &[KnowledgeObject]) {
    let storage = nabu_core::StorageManager::new(vault_path);
    for obj in corpus {
        storage.save(obj).expect("save fixture object");
    }
}

/// Persist the search index JSON into a vault for `indexer.load()` benchmarks.
pub fn persist_index(vault_path: &std::path::Path, corpus: &[KnowledgeObject]) {
    let indexer = nabu_core::Indexer::with_vault_path(vault_path);
    indexer.reindex(corpus).expect("reindex fixtures");
    indexer.persist().expect("persist index");
}

/// Persist the graph snapshot into a vault for `VaultGraph` load benchmarks.
pub fn persist_graph(vault_path: &std::path::Path, corpus: &[KnowledgeObject]) {
    let graph = nabu_core::VaultGraph::with_persistence(None, vault_path.to_path_buf())
        .expect("vault graph with persistence");
    graph
        .rebuild_from_objects(corpus)
        .expect("rebuild graph fixtures");
}

/// Stage a full vault: storage sidecars + content, persisted index, and
/// persisted graph. Each subsystem is staged independently so its load path
/// can be measured in isolation.
pub fn stage_full_vault(vault_path: &std::path::Path, corpus: &[KnowledgeObject]) {
    stage_vault(vault_path, corpus);
    persist_index(vault_path, corpus);
    persist_graph(vault_path, corpus);
}
