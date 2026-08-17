//! Inbox data model and shared classification helpers.
//!
//! Everything here is pure (no `StorageManager` / `Indexer` dependency) so the
//! filing service's decisions — *where* an item lands, *what* status it gets and
//! *when* it is considered ready — can be unit-tested in isolation.

use uuid::Uuid;

use crate::models::{CustomPropertyValue, KnowledgeObject, ObjectContent, ObjectType};

/// The lifecycle status of an inbox capture, persisted in the object's custom
/// properties as the text key `inbox_status`.
///
/// The vocabulary deliberately mirrors the status strings consumed by the
/// existing Tauri `inbox_*` commands so the core filing service and the UI
/// layer cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxItemStatus {
    /// Newly captured, awaiting triage.
    Pending,
    /// Being analysed / classified by the capture pipeline.
    Processing,
    /// Classification complete, ready for the human to triage.
    Ready,
    /// Approved — promoted into a real vault artifact.
    Approved,
    /// Dismissed by the user.
    Rejected,
    /// Automatic processing failed.
    Failed,
}

impl InboxItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InboxItemStatus::Pending => "pending",
            InboxItemStatus::Processing => "processing",
            InboxItemStatus::Ready => "ready",
            InboxItemStatus::Approved => "approved",
            InboxItemStatus::Rejected => "rejected",
            InboxItemStatus::Failed => "failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            InboxItemStatus::Approved | InboxItemStatus::Rejected | InboxItemStatus::Failed
        )
    }

    /// Whether the item is "active" (still awaiting human triage) and therefore
    /// eligible to appear in the inbox work queue.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            InboxItemStatus::Pending | InboxItemStatus::Processing | InboxItemStatus::Ready
        )
    }
}

impl std::fmt::Display for InboxItemStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Reads the inbox status of an object, defaulting to `Pending` for freshly
/// captured items that haven't been tagged yet.
pub fn inbox_item_status(obj: &KnowledgeObject) -> InboxItemStatus {
    match obj.custom_property_text("inbox_status").as_deref() {
        Some("pending") => InboxItemStatus::Pending,
        Some("processing") => InboxItemStatus::Processing,
        Some("ready") => InboxItemStatus::Ready,
        Some("approved") => InboxItemStatus::Approved,
        Some("rejected") => InboxItemStatus::Rejected,
        Some("failed") => InboxItemStatus::Failed,
        // An object with `inbox_status` unset is treated as freshly captured.
        Some(other) if other.is_empty() => InboxItemStatus::Pending,
        None => InboxItemStatus::Pending,
        Some(_) => InboxItemStatus::Pending,
    }
}

/// Writes the inbox status to the object's custom properties.
pub fn set_status(obj: &mut KnowledgeObject, status: InboxItemStatus) {
    obj.custom_properties.insert(
        "inbox_status".to_string(),
        CustomPropertyValue::Text(status.as_str().to_string()),
    );
}

/// The capture pipeline leaves an object in `inbox_status = "ready"` once it has
/// enough information (title, content type) to be filed.  An object that only
/// carries `inbox_status = "pending"` is treated as not yet triaged.
pub fn classify_ready(obj: &KnowledgeObject) -> bool {
    if let Some(current) = obj.custom_property_text("inbox_status") {
        return current == "ready";
    }
    // Back-compat: legacy captures only set `suggested_folder` or
    // `classification` to advertise "ready to file".
    obj.custom_properties.contains_key("suggested_folder")
        || obj.custom_properties.contains_key("classification")
}

/// Resolve the vault folder an inbox item should be filed into.
///
/// Resolution order (high → low):
/// 1. A user-chosen destination set by `inbox_move` (`destination_folder`).
/// 2. An AutoFiler suggestion (`suggested_folder`).
/// 3. The default `"Inbox"` folder.
pub fn resolve_destination(obj: &KnowledgeObject) -> String {
    let folder = obj
        .custom_property_text("destination_folder")
        .or_else(|| obj.custom_property_text("suggested_folder"))
        .unwrap_or_else(|| "Inbox".to_string());

    let folder = folder.trim().trim_matches('/').trim().to_string();
    if folder.is_empty() {
        "Inbox".to_string()
    } else {
        folder
    }
}

/// Render an inbox capture's content as Markdown body text.
///
/// Text captures (`Markdown`, `PlainText`, `Uri`, `RichHtml`) become a Markdown
/// note.  Binary captures are returned as `None` — they are filed as their
/// native file (see [`binary_extension`]) so no data is lost.
pub fn render_markdown(obj: &KnowledgeObject) -> Option<String> {
    let title = obj.metadata.title.as_deref().unwrap_or("Untitled");

    match &obj.content {
        ObjectContent::Markdown(s) | ObjectContent::PlainText(s) => Some(s.clone()),
        ObjectContent::Uri(s) => {
            let url = s.trim();
            if url.is_empty() {
                Some(title.to_string())
            } else {
                Some(format!("# {title}\n\n[{url}]({url})\n"))
            }
        }
        ObjectContent::RichHtml(s) => Some(strip_html(s)),
        ObjectContent::Binary { .. } => None,
    }
}

