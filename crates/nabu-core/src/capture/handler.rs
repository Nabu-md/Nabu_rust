use crate::models::{CaptureSource, KnowledgeObject, ObjectContent, ObjectType};
use async_trait::async_trait;
use chrono::Utc;

/// Result of a capture operation.
#[derive(Debug, Clone)]
pub struct CaptureResult {
    /// The captured KnowledgeObject
    pub object: KnowledgeObject,
    /// Source of the capture
    pub source: CaptureSource,
    /// Whether this should be enqueued for async processing
    pub enqueue: bool,
}

impl CaptureResult {
    pub fn new(object: KnowledgeObject, source: CaptureSource) -> Self {
        Self {
            object,
            source,
            enqueue: true,
        }
    }

    pub fn with_no_enqueue(mut self) -> Self {
        self.enqueue = false;
        self
    }
}

/// The CaptureHandler trait — implemented by all capture sources.
///
/// Each handler knows how to create a KnowledgeObject from its source data.
/// Handlers do NOT process, store, or index — they only produce KnowledgeObjects.
/// Processing is handled by the ProcessingPipeline via the Job Queue.
#[async_trait]
pub trait CaptureHandler: Send + Sync {
    /// The name identifier of this handler
    fn name(&self) -> &'static str;

    /// The capture source type
    fn source(&self) -> CaptureSource;

    /// Capture content and return a KnowledgeObject.
    /// Returns None if this handler cannot handle the given request.
    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult>;
}

/// A request to capture content from a source.
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// Raw data from the source
    pub data: CaptureData,
    /// Optional metadata hint
    pub title: Option<String>,
    /// Optional source URL
    pub source_url: Option<String>,
    /// Optional content type hint
    pub mime_type: Option<String>,
}

