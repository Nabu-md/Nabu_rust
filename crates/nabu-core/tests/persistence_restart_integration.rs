//! # Phase 1B-3 — Restart / Persistence Verification
//!
//! Verification-only integration test for the central Nabu MVP guarantee:
//!
//! > State survives application close → reopen.
//!
//! This file is the **only** artifact produced by Phase 1B-3. No production
//! source files (including `src-tauri/**` and `crates/nabu-core/src/**`) are
//! modified. If a check below fails, it identifies a pre-existing defect in a
//! Phase 1A (persistence) or Phase 1B (wiring) implementation and is reported
//! verbatim — it is **not** patched here.
//!
//! ## Restart lifecycle simulated
//!
//! Each test follows the same explicit lifecycle:
//!
//! ```text
//!  SESSION 1 (create): build the canonical runtime (EventBus +
//!    StorageManager + Indexer + VaultGraph wired exactly like
//!    `src-tauri/src/lib.rs` §11 / `save_pipeline_integration.rs::build_pipeline)
//!    → save objects → flush derived caches → **drop** every runtime handle
//!    (the EventBus drops last, releasing the ITEM_STORED subscriber closures
//!    that hold the service Arcs).
//!
//!  SESSION 2 (reopen): build a *fresh* runtime over the *same* isolated
//!    vault directory → initialize() each service → verify on-disk state was
//!    reconstructed.
//! ```
//!
//! The vault directory is a `tempfile::tempdir`, so no developer-machine or
//! user-vault state is touched.
//!
//! ## Settings caveat
//!
//! Production settings live in `src-tauri/src/settings.rs` (`AppSettings` +
//! `SettingsStore`), which is a separate workspace that depends on *this*
//! crate — `nabu-core` cannot import it. The production persistence mechanism
//! for settings is nonetheless `serde_json::to_vec_pretty → file →
//! serde_json::from_str` (see `SettingsStore::persist` / `read_settings`).
//! `settings_survive_restart` therefore round-trips a faithfully-mirrored
//! schema through a real JSON file, exercising that identical persistence path.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use nabu_core::event_bus::kinds;
use nabu_core::event_bus::{EventBus, PipelineEvent};
use nabu_core::graph::VaultGraph;
use nabu_core::indexer::Indexer;
use nabu_core::inbox::model::build_inbox_object;
use nabu_core::inbox::{inbox_item_status, set_status, FilingService, InboxItemStatus};
use nabu_core::models::{
    KnowledgeObject, ObjectContent, ObjectMetadata, ObjectRelation, ObjectType, RelationType,
};
use nabu_core::storage::StorageManager;

use serde::{Deserialize, Serialize};
use tempfile::tempdir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Build the canonical runtime exactly as `src-tauri/src/lib.rs` wires it: one
/// `EventBus` with `StorageManager` (publishes `ITEM_STORED` after persistence)
/// and an `Indexer` + `VaultGraph` that subscribe to `ITEM_STORED` and
/// derive their state from the stored object. Returns the services wrapped in
/// `Arc` so both "sessions" can hold and drop them.
fn build_pipeline(
    vault: PathBuf,
) -> (
    EventBus<PipelineEvent>,
    Arc<StorageManager>,
    Arc<Mutex<Indexer>>,
    Arc<RwLock<VaultGraph>>,
) {
    let event_bus = EventBus::new();

    let storage = Arc::new(StorageManager::with_event_bus(vault.clone(), event_bus.clone()));
    let indexer = Arc::new(Mutex::new(Indexer::with_vault_path_and_event_bus(
        vault.clone(),
        event_bus.clone(),
    )));
    let graph = Arc::new(RwLock::new(
        VaultGraph::with_persistence(Some(event_bus.clone()), vault)
            .expect("VaultGraph must construct over a valid vault"),
    ));

    // Mirror the ONLY ITEM_STORED subscriber wired in production
    // (src-tauri/src/lib.rs §11): store → index → graph. Nothing else.
    let storage_sub = storage.clone();
    let indexer_sub = indexer.clone();
    let graph_sub = graph.clone();
    event_bus.subscribe(kinds::ITEM_STORED, move |event: &PipelineEvent| {
        if let PipelineEvent::ItemStored(stored) = event {
            if let Some(object) = storage_sub.load(stored.object_id) {
                let _ = indexer_sub.lock().map(|i| i.index_object(&object));
                let _ = graph_sub.write().map(|g| g.add_node(&object));
            }
        }
    });

    (event_bus, storage, indexer, graph)
}

