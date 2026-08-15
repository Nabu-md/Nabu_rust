//! Integration tests for the content capture → pipeline → storage lifecycle.
//!
//! Verifies that real captured content (text and binary) survives the full
//! CaptureEngine → Job Queue → PipelineExecutor → ProcessingPipeline →
//! StorageManager pipeline and can be verified on the other side:
//!
//! 1. Text content reaches ContentClassifier with real body text (not placeholder).
//! 2. Binary content is preserved through the pipeline and stored correctly.
//! 3. Markdown formatting is preserved (not downgraded to plain text).
//! 4. Wiki-link edges are derivable from objects persisted through the full pipeline.
//! 5. Missing blob references cause explicit failure via `execute()`.
//! 6. Legacy jobs without `content_payload` fail explicitly via `execute()`.
//! 7. Durable restart: content survives queue persistence and is re-executed.

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;

use nabu_core::capture::{CaptureEngine, CaptureRequest, CaptureData, ClipboardHandler, FileDropHandler};
use nabu_core::graph::{build_graph_from_objects};
use nabu_core::jobs::job::{Job, JobType, ContentPayload};
use nabu_core::jobs::queue::{DurableJobQueue, Queue};
use nabu_core::jobs::cancellation::CancellationToken;
use nabu_core::jobs::workers::progress::ProgressReporter;
use nabu_core::jobs::workers::executor::JobExecutor;
use nabu_core::jobs::errors::JobError;
use nabu_core::models::{KnowledgeObject, ObjectContent, ObjectMetadata, ObjectType, CustomPropertyValue};
use nabu_core::processing::pipeline::ProcessingPipeline;
use nabu_core::processing::processors::ContentClassifier;
use nabu_core::storage::StorageManager;
use nabu_core::PipelineExecutor;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a CaptureEngine wired to a durable queue with ClipboardHandler and
/// FileDropHandler registered.  Text goes to ClipboardHandler; binary goes to
/// FileDropHandler (or ClipboardHandler for image/*).
fn build_capture_engine(queue: Arc<DurableJobQueue>) -> CaptureEngine {
    let mut engine = CaptureEngine::new();
    engine.set_queue(queue);
    engine.register(Arc::new(ClipboardHandler));
    engine.register(Arc::new(FileDropHandler));
    engine
}

/// Build a PipelineExecutor with only the ContentClassifier registered (plus
/// any additional processors passed in).  This keeps the test fast and
/// deterministic — no Harper, Whisper, or OCR native dependencies.
fn build_executor_with_classifier(storage: Arc<StorageManager>) -> PipelineExecutor {
    let pipeline = Arc::new({
        let mut p = ProcessingPipeline::new();
        p.register(Arc::new(ContentClassifier));
        p
    });
    PipelineExecutor::new(pipeline).with_storage(storage)
}

/// Build a PipelineExecutor with an empty pipeline and the given storage.
fn build_executor_empty(storage: Arc<StorageManager>) -> PipelineExecutor {
    let pipeline = Arc::new(ProcessingPipeline::new());
    PipelineExecutor::new(pipeline).with_storage(storage)
}

/// Extract a classification string from an object's custom properties.
fn classification_of(obj: &KnowledgeObject) -> Option<String> {
    obj.custom_properties
        .get("classification")
        .and_then(|v| {
            if let CustomPropertyValue::Text(t) = v {
                Some(t.clone())
            } else {
                None
            }
        })
}

// ---------------------------------------------------------------------------
// 1. Text content reaches ContentClassifier
// ---------------------------------------------------------------------------