/// Filed extension for a binary capture: prefer the original filename's
/// extension, then a MIME-type heuristic.  Owned so it can borrow from the
/// object's metadata without lifetime friction.
pub fn binary_extension(obj: &KnowledgeObject) -> String {
    if let Some(filename) = &obj.metadata.original_filename {
        if let Some(ext) = filename.rsplit('.').next() {
            if !ext.is_empty() && ext.len() <= 5 {
                return ext.to_string();
            }
        }
    }

    match obj.metadata.mime_type.as_deref().unwrap_or("") {
        m if m.starts_with("image/png") => "png".to_string(),
        m if m.starts_with("image/jpeg") => "jpg".to_string(),
        m if m.starts_with("image/svg") => "svg".to_string(),
        m if m.starts_with("image/webp") => "webp".to_string(),
        m if m.starts_with("audio/") => "mp3".to_string(),
        m if m.starts_with("video/") => "mp4".to_string(),
        m if m == "application/pdf" => "pdf".to_string(),
        _ => "bin".to_string(),
    }
}

/// A compact, filesystem-safe slug derived from a title.
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_sep = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_sep = false;
        } else {
            if !prev_sep && !out.is_empty() {
                out.push('-');
                prev_sep = true;
            }
        }
    }
    out.trim_matches('-').to_string()
}

/// Very small, regex-free HTML → text strip for turning RichHtml captures into
/// a Markdown body.  It is *safe* (no HTML is emitted) rather than lossless.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut last_was_space = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
            }
            '>' => {
                in_tag = false;
                // Tags become whitespace so adjacent words don't run together.
                if !last_was_space && !out.is_empty() {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ if in_tag => {}
            c if c.is_whitespace() => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            c => {
                out.push(c);
                last_was_space = false;
            }
        }
    }
    // Decode the handful of entities most likely to appear in captured pages.
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    decoded.trim().to_string()
}

/// SHA-256 content fingerprint, base16 lowercased.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let bytes = hasher.finalize();
    hex::encode(bytes)
}

/// Convenience: build a freshly-captured inbox object for tests / quick capture.
pub fn build_inbox_object(content: ObjectContent, title: Option<&str>) -> KnowledgeObject {
    let mut obj = KnowledgeObject::new(ObjectType::Note, content);
    obj.metadata.title = title.map(String::from);
    set_status(&mut obj, InboxItemStatus::Pending);
    obj
}

