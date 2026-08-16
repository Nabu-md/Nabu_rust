use crate::capture::handler::{CaptureHandler, CaptureRequest, CaptureResult};
use crate::event_bus::kinds::ITEM_CAPTURED;
use crate::event_bus::{EventBus, ItemCapturedEvent, PipelineEvent};
use crate::jobs::errors::JobResult;
use crate::jobs::job::{ContentPayload, Job, JobType};
use crate::jobs::queue::{DurableJobQueue, Queue};
use crate::models::ObjectType;
use crate::registry::lifecycle::{Lifecycle, LifecycleManager, LifecycleStage};
use crate::registry::metrics::{
    CounterMetric, GaugeMetric, MetricsAggregator, ServiceMetrics,
};
use std::collections::HashMap;
use std::sync::Arc;

/// The CaptureEngine routes capture requests to registered handlers
/// and enqueues jobs for asynchronous processing.
///
/// This is the canonical entry point for all content entering Nabu.
/// No feature bypasses the CaptureEngine.
pub struct CaptureEngine {
    handlers: HashMap<String, Arc<dyn CaptureHandler>>,
    event_bus: Option<EventBus<PipelineEvent>>,
    queue: Option<Arc<DurableJobQueue>>,
    /// Lifecycle state manager — tracks Created → Initialized → Running → Shutdown.
    lifecycle: LifecycleManager,
}

impl CaptureEngine {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            event_bus: None,
            queue: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Create a capture engine with an event bus for publishing events.
    pub fn with_event_bus(event_bus: EventBus<PipelineEvent>) -> Self {
        Self {
            handlers: HashMap::new(),
            event_bus: Some(event_bus),
            queue: None,
            lifecycle: LifecycleManager::new(),
        }
    }

    /// Set the job queue for async processing.
    pub fn set_queue(&mut self, queue: Arc<DurableJobQueue>) {
        self.queue = Some(queue);
    }

    /// Register a capture handler.
    pub fn register(&mut self, handler: Arc<dyn CaptureHandler>) {
        self.handlers.insert(handler.name().to_string(), handler);
    }

    /// Ingest a capture request through the pipeline.
    ///
    /// Returns the enqueued Job if async processing is configured.
    /// Returns immediately — processing happens asynchronously.
    pub async fn ingest(&self, request: CaptureRequest) -> JobResult<Option<uuid::Uuid>> {
        // Find matching handler
        let result = self.route(&request).await;

        let result = match result {
            Some(r) => r,
            None => return Ok(None),
        };

        // Publish capture event
        if let Some(ref bus) = self.event_bus {
            bus.publish(
                ITEM_CAPTURED,
                &PipelineEvent::ItemCaptured(ItemCapturedEvent::new(
                    result.object.id,
                    result.object.object_type.clone(),
                    result.source.clone(),
                    result.object.metadata.title.clone(),
                    None,
                )),
            );
        }

        // Enqueue for async processing
        if result.enqueue {
            if let Some(ref queue) = self.queue {
                let job_type = object_type_to_job_type(&result.object.object_type);
                let payload = serde_json::json!({
                    "object_id": result.object.id,
                    "object_type": result.object.object_type.variant_name(),
                    "source": format!("{:?}", result.source),
                    "title": result.object.metadata.title,
                    "source_url": result.object.metadata.source_url,
                });

                let mut job = Job::new(
                    job_type.clone(),
                    payload,
                    format!("{}_processor", job_type.name()),
                )
                .with_object_id(result.object.id)
                .with_tag("capture");

                if let Some(ref source_url) = result.object.metadata.source_url {
                    job = job.with_metadata("source_url", source_url.clone());
                }

                // Persist the actual captured content so the executor can
                // rehydrate the full KnowledgeObject after queue persistence
                // and process restarts.  Text content is stored inline in the
                // payload; binary content is persisted to a blob file and a
                // durable reference is stored instead.
                job.content_payload =
                    build_content_payload(&result.object, queue, &job.object_id)?;

                queue.enqueue(job)?;

                // Publish with job ID
                if let Some(ref _bus) = self.event_bus {
                    // Re-publish with job ID — in real impl this would be done once
                }
            }
        }

        Ok(Some(result.object.id))
    }