/// Real text content flows from CaptureEngine → job queue → PipelineExecutor
/// → ContentClassifier → StorageManager and the classification result is
/// verifiable after reload.
#[tokio::test]
async fn test_text_content_reaches_classifier() {
    let dir = tempdir().unwrap();
    let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let engine = build_capture_engine(queue.clone());

    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_with_classifier(storage.clone());

    // Distinctive invoice text — ContentClassifier requires >=2 keyword hits.
    let distinctive_text = "INVOICE #9999\nInvoice Date: 2024-01-15\nTotal Due: $500.00\nPayment Terms: Net 30\nbill to: Someone Corp";
    let request = CaptureRequest::new(CaptureData::Text(distinctive_text.to_string()));
    engine.ingest(request).await.unwrap();

    // Dequeue the persisted job.
    let job = queue.dequeue().unwrap().unwrap();

    // Execute through the pipeline (ContentClassifier runs on Note objects).
    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result.is_ok(), "execution should succeed: {:?}", result.err());

    // Reload the object from storage.
    let object_id = job.object_id.unwrap();
    let stored = storage.load(object_id).expect("object should be persisted");

    let classification = classification_of(&stored);
    assert_eq!(
        classification,
        Some("invoice".to_string()),
        "classifier should have detected invoice content from the real text"
    );
}

// ---------------------------------------------------------------------------
// 2. Binary content is preserved through the pipeline
// ---------------------------------------------------------------------------

/// Real binary bytes captured via CaptureEngine survive serialization to the
/// durable queue and are loadable from StorageManager after pipeline execution.
#[tokio::test]
async fn test_binary_capture_persists_bytes() {
    let dir = tempdir().unwrap();
    let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let engine = build_capture_engine(queue.clone());

    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_empty(storage.clone());

    // PNG header + trailing bytes — a minimal but recognisable binary blob.
    let image_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01, 0x02, 0x03];
    let request = CaptureRequest::new(CaptureData::Binary {
        mime_type: "image/png".to_string(),
        data: image_bytes.clone(),
        filename: Some("test.png".to_string()),
    });
    engine.ingest(request).await.unwrap();

    let job = queue.dequeue().unwrap().unwrap();

    // Execute through an empty pipeline (ContentClassifier skips non-text types).
    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result.is_ok(), "execution should succeed: {:?}", result.err());

    // Reload the object from storage.
    let object_id = job.object_id.unwrap();
    let stored = storage.load(object_id).expect("object should be persisted");

    match &stored.content {
        ObjectContent::Binary {
            mime_type,
            data,
            filename,
        } => {
            assert_eq!(mime_type, "image/png");
            assert_eq!(data, &image_bytes, "stored binary bytes must match captured bytes");
            assert_eq!(filename, &Some("test.png".to_string()));
        }
        _ => panic!("expected Binary content after pipeline, got {:?}", stored.content),
    }
}

// ---------------------------------------------------------------------------
// 3. Markdown formatting is preserved
// ---------------------------------------------------------------------------

/// Markdown content captured via CaptureEngine retains its Markdown variant
/// (not downgraded to PlainText) through the full pipeline and storage round-trip.
#[tokio::test]
async fn test_markdown_content_preserved() {
    let dir = tempdir().unwrap();
    let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let engine = build_capture_engine(queue.clone());

    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_with_classifier(storage.clone());

    // Text that starts with `#` is detected as Markdown by ClipboardHandler.
    let md_text = "# Meeting Notes\n\n## Action Items\n- [ ] Review wiki-link integration\n- [ ] Test binary capture\n\nSee [[Related Note]] for details.";
    let request = CaptureRequest::new(CaptureData::Text(md_text.to_string()));
    engine.ingest(request).await.unwrap();

    let job = queue.dequeue().unwrap().unwrap();

    // Verify the content_payload is Markdown before execution.
    let payload = job.content_payload.as_ref().expect("job must carry content");
    match payload {
        ContentPayload::Markdown(s) => assert_eq!(s, md_text),
        _ => panic!("expected Markdown content payload, got {:?}", payload),
    }

    // Execute through the pipeline.
    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result.is_ok(), "execution should succeed: {:?}", result.err());

    // Reload and verify content type is still Markdown.
    let object_id = job.object_id.unwrap();
    let stored = storage.load(object_id).expect("object should be persisted");

    match &stored.content {
        ObjectContent::Markdown(s) => {
            assert_eq!(s, md_text, "Markdown content must be preserved through pipeline");
        }
        _ => panic!("expected Markdown content after pipeline, got {:?}", stored.content),
    }
}

// ---------------------------------------------------------------------------
// 4. Wiki-link end-to-end: capture → execute → storage → graph
// ---------------------------------------------------------------------------