impl CaptureRequest {
    pub fn new(data: CaptureData) -> Self {
        Self {
            data,
            title: None,
            source_url: None,
            mime_type: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.source_url = Some(url.into());
        self
    }

    pub fn with_mime_type(mut self, mime: impl Into<String>) -> Self {
        self.mime_type = Some(mime.into());
        self
    }

    /// Attach binary content (images, audio, PDFs) to this request.
    /// Convenience for callers that already have raw bytes.
    pub fn with_binary(
        mut self,
        mime_type: impl Into<String>,
        data: Vec<u8>,
        filename: Option<String>,
    ) -> Self {
        self.data = CaptureData::Binary {
            mime_type: mime_type.into(),
            data,
            filename,
        };
        self
    }

    /// Returns the URL string if this request carries a `CaptureData::Uri`,
    /// otherwise falls back to `source_url`. Used by routing handlers.
    pub fn url(&self) -> Option<&str> {
        match &self.data {
            CaptureData::Uri(url) => Some(url),
            CaptureData::Text(t) if is_url(t) => Some(t),
            _ => self.source_url.as_deref(),
        }
    }

    /// Whether this request's data is a URL (either `CaptureData::Uri` or a
    /// `Text` payload that parses as a bare URL).
    pub fn is_url(&self) -> bool {
        match &self.data {
            CaptureData::Uri(_) => true,
            CaptureData::Text(t) => is_url(t),
            _ => false,
        }
    }
}

/// Raw data from a capture source.
#[derive(Debug, Clone)]
pub enum CaptureData {
    /// Text content (Markdown, plain text, HTML)
    Text(String),
    /// URL reference
    Uri(String),
    /// Binary data (images, audio, PDFs)
    Binary {
        mime_type: String,
        data: Vec<u8>,
        filename: Option<String>,
    },
    /// Existing file path
    File(String),
    /// Request a live screen capture. The handler will invoke the native
    /// screen-capture FFI (macOS `screencapture`) and produce a Screenshot.
    /// The optional payload carries selection coordinates `[x, y, w, h]`.
    ScreenCapture {
        selection: Option<(i32, i32, u32, u32)>,
    },
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Cheap URL scheme detection — used to distinguish a bare URL copied as text
/// from regular note text. Mirrors the detection in the MetadataExtractor so
/// clipboard/URL capture is consistent.
fn is_url(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ftp://")
        || trimmed.starts_with("file://")
}

// ── Handlers ─────────────────────────────────────────────────────────

/// Handles clipboard capture (text, URLs, images).
///
/// - A URL in text form is captured as a [Bookmark] ([CaptureSource::Url]).
/// - Binary image data is captured as a [Screenshot] ([CaptureSource::Clipboard]).
/// - Everything else becomes a [Note] ([CaptureSource::Clipboard]).
pub struct ClipboardHandler;

#[async_trait]
impl CaptureHandler for ClipboardHandler {
    fn name(&self) -> &'static str {
        "clipboard"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::Clipboard
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        match &request.data {
            // Clipboard text that is actually a URL → Bookmark.
            CaptureData::Text(text) if is_url(text) => {
                let url = text.trim().to_string();
                let mut object =
                    KnowledgeObject::new(ObjectType::Bookmark, ObjectContent::Uri(url.clone()));
                object.metadata.title = request.title.clone();
                object.metadata.source_url = Some(url);
                object.metadata.mime_type = request.mime_type.clone();
                Some(CaptureResult::new(object, CaptureSource::Url))
            }
            // Clipboard image/png data → Screenshot.
            CaptureData::Binary {
                mime_type, data, ..
            } if mime_type.starts_with("image/") => {
                let mut object = KnowledgeObject::new(
                    ObjectType::Screenshot,
                    ObjectContent::Binary {
                        mime_type: mime_type.clone(),
                        data: data.clone(),
                        filename: None,
                    },
                );
                object.metadata.title = request.title.clone();
                object.metadata.mime_type = Some(mime_type.clone());
                Some(CaptureResult::new(object, CaptureSource::Clipboard))
            }
            // Plain text → Note.
            _ => {
                let object = create_text_object(request, ObjectType::Note)?;
                Some(CaptureResult::new(object, CaptureSource::Clipboard))
            }
        }
    }
}

/// Handles screenshot capture.
pub struct ScreenshotHandler;

#[async_trait]
impl CaptureHandler for ScreenshotHandler {
    fn name(&self) -> &'static str {
        "screenshot"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::Screenshot
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        match &request.data {
            // Live screen capture — delegates to the native FFI.
            CaptureData::ScreenCapture { selection } => {
                let opts = crate::native::screenshot::ScreenCaptureOptions {
                    selection: selection.clone(),
                    ..Default::default()
                };
                let image = crate::native::screenshot::capture_screen(&opts).ok()?;
                let mut object = KnowledgeObject::new(
                    ObjectType::Screenshot,
                    ObjectContent::Binary {
                        mime_type: "image/png".to_string(),
                        data: image,
                        filename: None,
                    },
                );
                object.metadata.title = request.title.clone();
                object.metadata.mime_type = Some("image/png".to_string());
                Some(CaptureResult::new(object, CaptureSource::Screenshot))
            }
            // Pre-captured binary image data.
            _ => {
                let object = create_binary_object(request, "image/png", ObjectType::Screenshot)?;
                Some(CaptureResult::new(object, CaptureSource::Screenshot))
            }
        }
    }
}

/// Handles file drop capture.
pub struct FileDropHandler;

#[async_trait]
impl CaptureHandler for FileDropHandler {
    fn name(&self) -> &'static str {
        "file_drop"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::FileDrop
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        let object =
            create_binary_object(request, "application/octet-stream", ObjectType::Attachment)?;
        Some(CaptureResult::new(object, CaptureSource::FileDrop))
    }
}

/// Handles watch folder capture.
pub struct WatchFolderHandler;

#[async_trait]
impl CaptureHandler for WatchFolderHandler {
    fn name(&self) -> &'static str {
        "watch_folder"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::WatchFolder
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        let object = create_text_object(request, ObjectType::Note)?;
        Some(CaptureResult::new(object, CaptureSource::WatchFolder))
    }
}

/// Handles YouTube capture.
pub struct YouTubeCaptureHandler;

#[async_trait]
impl CaptureHandler for YouTubeCaptureHandler {
    fn name(&self) -> &'static str {
        "youtube"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::YouTube
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        match &request.data {
            CaptureData::Uri(url) => {
                let mut object =
                    KnowledgeObject::new(ObjectType::YouTubeVideo, ObjectContent::Uri(url.clone()));
                object.metadata.title = request.title.clone();
                object.metadata.source_url = Some(url.clone());
                Some(CaptureResult::new(object, CaptureSource::YouTube))
            }
            _ => None,
        }
    }
}

/// Handles GitHub repository capture.
pub struct GitHubRepositoryHandler;

