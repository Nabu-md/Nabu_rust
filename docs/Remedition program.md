Nabu Final Completion Audit

A read-only forensic audit to answer: **Is Nabu actually complete?**

---

## 1. Executive Verdict

**Nabu is NOT COMPLETE — and it borders on NOT RELEASE-READY.**

The repository contains a genuinely large, well-engineered Rust architecture: a real composition root, a wired EventBus bridged to the frontend, 11 real capture handlers, real macOS FFI (Vision OCR, PDFKit, Whisper), real Markdown+sidecar persistence, and ~1,700 passing tests. **But the product is a shell around a smaller set of actually-connected features.** The most prominent claimed capabilities are either placeholders, disconnected, or silently broken:

- **Full-text search does not search note content and is never persisted.**
- **The relationship graph is never read by the UI** (the UI builds its own separate graph).
- **The Inbox is an inert screen** — its list is never populated.
- **External-edit detection does not exist** (no filesystem watcher at all).
- **Six+ major UI views render hardcoded placeholder divs** while the real implementations are un-compiled Leptos leftovers.
- **The processing pipeline never receives real content** (the job queue discards it), so OCR/PDF/Whisper/classification never actually run on real captures.
- **Backend IPC errors panic the renderer** rather than surface to the user.
- **OCR, Whisper, PDF, screenshot** are macOS-only by design; the app claims Windows/Linux builds but core features are silently no-ops there.

The privacy/local-first claims are **genuinely true** (zero network, zero telemetry, zero external AI). That is the one area that fully passes.

---

## 2. Overall Completion Score

```
Overall completion estimate:      55%
Core functionality:               70%   (notes create/edit/save works)
Backend:                          65%   (services wired; indexer/graph disconnected)
Frontend:                         55%   (many views are placeholders)
Persistence:                      55%   (markdown+sidecar real; rename/move/index/history gaps)
Capture:                          65%   (handlers real; content dropped at queue)
Processing:                       55%   (processors real; never receive content in prod)
Search:                           30%   (no content indexing, no persistence, disconnected)
Graph:                            35%   (disconnected; UI builds own graph; no edges)
Inbox/workflows:                  25%   (screen only; list never populated)
Testing:                          60%   (large suite; but no IPC/UI/wiki-link/behavior tests)
Build/release:                    80%   (compiles, tests pass; fmt/clippy dirty; wasm unverified)
Documentation:                    60%   (extensive; ARCHITECTURE.md & architecture.md stale)
Privacy/local-first compliance:  100%   (verified: no network/telemetry/external AI)
```

---

## 3. Architecture Verification

**Actual architecture** (verified from code, not docs):

```
src-tauri (Tauri v2 shell)
   ├─ commands.rs  (130 #[tauri::command] handlers)
   ├─ lib.rs       (build_application_context: real composition root)
   ├─ event_bridge (EventBus → "nabu-event" → frontend)  [WIRED]
   ├─ history.rs   (reversible FS ops + in-memory HistoryManager)
   ├─ recovery.rs  (versions/session/crash markers)
   └─ native_messaging_socket (Unix socket server)
        ↓  (Rust calls / wasm-bindgen)
nabu-ui (Dioxus 0.6 CSR)
   └─ components/{app,note_editor,file_tree,inbox,settings,templates,
                   recovery,statistics,streaming,activity,layout}  [REAL]
        ↓
nabu-core
   ├─ CaptureEngine (11 real handlers) → DurableJobQueue → WorkerPool
   ├─ PipelineExecutor → ProcessingPipeline (15 processors)
   ├─ StorageManager (Markdown + .nabu/<uuid>.json sidecar)
   ├─ Indexer (in-memory, decorative)
   ├─ VaultGraph (persisted but disconnected)
   ├─ EventBus, JobQueue, Registry, Lifecycle, Capability platform
   ├─ plugin / agent / process_supervisor / sync / diagnostic / streaming
   └─ native/ (vision, whisper, pdfkit, screenshot — macOS FFI)
        ↓
Markdown vault (.md files + .nabu/ metadata)
```

The composition root is **genuinely functional** — services constructed once, initialized, started on launch (`lib.rs:269–274`), gracefully shut down (`lib.rs:518–541`). Not a scaffold.

**No authoritative roadmap file exists.** The specification is dispersed across `README.md`, `CHANGELOG.md`, `docs/architecture/architecture.md`, `docs/architecture/adrs/*`, and `docs/architecture/platform-readiness.md`. I used these as the specification.

---

## 4. Roadmap Completion Matrix