/// Two notes with wiki-links are captured, executed through the pipeline,
/// persisted via StorageManager, then fed to `build_graph_from_objects`
/// which must produce the expected content-derived edges.
#[tokio::test]
async fn test_wiki_link_end_to_end_through_pipeline() {
    let dir = tempdir().unwrap();
    let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let engine = build_capture_engine(queue.clone());

    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_with_classifier(storage.clone());

    // --- Capture Note A ---
    let note_a_content = "# Note A\n\nThis is the body of note A.";
    let request_a = CaptureRequest::new(CaptureData::Text(note_a_content.to_string()))
        .with_title("Note A");
    engine.ingest(request_a).await.unwrap();
    let job_a = queue.dequeue().unwrap().unwrap();
    let object_id_a = job_a.object_id.unwrap();

    // --- Capture Note B (links to Note A) ---
    let note_b_content = "# Note B\n\nSee [[Note A]] for context.";
    let request_b = CaptureRequest::new(CaptureData::Text(note_b_content.to_string()))
        .with_title("Note B");
    engine.ingest(request_b).await.unwrap();
    let job_b = queue.dequeue().unwrap().unwrap();
    let object_id_b = job_b.object_id.unwrap();

    // --- Execute both jobs through the pipeline → storage ---
    let result_a = executor
        .execute(&job_a, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result_a.is_ok(), "execution A should succeed: {:?}", result_a.err());

    let result_b = executor
        .execute(&job_b, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result_b.is_ok(), "execution B should succeed: {:?}", result_b.err());

    // --- Reload both objects from storage ---
    let obj_a = storage.load(object_id_a).expect("Note A should be in storage");
    let obj_b = storage.load(object_id_b).expect("Note B should be in storage");

    // Verify titles are preserved (critical for wiki-link resolution).
    assert_eq!(obj_a.metadata.title.as_deref(), Some("Note A"));
    assert_eq!(obj_b.metadata.title.as_deref(), Some("Note B"));

    // Verify wiki-link content is in Note B's body.
    let b_text = match &obj_b.content {
        ObjectContent::Markdown(s) => s.clone(),
        _ => panic!("expected Markdown content for Note B, got {:?}", obj_b.content),
    };
    assert!(b_text.contains("[[Note A]]"), "Note B content must contain the wiki-link");

    // --- Build graph from stored objects ---
    let (nodes, edges) = build_graph_from_objects(&[obj_a.clone(), obj_b.clone()]);

    assert_eq!(nodes.len(), 2, "expected 2 graph nodes");

    // Expect at least one content-derived "references" edge from B → A.
    let b_to_a = edges
        .iter()
        .find(|e| e.source == obj_b.id && e.target == obj_a.id && e.relationship == "references");
    assert!(
        b_to_a.is_some(),
        "expected wiki-link edge B→A from pipeline-persisted content"
    );
    assert!(
        b_to_a.unwrap().content_derived,
        "wiki-link edge must be content_derived"
    );
}

// ---------------------------------------------------------------------------
// 5. Missing blob fails explicitly via execute()
// ---------------------------------------------------------------------------