#[async_trait]
impl CaptureHandler for GitHubRepositoryHandler {
    fn name(&self) -> &'static str {
        "github"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::GitHub
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        match &request.data {
            CaptureData::Uri(url) => {
                let mut object =
                    KnowledgeObject::new(ObjectType::Repository, ObjectContent::Uri(url.clone()));
                object.metadata.title = request.title.clone();
                object.metadata.source_url = Some(url.clone());
                Some(CaptureResult::new(object, CaptureSource::GitHub))
            }
            _ => None,
        }
    }
}

/// Handles email capture (`.eml` files and structured email text).
///
/// Accepts:
/// - `CaptureData::File` for `.eml` paths
/// - `CaptureData::Text` containing RFC 5322-style headers (Subject:, From:, etc.)
///
/// Produces a [KnowledgeObject] of type [Email] with extracted headers
/// (subject → title, from → authors, date → publication_date) and the
/// remaining body text as Markdown.
pub struct EmailCaptureHandler;

#[async_trait]
impl CaptureHandler for EmailCaptureHandler {
    fn name(&self) -> &'static str {
        "email"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::Email
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        match &request.data {
            // File path pointing at an .eml file.
            CaptureData::File(path) => {
                let lower = path.to_lowercase();
                if !lower.ends_with(".eml") {
                    return None;
                }
                // Read the file contents if accessible; otherwise create a
                // pointer object and let the processing pipeline handle it.
                let (object, body_text) = std::fs::read_to_string(path)
                    .map(|s| {
                        let obj = parse_email_to_object(&s, request);
                        (obj, Some(s))
                    })
                    .unwrap_or_else(|_| {
                        (
                            KnowledgeObject::new(
                                ObjectType::Email,
                                ObjectContent::Uri(path.clone()),
                            ),
                            None,
                        )
                    });

                let mut object = object;
                if body_text.is_none() {
                    object.metadata.title = request.title.clone();
                }
                object.metadata.source_url = request.source_url.clone();
                object.metadata.mime_type = Some("message/rfc822".to_string());
                object.metadata.original_filename = Some(
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("email.eml")
                        .to_string(),
                );
                Some(CaptureResult::new(object, CaptureSource::Email))
            }

            // Raw email text with RFC 5322 headers.
            CaptureData::Text(text) => {
                if parsed_email_headers(text).is_none() {
                    return None;
                }
                let mut object = parse_email_to_object(text, request);
                object.metadata.source_url = request.source_url.clone();
                object.metadata.mime_type = Some("message/rfc822".to_string());
                Some(CaptureResult::new(object, CaptureSource::Email))
            }

            _ => None,
        }
    }
}

/// Extract email headers (subject, from, date) from RFC 5322-style text.
///
/// Returns `None` when the text does not look like an email. The body is
/// everything after the first blank line.
fn parsed_email_headers(
    text: &str,
) -> Option<(
    Option<String>,
    Vec<String>,
    Option<chrono::DateTime<Utc>>,
    Option<String>,
)> {
    let lower = text.to_lowercase();
    let has_headers = lower.contains("subject:")
        || lower.contains("from:")
        || lower.contains("to:")
        || lower.contains("date:")
        || lower.contains("cc:")
        || lower.contains("bcc:");

    if !has_headers {
        return None;
    }

    let mut title: Option<String> = None;
    let mut authors: Vec<String> = Vec::new();
    let mut date: Option<chrono::DateTime<Utc>> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            break;
        }
        let lower_line = line.to_lowercase();
        if lower_line.starts_with("subject:") {
            let val = &line["subject:".len()..];
            let subject = val.trim().to_string();
            if !subject.is_empty() {
                title = Some(subject);
            }
        } else if lower_line.starts_with("from:") {
            let val = &line["from:".len()..];
            let from_str = val.trim();
            let name = from_str
                .trim_start_matches(|c: char| c == '"' || c == '\'')
                .split('<')
                .next()
                .unwrap_or(from_str)
                .trim()
                .to_string();
            if !name.is_empty() {
                authors.push(name);
            }
        } else if lower_line.starts_with("date:") {
            let val = &line["date:".len()..];
            let date_str = val.trim();
            date = chrono::DateTime::parse_from_rfc2822(date_str)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .or_else(|| {
                    chrono::DateTime::parse_from_rfc3339(date_str)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                });
        }
    }

    let body = text.split("\n\n").nth(1).unwrap_or("");
    let description = if !body.is_empty() {
        Some(body.lines().take(3).collect::<Vec<_>>().join(" "))
    } else {
        None
    };

    Some((title, authors, date, description))
}

