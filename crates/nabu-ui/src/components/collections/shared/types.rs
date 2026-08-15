//! Shared types for the Collections module.
//!
//! `CollectionItem` mirrors the backend `NoteIndexEntry` (defined in
//! `src-tauri/src/commands.rs`) — it is a flat, deserialisable projection
//! that the four collection views share. Views never own data; they are
//! pure projections of `Vec<CollectionItem>`.

use serde::{Deserialize, Serialize};

/// One note in the vault index — mirrors the backend `NoteIndexEntry`.
///
/// The backend `notes_index` command returns this shape; the UI deserialises
/// directly into this struct and projects it into whichever view (Table,
/// Board, Gallery, Calendar) is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionItem {
    /// Vault-relative path (forward slashes). Used as the stable key/id.
    pub path: String,
    /// Display title (file name without `.md`).
    pub title: String,
    /// Parent folder ("" for the vault root).
    pub folder: String,
    /// Last modification time as an RFC 3339 string.
    pub modified_at: String,
    /// Whether the note is pinned.
    #[serde(default)]
    pub pinned: bool,
}

/// The four supported collection views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CollectionView {
    #[default]
    Table,
    Board,
    Gallery,
    Calendar,
}

/// Display label for each view, used by the view switcher.
impl CollectionView {
    pub fn label(self) -> &'static str {
        match self {
            CollectionView::Table => "Table",
            CollectionView::Board => "Board",
            CollectionView::Gallery => "Gallery",
            CollectionView::Calendar => "Calendar",
        }
    }

    /// All variants in display order.
    pub fn all() -> [CollectionView; 4] {
        [
            CollectionView::Table,
            CollectionView::Board,
            CollectionView::Gallery,
            CollectionView::Calendar,
        ]
    }
}

/// Filter state for the Table view.
#[derive(Clone, PartialEq, Default)]
pub struct TableFilter {
    pub query: String,
    pub object_type: Option<String>,
    pub sort_by: String,
    pub sort_ascending: bool,
}

/// Filter state for the Board view.
#[derive(Clone, PartialEq, Default)]
pub struct BoardFilter {
    pub query: String,
    pub object_type: Option<String>,
    pub group_by: String,
}

/// Filter state for the Gallery view.
#[derive(Clone, PartialEq, Default)]
pub struct GalleryFilter {
    pub query: String,
    pub object_type: Option<String>,
    pub sort_by: String,
    pub sort_ascending: bool,
}

/// Which calendar sub-view is active.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum CalendarViewMode {
    #[default]
    Month,
    Week,
    Day,
}

/// Filter state for the Calendar view.
#[derive(Clone, PartialEq, Default)]
pub struct CalendarFilter {
    pub query: String,
    pub object_type: Option<String>,
    pub view_mode: CalendarViewMode,
    pub group_by: String,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_item_serializes_round_trip() {
        let item = CollectionItem {
            path: "folder/note.md".to_string(),
            title: "note".to_string(),
            folder: "folder".to_string(),
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            pinned: true,
        };
        let json = serde_json::to_string(&item).unwrap();
        let back: CollectionItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, back);
    }

    #[test]
    fn pinned_defaults_to_false_on_missing_field() {
        let json = r#"{"path":"a.md","title":"a","folder":"","modified_at":"2024-01-01"}"#;
        let item: CollectionItem = serde_json::from_str(json).unwrap();
        assert!(!item.pinned);
    }

    #[test]
    fn collection_view_labels() {
        assert_eq!(CollectionView::Table.label(), "Table");
        assert_eq!(CollectionView::Board.label(), "Board");
        assert_eq!(CollectionView::Gallery.label(), "Gallery");
        assert_eq!(CollectionView::Calendar.label(), "Calendar");
    }

    #[test]
    fn collection_view_all_has_four() {
        assert_eq!(CollectionView::all().len(), 4);
    }
}