/// A job whose Binary content_payload references a nonexistent blob path
/// must cause `execute()` to return an explicit error — not an empty shell.
#[tokio::test]
async fn test_missing_blob_fails_explicitly() {
    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_empty(storage);

    let job = Job::new(
        JobType::Ocr,
        serde_json::json!({
            "object_type": "screenshot",
            "title": "Missing Blob",
            "source_url": null,
        }),
        "ocr_processor",
    )
    .with_object_id(uuid::Uuid::nil())
    .with_content_payload(ContentPayload::Binary {
        mime_type: "image/png".to_string(),
        filename: None,
        blob_path: "/nonexistent/path/to/blob.bin".to_string(),
    });

    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;

    assert!(result.is_err(), "missing blob must cause execute() to fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Failed to load binary blob"),
        "error should mention blob failure, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// 6. Legacy job without content_payload fails
// ---------------------------------------------------------------------------

/// A legacy job (no content_payload) must cause `execute()` to fail
/// explicitly rather than producing an empty-shell object.
#[tokio::test]
async fn test_legacy_job_without_content_payload_fails() {
    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_empty(storage);

    let job = Job::new(
        JobType::MetadataExtraction,
        serde_json::json!({ "title": "Old Job" }),
        "metadata_extraction_processor",
    );

    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;

    assert!(result.is_err(), "legacy job without content_payload must fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("no content_payload"),
        "error should mention missing content_payload, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// 7. Durable restart — content survives queue rebuild
// ---------------------------------------------------------------------------

/// A text capture is enqueued, the queue is dropped (simulating process
/// restart), and the job is rebuilt from disk.  `execute()` must still
/// succeed with the original content intact.
#[tokio::test]
async fn test_durable_text_restart_then_execute() {
    let dir = tempdir().unwrap();
    let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let engine = build_capture_engine(queue.clone());

    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_with_classifier(storage.clone());

    let distinctive_text = "INVOICE #42\nTotal Due: $100.00\nbill to: Test Corp\nInvoice Date: today";
    let request = CaptureRequest::new(CaptureData::Text(distinctive_text.to_string()));
    engine.ingest(request).await.unwrap();

    // Capture the job id before dropping the queue.
    let job_id = queue.peek().unwrap().unwrap().id;

    // Drop the queue — simulates process shutdown.
    drop(queue);

    // Recreate the queue from disk.
    let queue2 = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

    // Reload the job from disk.
    let job = queue2
        .load_job(&job_id.to_string())
        .unwrap()
        .expect("job must survive restart");

    // Verify the content payload survived.
    let payload = job
        .content_payload
        .as_ref()
        .expect("content_payload must survive restart");
    let payload_text = match payload {
        ContentPayload::PlainText(s) => s.clone(),
        _ => panic!("expected PlainText content payload after restart, got {:?}", payload),
    };
    assert_eq!(
        payload_text, distinctive_text,
        "text content must survive queue restart"
    );

    // Execute the reloaded job.
    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result.is_ok(), "execution after restart should succeed: {:?}", result.err());

    // Verify the object was persisted with classification.
    let object_id = job.object_id.unwrap();
    let stored = storage.load(object_id).expect("object should be persisted");
    assert_eq!(
        classification_of(&stored),
        Some("invoice".to_string()),
        "classifier should work on content that survived restart"
    );
}

/// A binary capture's blob survives a queue restart and is re-executable.
#[tokio::test]
async fn test_durable_binary_restart_then_execute() {
    let dir = tempdir().unwrap();
    let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let engine = build_capture_engine(queue.clone());

    let vault = tempdir().unwrap();
    let storage = Arc::new(StorageManager::new(vault.path()));
    let executor = build_executor_empty(storage.clone());

    let image_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let request = CaptureRequest::new(CaptureData::Binary {
        mime_type: "image/png".to_string(),
        data: image_bytes.clone(),
        filename: Some("restart.png".to_string()),
    });
    engine.ingest(request).await.unwrap();

    let job_id = queue.peek().unwrap().unwrap().id;
    drop(queue);

    let queue2 = Arc::new(DurableJobQueue::new(dir.path()).unwrap());
    let job = queue2
        .load_job(&job_id.to_string())
        .unwrap()
        .expect("binary job must survive restart");

    // Verify the blob path survived.
    let payload = job
        .content_payload
        .as_ref()
        .expect("content_payload must survive restart");
    let blob_path = match payload {
        ContentPayload::Binary { blob_path, .. } => blob_path.clone(),
        _ => panic!("expected Binary content payload after restart, got {:?}", payload),
    };

    // The blob file must still exist on disk after restart.
    assert!(
        PathBuf::from(&blob_path).exists(),
        "blob file must survive restart"
    );

    // Execute the reloaded job.
    let result = executor
        .execute(&job, ProgressReporter::noop(), CancellationToken::new())
        .await;
    assert!(result.is_ok(), "execution after restart should succeed: {:?}", result.err());

    // Reload from storage and verify bytes.
    let object_id = job.object_id.unwrap();
    let stored = storage.load(object_id).expect("object should be persisted");
    match &stored.content {
        ObjectContent::Binary {
            mime_type,
            data,
            ..
        } => {
            assert_eq!(mime_type, "image/png");
            assert_eq!(data, &image_bytes, "binary bytes must survive restart");
        }
        _ => panic!("expected Binary content after restart, got {:?}", stored.content),
    }
}
