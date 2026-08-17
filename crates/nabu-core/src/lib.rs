//! # Nabu Core Library
//!
//! The core library for the Nabu knowledge management platform.
//!
//! ## Architecture
//!
//! Every subsystem flows through the canonical pipeline:
//!
//! ```text
//! CaptureSource
//!     │  CaptureEngine.ingest()
//!     ▼
//! CaptureEvent (published to EventBus)
//!     │
//!     ▼
//! Job Queue (DurableJobQueue)
//!     │  WorkerPool dequeue → PipelineExecutor.execute()
//!     ▼
//! ProcessingPipeline
//!     │  All 14 processors execute in order
//!     ▼
//! StorageManager.save()
//!     │
//!     ├── Indexer.index_document()
//!     └── VaultGraph.update_node()
//! ```

pub mod acp;
pub mod capture;
pub mod conversations;
pub mod diagnostic;
pub mod diagnostics;
pub mod dictation;
pub mod event_bus;
pub mod graph;
pub mod history;
#[cfg(not(target_arch = "wasm32"))]
pub mod io_stream;
// IPC socket server is built on UNIX domain sockets (`tokio::net::UnixListener`
// / `std::os::unix::net`), which only exist on Unix targets. Windows support is
// a future transport (named pipes); until then the module (and its tests) are
// excluded from non-Unix builds so the rest of the crate compiles and its
// tests run on every platform.
#[cfg(not(target_arch = "wasm32"))]
pub mod agent;
pub mod inbox;
pub mod indexer;
#[cfg(unix)]
pub mod ipc_socket;
pub mod jobs;
#[cfg(not(target_arch = "wasm32"))]
pub mod mcp;
pub mod models;
pub mod native;
pub mod pipeline_migration;
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod process_supervisor;
pub mod processing;
pub mod registry;
pub mod rpc;
pub mod storage;
pub mod streaming;
pub mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub mod tool_calling;
#[cfg(not(target_arch = "wasm32"))]
pub mod watcher;

// Re-export key types for convenient access
// Ambiguous glob re-exports are intentional — all public API types should
// be available at the crate root for ergonomic use by consumers.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
#[allow(ambiguous_glob_reexports)]
pub use agent::*;
#[allow(ambiguous_glob_reexports)]
pub use capture::*;
#[allow(ambiguous_glob_reexports)]
pub use conversations::*;
#[allow(ambiguous_glob_reexports)]
pub use diagnostic::*;
#[allow(ambiguous_glob_reexports)]
pub use diagnostics::*;
#[allow(ambiguous_glob_reexports)]
pub use event_bus::*;
#[allow(ambiguous_glob_reexports)]
pub use graph::*;
#[allow(ambiguous_glob_reexports)]
pub use history::*;
#[allow(ambiguous_glob_reexports)]
pub use indexer::*;
#[cfg(not(target_arch = "wasm32"))]
#[allow(ambiguous_glob_reexports)]
pub use io_stream::*;
#[allow(ambiguous_glob_reexports)]
pub use jobs::*;
#[allow(ambiguous_glob_reexports)]
pub use models::*;
#[allow(ambiguous_glob_reexports)]
pub use pipeline_migration::*;
#[allow(ambiguous_glob_reexports)]
pub use plugin::*;
#[cfg(not(target_arch = "wasm32"))]
#[allow(ambiguous_glob_reexports)]
pub use process_supervisor::*;
#[allow(ambiguous_glob_reexports)]
pub use processing::*;
#[allow(ambiguous_glob_reexports)]
pub use registry::*;
#[allow(ambiguous_glob_reexports)]
pub use rpc::*;
#[allow(ambiguous_glob_reexports)]
pub use storage::*;
#[cfg(not(target_arch = "wasm32"))]
#[allow(ambiguous_glob_reexports)]
pub use streaming::*;
#[allow(ambiguous_glob_reexports)]
pub use sync::*;
#[cfg(not(target_arch = "wasm32"))]
#[allow(ambiguous_glob_reexports)]
pub use tool_calling::*;
#[cfg(not(target_arch = "wasm32"))]
#[allow(ambiguous_glob_reexports)]
pub use watcher::*;