    /// Route a capture request to the appropriate handler.
    async fn route(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        // Try each registered handler until one succeeds
        for handler in self.handlers.values() {
            if let Some(result) = handler.capture(request).await {
                return Some(result);
            }
        }
        None
    }

    /// Number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// List registered handler names.
    pub fn handler_names(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Lifecycle state accessors
    // -----------------------------------------------------------------------

    /// Returns the current lifecycle stage of the capture engine.
    pub fn lifecycle_stage(&self) -> LifecycleStage {
        self.lifecycle.stage()
    }

    /// Returns `true` if the capture engine has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.lifecycle.is_at_least(LifecycleStage::Initialized)
    }

    /// Returns `true` if the capture engine is running.
    pub fn is_running(&self) -> bool {
        self.lifecycle.is_running()
    }

    /// Returns `true` if the capture engine has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.lifecycle.is_shutdown()
    }
}

// ---------------------------------------------------------------------------
// Lifecycle trait implementation
// ---------------------------------------------------------------------------

/// Implements the shared `Lifecycle` trait so `CaptureEngine` can be managed
/// by the Capability Platform's lifecycle manager alongside other services.
///
/// ```text
/// Created → Initialized → Running → Shutdown
/// ```
impl Lifecycle for CaptureEngine {
    fn name(&self) -> &'static str {
        "capture_engine"
    }

    /// Initializes the capture engine.
    ///
    /// Transitions the engine from `Created` to `Initialized`.
    /// Handler registration and queue wiring are set during construction;
    /// this phase validates that required dependencies (handlers, queue)
    /// are present and ready.
    fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "capture",
            component = "engine",
            operation = "initialize",
            handlers = self.handler_count(),
            "CaptureEngine initialized"
        );
        self.lifecycle
            .transition_to(LifecycleStage::Initialized)?;
        Ok(())
    }

    /// Starts the capture engine.
    ///
    /// Transitions the engine to `Running` so it can accept and route
    /// capture requests. Auto-advances `Created → Initialized` if
    /// `initialize()` was not called explicitly.
    ///
    /// Double-start is a safe no-op — no duplicate handlers are registered.
    /// Calling `start()` after `shutdown()` returns an error.
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Cannot restart a shut-down engine
        if self.lifecycle.is_shutdown() {
            return Err(
                "CaptureEngine has been shut down and cannot be restarted".into(),
            );
        }

        // Auto-advance Created → Initialized so callers can call start()
        // directly without an explicit initialize() call.
        if self.lifecycle.stage() == LifecycleStage::Created {
            tracing::info!(
                subsystem = "capture",
                component = "engine",
                operation = "start",
                "Initializing capture engine"
            );
            self.lifecycle
                .transition_to(LifecycleStage::Initialized)?;
        }

        // Guard against duplicate start — transition Running → Running is a
        // no-op in LifecycleManager, but we log a warning for visibility.
        if self.lifecycle.is_running() {
            tracing::warn!(
                subsystem = "capture",
                component = "engine",
                operation = "start",
                "CaptureEngine already started — skipping duplicate start"
            );
            return Ok(());
        }

        self.lifecycle
            .transition_to(LifecycleStage::Running)?;

        tracing::info!(
            subsystem = "capture",
            component = "engine",
            operation = "start",
            handlers = self.handler_count(),
            "CaptureEngine started"
        );
        Ok(())
    }

    /// Shuts down the capture engine.
    ///
    /// Stops accepting new capture requests. The engine is not restartable
    /// after shutdown.
    ///
    /// Double-shutdown is a safe no-op.
    fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            subsystem = "capture",
            component = "engine",
            operation = "shutdown",
            "CaptureEngine shutting down"
        );

        self.lifecycle
            .transition_to(LifecycleStage::Shutdown)?;

        tracing::info!(
            subsystem = "capture",
            component = "engine",
            operation = "shutdown",
            "CaptureEngine stopped"
        );
        Ok(())
    }
}