| Promised capability | Status | Evidence |
|---|---|---|
| Setup wizard / vault | **COMPLETE** | `vault_setup_wizard` files are dead Leptos; but `check_vault_exists`/`select_vault_dialog`/`create_vault_dialog` wired | 
| File tree | **COMPLETE** | `tree_list`, real recursive scan |
| Markdown editor + save | **COMPLETE** | `note_save`→`save_note_content`, snapshot, undo wiring |
| Frontmatter/tag parsing | **PARTIAL** | tags extract; **tag editing is disconnected** (right_inspector empty values, no on_change) |
| Full-text search | **DISCONNECTED/BROKEN** | Indexer never persists & never indexes body; UI search is fs substring scan |
| Relationship graph | **DISCONNECTED** | UI `graph_data` builds own fs graph; `VaultGraph` unused; no edges ever |
| Templates | **COMPLETE** | template_* commands wired to TemplateEditor |
| Theme engine | **PARTIAL** | theme_toggle.rs is dead Leptos; theme applied via app.rs/lib.rs loop |
| Clipboard capture | **PARTIAL** | handler real, but content dropped at queue |
| Screenshot ingestion | **DISCONNECTED** | handler real (screencapture), but image bytes never reach pipeline |
| File drop | **PARTIAL** | `capture_file_drop` → CaptureEngine, but content dropped |
| Folder watch | **ABSENT** | no `notify`, no watcher; `WatchFolderHandler` has no loop |
| Browser/Safari extension | **PARTIAL** | native host + socket real (macOS); Safari ext is bare MV2 tree, no Xcode project, field-name mismatch |
| Dictation pill (Whisper) | **PARTIAL** | pill UI + `toggle_dictation_pill` real; `start_dictation`/`stop_dictation` are **placeholder strings**; whisper macOS-only, bytes dropped |
| Snapshot history / versions | **COMPLETE** | versions_* commands fully wired + tested |
| Session recovery / crash detection | **COMPLETE** | recovery_check/discard, session_* wired |
| Undo/Redo | **PARTIAL** | reversible in-session; **in-memory (no restart survival); content edits not covered** |
| Plugin foundation | **SCAFFOLD** | "no third-party loading implemented" (explicitly documented) |
| Process supervision | **SCAFFOLD/PARTIAL** | real code, no user-facing surface |
| Sync | **SCAFFOLD** | models only; "provider-agnostic" — no implementation |
| Diagnostics pipeline | **SCAFFOLD/PARTIAL** | backend `diagnostic_requested` is **DEAD** (never invoked by frontend) |
| Live event bus bridge | **COMPLETE** | `nabu-event` wired end-to-end |
| Capability registry | **PARTIAL** | `capability_list_with_state` wired; enable/disable commands dead |
| Health & metrics | **PARTIAL** | `metrics` wired; `health_check` dead; **`pool_health` BROKEN** (not registered) |
| Graceful shutdown | **COMPLETE** | `on_exit` persists index (fails harmlessly) + shutdown |
| macOS Vision OCR | **DISCONNECTED** | real FFI, but unreachable (queue drops image bytes) |
| PDF annotation | **DISCONNECTED** | real PDFKit, unreachable; `pdf_viewer.rs` is dead Leptos |
| Whisper dictation | **DISCONNECTED** | real binding, unreachable |
| Dashboard / Home / Search / Calendar / Archive / Smart Folders | **PLACEHOLDER** | hardcoded text, no real data |
| Graph / Trash / Canvas / Reader / Comparison / Reading Queue / Collections views | **PLACEHOLDER divs** | real impls are un-compiled Leptos |

---

## 5. Stub / Placeholder Inventory

| File | Symbol/Location | What exists | What's missing | Severity | Status |
|---|---|---|---|---|---|
| `src-tauri/src/commands.rs:570,575` | `start_dictation`/`stop_dictation` | return `Ok("Dictation started")` | real dictation | 🔴 | PLACEHOLDER (wired) |
| `src-tauri/src/commands.rs:3670` | `pool_health` | defined but **not registered** | registered in invoke_handler | 🔴 | BROKEN |
| `crates/nabu-core/src/indexer.rs:386-408` | `tokenize_object` | title/desc/tags/type only | **never reads note body** | 🔴 | PARTIAL |
| `crates/nabu-core/src/indexer.rs:16,220,236` | `persist`/`load` | implemented | **no vault path at runtime** (`lib.rs:209`) | 🔴 | DISCONNECTED |
| `crates/nabu-core/src/pipeline_migration/executor.rs:100-137` | `object_from_job` | reconstructs shell object | **never loads content/bytes from storage** | 🔴 | DISCONNECTED |
| `crates/nabu-core/src/capture/engine.rs:89-95` | job payload | id/type/source/title/source_url only | **no content, no binary bytes** | 🔴 | GAP |
| `crates/nabu-core/src/processing/processors/embedding_generator.rs:56` | `process` | hardcoded `vec![0.1..0.8]` | real embedding | 🟠 | MOCK |
| `crates/nabu-ui/src/components/inbox.rs:288` | `inbox_subscribe` | `let _ = tauri_invoke(...)` | **result discarded; items never populated** | 🔴 | BROKEN |
| `crates/nabu-core/src/processing/processors/duplicate_detector.rs:13-14` | `seed_hashes` | in-memory `HashSet` | never seeded from storage | 🟠 | PARTIAL |
| `crates/nabu-ui/src/components/app.rs:173-249` | Graph/ReadingQueue/Trash/Canvas/Reader/Comparison | placeholder divs | real views | 🔴 | PLACEHOLDER |
| `crates/nabu-ui/src/components/navigation/{dashboard,home_screen,search_page,calendar_page,archive_page,smart_folders}.rs` | navigation views | hardcoded empty-state text | real data/widgets | 🟠 | PLACEHOLDER |
| `crates/nabu-ui/src/components/layout/right_inspector.rs:88,110-146` | tags/backlinks/outgoing/mentions | empty values, `on_change: None`, empty-state text | real data + wiring | 🟠 | DISCONNECTED |
| `crates/nabu-ui/src/components/app.rs:139` | VaultSetup "Open Settings" | `onclick: move |_| {}` | action | 🟢 | DEAD |
| `src-tauri/src/history.rs:341,449,935` | rename/move/delete | raw `std::fs::rename`, no StorageManager call | sidecar/index/graph update | 🔴 | STALE |
| `crates/nabu-core/src/storage/manager.rs:440` | `sidecar_to_object` | hardcodes `relations: vec![]` | relation round-trip | 🟠 | DATA-LOSS |
| `crates/nabu-core/src/storage/manager.rs:400-412` | binary read | falls through to Markdown, reads empty content file | `.bin` read path | 🟠 | BROKEN |
| `crates/nabu-core/src/processing/processors/metadata_extractor.rs:154-158` | `detect_language` | returns `None` | language detection | 🟢 | PARTIAL |

**Note:** the six+ `navigation/*` placeholders are documented as intentional "Phase 0.3 structural placeholder — implementation arrives in a later phase." So they are *intentional deferred* rather than accidental — but they are still unfinished product surfaces that the roadmap/README does not clearly flag.

---

## 6. Backend Audit