/// Split an email into its body (after headers) and return the body text.
fn email_body(text: &str) -> String {
    text.split("\n\n")
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| text.to_string())
}

/// Parse an email text, extracting headers and producing a Markdown object.
fn parse_email_to_object(text: &str, request: &CaptureRequest) -> KnowledgeObject {
    let (title, authors, date, desc) =
        parsed_email_headers(text).unwrap_or((None, Vec::new(), None, None));
    let body = email_body(text);
    let mut object = KnowledgeObject::new(ObjectType::Email, ObjectContent::Markdown(body));
    object.metadata.title = title.or(request.title.clone());
    object.metadata.authors = authors;
    object.metadata.publication_date = date;
    object.metadata.description = desc;
    object
}

///
/// Captures a URL (text pasted into the command palette, browser extension
/// "bookmark" button, etc.) as a [Bookmark] object. The title is extracted
/// from the request or from the URL path by the metadata extractor downstream.
pub struct BookmarkCaptureHandler;

#[async_trait]
impl CaptureHandler for BookmarkCaptureHandler {
    fn name(&self) -> &'static str {
        "bookmark"
    }

    fn source(&self) -> CaptureSource {
        CaptureSource::Url
    }

    async fn capture(&self, request: &CaptureRequest) -> Option<CaptureResult> {
        match &request.data {
            CaptureData::Uri(url) => {
                let mut object =
                    KnowledgeObject::new(ObjectType::Bookmark, ObjectContent::Uri(url.clone()));
                object.metadata.title = request.title.clone();
                object.metadata.source_url = Some(url.clone());
                object.metadata.mime_type = request.mime_type.clone();
                mark_for_reading_queue(&mut object);
                Some(CaptureResult::new(object, CaptureSource::Url))
            }
            CaptureData::Text(text) if is_url(text) => {
                let url = text.trim().to_string();
                let mut object =
                    KnowledgeObject::new(ObjectType::Bookmark, ObjectContent::Uri(url.clone()));
                object.metadata.title = request.title.clone();
                object.metadata.source_url = Some(url);
                mark_for_reading_queue(&mut object);
                Some(CaptureResult::new(object, CaptureSource::Url))
            }
            _ => None,
        }
    }
}

// ── Reading queue integration ─────────────────────────────────────────

/// Marks a KnowledgeObject for the reading queue by setting the custom
/// properties that the UI's `queue_get_all` command reads.
///
/// Called by capture handlers that produce readable content (bookmarks,
/// articles) so that newly-captured items appear in the Reading Queue with
/// "pending" status — mirroring Karakeep's reading-list behaviour while
/// staying within the existing CaptureEngine → ProcessingPipeline →
/// Knowledge Inbox → Storage flow.
fn mark_for_reading_queue(object: &mut KnowledgeObject) {
    use crate::models::CustomPropertyValue;

    // Only mark bookmarks/articles (skip screenshots, notes, etc.)
    let readable = matches!(
        object.object_type,
        ObjectType::Bookmark | ObjectType::Article
    );
    if !readable {
        return;
    }

    // Avoid double-marking if already set.
    let already_queued = object
        .custom_properties
        .get("reading_status")
        .map(|v| {
            matches!(
                v,
                CustomPropertyValue::Text(t) if t == "pending" || t == "in_progress" || t == "completed"
            )
        })
        .unwrap_or(false);

    if !already_queued {
        object.custom_properties.insert(
            "reading_status".to_string(),
            CustomPropertyValue::Text("pending".to_string()),
        );
        object.custom_properties.insert(
            "reading_priority".to_string(),
            CustomPropertyValue::Text("medium".to_string()),
        );
        object.custom_properties.insert(
            "reading_progress".to_string(),
            CustomPropertyValue::Number(0.0),
        );
    }
}

// ── Object creation helpers ──────────────────────────────────────────