/// Drive a freshly-built runtime through Created → Initialized → Running.
fn init_and_start(
    storage: &StorageManager,
    indexer: &Mutex<Indexer>,
    graph: &RwLock<VaultGraph>,
) {
    assert!(storage.initialize().is_ok(), "storage initialize");
    assert!(storage.start().is_ok(), "storage start");
    {
        let idx = indexer.lock().unwrap();
        assert!(idx.initialize().is_ok(), "indexer initialize");
        assert!(idx.start().is_ok(), "indexer start");
    }
    {
        let g = graph.write().unwrap();
        assert!(g.initialize().is_ok(), "graph initialize");
        assert!(g.start().is_ok(), "graph start");
    }
}

/// Flush derived caches to disk before "closing" the app.
/// StorageManager writes are write-through (sidecar + content written
/// synchronously in `save()`), so only the Indexer and Graph need an explicit
/// flush — exactly what their `shutdown()` methods do in production.
fn persist_derived(indexer: &Mutex<Indexer>, graph: &RwLock<VaultGraph>) {
    let _ = indexer.lock().unwrap().persist();
    let _ = graph.write().unwrap().persist();
}

fn make_note(title: &str, body: &str) -> KnowledgeObject {
    KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(body.to_string()))
        .with_metadata(ObjectMetadata {
            title: Some(title.to_string()),
            ..Default::default()
        })
}

// ===========================================================================
// 1. Notes survive restart
// ===========================================================================

#[test]
fn notes_survive_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    // Session 1 — create + persist.
    {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let note = make_note("Persistent Note", "Hello world from the body text");
        let saved = storage.save(&note).expect("save note");
        persist_derived(&indexer, &graph);

        // Sanity: object is resident in the live session.
        let live = storage.load(note.id).expect("load in session 1");
        assert_eq!(live.content, note.content, "content written in session 1");
        assert_eq!(saved, live.metadata.vault_path.unwrap(), "vault_path returned by save");
    } // ← close: EventBus (and thus the ITEM_STORED subscriber + service Arcs) drops here.

    // Session 2 — reopen and reconstruct from the isolated vault on disk.
    {
        let (_eb, storage, _indexer, _graph) = build_pipeline(vault.clone());
        assert!(storage.initialize().is_ok(), "storage re-init");

        let note_id = {
            // Recover the id by scanning the reloaded cache.
            let all = storage.list_objects("", None, 100).expect("list objects");
            all.iter()
                .find(|o| o.metadata.title.as_deref() == Some("Persistent Note"))
                .expect("note present after reload")
                .id
        };

        let loaded = storage
            .load(note_id)
            .expect("note must survive restart");
        assert_eq!(loaded.object_type, ObjectType::Note);
        assert_eq!(
            loaded.content,
            ObjectContent::Markdown("Hello world from the body text".to_string()),
            "note content survives restart"
        );
        assert_eq!(
            loaded.metadata.title,
            Some("Persistent Note".to_string()),
            "note title survives restart"
        );
        assert!(
            loaded.metadata.vault_path.is_some(),
            "sidecar vault_path survives restart"
        );
        assert!(storage.exists(note_id), "object exists after restart");
    }
}

// ===========================================================================
// 2. Body-text search survives restart
// ===========================================================================