- **Composition root: COMPLETE.** All services constructed/registered/started/shutdown correctly (`lib.rs`).
- **Event bridge: COMPLETE** (backend `nabu-event` ↔ frontend listener).
- **History: PARTIAL** — reversible in-session; in-memory only; content edits not undoable.
- **Recovery/versions/settings/templates: COMPLETE.**
- **~57 dead commands** registered but never called by frontend: full inbox batch/queue API, smart folders, threads, plugin/capability runtime controls, diagnostics, platform integrations (notification/desktop-entry/taskbar), settings import/export/reset.
- **1 broken command** (`pool_health` not registered).
- **2 placeholder commands** (`start_dictation`/`stop_dictation`).

---

## 7. Frontend Audit

**Real & functional:** NoteEditor, FileTree, Settings (15 tabs), Templates, VersionHistory, Recovery, Statistics, Activity, Streaming, Inbox (shell).

**Placeholder:** Dashboard, HomeScreen, Search, Calendar, Archive, SmartFolders, Graph, Trash, Canvas, Reader, Comparison, ReadingQueue.

**Dead/un-compiled Leptos files** (21 files, not in module tree — the crate compiles, so they are never built):
`tree.rs`, `graph_view.rs`, `sandboxed_html.rs`, `canvas.rs`, `theme_toggle.rs`, `comparison.rs`, `reader.rs`, `sandbox.rs`, `trash.rs`, `vault_setup_wizard.rs`, `reading_queue.rs`, `workspace.rs`, `relation_editor.rs`, `collections/*` (board/calendar/table/gallery/view_switcher/container/shared), `pdf_viewer.rs`.

**Dead control:** VaultSetup "Open Settings" empty onclick (`app.rs:139`); right-inspector tags editing decorative.

---

## 8. Persistence Audit

| Feature | Survives restart? | Status |
|---|---|---|
| Note `.md` content | ✅ Yes | REAL |
| Object metadata sidecar (`.nabu/*.json`) | ✅ Yes | REAL |
| **Relations** in sidecar | ❌ **No** (dropped on reload) | DATA-LOSS |
| **Binary content** (image/audio/PDF) | ❌ **No** (`.bin` never read back) | BROKEN |
| **Search index** | ❌ **No** (no vault path at runtime) | DISCONNECTED |
| Graph snapshot | ✅ Yes | REAL (but not consumed by UI) |
| **Undo/Redo history** | ❌ **No** (in-memory) | PARTIAL |
| **Rename/Move/Delete** consistency | ❌ **No** (bypasses StorageManager → stale sidecars/index/graph) | BROKEN |
| Inbox queue | ✅ Yes (backend) | REAL, but UI never displays |
| Settings | ✅ Yes | REAL |

---

## 9. Capture Audit

All 11 handlers do real normalization work (verified). **But the chain breaks immediately after capture**: `engine.ingest` enqueues a payload with only `id/type/source/title/source_url` (`engine.rs:89-95`), discarding content and binary bytes. The executor rebuilds a PlainText shell. So:

- Browser/article/clipboard **text content never reaches classification/metadata/summarisation.**
- **Image/audio/PDF bytes never reach OCR/Whisper/PDFKit.**
- Only the object's title reaches the pipeline. **The capture → processing chain is architecturally present but functionally severed.**

---

## 10. Processing Audit

- Infrastructure (`ProcessingPipeline::run`): **REAL**.
- **REAL processors:** auto_filer, harper_processor, harper_conversion, metadata_enricher, metadata_extractor, ocr_processor, pdf_annotation/metadata/text_processor, timeline_extractor, whisper_processor.
- **PARTIAL:** ai_summariser (extractive, no AI), content_classifier (heuristics), duplicate_detector (in-memory only, never seeded), semantic_enricher (heuristics, consumes fake embeddings).
- **MOCK:** embedding_generator (hardcoded vector).
- **Critical:** most processors can never run on real content in the production path due to the queue content-drop.

---

## 11. Search Audit

**BROKEN as a product feature.** The `Indexer`:
- does not index note body (`tokenize_object` reads only title/description/tags/type),
- is instantiated without a vault path (`lib.rs:209`) so `persist()` errors and it never loads on restart,
- is **not consumed by any search feature** — `notes_search` (commands.rs:1578) does a linear substring scan of `.md` files and never touches the Indexer.

So the "in-memory index with relevance ranking" claim is false in practice; search is a plain grep. `search_index_on_startup` setting is dead.

---

## 12. Graph Audit

**DISCONNECTED.** `VaultGraph` is a real persisted structure, but:
- nodes are added only for objects saved *during the session* (no startup rebuild from existing vault),
- **`add_edge` is never called in production** (relations dropped on reload, no wiki-link parsing in core),
- **the UI never reads VaultGraph** — `graph_data` (commands.rs:1926) builds a separate graph by scanning `.md` files and extracting `[[...]]` on every call.

The incremental graph engine and `wire_incremental_graph_updates` are referenced only in tests.

---

## 13. Inbox / Workflow Audit

**Screen, not workflow.** The backend queue is real and persisted, but:
- the frontend discards the `inbox_subscribe` result (`inbox.rs:288`) — **the list is never populated, ever**;
- `inbox_approve` only flips an `inbox_status` property; it **does not create a vault note, move anything, or make it searchable/editable**;
- `inbox_move` only sets a `destination_folder` string; `inbox_retry` only resets status (no reprocessing).

Net: the Inbox always shows "Inbox is empty." Undo/redo works in-session; content edits aren't covered; history is lost on restart.

---

## 14. IPC / Wiring Audit

- **61 commands** registered + invoked (real work).
- **1 BROKEN:** `pool_health` (frontend `statistics.rs:138`) not registered → rejects.
- **~57 DEAD:** backend commands never called by frontend.
- **2 PLACEHOLDER:** `start_dictation`/`stop_dictation`.
- **No backend command is unit/integration tested.**

**Frontend ↔ backend state inconsistency:** the primary note reads (`note_read`, `tree_list`, `notes_index`) read the filesystem directly, bypassing StorageManager, while writes go through StorageManager — so in-memory caches and the Indexer/graph are kept out of sync with the actual read path.

---

## 15. Markdown Integrity Audit