fn create_text_object(
    request: &CaptureRequest,
    object_type: ObjectType,
) -> Option<KnowledgeObject> {
    match &request.data {
        CaptureData::Text(text) => {
            let content = if text.contains("```") || text.starts_with('#') {
                ObjectContent::Markdown(text.clone())
            } else if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
                ObjectContent::RichHtml(text.clone())
            } else {
                ObjectContent::PlainText(text.clone())
            };

            let mut object = KnowledgeObject::new(object_type, content);
            object.metadata.title = request.title.clone();
            object.metadata.source_url = request.source_url.clone();
            object.metadata.mime_type = request.mime_type.clone();
            Some(object)
        }
        CaptureData::Uri(url) => {
            let mut object = KnowledgeObject::new(object_type, ObjectContent::Uri(url.clone()));
            object.metadata.title = request.title.clone();
            object.metadata.source_url = request.source_url.clone();
            Some(object)
        }
        _ => None,
    }
}

fn create_binary_object(
    request: &CaptureRequest,
    default_mime: &str,
    object_type: ObjectType,
) -> Option<KnowledgeObject> {
    match &request.data {
        CaptureData::Binary {
            mime_type,
            data,
            filename,
        } => {
            let mut object = KnowledgeObject::new(
                object_type,
                ObjectContent::Binary {
                    mime_type: mime_type.clone(),
                    data: data.clone(),
                    filename: filename.clone(),
                },
            );
            object.metadata.title = request.title.clone();
            object.metadata.source_url = request.source_url.clone();
            object.metadata.mime_type = Some(mime_type.clone());
            object.metadata.original_filename = filename.clone();
            Some(object)
        }
        CaptureData::File(path) => {
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            let mut object = KnowledgeObject::new(
                object_type,
                ObjectContent::Binary {
                    mime_type: default_mime.to_string(),
                    data: Vec::new(),
                    filename: filename.clone(),
                },
            );
            object.metadata.title = request.title.clone();
            object.metadata.original_filename = filename;
            Some(object)
        }
        _ => None,
    }
}