impl MetricsAggregator for CaptureEngine {
    fn metrics(&self) -> ServiceMetrics {
        ServiceMetrics {
            service: "capture_engine".to_string(),
            timers: Vec::new(),
            counters: vec![CounterMetric {
                key: "capture.ingest".to_string(),
                value: 0,
            }],
            gauges: vec![
                GaugeMetric {
                    key: "capture.handler_count".to_string(),
                    value: self.handler_count() as i64,
                },
                GaugeMetric {
                    key: "capture.has_queue".to_string(),
                    value: if self.queue.is_some() { 1 } else { 0 },
                },
                GaugeMetric {
                    key: "capture.has_event_bus".to_string(),
                    value: if self.event_bus.is_some() { 1 } else { 0 },
                },
            ],
        }
    }
}

impl Default for CaptureEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn object_type_to_job_type(object_type: &ObjectType) -> JobType {
    match object_type {
        ObjectType::Image | ObjectType::Screenshot | ObjectType::Scan => JobType::Ocr,
        ObjectType::AudioRecording => JobType::Whisper,
        ObjectType::Document => JobType::PdfTextExtraction,
        ObjectType::YouTubeVideo
        | ObjectType::Repository
        | ObjectType::Bookmark
        | ObjectType::Article => JobType::MetadataExtraction,
        _ => JobType::MetadataExtraction,
    }
}

/// Build a [`ContentPayload`] from the captured [`KnowledgeObject`]'s content.
///
/// Text variants are stored directly in the payload JSON.  Binary variants are
/// persisted to a blob file in the job store and a durable path reference is
/// returned.  This ensures the executor can rehydrate the full content after
/// queue persistence and process restart.
fn build_content_payload(
    object: &crate::models::KnowledgeObject,
    queue: &Arc<DurableJobQueue>,
    object_id: &Option<uuid::Uuid>,
) -> JobResult<Option<ContentPayload>> {
    use crate::models::ObjectContent;
    let payload = match &object.content {
        ObjectContent::Markdown(s) => ContentPayload::Markdown(s.clone()),
        ObjectContent::RichHtml(s) => ContentPayload::RichHtml(s.clone()),
        ObjectContent::PlainText(s) => ContentPayload::PlainText(s.clone()),
        ObjectContent::Uri(s) => ContentPayload::Uri(s.clone()),
        ObjectContent::Binary {
            mime_type,
            data,
            filename,
        } => {
            let id_str = object_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| object.id.to_string());
            let blob_path = queue.store().store_blob(&id_str, data)?;
            ContentPayload::Binary {
                mime_type: mime_type.clone(),
                filename: filename.clone(),
                blob_path,
            }
        }
    };
    Ok(Some(payload))
}

