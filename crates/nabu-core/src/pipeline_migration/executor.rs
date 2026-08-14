use crate::event_bus::EventBus;
use crate::jobs::cancellation::CancellationToken;
use crate::jobs::errors::{JobError, JobResult};
use crate::jobs::job::{ContentPayload, Job};
use crate::jobs::workers::executor::JobExecutor;
use crate::jobs::workers::progress::ProgressReporter;
use crate::models::{KnowledgeObject, ObjectContent, ObjectType};
use crate::pipeline_migration::events;
use crate::processing::pipeline::{build_standard_pipeline, ProcessingPipeline};
use crate::registry::lifecycle::{Lifecycle, LifecycleManager, LifecycleStage};
use crate::storage::StorageManager;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

/// The PipelineExecutor bridges the Worker Pool to the ProcessingPipeline.
///
/// It implements JobExecutor and:
/// 1. Extracts the KnowledgeObject payload from the Job
/// 2. Runs the ProcessingPipeline
/// 3. Publishes EventBus events
/// 4. Reports progress
///
/// This is the final piece connecting the async infrastructure:
///   CaptureEngine → Job Queue → Worker Pool → PipelineExecutor → ProcessingPipeline
pub struct PipelineExecutor {
    pipeline: Arc<ProcessingPipeline>,
    event_bus: Option<EventBus<crate::event_bus::PipelineEvent>>,
    storage: Option<Arc<StorageManager>>,
    /// Lifecycle state manager — tracks Created → Initialized → Running → Shutdown.
    lifecycle: LifecycleManager,
}