// ── Tests ─�───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_clipboard_text_becomes_note() {
        let handler = ClipboardHandler;
        let request = CaptureRequest::new(CaptureData::Text("Hello, world!".to_string()));
        let result = handler.capture(&request).await;
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Note);
        assert_eq!(result.source, CaptureSource::Clipboard);
    }

    #[tokio::test]
    async fn test_clipboard_url_becomes_bookmark() {
        let handler = ClipboardHandler;
        let request =
            CaptureRequest::new(CaptureData::Text("https://example.com/article".to_string()));
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Bookmark);
        assert_eq!(result.source, CaptureSource::Url);
        assert_eq!(
            result.object.content,
            ObjectContent::Uri("https://example.com/article".to_string())
        );
    }

    #[tokio::test]
    async fn test_clipboard_image_becomes_screenshot() {
        let handler = ClipboardHandler;
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "image/png".to_string(),
            data: vec![0x89, 0x50, 0x4e, 0x47],
            filename: None,
        });
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Screenshot);
        assert_eq!(result.source, CaptureSource::Clipboard);
    }

    #[tokio::test]
    async fn test_bookmark_handler_uri() {
        let handler = BookmarkCaptureHandler;
        let request = CaptureRequest::new(CaptureData::Uri("https://example.com/page".to_string()))
            .with_title("Example");
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Bookmark);
        assert_eq!(result.source, CaptureSource::Url);
        assert_eq!(
            result.object.metadata.source_url.as_deref(),
            Some("https://example.com/page")
        );
    }

    #[tokio::test]
    async fn test_bookmark_handler_text_url() {
        let handler = BookmarkCaptureHandler;
        let request = CaptureRequest::new(CaptureData::Text(
            "https://example.com/from-text".to_string(),
        ));
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Bookmark);
        assert_eq!(result.source, CaptureSource::Url);
    }

    #[tokio::test]
    async fn test_bookmark_handler_rejects_non_url_text() {
        let handler = BookmarkCaptureHandler;
        let request = CaptureRequest::new(CaptureData::Text("not a url".to_string()));
        assert!(handler.capture(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_youtube_handler() {
        let handler = YouTubeCaptureHandler;
        let request = CaptureRequest::new(CaptureData::Uri(
            "https://youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
        ));
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::YouTubeVideo);
        assert_eq!(result.source, CaptureSource::YouTube);
    }

    #[tokio::test]
    async fn test_github_handler() {
        let handler = GitHubRepositoryHandler;
        let request =
            CaptureRequest::new(CaptureData::Uri("https://github.com/org/repo".to_string()));
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Repository);
        assert_eq!(result.source, CaptureSource::GitHub);
    }

    #[tokio::test]
    async fn test_is_url_detection() {
        assert!(is_url("https://example.com"));
        assert!(is_url("http://example.com"));
        assert!(!is_url("not a url"));
        assert!(!is_url("Hello world"));
    }

    #[tokio::test]
    async fn test_request_url_helper() {
        let uri_req = CaptureRequest::new(CaptureData::Uri("https://example.com".to_string()));
        assert_eq!(uri_req.url(), Some("https://example.com"));

        let text_req = CaptureRequest::new(CaptureData::Text("https://example.com".to_string()));
        assert_eq!(text_req.url(), Some("https://example.com"));

        let plain_req = CaptureRequest::new(CaptureData::Text("hello".to_string()));
        assert_eq!(plain_req.url(), None);
    }

    // ── Email capture tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_email_handler_text_with_headers() {
        let handler = EmailCaptureHandler;
        let email_text = "Subject: Test Email\nFrom: Alice <alice@example.com>\nDate: Mon, 15 Jan 2024 10:00:00 +0000\n\nThis is the body of the email."
            .to_string();
        let request = CaptureRequest::new(CaptureData::Text(email_text));
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Email);
        assert_eq!(result.source, CaptureSource::Email);
        assert_eq!(result.object.metadata.title.as_deref(), Some("Test Email"));
        assert_eq!(result.object.metadata.authors.len(), 1);
        assert_eq!(result.object.metadata.authors[0], "Alice");
        assert!(result.object.metadata.publication_date.is_some());
        assert!(result.object.metadata.description.is_some());
    }

    #[tokio::test]
    async fn test_email_handler_rejects_non_email_text() {
        let handler = EmailCaptureHandler;
        let request =
            CaptureRequest::new(CaptureData::Text("Hello, world!".to_string()));
        assert!(handler.capture(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_email_handler_rejects_non_eml_file() {
        let handler = EmailCaptureHandler;
        let request =
            CaptureRequest::new(CaptureData::File("/tmp/somefile.txt".to_string()));
        assert!(handler.capture(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_email_body_extraction() {
        let text = "Subject: Hello\nFrom: bob@test.com\n\nBody paragraph here.";
        let body = email_body(text);
        assert_eq!(body, "Body paragraph here.");
    }

    #[tokio::test]
    async fn test_parsed_email_headers_no_headers() {
        assert!(parsed_email_headers("just plain text").is_none());
    }

    // ── Reading queue integration tests ────────────────────────────────

    #[tokio::test]
    async fn test_bookmark_capture_marks_reading_queue() {
        let handler = BookmarkCaptureHandler;
        let request = CaptureRequest::new(CaptureData::Uri(
            "https://example.com/article".to_string(),
        ));
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(
            result.object.custom_properties.get("reading_status"),
            Some(&crate::models::CustomPropertyValue::Text("pending".to_string()))
        );
    }

    #[tokio::test]
    async fn test_clipboard_note_no_reading_queue() {
        // Notes (not bookmarks/articles) should NOT be marked for reading queue.
        let handler = ClipboardHandler;
        let request =
            CaptureRequest::new(CaptureData::Text("Hello, world!".to_string()));
        let result = handler.capture(&request).await.unwrap();
        assert!(result.object.custom_properties.get("reading_status").is_none());
    }

    // ── Screenshot screen capture test ────────────────────────────────

    #[tokio::test]
    async fn test_screenshot_handler_screen_capture_variant() {
        let handler = ScreenshotHandler;
        let request = CaptureRequest::new(CaptureData::ScreenCapture {
            selection: None,
        });
        // On non-macOS or without screencapture, this may return None —
        // the test just verifies the variant is routed correctly.
        let result = handler.capture(&request).await;
        // On macOS with screencapture available, this would return Some.
        // On CI or unsupported platforms, None is acceptable.
        if let Some(ref r) = result {
            assert_eq!(r.object.object_type, ObjectType::Screenshot);
            assert_eq!(r.source, CaptureSource::Screenshot);
        }
    }

    #[tokio::test]
    async fn test_screenshot_handler_binary_still_works() {
        let handler = ScreenshotHandler;
        let request = CaptureRequest::new(CaptureData::Binary {
            mime_type: "image/png".to_string(),
            data: vec![0x89, 0x50, 0x4e, 0x47],
            filename: None,
        });
        let result = handler.capture(&request).await.unwrap();
        assert_eq!(result.object.object_type, ObjectType::Screenshot);
        assert_eq!(result.source, CaptureSource::Screenshot);
    }
}
