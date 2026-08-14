//! Content-derived link parsing for the knowledge graph.
//!
//! This module provides the primitives used by [`crate::graph`] to derive
//! *content-derived edges* (wiki-links and block references) from a note's
//! Markdown body, and a resolution index that maps link text to object UUIDs
//! so those edges can be materialised during a graph rebuild.
//!
//! This is intentionally kept dependency-light (regex-free for the common case)
//! so the graph recovery path stays fast and has no extra moving parts beyond
//! what [`crate::graph::serializer`] already requires.
//!
//! NOTE: This module was scaffolded to unblock compilation of the in-progress
//! graph-recovery/event-wiring modules. Only the symbols they consume are
//! implemented; the full wikilink engine (transitive resolution, alias tables,
//! block-definition indexing) is deferred to Phase 1A-2.

use crate::models::{KnowledgeObject, ObjectContent};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// A wiki-link as it appears in Markdown body text: the inner text of `[[...]]`.
///
/// Examples:
/// - `[[My Note Title]]` → `"My Note Title"`
/// - `[[some/path/to/note]]` → `"some/path/to/note"`
pub fn parse_wiki_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the opening `[[`.
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Find the closing `]]`, respecting embedded backticks so that
            // `[[code with [brackets]]]` doesn't terminate early.
            if let Some(end) = find_wiki_link_close(text, i + 2) {
                let inner = text[i + 2..end].trim().to_string();
                if !inner.is_empty() {
                    links.push(inner);
                }
                i = end + 2;
                continue;
            }
        }
        i += 1;
    }
    links
}

/// Find the index of the closing `]]` for a wiki-link starting after position
/// `start` (which is just past the opening `[[`).  Skips over inline code spans
/// delimited by backticks so bracket-like content inside code isn't treated as
/// a closing delimiter.
fn find_wiki_link_close(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'`' {
            // Skip to the next backtick (naive single-backtick code span).
            i += 1;
            while i < bytes.len() && bytes[i] != b'`' {
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b']' && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Extract the inner text of block-reference links.
///
/// Recognises the `((reference))` transclusion syntax (used for block/embed
/// references).  Returns the raw inner text of each reference, trimmed.
pub fn parse_block_references(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'(' && bytes[i + 1] == b'(' {
            if let Some(end) = find_block_ref_close(text, i + 2) {
                let inner = text[i + 2..end].trim().to_string();
                if !inner.is_empty() {
                    refs.push(inner);
                }
                i = end + 2;
                continue;
            }
        }
        i += 1;
    }
    refs
}

/// Find the closing `))` for a block reference beginning after `start`.
fn find_block_ref_close(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b')' && i + 1 < bytes.len() && bytes[i + 1] == b')' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Project the searchable text content of a [`ObjectContent`] variant onto a
/// `&str`.  Binary content has no textual body, so an empty slice is returned.
///
/// Used by content-edge derivation so the same parsing functions can operate
/// uniformly on any object type.
pub fn content_as_str(content: &ObjectContent) -> &str {
    match content {
        ObjectContent::Markdown(s)
        | ObjectContent::RichHtml(s)
        | ObjectContent::PlainText(s)
        | ObjectContent::Uri(s) => s,
        ObjectContent::Binary { .. } => "",
    }
}

/// Indexes objects by the keys wiki-links and block references are resolved
/// against (title and vault path), so a `[[link]]` can be mapped to a target
/// object UUID during graph rebuild.
#[derive(Debug, Clone, Default)]
pub struct ResolutionIndex {
    /// Lower-cased display title → object id.
    title_to_id: HashMap<String, Uuid>,
    /// Lower-cased vault-path stem (filename without extension) → object id.
    path_to_id: HashMap<String, Uuid>,
}

impl ResolutionIndex {
    /// Build an index from a slice of canonical objects.
    pub fn from_objects(objects: &[KnowledgeObject]) -> Self {
        let mut title_to_id = HashMap::new();
        let mut path_to_id = HashMap::new();
        for obj in objects {
            if let Some(title) = &obj.metadata.title {
                if !title.is_empty() {
                    title_to_id.insert(title.to_lowercase(), obj.id);
                }
            }
            if let Some(vault_path) = &obj.metadata.vault_path {
                let stem = Path::new(vault_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if !stem.is_empty() {
                    path_to_id.insert(stem.to_lowercase(), obj.id);
                }
            }
        }
        Self {
            title_to_id,
            path_to_id,
        }
    }

    /// Resolve a wiki-link target (title or path stem) to an object id.
    pub fn resolve_wiki_link(&self, link: &str) -> Option<Uuid> {
        let key = link.to_lowercase();
        self.title_to_id
            .get(&key)
            .copied()
            .or_else(|| self.path_to_id.get(&key).copied())
    }

    /// Resolve a block-reference target to an object id.
    ///
    /// Block references typically point at a note by title; resolution uses the
    /// same index as wiki-links.
    pub fn resolve_block_ref(&self, link: &str) -> Option<Uuid> {
        self.resolve_wiki_link(link)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ObjectContent, ObjectMetadata, ObjectType};

    fn obj(id: uuid::Uuid, title: &str, vault_path: Option<&str>, content: &str) -> KnowledgeObject {
        let mut o = KnowledgeObject::new(ObjectType::Note, ObjectContent::Markdown(content.into()));
        o.metadata.title = Some(title.into());
        if let Some(p) = vault_path {
            o.metadata.vault_path = Some(p.into());
        }
        o.id = id;
        o
    }

    #[test]
    fn parse_wiki_links_extracts_bracketed_text() {
        let text = "See [[My Note]] and [[Another Page]] for details.";
        let links = parse_wiki_links(text);
        assert_eq!(links, vec!["My Note".to_string(), "Another Page".to_string()]);
    }

    #[test]
    fn parse_wiki_links_ignores_unclosed_and_code() {
        let text = "[[open one]] and [[no close, [[code with ]]] here";
        let links = parse_wiki_links(text);
        // The first complete link; the trailing `[[no close` has no `]]` close.
        assert!(links.iter().any(|l| l == "open one"));
    }

    #[test]
    fn parse_block_references_extracts_parens() {
        let text = "Reference ((my-ref)) inline and ((another)) here";
        assert_eq!(parse_block_references(text), vec!["my-ref", "another"]);
    }

    #[test]
    fn content_as_str_variants() {
        let m = ObjectContent::Markdown("body".into());
        assert_eq!(content_as_str(&m), "body");
        let b = ObjectContent::Binary {
            mime_type: "image/png".into(),
            data: vec![],
            filename: None,
        };
        assert_eq!(content_as_str(&b), "");
    }

    #[test]
    fn resolution_index_resolves_by_title_and_path() {
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let objects = vec![
            obj(id1, "My Note", Some("Inbox/My Note.md"), "[[Other]]"),
            obj(id2, "Other", Some("Other.md"), "Hello"),
        ];
        let index = ResolutionIndex::from_objects(&objects);
        assert_eq!(index.resolve_wiki_link("My Note"), Some(id1));
        assert_eq!(index.resolve_wiki_link("my note"), Some(id1)); // case-insensitive
        assert_eq!(index.resolve_wiki_link("other"), Some(id2));
        assert_eq!(index.resolve_block_ref("Other"), Some(id2));
        assert_eq!(index.resolve_wiki_link("Missing"), None);
    }
}