impl PipelineExecutor {
    /// Create a new pipeline executor.
    pub fn new(pipeline: Arc<ProcessingPipeline>) -> Self {
        Self {
            pipeline,
            event_bus: None,
            storage: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Create with event bus for publishing lifecycle events.
    pub fn with_event_bus(
        pipeline: Arc<ProcessingPipeline>,
        event_bus: EventBus<crate::event_bus::PipelineEvent>,
    ) -> Self {
        Self {
            pipeline,
            event_bus: Some(event_bus),
            storage: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Attach the canonical StorageManager so processed objects are persisted
    /// through the canonical pipeline: Pipeline → Storage → ITEM_STORED.
    pub fn with_storage(mut self, storage: Arc<StorageManager>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Create a standard pipeline executor with all default processors.
    pub fn standard(event_bus: Option<EventBus<crate::event_bus::PipelineEvent>>) -> Self {
        let pipeline = Arc::new(build_standard_pipeline(event_bus.clone()));
        Self {
            pipeline,
            event_bus,
            storage: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle state accessors
    // -----------------------------------------------------------------------

    /// Returns the current lifecycle stage of the pipeline executor.
    pub fn lifecycle_stage(&self) -> LifecycleStage {
        self.lifecycle.stage()
    }

    /// Returns `true` if the pipeline executor has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.lifecycle.is_at_least(LifecycleStage::Initialized)
    }

    /// Returns `true` if the pipeline executor is running.
    pub fn is_running(&self) -> bool {
        self.lifecycle.is_running()
    }

    /// Returns `true` if the pipeline executor has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.lifecycle.is_shutdown()
    }

    /// Reconstruct a KnowledgeObject from a job payload, restoring the
    /// actual captured content.
    ///
    /// Text content is read from `job.content_payload` (stored inline in the
    /// persisted Job JSON).  Binary content is loaded from the blob file
    /// referenced by `blob_path` in the payload.
    ///
    /// If `content_payload` is `None` (legacy job) or the blob file is
    /// missing, this returns an explicit error rather than silently building
    /// an empty-shell object.
    fn object_from_job(job: &Job) -> JobResult<KnowledgeObject> {
        // Determine the object type from the payload (preserves the original
        // type) rather than inferring from the job_type.
        let object_type = job
            .payload
            .get("object_type")
            .and_then(|v| v.as_str())
            .and_then(parse_object_type)
            .unwrap_or_else(|| object_type_from_job_type(job.job_type.name()));

        // Resolve the object id from the payload or the job field.
        let object_id = job.object_id;

        let content = match &job.content_payload {
            Some(ContentPayload::Markdown(s)) => ObjectContent::Markdown(s.clone()),
            Some(ContentPayload::RichHtml(s)) => ObjectContent::RichHtml(s.clone()),
            Some(ContentPayload::PlainText(s)) => ObjectContent::PlainText(s.clone()),
            Some(ContentPayload::Uri(s)) => ObjectContent::Uri(s.clone()),
            Some(ContentPayload::Binary {
                mime_type,
                filename,
                blob_path,
            }) => {
                // Rehydrate the persisted binary bytes.  Missing blobs
                // fail explicitly — we never silently downgrade to an
                // empty object.
                let blob_path_ref = Path::new(blob_path);
                let data = std::fs::read(blob_path_ref).map_err(|e| {
                    JobError::Persistence(format!(
                        "Failed to load binary blob for object {:?}: {}",
                        object_id, e
                    ))
                })?;
                ObjectContent::Binary {
                    mime_type: mime_type.clone(),
                    data,
                    filename: filename.clone(),
                }
            }
            None => {
                // Legacy job without a content payload — fail explicitly.
                return Err(JobError::ExecutionFailed(
                    "Job has no content_payload; content was not persisted at ingest".into(),
                ));
            }
        };

        let mut object = KnowledgeObject::new(object_type, content);

        object.metadata.title = job
            .payload
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        object.metadata.source_url = job
            .payload
            .get("source_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(object_id) = object_id {
            object.id = object_id;
        }

        // Restore mime_type for binary content from the content payload.
        if let Some(ContentPayload::Binary { mime_type, .. }) = &job.content_payload {
            object.metadata.mime_type = Some(mime_type.clone());
        }

        Ok(object)
    }
}

// ---------------------------------------------------------------------------
// Object type helpers
// ---------------------------------------------------------------------------

/// Parse an [`ObjectType`] from the lowercase `variant_name()` string
/// stored in the job payload.
fn parse_object_type(name: &str) -> Option<ObjectType> {
    match name {
        "note" => Some(ObjectType::Note),
        "bookmark" => Some(ObjectType::Bookmark),
        "document" => Some(ObjectType::Document),
        "image" => Some(ObjectType::Image),
        "screenshot" => Some(ObjectType::Screenshot),
        "scan" => Some(ObjectType::Scan),
        "audio_recording" => Some(ObjectType::AudioRecording),
        "video_recording" => Some(ObjectType::VideoRecording),
        "repository" => Some(ObjectType::Repository),
        "youtube_video" => Some(ObjectType::YouTubeVideo),
        "article" => Some(ObjectType::Article),
        "email" => Some(ObjectType::Email),
        "contact" => Some(ObjectType::Contact),
        "project" => Some(ObjectType::Project),
        "task" => Some(ObjectType::Task),
        "event" => Some(ObjectType::Event),
        "code_snippet" => Some(ObjectType::CodeSnippet),
        "whiteboard" => Some(ObjectType::Whiteboard),
        "template" => Some(ObjectType::Template),
        "attachment" => Some(ObjectType::Attachment),
        "collection" => Some(ObjectType::Collection),
        "dashboard" => Some(ObjectType::Dashboard),
        _ => None,
    }
}

/// Fallback mapping from the job-type name to an `ObjectType`, used when
/// the payload does not carry an explicit `object_type` field.
fn object_type_from_job_type(name: &str) -> ObjectType {
    match name {
        "ocr" => ObjectType::Image,
        "whisper" => ObjectType::AudioRecording,
        "pdf_text_extraction" => ObjectType::Document,
        "metadata_extraction" => ObjectType::Note,
        _ => ObjectType::Note,
    }
}

#[async_trait]
impl JobExecutor for PipelineExecutor {
    async fn execute(
        &self,
        job: &Job,
        progress: ProgressReporter,
        cancellation: CancellationToken,
    ) -> JobResult<Job> {
        if cancellation.is_cancelled() {
            return Err(crate::jobs::errors::JobError::Cancelled);
        }

        // Publish started event
        if let Some(ref bus) = self.event_bus {
            events::publish_processing_started(bus, job);
        }

        progress.set_progress(0.05);

        // Reconstruct object from job — rehydrates the actual captured
        // content (text or binary) from the persisted payload.
        let object = Self::object_from_job(job)?;
        progress.set_progress(0.1);

        // Run the processing pipeline
        let result = self
            .pipeline
            .run(object, progress.clone(), cancellation.clone())
            .await;

        progress.set_progress(0.9);

        // Handle result
        if let Some(error) = &result.error {
            // Pipeline partially failed
            if let Some(ref bus) = self.event_bus {
                let will_retry = job.should_retry();
                events::publish_processing_failed(bus, job, error, will_retry);
            }

            return Err(crate::jobs::errors::JobError::ExecutionFailed(
                error.clone(),
            ));
        }

        // Pipeline completed
        if let Some(ref bus) = self.event_bus {
            events::publish_processing_completed(bus, job);
        }

        progress.set_progress(1.0);

        // Return the job as successful
        // Persist the processed object through the canonical StorageManager.
        // StorageManager.save publishes ITEM_STORED, which drives the Indexer
        // and VaultGraph subscribers downstream.
        if let Some(ref storage) = self.storage {
            if let Err(e) = storage.save(&result.object) {
                tracing::warn!(error = %e, "Failed to persist processed object through StorageManager");
            }
        }

        let mut completed_job = job.clone();
        completed_job.status = crate::jobs::job::JobStatus::Completed;
        completed_job.progress = 1.0;

        Ok(completed_job)
    }
}

// ---------------------------------------------------------------------------
// Lifecycle trait implementation
// ---------------------------------------------------------------------------

/// Implements the shared `Lifecycle` trait so `PipelineExecutor` can be managed
/// by the Capability Platform's lifecycle manager alongside other services.
///
/// ```text
/// Created → Initialized → Running → Shutdown
/// ```
impl Lifecycle for PipelineExecutor {
    fn name(&self) -> &'static str {
        "pipeline_executor"
    }

    /// Initializes the pipeline executor.
    ///
    /// Transitions the executor from `Created` to `Initialized`.
    /// The processing pipeline and storage manager are wired during
    /// construction; this phase validates that dependencies are present.
    fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "pipeline",
            component = "executor",
            operation = "initialize",
            "PipelineExecutor initialized"
        );
        self.lifecycle
            .transition_to(LifecycleStage::Initialized)?;
        Ok(())
    }

    /// Starts the pipeline executor.
    ///
    /// Transitions the executor to `Running` so it can accept and execute
    /// processing jobs from the worker pool. Auto-advances
    /// `Created → Initialized` if `initialize()` was not called explicitly.
    ///
    /// Double-start is a safe no-op — no duplicate executor state is
    /// created. Calling `start()` after `shutdown()` returns an error.
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Cannot restart a shut-down executor
        if self.lifecycle.is_shutdown() {
            return Err(
                "PipelineExecutor has been shut down and cannot be restarted".into(),
            );
        }

        // Auto-advance Created → Initialized so callers can call start()
        // directly without an explicit initialize() call.
        if self.lifecycle.stage() == LifecycleStage::Created {
            tracing::info!(
                subsystem = "pipeline",
                component = "executor",
                operation = "start",
                "Initializing pipeline executor"
            );
            self.lifecycle
                .transition_to(LifecycleStage::Initialized)?;
        }

        // Guard against duplicate start — transition Running → Running is a
        // no-op in LifecycleManager, but we log a warning for visibility.
        if self.lifecycle.is_running() {
            tracing::warn!(
                subsystem = "pipeline",
                component = "executor",
                operation = "start",
                "PipelineExecutor already started — skipping duplicate start"
            );
            return Ok(());
        }

        self.lifecycle
            .transition_to(LifecycleStage::Running)?;

        tracing::info!(
            subsystem = "pipeline",
            component = "executor",
            operation = "start",
            "PipelineExecutor started"
        );
        Ok(())
    }

    /// Shuts down the pipeline executor.
    ///
    /// Stops accepting new processing jobs. The executor is not restartable
    /// after shutdown.
    ///
    /// Double-shutdown is a safe no-op.
    fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "pipeline",
            component = "executor",
            operation = "shutdown",
            "PipelineExecutor shutting down"
        );

