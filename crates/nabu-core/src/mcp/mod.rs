//! # MCP Server — Model Context Protocol Integration
//!
//! Exposes Nabu's knowledge management operations (search, read, write, list
//! notes) and vault resources through the Model Context Protocol (MCP).
//!
//! The MCP server reuses Nabu's existing infrastructure:
//!
//! - **JSON-RPC [`Router`](crate::rpc::Router)** for method dispatch — the same
//!   router used by ACP.
//! - **`ToolRegistry`** from the tool calling framework — tools registered for
//!   MCP can also be discovered through ACP, and vice versa.
//! - **`StorageManager`** and **`Indexer`** as the data-access and search layers.
//!
//! ## Usage
//!
//! ```no_run
//! use nabu_core::mcp::McpServer;
//! use nabu_core::rpc::Router;
//! use nabu_core::storage::StorageManager;
//! use nabu_core::indexer::Indexer;
//! use std::sync::Arc;
//!
//! let storage = Arc::new(StorageManager::new("/path/to/vault"));
//! let indexer = Arc::new(Indexer::with_vault_path("/path/to/vault"));
//! let server = Arc::new(McpServer::new(storage, indexer));
//!
//! let router = Router::new();
//! server.register_handlers(&router).await;
//! ```
//!
//! ## Modules
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`error`] | MCP error types with JSON-RPC conversion |
//! | [`protocol`] | MCP wire-format types (Initialize, Tool, Resource, etc.) |
//! | [`tools`] | Nabu `Tool` implementations for MCP tool calls |
//! | [`resources`] | Resource provider for `resources/list` and `resources/read` |
//! | [`server`] | `McpServer` — protocol coordinator and handler registration |

pub mod error;
pub mod protocol;
pub mod resources;
pub mod server;
pub mod tools;

pub use error::{McpError, McpErrorKind};
pub use protocol::{
    CallToolParams, CallToolResult, ContentBlock, InitializeParams, InitializeResult,
    ListResourcesResult, ListToolsResult, McpTool, ReadResourceParams, ReadResourceResult,
    Resource, ResourceContents, ServerCapabilities,
};
pub use resources::ResourceProvider;
pub use server::McpServer;
pub use tools::{
    ListNotesTool, ReadNoteTool, SearchNoteTool, WriteNoteTool, register_nabu_tools,
};