/// The object id typed as a convenience alias used by the filing service's
/// error type.
pub fn object_id(obj: &KnowledgeObject) -> Uuid {
    obj.id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ObjectContent;

    fn with_status(content: ObjectContent, title: &str, status: &str) -> KnowledgeObject {
        let mut obj = KnowledgeObject::new(ObjectType::Note, content);
        obj.metadata.title = Some(title.to_string());
        obj.custom_properties.insert(
            "inbox_status".to_string(),
            CustomPropertyValue::Text(status.to_string()),
        );
        obj
    }

    // --- InboxItemStatus lifecycle ---

    #[test]
    fn status_lifecycle_active_vs_terminal() {
        assert!(InboxItemStatus::Pending.is_active());
        assert!(InboxItemStatus::Processing.is_active());
        assert!(InboxItemStatus::Ready.is_active());
        assert!(!InboxItemStatus::Approved.is_active());
        assert!(!InboxItemStatus::Rejected.is_active());
        assert!(!InboxItemStatus::Failed.is_active());

        assert!(!InboxItemStatus::Pending.is_terminal());
        assert!(InboxItemStatus::Approved.is_terminal());
        assert!(InboxItemStatus::Rejected.is_terminal());
        assert!(InboxItemStatus::Failed.is_terminal());
    }

    #[test]
    fn inbox_item_status_reads_custom_property() {
        let mut obj = KnowledgeObject::new(ObjectType::Note, ObjectContent::PlainText("x".into()));
        // Unset → defaults to pending (freshly captured).
        assert_eq!(inbox_item_status(&obj), InboxItemStatus::Pending);

        for (raw, status) in [
            ("pending", InboxItemStatus::Pending),
            ("ready", InboxItemStatus::Ready),
            ("approved", InboxItemStatus::Approved),
            ("rejected", InboxItemStatus::Rejected),
            ("failed", InboxItemStatus::Failed),
        ] {
            set_status(&mut obj, status);
            assert_eq!(inbox_item_status(&obj), status, "status {raw} round-trips");
            assert_eq!(
                obj.custom_property_text("inbox_status").as_deref(),
                Some(raw)
            );
        }
    }

    #[test]
    fn classify_ready_distinguishes_ready_from_pending() {
        let ready = with_status(ObjectContent::Markdown(".".into()), "R", "ready");
        assert!(classify_ready(&ready), "ready status is ready");

        let pending = with_status(ObjectContent::Markdown(".".into()), "P", "pending");
        assert!(!classify_ready(&pending), "pending is not ready");

        // Legacy capture: no `inbox_status`, but a `suggested_folder` marker →
        // advertised as ready by the back-compat branch.
        let mut legacy =
            KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(".".into()));
        legacy.metadata.title = Some("legacy".into());
        legacy.custom_properties.insert(
            "suggested_folder".to_string(),
            CustomPropertyValue::Text("Suggestions".into()),
        );
        assert!(
            classify_ready(&legacy),
            "legacy suggested_folder marks an item ready"
        );

        // A freshly-built pending object (explicit status) is not ready.
        assert!(!classify_ready(&build_inbox_object(
            ObjectContent::Markdown(".".into()),
            Some("legacy")
        )));
    }

    // --- resolve_destination ---

    #[test]
    fn destination_resolution_order() {
        let mut obj = KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(".".into()));
        obj.metadata.title = Some("T".into());

        // No markers → default Inbox.
        assert_eq!(resolve_destination(&obj), "Inbox");

        // suggested_folder is the fallback when no user dest is set.
        obj.custom_properties.insert(
            "suggested_folder".to_string(),
            CustomPropertyValue::Text("Articles".to_string()),
        );
        assert_eq!(resolve_destination(&obj), "Articles");

        // destination_folder (user-chosen) wins over suggested_folder.
        obj.custom_properties.insert(
            "destination_folder".to_string(),
            CustomPropertyValue::Text("Projects/Nabu".to_string()),
        );
        assert_eq!(resolve_destination(&obj), "Projects/Nabu");
    }

    #[test]
    fn destination_trims_slashes_and_whitespace() {
        let mut obj = KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(".".into()));
        obj.metadata.title = Some("T".into());
        obj.custom_properties.insert(
            "destination_folder".to_string(),
            CustomPropertyValue::Text("  /Writing/Notes/  ".to_string()),
        );
        assert_eq!(resolve_destination(&obj), "Writing/Notes");

        // Empty after trimming → default Inbox.
        obj.custom_properties.insert(
            "destination_folder".to_string(),
            CustomPropertyValue::Text("///".into()),
        );
        assert_eq!(resolve_destination(&obj), "Inbox");
    }

    // --- slugify ---

    #[test]
    fn slugify_replaces_separators_and_lowercases() {
        assert_eq!(slugify("My Cool Note"), "my-cool-note");
        assert_eq!(slugify("a/b c"), "a-b-c");
        assert_eq!(slugify("  Spaced  "), "spaced");
        assert_eq!(slugify("!!!"), "");
    }

    // --- render_markdown ---

    #[test]
    fn render_markdown_passthrough_for_text() {
        let mk = KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown("# Hi".into()));
        assert_eq!(render_markdown(&mk), Some("# Hi".to_string()));

        let txt = KnowledgeObject::new(ObjectType::Note, ObjectContent::PlainText("plain".into()));
        assert_eq!(render_markdown(&txt), Some("plain".to_string()));
    }

    #[test]
    fn render_markdown_turns_uri_into_bookmark() {
        let mut obj =
            KnowledgeObject::new(ObjectType::Note, ObjectContent::Uri("https://x.com".into()));
        obj.metadata.title = Some("Bookmark".into());
        assert_eq!(
            render_markdown(&obj),
            Some("# Bookmark\n\n[https://x.com](https://x.com)\n".to_string())
        );
    }

    #[test]
    fn render_markdown_strips_html() {
        let mut obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::RichHtml("<p>Hello <b>world</b> &amp; <i>friends</i></p>".into()),
        );
        obj.metadata.title = Some("Article".into());
        let md = render_markdown(&obj).expect("html renders to markdown");
        assert!(md.contains("Hello world"));
        assert!(md.contains('&'));
        assert!(!md.contains('<'));
        assert!(!md.contains('>'));
    }

    #[test]
    fn render_markdown_binary_is_none() {
        let mut obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Binary {
                mime_type: "image/png".into(),
                data: vec![1, 2, 3],
                filename: Some("shot.png".into()),
            },
        );
        obj.metadata.title = Some("Screenshot".into());
        assert_eq!(render_markdown(&obj), None);
    }

    // --- binary_extension ---

    #[test]
    fn binary_extension_prefers_filename_then_mime() {
        let mut obj = KnowledgeObject::new(ObjectType::Note, ObjectContent::PlainText(".".into()));
        obj.metadata.original_filename = Some("photo.JPEG".into());
        assert_eq!(binary_extension(&obj), "JPEG");

        obj.metadata.original_filename = None;
        obj.metadata.mime_type = Some("image/png".into());
        assert_eq!(binary_extension(&obj), "png");

        obj.metadata.mime_type = None;
        assert_eq!(binary_extension(&obj), "bin");
    }

    // --- sha256_hex ---

    #[test]
    fn sha256_is_deterministic_and_64_chars() {
        let h1 = sha256_hex(b"hello");
        let h2 = sha256_hex(b"hello");
        let h3 = sha256_hex(b"world");
        assert_eq!(h1.len(), 64);
        assert_eq!(h1, h2, "same input → same hash");
        assert_ne!(h1, h3, "different input → different hash");
    }
}