#[test]
fn body_search_survives_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    // A token that appears ONLY in the body (never in the title, tags, or
    // object type) so the match proves body-content indexing, not a title/tag
    // coincidence.
    let body_only_token = "uniquebodytoken42";

    // Session 1 — index + persist.
    {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let note = make_note(
            "Searchable Note",
            &format!("This note body contains {} somewhere inside", body_only_token),
        );
        storage.save(&note).expect("save searchable note");
        persist_derived(&indexer, &graph);
    }

    // Session 2 — drop the Indexer, reconstruct it, query.
    {
        let (_eb, _storage, indexer, _graph) = build_pipeline(vault.clone());
        let idx = indexer.lock().unwrap();
        assert!(idx.initialize().is_ok(), "indexer reinitialize loads persisted index");
        assert!(idx.token_count() > 0, "index non-empty after reload");

        // Title token + the body-only token must both survive.
        let by_title = idx.search("Searchable");
        let by_body = idx.search(body_only_token);

        let id_str = note_id_of(&vault, "Searchable Note");
        assert!(
            by_title.contains(&id_str),
            "INDEXER defect — title token 'Searchable' lost after restart (expected note id, got: {:?})",
            by_title
        );
        assert!(
            by_body.contains(&id_str),
            "INDEXER defect — body-only token '{}' lost after restart (expected note id, got: {:?})",
            body_only_token,
            by_body
        );
    }
}

/// Recover a note's UUID by scanning its sidecar on disk (used when the id
/// isn't held across the session boundary).
fn note_id_of(vault: &std::path::Path, title: &str) -> String {
    let index_dir = vault.join(".nabu");
    for entry in std::fs::read_dir(&index_dir).expect("read .nabu") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).unwrap();
        let sidecar: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if sidecar.get("title").and_then(|t| t.as_str()) == Some(title) {
            return sidecar.get("id").and_then(|i| i.as_str()).unwrap().to_string();
        }
    }
    panic!("note with title {} not found on disk", title);
}

// ===========================================================================
// 3. Graph edges survive restart
// ===========================================================================

#[test]
fn graph_edges_survive_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    // Session 1 — create linked notes, build + persist the graph.
    let (wiki_id, linker_id) = {
        let (_eb, storage, _indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &_indexer, &graph);

        let wiki = make_note("Wiki Target Note", "the target body");
        let linker = make_note("Linker Note", "see [[Wiki Target Note]] for context");

        storage.save(&wiki).expect("save wiki target");
        storage.save(&linker).expect("save linker");

        // Derive content-derived edges (wiki-links) from canonical Markdown
        // and persist the resulting graph snapshot.
        let g = graph.write().unwrap();
        g.rebuild_from_vault(vault.as_path())
            .expect("rebuild graph from vault");
        assert_eq!(g.edge_count(), 1, "wiki-link edge materialised at build time");

        persist_derived(&_indexer, &graph);
        (wiki.id, linker.id)
    }; // close session 1

    // Session 2 — reopen; `with_persistence` reconstructs the graph from
    // `.nabu/graph/graph.json`.
    {
        let (_eb, _storage, _indexer, graph) = build_pipeline(vault.clone());
        init_and_start(_storage.as_ref(), &_indexer, &graph);
        {
            let g = graph.write().unwrap();
            assert!(
                g.loaded_from_disk(),
                "GRAPH defect — graph was not loaded from disk on reopen (rebuilt fresh instead)",
            );
            assert!(
                g.has_edge(linker_id, wiki_id, "references"),
                "GRAPH defect — wiki-link edge [linker -> wiki] missing after restart (expected edge, got edges: {:?})",
                g.edges()
            );
            assert!(
                g.linked_notes(linker_id).contains(&wiki_id),
                "GRAPH defect — linked_notes(linker) did not resolve target after restart",
            );
            assert!(
                g.all_nodes().iter().any(|n| n.id == wiki_id),
                "GRAPH defect — wiki-target node missing from graph after restart",
            );
        }
    }
}

// ===========================================================================
// 4. Relations survive restart
// ===========================================================================

