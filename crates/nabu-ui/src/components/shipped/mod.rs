//! # Shipped Views — Reader & Comparison
//!
//! Phase 2A-4 ships two views that were previously stubbed in `app.rs`:
//! - [`ReaderView`] — distraction-free reading via `note_read`
//! - [`ComparisonView`] — side-by-side diff via `notes_diff`
//!
//! Both components surface loading / success / empty / error states and use
//! the canonical IPC + LoadState patterns established by `note_editor.rs` and
//! `version_history.rs`.

pub mod comparison;
pub mod reader;

pub use comparison::ComparisonView;
pub use reader::ReaderView;

/// Returns `true` when a received result nonce does not match the current
/// nonce — i.e. the async load that produced `received` has been superseded
/// by a newer load and its result should be discarded.
///
/// Pure logic; safe to call from async blocks (`peek` is not needed).
pub fn nonce_is_stale(current: u32, received: u32) -> bool {
    current != received
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_is_stale_when_mismatched() {
        assert!(nonce_is_stale(2, 1));
    }

    #[test]
    fn nonce_is_not_stale_when_matched() {
        assert!(!nonce_is_stale(3, 3));
    }

    #[test]
    fn nonce_is_stale_after_wrap() {
        assert!(nonce_is_stale(0u32, u32::MAX));
    }
}
