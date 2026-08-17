//! Normalized file-system change events emitted by the vault watcher.
//!
//! These types are the watcher's *stable* public API boundary. They decouple
//! downstream consumers (the Indexer, VaultGraph, UI) from the raw, platform
//! specific `notify` event shape. Raw OS notifications are normalized by the
//! [`crate::watcher::VaultWatcher`] into [`VaultEvent`] values before they are
//! ever handed to a consumer.

use serde::{Deserialize, Serialize};

/// Normalized kind of a vault file-system change.
///
/// Raw OS notifications are far more granular (create, data-modify,
/// metadata-modify, access-open, access-close, …). The watcher collapses that
/// platform noise into the four logical operations that have meaning for the
/// vault model: content creation, content modification, deletion, and rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherChangeKind {
    /// A file (or directory) was created in the vault.
    Created,
    /// The content/data of a vault file changed.
    Modified,
    /// A file (or directory) was deleted from the vault.
    Deleted,
    /// A file or directory was renamed or moved.
    Renamed,
}

/// A normalized, debounced file-system change on a vault-managed path.
///
/// `path` is always a vault-relative path with forward-slash separators
/// (e.g. `"Inbox/note.md"`), matching the `vault_path` shape used by
/// [`crate::models::KnowledgeObject`] metadata and the `StorageManager`.
///
/// `old_path` carries the previous vault-relative path for [`Renamed`]
/// events **when the backing platform reported the rename source**. It is
/// `None` when only one side of the rename was observable at the OS level —
/// a platform limitation the watcher cannot fabricate, not an error.
///
/// [`Renamed`]: WatcherChangeKind::Renamed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultEvent {
    /// The normalized change kind.
    pub kind: WatcherChangeKind,
    /// Vault-relative path (forward-slash separated) of the affected entry.
    pub path: String,
    /// For [`Renamed`](WatcherChangeKind::Renamed) events: the previous
    /// vault-relative path, when known.
    pub old_path: Option<String>,
}

impl VaultEvent {
    #[must_use]
    pub fn created(path: impl Into<String>) -> Self {
        Self {
            kind: WatcherChangeKind::Created,
            path: path.into(),
            old_path: None,
        }
    }

    #[must_use]
    pub fn modified(path: impl Into<String>) -> Self {
        Self {
            kind: WatcherChangeKind::Modified,
            path: path.into(),
            old_path: None,
        }
    }

    #[must_use]
    pub fn deleted(path: impl Into<String>) -> Self {
        Self {
            kind: WatcherChangeKind::Deleted,
            path: path.into(),
            old_path: None,
        }
    }

    #[must_use]
    pub fn renamed(path: impl Into<String>, old_path: impl Into<String>) -> Self {
        Self {
            kind: WatcherChangeKind::Renamed,
            path: path.into(),
            old_path: Some(old_path.into()),
        }
    }

    /// Human-readable summary, primarily for diagnostics.
    pub fn summary(&self) -> String {
        match (self.kind, &self.old_path) {
            (WatcherChangeKind::Renamed, Some(old)) => {
                format!("rename {} -> {}", old, self.path)
            }
            _ => format!("{:?} {}", self.kind, self.path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_event_factories_round_trip() {
        assert_eq!(
            VaultEvent::created("a.md"),
            VaultEvent {
                kind: WatcherChangeKind::Created,
                path: "a.md".into(),
                old_path: None
            }
        );
        assert_eq!(
            VaultEvent::modified("b.md"),
            VaultEvent {
                kind: WatcherChangeKind::Modified,
                path: "b.md".into(),
                old_path: None
            }
        );
        assert_eq!(
            VaultEvent::deleted("c.md"),
            VaultEvent {
                kind: WatcherChangeKind::Deleted,
                path: "c.md".into(),
                old_path: None
            }
        );
        assert_eq!(
            VaultEvent::renamed("new.md", "old.md"),
            VaultEvent {
                kind: WatcherChangeKind::Renamed,
                path: "new.md".into(),
                old_path: Some("old.md".into())
            }
        );
    }

    #[test]
    fn watcher_change_kind_is_serializable() {
        for kind in [
            WatcherChangeKind::Created,
            WatcherChangeKind::Modified,
            WatcherChangeKind::Deleted,
            WatcherChangeKind::Renamed,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: WatcherChangeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn vault_event_is_serializable() {
        let ev = VaultEvent::renamed("Inbox/new.md", "Inbox/old.md");
        let json = serde_json::to_string(&ev).unwrap();
        let back: VaultEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn summary_formats_rename_with_both_paths() {
        let ev = VaultEvent::renamed("new.md", "old.md");
        assert_eq!(ev.summary(), "rename old.md -> new.md");
    }

    #[test]
    fn summary_formats_other_kinds() {
        assert_eq!(VaultEvent::deleted("gone.md").summary(), "Deleted gone.md");
        assert_eq!(
            VaultEvent::modified("note.md").summary(),
            "Modified note.md"
        );
    }
}