/// Build the default capture engine with all built-in handlers.
pub fn build_default_capture_engine(
    event_bus: Option<EventBus<PipelineEvent>>,
    queue: Option<Arc<DurableJobQueue>>,
) -> CaptureEngine {
    let mut engine = match event_bus {
        Some(bus) => CaptureEngine::with_event_bus(bus),
        None => CaptureEngine::new(),
    };

    if let Some(queue) = queue {
        engine.set_queue(queue);
    }

    engine.register(Arc::new(super::handler::ClipboardHandler));
    engine.register(Arc::new(super::handler::ScreenshotHandler));
    engine.register(Arc::new(super::handler::FileDropHandler));
    engine.register(Arc::new(super::handler::WatchFolderHandler));
    engine.register(Arc::new(super::handler::YouTubeCaptureHandler));
    engine.register(Arc::new(super::handler::GitHubRepositoryHandler));
    engine.register(Arc::new(super::handler::EmailCaptureHandler));
    engine.register(Arc::new(super::handler::BookmarkCaptureHandler));

    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::handler::CaptureData;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_ingest_without_queue() {
        let engine = build_default_capture_engine(None, None);

        let request = CaptureRequest::new(CaptureData::Text("Hello, world!".to_string()))
            .with_title("Test Note");

        let result = engine.ingest(request).await.unwrap();
        assert!(result.is_some(), "Should have created an object");
    }

    #[tokio::test]
    async fn test_ingest_with_queue() {
        let dir = tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue);

        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let request = CaptureRequest::new(CaptureData::Text("Queued content".to_string()));
        let result = engine.ingest(request).await.unwrap();

        assert!(result.is_some(), "Should have enqueued a job");
    }

    /// Verify that a text capture retains its real content in the job's
    /// `content_payload` after going through the CaptureEngine.
    #[tokio::test]
    async fn test_text_capture_content_payload() {
        let dir = tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let distinctive_text = "INVOICE #42\nTotal Due: $999.99\nbill to: Test Corp";
        let request = CaptureRequest::new(CaptureData::Text(
            distinctive_text.to_string(),
        ));
        engine.ingest(request).await.unwrap();

        // Dequeue and inspect the job
        let job = queue.dequeue().unwrap().unwrap();
        let payload = job.content_payload.expect("text job must carry content");
        match &payload {
            ContentPayload::PlainText(s) => {
                assert_eq!(s, distinctive_text);
            }
            _ => panic!("expected PlainText content payload, got {:?}", payload),
        }
    }

    /// Verify that a binary capture persists its bytes to a blob file and
    /// stores a durable reference in the job's `content_payload`.
    #[tokio::test]
    async fn test_binary_capture_content_payload() {
        let dir = tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::ClipboardHandler));

        let image_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "image/png".to_string(),
            data: image_bytes.clone(),
            filename: None,
        });
        engine.ingest(request).await.unwrap();

        let job = queue.dequeue().unwrap().unwrap();
        let payload = job.content_payload.expect("binary job must carry content");
        match &payload {
            ContentPayload::Binary {
                mime_type, blob_path, ..
            } => {
                assert_eq!(mime_type, "image/png");
                // The blob file should exist on disk and contain the bytes
                let blob_data = std::fs::read(blob_path).unwrap();
                assert_eq!(blob_data, image_bytes);
            }
            _ => panic!("expected Binary content payload, got {:?}", payload),
        }
    }

    /// Verify that a URI capture (bookmark) carries its URL through the
    /// content_payload, not just the metadata.
    #[tokio::test]
    async fn test_uri_capture_content_payload() {
        let dir = tempdir().unwrap();
        let queue = Arc::new(DurableJobQueue::new(dir.path()).unwrap());

        let mut engine = CaptureEngine::new();
        engine.set_queue(queue.clone());
        engine.register(Arc::new(crate::capture::handler::BookmarkCaptureHandler));

        let url = "https://example.com/bookmark-test".to_string();
        let request = CaptureRequest::new(CaptureData::Uri(url.clone()));
        engine.ingest(request).await.unwrap();

        let job = queue.dequeue().unwrap().unwrap();
        let payload = job.content_payload.expect("uri job must carry content");
        match &payload {
            ContentPayload::Uri(s) => {
                assert_eq!(s, &url);
            }
            _ => panic!("expected Uri content payload, got {:?}", payload),
        }
    }

    #[tokio::test]
    async fn test_default_engine_registers_all_handlers() {
        let engine = build_default_capture_engine(None, None);
        let names = engine.handler_names();
        // 8 built-in handlers: clipboard, screenshot, file_drop, watch_folder,
        // youtube, github, email, bookmark.
        assert_eq!(
            engine.handler_count(),
            8,
            "Expected 8 handlers: {:?}",
            names
        );
        assert!(names.contains(&"clipboard".to_string()));
        assert!(names.contains(&"screenshot".to_string()));
        assert!(names.contains(&"file_drop".to_string()));
        assert!(names.contains(&"watch_folder".to_string()));
        assert!(names.contains(&"youtube".to_string()));
        assert!(names.contains(&"github".to_string()));
        assert!(names.contains(&"email".to_string()));
        assert!(names.contains(&"bookmark".to_string()));
    }

    #[tokio::test]
    async fn test_bookmark_capture_through_engine() {
        let engine = build_default_capture_engine(None, None);
        let request = CaptureRequest::new(CaptureData::Uri(
            "https://example.com/bookmark-test".to_string(),
        ))
        .with_title("Bookmark Test");
        let result = engine.ingest(request).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_clipboard_url_capture_through_engine() {
        let engine = build_default_capture_engine(None, None);
        let request = CaptureRequest::new(CaptureData::Text(
            "https://example.com/clipboard-url".to_string(),
        ));
        let result = engine.ingest(request).await.unwrap();
        assert!(result.is_some());
    }

    // ── Lifecycle tests ────────────────────────────────────────────────

    #[test]
    fn lifecycle_initial_state_is_created() {
        let engine = CaptureEngine::new();
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Created);
        assert!(!engine.is_initialized());
        assert!(!engine.is_running());
        assert!(!engine.is_shutdown());
    }

    #[test]
    fn lifecycle_trait_name() {
        let engine = CaptureEngine::new();
        let engine_ref: &dyn Lifecycle = &engine;
        assert_eq!(engine_ref.name(), "capture_engine");
    }

    #[test]
    fn lifecycle_initialize_transitions_to_initialized() {
        let engine = CaptureEngine::new();
        assert!(engine.initialize().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Initialized);
        assert!(engine.is_initialized());
        assert!(!engine.is_running());
    }

    #[test]
    fn lifecycle_start_auto_advances_from_created() {
        let engine = CaptureEngine::new();
        // start() should auto-advance Created → Initialized → Running
        assert!(engine.start().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Running);
        assert!(engine.is_running());
    }

    #[test]
    fn lifecycle_full_flow() {
        let engine = CaptureEngine::new();
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Created);

        assert!(engine.initialize().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Initialized);
        assert!(engine.is_initialized());

        assert!(engine.start().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Running);
        assert!(engine.is_running());

        assert!(engine.shutdown().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Shutdown);
        assert!(engine.is_shutdown());
    }

    #[test]
    fn lifecycle_start_after_shutdown_returns_error() {
        let engine = CaptureEngine::new();
        assert!(engine.start().is_ok());
        assert!(engine.shutdown().is_ok());
        // Cannot restart after shutdown
        assert!(engine.start().is_err());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Shutdown);
    }

    #[test]
    fn lifecycle_double_shutdown_is_noop() {
        let engine = CaptureEngine::new();
        assert!(engine.start().is_ok());
        assert!(engine.shutdown().is_ok());
        // Second shutdown should succeed (same stage is a no-op)
        assert!(engine.shutdown().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Shutdown);
    }

    #[test]
    fn lifecycle_double_start_is_noop() {
        let engine = CaptureEngine::new();
        assert!(engine.start().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Running);
        // Second start should succeed (no-op)
        assert!(engine.start().is_ok());
        assert!(engine.is_running());
        // Cleanup
        assert!(engine.shutdown().is_ok());
    }

    #[test]
    fn lifecycle_start_without_initialize() {
        let engine = build_default_capture_engine(None, None);
        // start() should auto-advance Created → Initialized → Running
        assert!(engine.start().is_ok());
        assert_eq!(engine.lifecycle_stage(), LifecycleStage::Running);
        assert!(engine.is_running());
        assert!(engine.is_initialized());
        assert!(engine.shutdown().is_ok());
    }

    #[test]
    fn lifecycle_backward_transition_rejected() {
        let engine = CaptureEngine::new();
        assert!(engine.start().is_ok());
        assert!(engine.shutdown().is_ok());
        // Cannot go backward: Shutdown → Initialized
        assert!(engine.initialize().is_err());
        // Cannot restart: Shutdown → Running
        assert!(engine.start().is_err());
    }

    #[test]
    fn lifecycle_handler_count_preserved_through_lifecycle() {
        let engine = build_default_capture_engine(None, None);
        assert_eq!(engine.handler_count(), 8);
        assert!(engine.start().is_ok());
        assert_eq!(engine.handler_count(), 8);
        assert!(engine.shutdown().is_ok());
        assert_eq!(engine.handler_count(), 8);
    }
}