**Markdown remains canonical** — good. Content round-trips through save/close/reopen (verified by `test_save_and_load_from_disk`). **But:**
- metadata relationships are lost (relations dropped on reload);
- binary objects are written but never read back;
- rename/move/delete leave stale sidecars (the sidecar's vault-relative path is not updated);
- **external edits are not detected** (no watcher), so "edits made outside Nabu propagate" is false;
- frontmatter preservation on save is not proven end-to-end (no test).

User *text* data survives intact; the surrounding knowledge-graph/index state does not.

---

## 16. Filesystem Watcher Audit

**ABSENT.** No `notify`/`fs::watch`/watcher exists anywhere. `WatchFolderHandler` has no polling loop (it only transforms an incoming request); `SyncFolder::Continuous` is a comment. The README's "Folder watch," "External edit detection," and legacy "chokidar" claims are **not implemented.** No external-change path, no self-generated-event handling, no rapid-change debounce — because there is no watcher.

---

## 17. Privacy / Local-First Audit

**PASS — genuinely verified:**
- Zero outbound HTTP in source (`reqwest`/`hyper` only transitive in lockfiles, unused).
- Only network transport = local Unix sockets.
- Zero telemetry/analytics/crash SDKs (`analytics_enabled` is an inert flag).
- No CDN/external assets; no CSP grant, empty permissions; updater disabled.
- README claims are **accurate.**

**Caveats:** documented optional model downloads (whisper/BGE) have **no download code/script in the repo** — aspirational. Model downloads also contradict "no network" only if user-initiated, so acceptable per claim.

---

## 18. AI Boundary Audit

**PASS.** The core knowledge base has no AI dependency. The three "AI" processors are non-AI: embedding_generator = hardcoded fake vector; ai_summariser = extractive; semantic_enricher = heuristics. No OpenAI/Anthropic/API-key/HTTP code exists. App works fully offline and without AI. **However**, this means the claimed "AI summarisation / semantic enrichment / embeddings" features are not real — they're stubs/heuristics mislabeled as AI in the README.

---

## 19. Error Handling Audit

**Production blocker found:**
- `crates/nabu-ui/src/ipc.rs:18` — `tauri_invoke` `.unwrap()`s the promise. Any backend command returning `Err` **panics the renderer** instead of surfacing an error. All 103 call sites are affected; the pervasive `if ...is_err() { toast }` patterns are dead code for genuine errors.

Other issues:
- Batch inbox/queue handlers loop, `eprintln!`, and return `Ok(())` — a fully-failed batch reports success (`commands.rs:1155-1205, 1422-1433`).
- `let _ = push_history` in archive/archive_restore (`commands.rs:2534,2583`).
- Trash delete counts `.is_ok()` successes silently.
- Lock-poison `expect(...)` guards are panic sources.

---

## 20. Test Audit

- **~1,748 test functions; all pass** (`cargo test --workspace` green). Doc-tests: 18 pass / 16 ignored.
- **Genuinely meaningful:** disk persistence & restart (storage, indexer, graph, conversations, save pipeline), real content search via Indexer, end-to-end note-save → store → index → graph → event pipeline, JSON-RPC transport.
- **Zero meaningful coverage:**
  - every Tauri command / IPC handler (`src-tauri/src/commands.rs`) — only serialization helpers tested;
  - every Dioxus component — only pure-logic helpers tested, no DOM/render tests;
  - wiki-link → graph edges (feature absent in core);
  - real AI processor inference (all graceful-noop shells);
  - OCR on non-macOS (never validated); pipeline.rs orchestration (0 tests).
- Fixtures `ocr_fixture.png` / `pdf_fixture.pdf` are real and used, but OCR assertions no-op off macOS.

---

## 21. Build / Release Audit

| Check | Result |
|---|---|
| `cargo check --workspace` | ✅ pass |
| `cargo check` (nabu-ui) | ✅ pass (15 warnings) |
| `cargo test --workspace` | ✅ pass (~1,748 tests) |
| `cargo test` (nabu-ui) | ✅ 93 pass |
| `cargo clippy --workspace` | ⚠️ 0 errors; **39 warnings (core), 15 (ui)**; 29 auto-fix suggestions |
| `cargo fmt --check` | ❌ **not clean** (diffs throughout) |
| WASM build | ⚠️ not executed (target installed); host check passes |
| Tauri prod build | ⚠️ not executed |

No compile errors; but the codebase is not `fmt`-clean and carries clippy warnings — the "polish / release-ready" bar isn't met. Windows/Linux builds are not verified; several features are macOS-gated by design.

---

## 22. Dead Code / Migration Audit

- **21 un-compiled Leptos files** in `nabu-ui` (graph_view, trash, canvas, reader, comparison, reading_queue, workspace, sandbox, sandboxed_html, theme_toggle, vault_setup_wizard, relation_editor, collections/*, pdf_viewer, tree) — a large unfinished Dioxus migration.
- `menu.rs.bak` committed to the repo.
- `~57` dead backend IPC commands.
- `incremental_graph` engine used only in tests.
- `search_index_on_startup` dead setting; `detect_language` stub.
- Dead `inspect_placeholder` and empty-onclick in UI.
- **ARCHITECTURE.md is entirely stale** (describes the legacy Electron/React app); `docs/architecture/architecture.md:247` still says "Leptos-based UI."

---

## 23. Documentation-vs-Reality Audit

### Marketing/Documentation Claims That Are Not Yet True
- **"Full-text search — in-memory index with relevance ranking"** → Indexer doesn't index content and is disconnected.
- **"Relationship graph — interactive canvas of your vault's link structure"** → the graph view is a placeholder; `VaultGraph` isn't consumed by UI; no edges are ever built.
- **"Folder watch / external edit detection / hot-reload"** → no watcher exists.
- **"Clipboard/Screenshot/File-drop capture"** → handlers exist but content never reaches processing.
- **"macOS Vision OCR ... automatic text extraction on file add"** → unreachable in production path.
- **"PDF annotation ... highlight-to-note"** → unreachable; viewer is dead Leptos.
- **"Whisper dictation"** → `start_dictation` returns a placeholder string.
- **"Full Inbox / Knowledge Inbox workflow"** → screen-only, list never populated.
- **"AI summarisation / semantic enrichment / embeddings"** → stubs/heuristics, not AI.
- **"Windows 10+ / Linux AppImage"** → core native features are macOS-only; Windows build unverified and Unix-only socket code won't compile on Windows.
- **"~60+ Tauri invoke handlers"** → ~57 of ~130 are never invoked.

---

## 24. Adversarial "Looks Complete But Isn't" Audit

Confirmed by direct file inspection:
- Impressive architecture with missing execution paths — **processing pipeline is never fed content.**
- APIs that exist but are never called — **Indexer never consumed; ~57 dead commands; incremental graph engine test-only.**
- UI that exists but isn't wired — **Inbox subscribe discarded; right-inspector tags/backlinks/mentions empty.**
- Backend services instantiated but not used by any command — **indexer, vault_graph never retrieved from context by any command.**
- Code returning plausible fake values — **embedding_generator hardcoded vector.**
- Real implementations behind un-compiled files — **all the Leptos views.**
- Persistence gaps — **rename/move/delete leave stale state; history in-memory; index not persisted.**
- Restart failures — **index empty, relations lost, binary lost, graph node-less.**
- External-change failures — **no watcher.**
- Features implemented for one platform only — **OCR/PDF/Whisper/screenshot macOS-only, silently no-op elsewhere.**
- Commands returning success without work — **start_dictation/stop_dictation; batch handlers.**

---

## 25. User Journey Audit

- **A — New vault:** ✅ works (`check_vault_exists` / `select_vault_dialog` / `create_vault_dialog`).
- **B — Create knowledge:** ⚠️ note saves to `.md` correctly, but search can't find content, graph shows nothing meaningful, metadata relations lost.
- **C — External edit:** ❌ **not detected** (no watcher).
- **D — Capture:** ❌ **breaks** — content dropped at queue; inbox never populated; OCR/whisper unreachable.
- **E — Restart:** ⚠️ notes/settings survive; **search index, graph edges, history, relations, binary do not.**
- **F — Failure:** ❌ **IPC errors panic the renderer**; batch failures report success; no useful feedback.

---

## 26. Exact Remaining Work (prioritized)

| # | What's missing | Why it matters | Location | Current state | Blocks release? | Complexity |
|---|---|---|---|---|---|---|
| 1 | **Feed real content through the pipeline** | All processing/OCR/PDF/whisper is dead in prod | `engine.rs:89-95`, `executor.rs:100-137` | job payload omits content; executor builds empty shell | **Yes** | medium |
| 2 | **Make Indexer real: persist, index note body, wire to search** | Search is a grep; not persistent | `indexer.rs`, `lib.rs:209`, `commands.rs:1578` | no vault path; no content; unused | **Yes** | medium |
| 3 | **Wire the Graph: feed edges, rebuild on startup, consume in UI** | Relationship graph is fake | `VaultGraph`, `commands.rs:1926` | no edges; UI builds own fs graph | **Yes** | medium |
| 4 | **Fix Inbox: populate list, make approve file a real vault note + index** | Inbox is an inert screen | `inbox.rs:288`, `commands.rs:1043` | result discarded; approve = status flag | **Yes** | medium |
| 5 | **Add a filesystem watcher** | External edits/renames/deletes untracked | repo-wide (none) | absent | **Yes** | medium-large |
| 6 | **Surface IPC errors instead of panicking** | Errors crash renderer; feedback dead | `ipc.rs:18` | `.unwrap()` on 103 sites | **Yes** | small-medium |
| 7 | **Fix rename/move/delete to route through StorageManager** | Stale sidecars/index/graph after ops | `history.rs:341,449,935` | raw fs ops | **Yes** | medium |
| 8 | **Replace placeholder views or wire real ones** | Graph/Trash/Canvas/Reader/Comparison/ReadingQueue/Collections/Dashboard/Home/Search/Calendar/Archive/SmartFolders | `app.rs:173-249`, `navigation/*`, 21 dead Leptos files | placeholder divs / un-compiled files | **Yes** (product promise) | large |
| 9 | **Implement real dictation or remove claim** | Dictation broken | `commands.rs:570-575` | placeholder string | Yes (claimed feature) | medium |
| 10 | **Make search actually search (ties to #2)** | see #2 | — | — | — | — |
| 11 | **Fix binary object round-trip + relation persistence** | images/PDFs unreadable; relations lost | `manager.rs:400-412,440` | broken | Yes | small |
| 12 | **Implement or remove "AI" features (embeddings/summarise/enrich)** | Mislabeled stubs | `embedding_generator.rs`, `ai_summariser.rs`, `semantic_enricher.rs` | fake/heuristic | No (not core) | medium |
| 13 | **Test the IPC layer + Dioxus components** | No coverage of the actual product surface | `src-tauri/src/commands.rs`, `nabu-ui` | none | No | large |
| 14 | **Update stale docs** | ARCHITECTURE.md/architecture.md/README misleading | docs | stale | No | small |
| 15 | **Remove dead Leptos files + dead commands + menu.rs.bak** | Cleanup; hides true state | nabu-ui, commands.rs | dead | No | small |
| 16 | **Cross-platform: Windows/Linux builds; macOS-gated features** | Windows/Linux claims unverifiable | native/*, socket code | macOS-only | Yes for those platforms | large |
| 17 | **`cargo fmt` clean + clippy warnings** | Release polish | repo-wide | dirty | No | small |

---

## 27. Final Verdict

> ### **NOT COMPLETE** (bordering on **NOT RELEASE-READY**)

Nabu is a large, well-structured, genuinely local-first **foundation** with a real, working composition root, a functional Markdown editor/save/version/settings core, real macOS FFI engines, and a green test suite. But the product as described by its README and architecture docs is **not delivered**:

- Search does not search content and doesn't persist.
- The graph is disconnected (no edges, not consumed by UI).
- The Inbox is an inert screen.
- External-edit detection does not exist.
- Most of the headline views are placeholders backed by un-compiled legacy code.
- The entire capture→processing→OCR/PDF/Whisper chain is severed at the queue boundary.
- IPC errors panic the renderer.

These are **not** merely "cleanup opportunities" — they are promised, user-facing capabilities that do not work end-to-end and do not survive real-world usage. **Nabu is not finished.**

The one area that fully passes is **privacy/local-first compliance** — the core promise of "runs on your machine, no telemetry, no network" is genuinely true and verified in code. That is a real strength.

For the product to honestly be called complete, the items in **§26 #1–#8** (and ideally #11) must be implemented; they represent the difference between a well-architected codebase and a usable knowledge base.


--- ACTION PLAN
Parallelism contract (applies to every phase)

For subphases in the same phase to truly run in parallel without colliding:

- **Disjoint file ownership.** Each subphase lists the exact files it owns and a "do not touch" list. No two subphases in the same phase edit the same file.
- **Stable public API.** Backend/UI subphases build capabilities behind stable interfaces; the phase's **integration subphase** is the *only* owner of shared wiring files (`src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `crates/nabu-ui/src/components/app.rs`, `components/mod.rs`).
- **Dependency coordination.** Only the subphase marked "owns Cargo.toml deps" adds new crates that phase; siblings avoid Cargo.toml.
- **Verify per subphase.** Each subphase ends with `cargo check -p nabu-core` / `cargo check` (in `crates/nabu-ui`) and its targeted tests. The phase's verification subphase runs the full `cargo test --workspace`.
- **Phases run sequentially; subphases within a phase run in parallel.**

---

# SECTION 1 — MVP Remediation
*Make the core product actually function end-to-end. Nothing ships without this.*

## Phase 1A — Backend core correctness (parallel)

> All subphases live in `crates/nabu-core`. Public service APIs must remain stable so Phase 1B can wire them.

### 1A-1 · Search Index — make it real
- **What:** Make `Indexer` tokenize the note **body** (not just title/tags/type), persist to `.nabu/search_index.json` (add/update/remove/reindex), load on startup, and expose a query API. Delete stale-index handling.
- **Why:** Search currently never matches note content and is lost on restart.
- **Owns:** `crates/nabu-core/src/indexer.rs` (+ its tests).
- **Do not touch:** `commands.rs`, `lib.rs`, `settings.rs`, other modules.
- **Verify:** `cargo test -p nabu-core indexer`; add a test proving body-text search survives a restart (write→drop→reload→query).
- **Complexity:** medium.

### 1A-2 · Relationship Graph — wire real edges
- **What:** Add real edge production: parse `[[wiki-links]]` and block refs from note content, build edges on save, rebuild from existing vault at startup, handle delete/rename (remove stale nodes/edges), expose a query API. Remove the dead-code gap where `add_edge` is never called.
- **Why:** The graph is currently node-only, never read by the UI, and goes stale on rename/delete.
- **Owns:** `crates/nabu-core/src/graph/**` (+ tests). New `wikilink` parser lives inside `graph/`.
- **Do not touch:** `commands.rs`, `lib.rs`, `storage/manager.rs`, `indexer.rs`.
- **Verify:** tests proving content-to-edge extraction and that edges survive restart.
- **Complexity:** medium.

### 1A-3 · Pipeline content pass-through
- **What:** Carry real content + binary bytes through the job queue to the executor so OCR/PDF/Whisper/classifier/metadata actually run on real captures. Persist captured bytes at ingest so the executor can rehydrate; remove the empty-shell reconstruction in `object_from_job`.
- **Why:** The capture→processing→OCR chain is currently severed — processors never receive real data.
- **Owns:** `crates/nabu-core/src/capture/engine.rs`, `pipeline_migration/executor.rs`, `jobs/**`, and the affected `processing/processors/*`.
- **Do not touch:** `indexer.rs`, `graph/**`, `storage/manager.rs`, `commands.rs`, `lib.rs`.
- **Verify:** a test proving an image capture reaches `ocr_processor` with bytes; a text capture reaches `content_classifier` with content.
- **Complexity:** medium–large.

### 1A-4 · Storage correctness
- **What:** Add `StorageManager` methods for **rename / move / delete** that update content + sidecar + fire events; fix **binary round-trip** (`.bin` read path); persist **relations / content_hash / processing_state** in the sidecar so they survive reload.
- **Why:** Rename/move/delete bypass storage (leaving stale caches), binaries are written but never read back, and relations are dropped on restart.
- **Owns:** `crates/nabu-core/src/storage/manager.rs` (+ tests).
- **Do not touch:** `indexer.rs`, `graph/**`, `history.rs`, `commands.rs`, `lib.rs`.
- **Verify:** tests for rename→restart→reload, binary load, and relation round-trip.
- **Complexity:** medium.

### 1A-5 · Inbox workflow (service + UI)
- **What:** Populate the inbox list on load and refresh after actions; make **approve file a real vault Markdown note** (write `.md`, fire `ITEM_STORED`, get indexed/searchable); keep reject/delete/retry meaningful; support undo. Fix the discarded `inbox_subscribe` call.
- **Why:** The Inbox is currently an inert screen that always shows "empty"; approve only flips a status flag.
- **Owns:** `crates/nabu-ui/src/components/inbox.rs`, new `crates/nabu-core/src/inbox/**` (filing service).
- **Do not touch:** `indexer.rs`, `storage/manager.rs`, `commands.rs`, `lib.rs`, other frontend components.
- **Verify:** UI component test (list populates) + core test (approve → `.md` created + indexed).
- **Complexity:** medium.

### 1A-6 · Filesystem watcher
- **What:** Add a `notify`-based watcher (create/modify/delete/rename/move) with debounce and self-event filtering; on change, update index + graph and notify UI. Handle rapid/duplicate events and conflict/self-generated suppression.
- **Why:** External edits/renames/deletes are never detected — a core local-first promise is absent.
- **Owns:** new `crates/nabu-core/src/watcher/**`; **Cargo.toml deps owner** for this phase (add `notify`).
- **Do not touch:** `indexer.rs`, `graph/**`, `storage/manager.rs`, `commands.rs`, `lib.rs`.
- **Verify:** integration test: create/edit/delete a file outside the app → event emitted → index/graph updated.
- **Complexity:** medium–large.

## Phase 1B — Integration & frontend resilience (parallel)

> Depends on Phase 1A services. One subphase owns all shared wiring files.

### 1B-1 · Backend integration & IPC (owns shared wiring)
- **What:** Re-point `notes_search`/`notes_index` → real Indexer; `graph_data` → real VaultGraph; implement inbox approve→file via the inbox service; route rename/move/delete through `StorageManager`; register `pool_health`; wire the watcher, indexer, and graph into the composition root; fix batch handlers that swallow errors and return `Ok(())`.
- **Why:** This is the only subphase that closes the wiring gaps in the product surface.
- **Owns:** `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/history.rs`, `src-tauri/src/settings.rs`.
- **Do not touch:** `crates/nabu-core/src/**`, `crates/nabu-ui/src/**` (except no changes).
- **Verify:** `cargo check --workspace` + a smoke test that search returns body matches and graph shows edges.
- **Complexity:** large (largest subphase).

### 1B-2 · Frontend IPC error handling
- **What:** Make `tauri_invoke` return a `Result` and surface backend errors as toasts/empty states instead of `.unwrap()` panics; convert the 103 raw call sites to the safe path; make batch/operation failures visible.
- **Why:** Every backend error currently panics the renderer and the existing `is_err()` toast checks are dead code.
- **Owns:** `crates/nabu-ui/src/ipc.rs` and the UI call sites.
- **Do not touch:** `inbox.rs` (owned by 1A-5), other components.
- **Verify:** `cargo check` in `crates/nabu-ui`; unit test that a rejected invoke produces an error, not a panic.
- **Complexity:** small–medium.

### 1B-3 · Restart / persistence verification
- **What:** Write an integration test spanning close→reopen: notes, search index, graph edges, relations, inbox queue, settings all survive; rename/move/delete stay consistent.
- **Why:** The central MVP guarantee is "state survives restart" and it is currently broken in several subsystems.
- **Owns:** a new test file only (e.g. `crates/nabu-core/tests/persistence_restart_integration.rs`).
- **Do not touch:** source files.
- **Verify:** `cargo test` for the new file; report failures to 1A/1B owners.
- **Complexity:** medium.

### 1B-4 · Loading / empty / failure states
- **What:** Add real loading, empty, and error states to the now-functional views (search, graph, statistics, versions, recovery, note editor) so the UI reflects reality instead of showing blank or fake content.
- **Why:** Several screens render static/empty content without indicating loading or failure.
- **Owns:** frontend state/UX in views not owned by others (search, graph, statistics, versions, recovery, note_editor).
- **Do not touch:** `inbox.rs`, `ipc.rs`.
- **Verify:** `cargo check` in `crates/nabu-ui`.
- **Complexity:** small.

---

# SECTION 2 — Remediation of Promised Features
*Ship the placeholders and real integrations that the README already advertises.*

## Phase 2A — Ship the real views (parallel)

> Each subphase converts an un-compiled Leptos file (or builds fresh) into a Dioxus component placed under `components/shipped/<name>.rs`. **No subphase touches `app.rs` or `components/mod.rs`** — 2A-7 owns routing.

### 2A-1 · Graph view (real canvas)
- **Owns:** `components/shipped/graph.rs` (was `graph_view.rs`). Wire to `graph_data` (now real via 1B-1).
- **Complexity:** large.

### 2A-2 · Trash view
- **Owns:** `components/shipped/trash.rs` (was `trash.rs`). Wire `trash_list/restore_many/delete/empty`, confirm destructive actions.
- **Complexity:** medium.

### 2A-3 · Canvas view
- **Owns:** `components/shipped/canvas.rs` (was `canvas.rs`). Wire `canvas_list/save/get/delete`.
- **Complexity:** large.

### 2A-4 · Reader + Comparison views
- **Owns:** `components/shipped/reader.rs`, `components/shipped/comparison.rs`. Wire `note_read`, `notes_diff`.
- **Complexity:** medium.

### 2A-5 · Reading Queue + Collections (board/calendar/table/gallery)
- **Owns:** `components/shipped/reading_queue.rs`, `components/shipped/collections/**` (was `collections/*`).
- **Complexity:** large.

### 2A-6 · Dashboard / Home / Search / Calendar / Archive / Smart Folders screens
- **Owns:** real replacements for `navigation/{dashboard,home_screen,search_page,calendar_page,archive_page,smart_folders}.rs` (replace hardcoded placeholder text with real data).
- **Complexity:** medium.

### 2A-7 · View routing & integration
- **What:** Declare all new modules in `components/mod.rs`, map every `ViewMode` in `app.rs` to a real component, remove placeholder divs, wire left/right sidebar and inspector (backlinks/outgoing/mentions/tags) to real data.
- **Owns:** `crates/nabu-ui/src/components/app.rs`, `components/mod.rs`, `layout/**`.
- **Complexity:** large.

## Phase 2B — Real integrations (parallel)

> Feature subphases implement logic in their own modules/components; 2B-5 is the only backend-wiring owner.

### 2B-1 · Dictation (Whisper) real
- **What:** Replace `start_dictation`/`stop_dictation` placeholder strings with a real capture path: record audio → bytes → `whisper_processor` (macOS) → text inserted into the editor/pill. Handle missing model gracefully.
- **Owns:** new `crates/nabu-core/src/dictation/**`, `crates/nabu-ui/src/components/dictation_pill.rs`.
- **Complexity:** large.

### 2B-2 · OCR / PDF end-to-end
- **What:** Confirm a real image/PDF capture → bytes → `ocr_processor`/PDF processors (via 1A-3) → `.ocr.md` companion note → indexed/searchable → visible in UI. Fix any break in the chain.
- **Owns:** `processing/processors/ocr_processor.rs`, `pdf_*_processor.rs`, companion-note UI.
- **Complexity:** medium.

### 2B-3 · Native-messaging host — finish & package (browser-agnostic)
- **What:** Fix the `captureType` (extension/JS) vs `capture_type` (Rust host) field mismatch so the web→host hop validates; auto-bundle the `native-messaging-host` binary into the app bundle instead of requiring a manual copy; document a single generic install path that works for Chrome/Chromium/Firefox.
- **Why:** The host + socket server are genuinely real and wired; the hop is currently broken by the field mismatch and the binary isn't packaged.
- **Owns:** `src-tauri/src/bin/native_messaging_host.rs`, `src-tauri/src/native_messaging.rs`, `src-tauri/src/native_messaging_socket.rs`, bundle config.
- **Do not touch:** `extensions/**` (owned by 2B-4).
- **Complexity:** small–medium.

### 2B-4 · Remove Safari-specific drift
- **What:** Delete Safari packaging artifacts (`extensions/safari/Info.plist`, `extensions/safari/native-messaging/com.nabu.capture.host.plist`); strip Safari install instructions from the extension README; rename `extensions/safari/` → `extensions/browser/` (it is a generic Manifest-V2 web extension, not Safari-specific code); update README/docs so the claim is "Browser extension (native messaging host)" with no Safari mention.
- **Why:** Safari packaging is abandoned drift; the generic web-extension + host is what the product actually has.
- **Owns:** `extensions/safari/**`, README/docs references.
- **Do not touch:** `src-tauri/src/bin/native_messaging_host.rs`, `native_messaging*.rs` (owned by 2B-3).
- **Complexity:** trivial–small.

### 2B-5 · Backend wiring for Section 2 (owns shared wiring)
- **What:** Wire dictation, smart-folder, settings import/export/reset, and diagnostics (`diagnostic_requested`) commands; register anything new in `lib.rs`; remove the now-wired commands from the dead list.
- **Owns:** `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/diagnostics.rs`, `src-tauri/src/settings.rs`.
- **Complexity:** large.

### 2B-6 · Smart Folders + settings import/export + diagnostics UI
- **What:** Build the UI surfaces for the features 2B-5 wires: smart-folders management, settings export/import/reset, and diagnostics display.
- **Owns:** `crates/nabu-ui/src/components/navigation/smart_folders.rs`, `settings/settings_panel.rs`, new diagnostics UI component.
- **Complexity:** medium.

---

# SECTION 3 — Nice to Haves / Optimisations

## Phase 3A — Test hardening (parallel)

- **3A-1 · IPC/command tests** — unit-test the real `#[tauri::command]` handlers (`src-tauri/src/commands.rs`). *Owns:* `src-tauri` test modules. Complexity: large.
- **3A-2 · Dioxus component tests** — render tests for Inbox, Settings, Graph, recovery, trash. *Owns:* `crates/nabu-ui` tests. Complexity: large.
- **3A-3 · Wiki-link graph + pipeline tests** — real content→edge extraction and full capture→pipeline tests. *Owns:* `crates/nabu-core/tests/*`. Complexity: medium.
- **3A-4 · Cross-platform CI** — Windows + Linux builds; gate macOS-only features; catch Unix-only socket/FFI issues. *Owns:* `.github/workflows`, build scripts. Complexity: medium.
- **3A-5 · Performance benchmarks** — search latency, startup, graph rebuild; enforce budgets. *Owns:* new `crates/nabu-core/benches/*`. Complexity: small.

## Phase 3B — Cleanup & polish (parallel)

- **3B-1 · Dead-code removal** — delete un-wired Leptos files, `menu.rs.bak`, and ~57 dead IPC commands (after 2B-5 confirms what's dead). *Owns:* the specific files. Complexity: small.
- **3B-2 · Documentation refresh** — rewrite stale `ARCHITECTURE.md` and `docs/architecture/architecture.md` (remove Electron/React and "Leptos" claims), make README claims match reality, reconcile roadmap. *Owns:* `*.md`, `docs/**`. Complexity: small.
- **3B-3 · AI honesty** — either implement real embeddings/summarisation (local) or relabel the stubs as heuristics and stop claiming AI. *Owns:* `embedding_generator.rs`, `ai_summariser.rs`, `semantic_enricher.rs`. Complexity: medium.
- **3B-4 · Cross-platform feature gating** — ensure Windows/Linux builds compile and degraded features are clearly surfaced. *Owns:* `native/**`, `ipc_socket/**`, `commands.rs`. Complexity: medium.

## Phase 3C — Release pass (single subphase, not parallel)

- **3C-1 · `cargo fmt` clean + zero clippy warnings** across the repo, then full `cargo test --workspace`, `cargo build` of nabu-ui for wasm, and a `tauri build` for the target platform. Runs last because it touches all files. *Owns:* whole repo. Complexity: small–medium.

---

## Dependency / launch notes

- **Run order:** Phase 1A → 1B → 2A → 2B → 3A → 3B → 3C. Within each phase, launch all subphases **in parallel**.
- **Do NOT launch 1B-1 before 1A completes** (it consumes the 1A services). Same for 2A-7 (needs 2A-1…6) and 2B-5 (needs 2B-1/2/3/6).
- **Definition of "MVP done":** Phases 1A + 1B complete. At that point search matches content and persists, the graph is real and reflected in UI, the inbox files real notes, external edits are detected, IPC errors surface instead of panicking, and rename/move/delete stay consistent.
- **Definition of "promised features done":** Phase 2 complete — every advertised view and integration is real, not a placeholder.
- **Definition of "done":** Phases 3A–3C complete, tests green, fmt/clippy clean, cross-platform builds verified, docs truthful.

Want me to write this to `docs/REMEDIATION_ROADMAP.md` (or a `docs/roadmap/` location) and/or generate the exact agent-launch prompt for a specific subphase?