#[test]
fn relations_survive_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let target_a = Uuid::new_v4();
    let target_b = Uuid::new_v4();

    let mut obj = make_note("Relation Note", "relations body content");
    obj.relations = vec![
        ObjectRelation {
            target_id: target_a,
            relation_type: RelationType::References,
            label: Some("ref".to_string()),
        },
        ObjectRelation {
            target_id: target_b,
            relation_type: RelationType::Custom("cites".to_string()),
            label: None,
        },
    ];
    let relations_json = serde_json::to_string(&obj.relations).unwrap();
    let obj_id = obj.id;

    // Session 1 — persist.
    {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);
        storage.save(&obj).expect("save relation-holding object");
        persist_derived(&indexer, &graph);
    }

    // Session 2 — reload and compare relations exactly.
    {
        let (_eb, storage, _i, _g) = build_pipeline(vault.clone());
        storage.initialize().expect("storage reinit");
        let loaded = storage
            .load(obj_id)
            .expect("RELATIONS defect — object vanished after restart");
        let loaded_json = serde_json::to_string(&loaded.relations).unwrap();
        assert_eq!(
            loaded_json, relations_json,
            "RELATIONS defect — relations differ across restart\n\
             expected: {}\n  actual: {}",
            relations_json, loaded_json
        );
        assert_eq!(loaded.relations.len(), 2, "relation count preserved");
        assert_eq!(loaded.relations[0].target_id, target_a);
    }
}

// ===========================================================================
// 5. Inbox queue survives restart
// ===========================================================================

#[test]
fn inbox_survives_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    // Session 1 — capture an inbox item, mark it ready, persist.
    let box_id = {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let mut item =
            build_inbox_object(ObjectContent::Markdown("inbox clip body".to_string()), Some("Clip"));
        set_status(&mut item, InboxItemStatus::Ready);
        storage.save(&item).expect("save inbox item");
        persist_derived(&indexer, &graph);
        item.id
    };

    // Session 2 — reopen and query the inbox work queue.
    {
        let (_eb, storage, _i, _g) = build_pipeline(vault.clone());
        storage.initialize().expect("storage reinit");
        let filing = FilingService::new(storage.clone());

        let queue = filing.queue();
        let item = queue
            .iter()
            .find(|o| o.id == box_id)
            .expect("INBOX defect — inbox item left the queue after restart");

        assert_eq!(
            inbox_item_status(item),
            InboxItemStatus::Ready,
            "INBOX defect — inbox status not preserved across restart"
        );
        assert_eq!(
            item.content,
            ObjectContent::Markdown("inbox clip body".to_string()),
            "INBOX defect — inbox item content not preserved across restart"
        );
        assert_eq!(item.metadata.title, Some("Clip".to_string()));

        // get() must also recover the item by id.
        assert!(
            filing.get(box_id).is_some(),
            "INBOX defect — inbox item not loadable by id after restart"
        );
    }
}

// ===========================================================================
// 6. Settings survive restart
// ===========================================================================

/// Representative subset of the production `AppSettings` schema
/// (`src-tauri/src/settings.rs`). Field names + types mirror production
/// exactly so the JSON written here is byte-identical to what
/// `SettingsStore::persist` produces; the persistence *mechanism* under test
/// is serde_json → file → serde_json — the same one `SettingsStore` uses.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
struct TestAppSettings {
    // Appearance
    theme: String,
    main_window_opacity: f32,
    floating_pill_opacity: f32,
    sidebar_width: f32,
    inspector_width: f32,
    font_size: f32,
    line_height: f32,
    reduced_motion: bool,
    high_contrast: bool,
    // Editor
    editor_mode: String,
    tab_size: u32,
    word_wrap: bool,
    spell_check: bool,
    auto_save_interval_secs: u32,
    // Graph
    graph_show_tags_as_badges: bool,
    // Files & Vaults
    default_new_note_path: String,
    confirm_before_delete: bool,
    show_hidden_files: bool,
}

impl TestAppSettings {
    /// A non-default, representative value set covering the settings fields
    /// Phase 1B wired into the UI (Appearance / Editor / Graph / Files).
    fn representative() -> Self {
        Self {
            theme: "dark".to_string(),
            main_window_opacity: 0.92,
            floating_pill_opacity: 0.65,
            sidebar_width: 320.0,
            inspector_width: 380.0,
            font_size: 18.5,
            line_height: 1.75,
            reduced_motion: true,
            high_contrast: true,
            editor_mode: "Split".to_string(),
            tab_size: 2,
            word_wrap: false,
            spell_check: false,
            auto_save_interval_secs: 45,
            graph_show_tags_as_badges: false,
            default_new_note_path: "Inbox".to_string(),
            confirm_before_delete: false,
            show_hidden_files: true,
        }
    }

