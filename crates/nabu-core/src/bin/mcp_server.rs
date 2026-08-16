//! # Nabu MCP Server — stdio entry point
//!
//! This binary exposes Nabu's knowledge management tools (`search_note`,
//! `read_note`, `write_note`, `list_notes`) and vault resources through the
//! Model Context Protocol (MCP) over stdin/stdout.
//!
//! It is designed to be spawned as a subprocess by an ACP agent (e.g.
//! claude-code) that receives its configuration through the ACP `mcp_servers`
//! field in `session/new`.  The binary reuses the in-process [`McpServer`]
//! coordinator and the existing [`StdioTransport`] JSON-RPC loop — no
//! protocol or tool implementation is duplicated here.
//!
//! ## Usage
//!
//! ```text
//! nabu-mcp-server <vault_path>
//! ```
//!
//! The server reads newline-delimited JSON-RPC requests from stdin and writes
//! responses to stdout, following the LSP/stdio MCP transport convention.

use std::path::PathBuf;
use std::sync::Arc;

use nabu_core::indexer::Indexer;
use nabu_core::io_stream::StdioTransport;
use nabu_core::mcp::McpServer;
use nabu_core::registry::lifecycle::Lifecycle;
use nabu_core::rpc::Router;
use nabu_core::storage::StorageManager;

fn parse_args() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: nabu-mcp-server <vault_path>");
        std::process::exit(1);
    }
    PathBuf::from(&args[1])
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let vault_path = parse_args();

    eprintln!(
        "Nabu MCP server starting — vault: {}",
        vault_path.display()
    );

    let storage = Arc::new(StorageManager::new(&vault_path));
    if let Err(e) = storage.initialize() {
        eprintln!("Failed to initialize storage: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = storage.start() {
        eprintln!("Failed to start storage: {}", e);
        std::process::exit(1);
    }

    let indexer = Arc::new(Indexer::with_vault_path(&vault_path));
    if let Err(e) = indexer.initialize() {
        eprintln!("Failed to initialize indexer: {}", e);
        std::process::exit(1);
    }

    if let Ok(objects) = storage.list_objects("", None, 100_000) {
        if let Err(e) = indexer.reindex(&objects) {
            eprintln!("Warning: failed to rebuild search index: {}", e);
        }
    }

    let server = Arc::new(McpServer::new(storage, indexer));
    let router = Arc::new(Router::new());
    server.register_handlers(&router).await;

    let tool_count = server.registry().tool_count().await;
    eprintln!("Nabu MCP server ready — {} tools registered", tool_count);

    let transport = StdioTransport::new(router);
    transport.initialize().expect("transport initialize failed");
    transport.start_transport().expect("transport start_transport failed");

    match transport.run().await {
        Ok(()) => {
            let _ = transport.flush().await;
        }
        Err(e) if e.is_eof() || e.is_shutdown() => {}
        Err(e) => {
            eprintln!("stdio transport error: {}", e);
        }
    }

    eprintln!("Nabu MCP server shutting down");
}
