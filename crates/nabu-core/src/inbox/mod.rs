//! Inbox processing for Nabu.
//!
//! The inbox is the staging area for freshly captured knowledge objects
//! (clipboard clips, web articles, file drops, voice recordings …).  Items
//! enter with a pending classification and are shepherded toward one of three
//! durable outcomes:
//!
//! * **Approved / filed** — promoted into a real vault artifact (a Markdown
//!   note for textual content, or its native binary file for images/audio)
//!   that is persisted, indexed and surfaced to the editor/graph.
//! * **Rejected** — dismissed, with an optional reason, and hidden from the
//!   *active* inbox.
//! * **Deleted** — removed from the vault entirely.
//!
//! The [`FilingService`] is the canonical, testably-isolated implementation of
//! these outcomes.  It consumes only the *public* API surfaces of the storage
//! manager and the event bus — it never reaches into the Indexer or the
//! graph's internals (those integrations are wired upstream through the
//! canonical `ITEM_STORED` pipeline event, see `src/lib.rs`).
//!
//! Phase 1B owns the Tauri command bridge (`inbox_approve`,
//! `inbox_reject`, `inbox_retry`, `inbox_delete`) which delegates to this
//! service.

pub mod filing_service;
pub mod model;

pub use filing_service::{FilingError, FilingResult, FilingService, FilingStatus};
pub use model::{
    classify_ready, inbox_item_status, resolve_destination, set_status, InboxItemStatus,
};