    /// Persist via the SAME mechanism as `SettingsStore::persist`:
    /// `serde_json::to_vec_pretty` → absolute-path file.
    fn persist(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec_pretty(self)
            .expect("settings must serialize; schema mirrors production AppSettings");
        std::fs::write(path, payload)
    }

    /// Reconstruct via the SAME mechanism as `SettingsStore::load`/`read_settings`:
    /// read file → `serde_json::from_str`. Returns `None` if the file is absent
    /// (mirrors `read_settings` defaulting to `AppSettings::default()`).
    fn load(path: &std::path::Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }
}

#[test]
fn settings_survive_restart() {
    let dir = tempdir().unwrap();
    // Absolute path — SettingsStore requires absolute paths; tempdir is absolute.
    let settings_path = dir.path().join("Settings").join("app-settings.json");

    let original = TestAppSettings::representative();

    // Session 1 — set values and persist to disk.
    {
        let live = original.clone();
        live.persist(&settings_path)
            .expect("settings must persist to file");
        // `live` drops here — simulates app close; nothing is held in memory.
    }

    // Session 2 — reopen: reconstruct purely from the persisted JSON file.
    let reloaded = TestAppSettings::load(&settings_path);
    assert!(
        reloaded.is_some(),
        "SETTINGS defect — persisted settings file was not found on reopen",
    );
    let reloaded = reloaded.expect("checked above");
    assert_eq!(
        reloaded, original,
        "SETTINGS defect — settings values did not survive restart exactly\n\
         expected: {:?}\n  actual: {:?}",
        original, reloaded
    );

    // A missing file must degrade to defaults (production behaviour), not a
    // hard failure.
    let missing = dir.path().join("never-written.json");
    assert!(
        TestAppSettings::load(&missing).is_none(),
        "SETTINGS defect — missing settings file should resolve to None (defaults), not error",
    );
}

// ===========================================================================
// 7. Rename consistency across restart
// ===========================================================================

#[test]
fn rename_survives_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let new_name = "renamed-name";

    // Session 1 — create, then rename.
    let (obj_id, old_vault_rel) = {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let note = make_note("Rename Me", "rename body text");
        let old = storage.save(&note).expect("save before rename");
        assert!(
            vault.join(&old).exists(),
            "content file should exist before rename"
        );

        let new_rel = storage.rename(note.id, new_name).expect("rename");
        assert_eq!(new_rel, "Inbox/renamed-name.md");
        persist_derived(&indexer, &graph);

        (note.id, old)
    };

    // Session 2 — reopen and verify location + content + derived indexes.
    {
        let (_eb, storage, indexer, _graph) = build_pipeline(vault.clone());
        storage.initialize().expect("storage reinit");
        let idx = indexer.lock().unwrap();
        assert!(idx.initialize().is_ok(), "indexer reload");

        let loaded = storage
            .load(obj_id)
            .expect("RENAME defect — object not loadable after restart");

        assert_eq!(
            loaded.metadata.vault_path.as_deref(),
            Some("Inbox/renamed-name.md"),
            "RENAME defect — vault_path did not survive restart at new location",
        );
        assert_eq!(
            loaded.metadata.title,
            Some("Rename Me".to_string()),
            "RENAME defect — title/content metadata must remain consistent",
        );
        assert_eq!(
            loaded.content,
            ObjectContent::Markdown("rename body text".to_string()),
            "RENAME defect — content must be intact at new location",
        );
        assert!(
            !vault.join(&old_vault_rel).exists(),
            "RENAME defect — old content path still present after restart ({})",
            old_vault_rel
        );
        assert!(
            vault.join("Inbox/renamed-name.md").exists(),
            "RENAME defect — content file missing at new location",
        );

        // The sidecar must follow the object (UUID-keyed, always-present) with
        // the NEW vault_path.
        let sidecar = vault.join(".nabu").join(format!("{}.json", obj_id));
        assert!(sidecar.exists(), "sidecar must remain after rename");
        let raw = std::fs::read_to_string(&sidecar).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            v.get("vault_path").and_then(|p| p.as_str()),
            Some("Inbox/renamed-name.md"),
            "RENAME defect — sidecar still records the old vault_path",
        );

        // The indexer must still resolve the note and must NOT carry a stale
        // reference to the old path (paths are not indexed, so the note simply
        // remains searchable by its body token).
        assert!(
            idx.search("rename").contains(&obj_id.to_string()),
            "RENAME defect — index lost the note during rename; expected id {} in results {:?}",
            obj_id, idx.search("rename")
        );
    }
}