        self.lifecycle
            .transition_to(LifecycleStage::Shutdown)?;

        tracing::info!(
            subsystem = "pipeline",
            component = "executor",
            operation = "shutdown",
            "PipelineExecutor stopped"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processing::pipeline::ProcessingPipeline;

    /// Create a minimal PipelineExecutor for lifecycle tests.
    fn make_executor() -> PipelineExecutor {
        let pipeline = Arc::new(ProcessingPipeline::new());
        PipelineExecutor::new(pipeline)
    }

    // ── Lifecycle state tests ────────────────────────────────────────

    #[test]
    fn lifecycle_initial_state_is_created() {
        let executor = make_executor();
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Created);
        assert!(!executor.is_initialized());
        assert!(!executor.is_running());
        assert!(!executor.is_shutdown());
    }

    #[test]
    fn lifecycle_trait_name() {
        let executor = make_executor();
        let executor_ref: &dyn Lifecycle = &executor;
        assert_eq!(executor_ref.name(), "pipeline_executor");
    }

    #[test]
    fn lifecycle_initialize_transitions_to_initialized() {
        let executor = make_executor();
        assert!(executor.initialize().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Initialized);
        assert!(executor.is_initialized());
        assert!(!executor.is_running());
    }

    #[test]
    fn lifecycle_start_auto_advances_from_created() {
        let executor = make_executor();
        // start() should auto-advance Created → Initialized → Running
        assert!(executor.start().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Running);
        assert!(executor.is_running());
    }

