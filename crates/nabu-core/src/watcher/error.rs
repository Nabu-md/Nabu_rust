//! Error types for the vault watcher.

use std::path::PathBuf;
use thiserror::Error;

/// All failures that can be produced by the [`crate::watcher::VaultWatcher`].
///
/// Errors are structured (no stringly-typed failures) so that callers —
/// including the Tauri integration layer — can pattern-match on a variant
/// instead of parsing error text.
#[derive(Debug, Error)]
pub enum WatcherError {
    /// The vault path passed to the watcher does not exist or is not a
    /// directory.
    #[error("vault path does not exist or is not a directory: {path}")]
    VaultNotFound { path: PathBuf },

    /// [`crate::watcher::VaultWatcher::start`] was called while the watcher was
    /// already running.
    #[error("watcher is already started")]
    AlreadyStarted,

    /// [`crate::watcher::VaultWatcher::stop`] was called while the watcher was
    /// not running.
    #[error("watcher is not running")]
    NotRunning,

    /// A fallible operation was attempted on a watcher that had not been
    /// started. Call [`crate::watcher::VaultWatcher::start`] first.
    #[error("watcher has not been started")]
    NotStarted,

    /// Error originating from the underlying `notify` backend (e.g. the OS
    /// rejected a recursive watch on an inaccessible path).
    #[error("filesystem notification error: {0}")]
    Notify(#[from] notify::Error),

    /// An I/O error raised while resolving or canonicalizing a vault path.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl WatcherError {
    /// Stable, human-readable variant name — useful for metrics and logging
    /// without serializing the full error payload.
    pub fn variant_name(&self) -> &'static str {
        match self {
            WatcherError::VaultNotFound { .. } => "vault_not_found",
            WatcherError::AlreadyStarted => "already_started",
            WatcherError::NotRunning => "not_running",
            WatcherError::NotStarted => "not_started",
            WatcherError::Notify(_) => "notify",
            WatcherError::Io(_) => "io",
        }
    }
}

/// Result alias used throughout the watcher module.
pub type Result<T> = std::result::Result<T, WatcherError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_names_are_stable() {
        assert_eq!(
            WatcherError::VaultNotFound { path: PathBuf::from("/x") }.variant_name(),
            "vault_not_found"
        );
        assert_eq!(WatcherError::AlreadyStarted.variant_name(), "already_started");
        assert_eq!(WatcherError::NotRunning.variant_name(), "not_running");
        assert_eq!(WatcherError::NotStarted.variant_name(), "not_started");
        assert_eq!(
            WatcherError::Io(std::io::Error::new(std::io::ErrorKind::Other, "x"))
                .variant_name(),
            "io"
        );
    }

    #[test]
    fn notify_error_is_wrapped() {
        let err = notify::Error::generic("boom");
        let wrapped: WatcherError = err.into();
        assert_eq!(wrapped.variant_name(), "notify");
    }
}