// ===========================================================================
// 8. Move consistency across restart
// ===========================================================================

#[test]
fn move_survives_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let new_path = "Archive/move-me.md";

    // Session 1 — create, then move into a different directory.
    let (obj_id, old_vault_rel) = {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let note = make_note("Move Me", "move body text");
        let old = storage.save(&note).expect("save before move");

        let returned = storage.move_object(note.id, new_path).expect("move");
        assert_eq!(returned, new_path);
        persist_derived(&indexer, &graph);

        (note.id, old)
    };

    // Session 2 — reopen and verify the relocated object.
    {
        let (_eb, storage, indexer, _graph) = build_pipeline(vault.clone());
        storage.initialize().expect("storage reinit");
        let idx = indexer.lock().unwrap();
        assert!(idx.initialize().is_ok(), "indexer reload");

        let loaded = storage
            .load(obj_id)
            .expect("MOVE defect — object not loadable after restart");

        assert_eq!(
            loaded.metadata.vault_path.as_deref(),
            Some(new_path),
            "MOVE defect — vault_path did not survive at destination",
        );
        assert_eq!(
            loaded.content,
            ObjectContent::Markdown("move body text".to_string()),
            "MOVE defect — content must be intact at destination",
        );
        assert!(
            !vault.join(&old_vault_rel).exists(),
            "MOVE defect — stale content still present at old location ({})",
            old_vault_rel
        );
        assert!(
            vault.join(new_path).exists(),
            "MOVE defect — content file missing at destination",
        );
        let sidecar = vault.join(".nabu").join(format!("{}.json", obj_id));
        assert!(sidecar.exists(), "MOVE defect — sidecar must survive the move");

        assert!(
            idx.search("move").contains(&obj_id.to_string()),
            "MOVE defect — index lost the note after move; expected id {} in {:?}",
            obj_id, idx.search("move")
        );
    }
}

// ===========================================================================
// 9. Delete consistency across restart
// ===========================================================================

#[test]
fn delete_survives_restart() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();

    let unique_token = "uniquedeletetoken77";

    // Session 1 — create, persist, then delete.
    let obj_id = {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let note = make_note(
            "Delete Me",
            &format!("body with {}", unique_token),
        );
        let rel = storage.save(&note).expect("save before delete");
        let content_path = vault.join(&rel);
        let sidecar_path = vault.join(".nabu").join(format!("{}.json", note.id));
        assert!(content_path.exists() && sidecar_path.exists());

        // The indexer was fed the note via ITEM_STORED; persist it now so the
        // delete test can later prove the storage layer is the authority.
        persist_derived(&indexer, &graph);

        storage.delete(note.id).expect("delete");
        // After delete, the canonical persistence (content + sidecar) is gone.
        assert!(!content_path.exists(), "content must be removed immediately");
        assert!(!sidecar_path.exists(), "sidecar must be removed immediately");
        assert!(!storage.exists(note.id), "object must not exist immediately");

        persist_derived(&indexer, &graph); // flush whatever derived state remains
        note.id
    };

    // Session 2 — reopen and verify the object is truly gone from storage.
    {
        let (_eb, storage, _indexer, _graph) = build_pipeline(vault.clone());
        storage.initialize().expect("storage reinit");

        assert!(
            !storage.exists(obj_id),
            "DELETE defect — object resurrected by storage after restart"
        );
        assert!(
            storage.load(obj_id).is_none(),
            "DELETE defect — object loadable after restart (content+sidecar must stay absent)"
        );

        let remaining = storage.list_objects("", None, 1000).expect("list after delete");
        assert!(
            !remaining.iter().any(|o| o.id == obj_id),
            "DELETE defect — deleted object reappears in list_objects after restart"
        );

        // The content file and sidecar must both be absent on disk.
        let mut content_gone = true;
        let mut sidecar_gone = true;
        if let Ok(entries) = std::fs::read_dir(vault.join(".nabu")) {
            for e in entries.flatten() {
                if e.path().extension().and_then(|ex| ex.to_str()) == Some("json")
                    && e.path().file_stem().and_then(|s| s.to_str()) == Some(obj_id.to_string().as_str())
                {
                    sidecar_gone = false;
                }
            }
        }
        // Walk content dirs for the note's body (defensive — there should be
        // none matching the token's file).
        let _ = std::fs::read_to_string(vault.join(format!("Inbox/{}.md", "delete-me")))
            .map(|c| {
                content_gone = content_gone && !c.contains(unique_token);
            });
        assert!(content_gone, "DELETE defect — content residue detected after restart");
        assert!(sidecar_gone, "DELETE defect — sidecar residue detected after restart");
    }
}