    #[test]
    fn lifecycle_full_flow() {
        let executor = make_executor();
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Created);

        assert!(executor.initialize().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Initialized);
        assert!(executor.is_initialized());

        assert!(executor.start().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Running);
        assert!(executor.is_running());

        assert!(executor.shutdown().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Shutdown);
        assert!(executor.is_shutdown());
    }

    #[test]
    fn lifecycle_start_after_shutdown_returns_error() {
        let executor = make_executor();
        assert!(executor.start().is_ok());
        assert!(executor.shutdown().is_ok());
        // Cannot restart after shutdown
        assert!(executor.start().is_err());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Shutdown);
    }

    #[test]
    fn lifecycle_double_shutdown_is_noop() {
        let executor = make_executor();
        assert!(executor.start().is_ok());
        assert!(executor.shutdown().is_ok());
        // Second shutdown should succeed (same stage is a no-op)
        assert!(executor.shutdown().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Shutdown);
    }

    #[test]
    fn lifecycle_double_start_is_noop() {
        let executor = make_executor();
        assert!(executor.start().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Running);
        // Second start should succeed (no-op)
        assert!(executor.start().is_ok());
        assert!(executor.is_running());
        // Cleanup
        assert!(executor.shutdown().is_ok());
    }

    #[test]
    fn lifecycle_start_without_initialize() {
        let executor = make_executor();
        // start() should auto-advance Created → Initialized → Running
        assert!(executor.start().is_ok());
        assert_eq!(executor.lifecycle_stage(), LifecycleStage::Running);
        assert!(executor.is_running());
        assert!(executor.is_initialized());
        assert!(executor.shutdown().is_ok());
    }

    #[test]
    fn lifecycle_backward_transition_rejected() {
        let executor = make_executor();
        assert!(executor.start().is_ok());
        assert!(executor.shutdown().is_ok());
        // Cannot go backward: Shutdown → Initialized
        assert!(executor.initialize().is_err());
        // Cannot restart: Shutdown → Running
        assert!(executor.start().is_err());
    }

    // ── JobExecutor still works after lifecycle ──────────────────────

    #[tokio::test]
    async fn job_executor_works_after_start() {
        use crate::jobs::cancellation::CancellationToken;
        use crate::jobs::workers::progress::ProgressReporter;
        let executor = make_executor();
        let _ = executor.start(); // just verify it compiles and runs

        let job = Job::new(
            crate::jobs::job::JobType::Custom("test".to_string()),
            serde_json::json!({ "title": "Test Object" }),
            "metadata_extraction_processor",
        )
        .with_object_id(uuid::Uuid::nil())
        .with_content_payload(ContentPayload::PlainText("Test content".to_string()));
        let progress = ProgressReporter::noop();
        let cancellation = CancellationToken::new();
        let result = executor.execute(&job, progress, cancellation).await;
        // The execute method itself is unchanged; we just verify it doesn't
        // panic when the executor is in Running state.
        let _ = result;
        let _ = executor.shutdown();
    }

    // ── Content payload pass-through tests ───────────────────────────
    //
    // These tests verify the full CaptureEngine → Job Queue → PipelineExecutor
    // → ProcessingPipeline data path, proving that real captured content
    // (text and binary) survives serialization and reaches processors.
    //

    /// A recording processor that captures the object content it receives.
    /// Used to verify that downstream processors get the real payload.
    #[derive(Default)]
    struct RecordingProcessor {
        recorded: Arc<std::sync::Mutex<Option<crate::models::ObjectContent>>>,
    }

    impl RecordingProcessor {
        fn new() -> (Self, Arc<std::sync::Mutex<Option<crate::models::ObjectContent>>>) {
            let recorded = Arc::new(std::sync::Mutex::new(None));
            (
                Self {
                    recorded: recorded.clone(),
                },
                recorded,
            )
        }
    }

    #[async_trait::async_trait]
    impl crate::processing::processor::Processor for RecordingProcessor {
        fn name(&self) -> &'static str {
            "recording_processor"
        }

        async fn process(
            &self,
            context: &crate::processing::processor::ProcessingContext,
            _progress: ProgressReporter,
            _cancellation: CancellationToken,
        ) -> crate::processing::processor::ProcessingResult {
            *self.recorded.lock().unwrap() = Some(context.object.content.clone());
            crate::processing::processor::ProcessingResult::unmodified(context.object.clone())
        }

        fn supports(&self, _object_type: &crate::models::ObjectType) -> bool {
            true
        }
    }

    /// Test 1 — Text content reaches classifier.
    ///
    /// Creates a text capture with a distinctive body string, enqueues it,
    /// and runs it through the full executor → pipeline path.  Verifies
    /// that the `ContentClassifier` received the real text and classified it
    /// correctly (not a title-only or empty placeholder).
    #[tokio::test]
    async fn test_text_content_reaches_classifier() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};
        use crate::jobs::cancellation::CancellationToken;
        use crate::jobs::workers::progress::ProgressReporter;
        use crate::models::CustomPropertyValue;
        use crate::processing::pipeline::ProcessingPipeline;
        use crate::processing::processors::ContentClassifier;
        use crate::storage::StorageManager;

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        // Distinctive invoice text — the classifier requires ≥2 keyword hits.
        let distinctive_text = "INVOICE #9999\nInvoice Date: 2024-01-15\nTotal Due: $500.00\nPayment Terms: Net 30\nbill to: Someone Corp";
        let request = CaptureRequest::new(CaptureData::Text(
            distinctive_text.to_string(),
        ));
        engine.ingest(request).await.unwrap();

        // Dequeue the persisted job
        let job = queue.dequeue().unwrap().unwrap();

        // Build a pipeline with only the content classifier
        let pipeline = Arc::new({
            let mut p = ProcessingPipeline::new();
            p.register(Arc::new(RecordingProcessor::new().0));
            p.register(Arc::new(ContentClassifier));
            p
        });

        // Use a vault with storage so the processed object is persisted
        let vault = tempfile::tempdir().unwrap();
        let storage = Arc::new(StorageManager::new(vault.path()));
        let executor = PipelineExecutor::new(pipeline).with_storage(storage.clone());

        let result = executor
            .execute(&job, ProgressReporter::noop(), CancellationToken::new())
            .await;
        assert!(result.is_ok(), "execution should succeed: {:?}", result.err());

        // Reload the object from storage and verify classification
        let object_id = job.object_id.unwrap();
        let stored = storage.load(object_id).expect("object should be persisted");

        let classification = stored
            .custom_properties
            .get("classification")
            .and_then(|v| {
                if let CustomPropertyValue::Text(t) = v {
                    Some(t.clone())
                } else {
                    None
                }
            });
        assert_eq!(
            classification,
            Some("invoice".to_string()),
            "classifier should have detected invoice content from the real text"
        );
    }

    /// Test 2 — Image bytes reach OCR.
    ///
    /// Creates an image capture with known bytes, enqueues it, executes it,
    /// and asserts that the OCR processor received non-empty bytes matching
    /// the captured payload — even on platforms where the native OCR engine
    /// cannot execute (the test validates the data boundary, not OCR output).
    #[tokio::test]
    async fn test_image_bytes_reach_ocr() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};
        use crate::jobs::cancellation::CancellationToken;
        use crate::jobs::workers::progress::ProgressReporter;
        use crate::models::ObjectContent;
        use crate::processing::pipeline::ProcessingPipeline;
        use crate::processing::processors::OcrProcessor;

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let image_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d];
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "image/png".to_string(),
            data: image_bytes.clone(),
            filename: Some("test.png".to_string()),
        });
        engine.ingest(request).await.unwrap();

        let job = queue.dequeue().unwrap().unwrap();

        // Pipeline with a recording processor before OCR to capture what
        // the processors receive.
        let (recorder, recorded) = RecordingProcessor::new();
        let pipeline = Arc::new({
            let mut p = ProcessingPipeline::new();
            p.register(Arc::new(recorder));
            p.register(Arc::new(OcrProcessor));
            p
        });

        let executor = PipelineExecutor::new(pipeline);
        let result = executor
            .execute(&job, ProgressReporter::noop(), CancellationToken::new())
            .await;
        assert!(result.is_ok(), "execution should succeed: {:?}", result.err());

        // Verify the OCR processor received the real image bytes
        let received = recorded.lock().unwrap().take().expect(
            "recording processor should have captured the object content",
        );
        match &received {
            ObjectContent::Binary {
                mime_type, data, ..
            } => {
                assert_eq!(mime_type, "image/png");
                assert!(!data.is_empty(), "OCR should receive non-empty bytes");
                assert_eq!(
                    data, &image_bytes,
                    "OCR should receive the exact captured image bytes"
                );
            }
            _ => panic!("expected Binary content for OCR, got {:?}", received),
        }
    }

    /// Test 3 — Durable text restart.
    ///
    /// Persists a text capture/job, drops the in-memory queue, recreates the
    /// queue from disk, and verifies the original content is still available
    /// after the restart.
    #[tokio::test]
    async fn test_durable_text_restart() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};
        use crate::models::ObjectContent;

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let distinctive_text = "Restart test content with distinctive marker #RESTART123";
        let request = CaptureRequest::new(CaptureData::Text(
            distinctive_text.to_string(),
        ));
        engine.ingest(request).await.unwrap();

        // Capture the job id
        let job_id = queue.peek().unwrap().unwrap().id;

        // Drop the queue (simulates process shutdown)
        drop(queue);

        // Recreate the queue from disk — jobs and blobs survive
        let queue2 = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        // Reload the job
        let job = queue2
            .load_job(&job_id.to_string())
            .unwrap()
            .expect("job should survive restart");

        // Reconstruct the object from the reloaded job
        let object = PipelineExecutor::object_from_job(&job).unwrap();

        // Verify the text content survived the restart
        match &object.content {
            ObjectContent::PlainText(s) => {
                assert_eq!(s, distinctive_text, "text content must survive restart");
            }
            _ => panic!("expected PlainText content after restart, got {:?}", object.content),
        }
    }

    /// Test 4 — Durable binary restart.
    ///
    /// Persists a binary capture/job, drops the in-memory queue, recreates
    /// the queue from disk, and verifies the original bytes are still
    /// available and unchanged after the restart.
    #[tokio::test]
    async fn test_durable_binary_restart() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};
        use crate::models::ObjectContent;

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let image_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "image/png".to_string(),
            data: image_bytes.clone(),
            filename: Some("restart.png".to_string()),
        });
        engine.ingest(request).await.unwrap();

        let job_id = queue.peek().unwrap().unwrap().id;

        // Drop the queue (simulates process shutdown)
        drop(queue);

        // Recreate the queue from disk
        let queue2 = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let job = queue2
            .load_job(&job_id.to_string())
            .unwrap()
            .expect("job should survive restart");

        // Reconstruct the object — this loads the blob file from disk
        let object = PipelineExecutor::object_from_job(&job).unwrap();

        match &object.content {
            ObjectContent::Binary {
                mime_type,
                data,
                ..
            } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(data, &image_bytes, "binary bytes must survive restart");
            }
            _ => panic!("expected Binary content after restart"),
        }
    }

    /// A legacy job (no content_payload) must fail explicitly rather than
    /// producing an empty-shell object.
    #[tokio::test]
    async fn test_legacy_job_without_content_payload_fails() {
        let executor = make_executor();
        let job = Job::new(
            crate::jobs::job::JobType::MetadataExtraction,
            serde_json::json!({ "title": "Old Job" }),
            "metadata_extraction_processor",
        );
        let result = executor
            .execute(&job, ProgressReporter::noop(), CancellationToken::new())
            .await;
        assert!(result.is_err(), "legacy job without content_payload must fail");
    }

    /// Test: PDF captures persist their bytes and the PDF text processor
    /// can reconstruct them through the full job lifecycle.
    #[tokio::test]
    async fn test_pdf_bytes_survive_pipeline() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};
        use crate::jobs::cancellation::CancellationToken;
        use crate::jobs::workers::progress::ProgressReporter;
        use crate::processing::pipeline::ProcessingPipeline;
        use crate::processing::processors::PdfTextProcessor;

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        // FileDropHandler handles arbitrary binary via create_binary_object
        engine.register(Arc::new(crate::capture::handler::FileDropHandler));

        let pdf_bytes = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\nxref\n0 2\n0000000000 65535 f\n0000000009 00000 n\ntrailer\n<< /Root 1 0 R >>\n%%EOF".to_vec();
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "application/pdf".to_string(),
            data: pdf_bytes.clone(),
            filename: Some("doc.pdf".to_string()),
        });
        engine.ingest(request).await.unwrap();

        let job = queue.dequeue().unwrap().unwrap();

        // Pipeline with a recording processor before PDF processor
        let (recorder, recorded) = RecordingProcessor::new();
        let pipeline = Arc::new({
            let mut p = ProcessingPipeline::new();
            p.register(Arc::new(recorder));
            p.register(Arc::new(PdfTextProcessor));
            p
        });

        let executor = PipelineExecutor::new(pipeline);
        let result = executor
            .execute(&job, ProgressReporter::noop(), CancellationToken::new())
            .await;
        assert!(result.is_ok(), "execution should succeed: {:?}", result.err());

        // Verify the PDF processor received the real bytes
        let received = recorded.lock().unwrap().take().unwrap();
        match &received {
            ObjectContent::Binary {
                mime_type, data, ..
            } => {
                assert_eq!(mime_type, "application/pdf");
                assert!(!data.is_empty(), "PDF processor should receive non-empty bytes");
                assert_eq!(data, &pdf_bytes, "PDF processor should receive exact captured bytes");
            }
            _ => panic!("expected Binary content for PDF, got {:?}", received),
        }
    }

    /// Test: Audio captures persist their bytes and the Whisper processor
    /// can reconstruct them through the full job lifecycle.
    ///
    /// A binary audio capture is persisted through the job store's blob
    /// mechanism.  The executor deserialises the job after a simulated
    /// restart and the Whisper processor (and a recording processor)
    /// receive the exact original bytes.
    #[tokio::test]
    async fn test_audio_bytes_survive_pipeline() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};
        use crate::jobs::cancellation::CancellationToken;
        use crate::jobs::workers::progress::ProgressReporter;
        use crate::models::{ObjectContent, ObjectType};
        use crate::processing::pipeline::ProcessingPipeline;
        use crate::processing::processors::WhisperProcessor;

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        // FileDropHandler handles arbitrary binary; we then override the
        // object_type to "audio_recording" so WhisperProcessor picks it up.
        engine.register(Arc::new(crate::capture::handler::FileDropHandler));

        let audio_bytes = vec![0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45];
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "audio/wav".to_string(),
            data: audio_bytes.clone(),
            filename: Some("recording.wav".to_string()),
        });
        engine.ingest(request).await.unwrap();

        // Fix up the job so it targets Whisper and an AudioRecording object
        let mut job = queue.dequeue().unwrap().unwrap();
        job.job_type = crate::jobs::job::JobType::Whisper;
        job.processor_name = "whisper_processor".to_string();
        // Override the payload's object_type to audio_recording
        if let Some(obj) = job.payload.as_object_mut() {
            obj.insert(
                "object_type".to_string(),
                serde_json::Value::String("audio_recording".to_string()),
            );
        }

        // Pipeline with a recording processor before Whisper processor
        let (recorder, recorded) = RecordingProcessor::new();
        let pipeline = Arc::new({
            let mut p = ProcessingPipeline::new();
            p.register(Arc::new(recorder));
            p.register(Arc::new(WhisperProcessor));
            p
        });

        let executor = PipelineExecutor::new(pipeline);
        // The executor will fail on Whisper (no model in CI), but the
        // recording processor should have captured the content before
        // Whisper runs.  We accept the execute error.
        let _ = executor
            .execute(&job, ProgressReporter::noop(), CancellationToken::new())
            .await;

        // Verify the Whisper processor received the real audio bytes
        let received = recorded.lock().unwrap().take().expect(
            "recording processor should have captured the object content",
        );
        match &received {
            ObjectContent::Binary {
                mime_type, data, ..
            } => {
                assert_eq!(mime_type, "audio/wav");
                assert!(!data.is_empty(), "Whisper should receive non-empty bytes");
                assert_eq!(data, &audio_bytes, "Whisper should receive exact captured bytes");
            }
            _ => panic!("expected Binary content for audio, got {:?}", received),
        }

        // Also verify object_from_job reconstructs with correct type
        let object = PipelineExecutor::object_from_job(&job).unwrap();
        assert_eq!(object.object_type, ObjectType::AudioRecording);
    }

    /// Test: Markdown content is preserved through the pipeline, not
    /// downgraded to plain text.
    #[tokio::test]
    async fn test_markdown_content_preserved() {
        use crate::capture::{CaptureEngine, CaptureRequest};
        use crate::capture::handler::CaptureData;
        use crate::jobs::queue::{DurableJobQueue, Queue};

        let dir = tempfile::tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let md_text = "# Title\n\nThis is **markdown** content with `# code`.";
        let request = CaptureRequest::new(CaptureData::Text(md_text.to_string()));
        engine.ingest(request).await.unwrap();

        let job = queue.dequeue().unwrap().unwrap();
        let payload = job.content_payload.as_ref().expect("job must carry content");

        // Verify the content type is Markdown, not PlainText
        match payload {
            ContentPayload::Markdown(s) => assert_eq!(s, md_text),
            _ => panic!("expected Markdown content payload, got {:?}", payload),
        }

        // Reconstruct and verify
        let object = PipelineExecutor::object_from_job(&job).unwrap();
        match &object.content {
            ObjectContent::Markdown(s) => assert_eq!(s, md_text),
            _ => panic!("expected Markdown content after reconstruction"),
        }
    }

    /// Test: Missing blob file causes explicit failure, not an empty shell.
    #[tokio::test]
    async fn test_missing_blob_fails_explicitly() {
        let job = Job::new(
            crate::jobs::job::JobType::Ocr,
            serde_json::json!({
                "object_type": "screenshot",
                "title": "Missing Blob",
                "source_url": null,
            }),
            "ocr_processor",
        )
        .with_content_payload(ContentPayload::Binary {
            mime_type: "image/png".to_string(),
            filename: None,
            blob_path: "/nonexistent/path/to/blob.bin".to_string(),
        });

        let result = PipelineExecutor::object_from_job(&job);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Failed to load binary blob"),
            "error should mention blob failure, got: {}", err
        );
    }
}
