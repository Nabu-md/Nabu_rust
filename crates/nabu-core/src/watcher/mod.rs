//! # Vault Filesystem Watcher
//!
//! A thin, normalized, debounced adapter over the platform `notify` backend
//! that surfaces **stable** [`VaultEvent`]s for vault file changes:
//!
//! - creation
//! - modification
//! - deletion
//! - rename / move
//!
//! The watcher is intentionally *only* an event source. It does **not** mutate
//! the [`Indexer`], [`VaultGraph`], or any storage state. It does **not** wire
//! itself into the application composition root — that is the job of the later
//! Phase 1B integration, which consumes the event channel exposed by
//! [`VaultWatcher::start`].
//!
//! ## Design summary
//!
//! - **Normalization**: raw, noisy OS notifications (create + metadata +
//!   data per save, unpaired rename halves, …) are folded into the four
//!   logical kinds in [`WatcherChangeKind`].
//! - **Debounce / coalescing**: rapid bursts for a path collapse to a single
//!   emitted event per path after a short quiet period.
//! - **Duplicate suppression**: identical signals within a burst deduplicate
//!   to one event per path.
//! - **Self-event suppression**: callers register Nabu-owned operations via
//!   [`VaultWatcher::expect_self_operation`]; the matching notification is
//!   suppressed rather than re-processed.
//! - **Lifecycle**: `start` / `stop` with clean thread + native-watcher
//!   teardown; dropping the watcher also stops it.
//!
//! [`Indexer`]: crate::indexer::Indexer
//! [`VaultGraph`]: crate::graph::VaultGraph

pub mod config;
pub mod error;
pub mod event;
pub mod service;

pub use config::*;
pub use error::*;
pub use event::*;
pub use service::VaultWatcher;