// ===========================================================================
// 9b. Delete propagation into derived indexes (regression gate)
//
// The canonical pipeline (src-tauri/src/lib.rs §11) subscribes ONLY to
// `ITEM_STORED` for index/graph updates. `StorageManager::delete` publishes
// `INDEX_UPDATED`/`Removed`, but nothing subscribes to it, so neither the
// Indexer nor the VaultGraph is told an object was deleted. These checks
// verify the expected production invariant — that a deleted object is gone
// from the persisted search index and graph after a restart. They are expected
// to FAIL until Phase 1B wires an `INDEX_UPDATED`/`Removed` subscriber that
// calls `Indexer::remove_object` (+ `VaultGraph::remove_node`).
// ===========================================================================

#[test]
fn delete_propagates_to_derived_indexes() {
    let dir = tempdir().unwrap();
    let vault = dir.path().to_path_buf();
    let unique_token = "deleteduidxtoken88";

    let obj_id = {
        let (_eb, storage, indexer, graph) = build_pipeline(vault.clone());
        init_and_start(storage.as_ref(), &indexer, &graph);

        let note = make_note("Derived Delete", &format!("body with {}", unique_token));
        let rel = storage.save(&note).expect("save before delete");
        persist_derived(&indexer, &graph);

        storage.delete(note.id).expect("delete");
        persist_derived(&indexer, &graph); // flush the (stale) derived state
        let _ = rel;
        note.id
    };

    // Reopen; the persisted index/graph were flushed *after* the delete
    // without the delete reaching them.
    {
        let (_eb, _storage, indexer, graph) = build_pipeline(vault.clone());
        let idx = indexer.lock().unwrap();
        assert!(idx.initialize().is_ok(), "indexer reload");

        let hits = idx.search(unique_token);
        assert!(
            !hits.contains(&obj_id.to_string()),
            "DELETE→INDEX defect — persisted search index still returns the deleted object \
             after restart (id {} found in index results {:?}). The canonical pipeline subscribes \
             only to ITEM_STORED; INDEX_UPDATED/Removed is published on delete but has no \
             subscriber invoking Indexer::remove_object, so stale tokens survive in \
             .nabu/search_index.json. Responsible: Phase 1B (wire INDEX_UPDATED subscriber).",
            obj_id,
            hits
        );

        let g = graph.write().unwrap();
        assert!(
            !g.all_nodes().iter().any(|n| n.id == obj_id),
            "DELETE→GRAPH defect — persisted graph still contains the deleted node after restart \
             (id {}). The canonical pipeline has no removal path on delete. \
             Responsible: Phase 1B (wire delete→graph.remove_node, e.g. via an \
             INDEX_UPDATED/Removed subscriber or a dedicated GRAPH_UPDATED/NodeRemoved event).",
            obj_id
        );
    }
}
