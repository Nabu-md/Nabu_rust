//! # MCP Server — Protocol Coordinator
//!
//! [`McpServer`] is the central coordinator for the Model Context Protocol (MCP)
//! layer. It owns the tool registry, delegates resource operations to
//! [`ResourceProvider`](super::resources::ResourceProvider), tracks the
//! `initialize` state machine, and integrates with the existing
//! [`Router`](crate::rpc::Router) by registering one
//! [`RpcHandler`](crate::rpc::RpcHandler) per MCP method.
//!
//! ## MCP Methods
//!
//! | Method            | Handler             | Description |
//! |-------------------|---------------------|-------------|
//! | `initialize`      | InitializeHandler   | Capability exchange & server info |
//! | `tools/list`      | ToolsListHandler    | List available Nabu tools |
//! | `tools/call`      | ToolsCallHandler    | Invoke a Nabu tool |
//! | `resources/list`  | ResourcesListHandler| List static resource templates |
//! | `resources/read`  | ResourcesReadHandler| Read a resource by URI |
//!
//! ## Architecture
//!
//! ```text
//! MCP Client
//!     │  (Request: { method, params })
//!     ▼
//! Router::dispatch
//!     │  (looks up method → RpcHandler)
//!     ▼
//! Per-method RpcHandler  (deserialize params → MCP request)
//!     │
//!     ▼
//! McpServer::do_*  (validate state → delegate to registry/provider → serialize)
//!     │
//!     ▼
//! MCP Response
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde_json::Value;

use crate::rpc::{JsonRpcError, Router, RpcHandler};
use crate::tool_calling::{ToolCall, ToolRegistry};

use super::error::McpError;
use super::protocol::{
    decode_params, tool_id_from_mcp_name, tool_result_to_call_result, tool_spec_to_mcp_tool,
    CallToolParams, CallToolResult, ContentBlock, InitializeParams, InitializeResult,
    ListResourcesResult, ListToolsResult, ReadResourceParams, ReadResourceResult,
    MCP_PROTOCOL_VERSION, Implementation, ResourcesCapability,
    ServerCapabilities, ToolsCapability,
};
use super::resources::ResourceProvider;
use super::tools::register_nabu_tools;
use crate::indexer::Indexer;
use crate::storage::StorageManager;

// ---------------------------------------------------------------------------
// Method name constants — match the MCP specification
// ---------------------------------------------------------------------------

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";
pub const METHOD_RESOURCES_LIST: &str = "resources/list";
pub const METHOD_RESOURCES_READ: &str = "resources/read";

// ---------------------------------------------------------------------------
// McpServer
// ---------------------------------------------------------------------------

/// The MCP protocol coordinator.
///
/// Holds the tool registry, resource provider, and protocol state.
/// Registered as RPC handlers on a [`Router`] via [`register_handlers`](Self::register_handlers).
pub struct McpServer {
    registry: Arc<ToolRegistry>,
    resource_provider: ResourceProvider,
    indexer: Arc<Indexer>,
    initialized: AtomicBool,
    server_info: Implementation,
}

impl McpServer {
    /// Create a new MCP server backed by the given storage and indexer.
    ///
    /// The tool registry starts empty — call [`register_handlers`](Self::register_handlers)
    /// to populate it with Nabu tools and register RPC method handlers.
    pub fn new(storage: Arc<StorageManager>, indexer: Arc<Indexer>) -> Self {
        let resource_provider = ResourceProvider::new(storage);
        Self {
            registry: Arc::new(ToolRegistry::new()),
            resource_provider,
            indexer,
            initialized: AtomicBool::new(false),
            server_info: Implementation {
                name: "Nabu MCP Server".to_string(),
                title: Some("Nabu Knowledge Base".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                _meta: None,
            },
        }
    }

    /// Returns the tool registry backing this server.
    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }

    /// Returns the resource provider backing this server.
    pub fn resource_provider(&self) -> &ResourceProvider {
        &self.resource_provider
    }

    /// Returns `true` if `initialize` has completed.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Mark the server as initialized.
    fn set_initialized(&self) {
        self.initialized.store(true, Ordering::Release);
    }

    /// Registers all MCP method handlers on the given router.
    ///
    /// Also registers the four default Nabu tools (`search_note`, `read_note`,
    /// `write_note`, `list_notes`) on the internal tool registry.
    ///
    /// The `Arc<McpServer>` is cloned into each handler so they share ownership
    /// of the server's state.
    pub async fn register_handlers(self: &Arc<Self>, router: &Router) {
        // Register Nabu tools on the registry.
        register_nabu_tools(
            &self.registry,
            self.resource_provider.storage().clone(),
            self.indexer.clone(),
        )
        .await;

        // Register MCP protocol method handlers.
        router
            .register(METHOD_INITIALIZE, Arc::new(InitializeHandler::new(self.clone())))
            .await;
        router
            .register(METHOD_TOOLS_LIST, Arc::new(ToolsListHandler::new(self.clone())))
            .await;
        router
            .register(METHOD_TOOLS_CALL, Arc::new(ToolsCallHandler::new(self.clone())))
            .await;
        router
            .register(METHOD_RESOURCES_LIST, Arc::new(ResourcesListHandler::new(self.clone())))
            .await;
        router
            .register(METHOD_RESOURCES_READ, Arc::new(ResourcesReadHandler::new(self.clone())))
            .await;

        tracing::info!(
            "MCP protocol handlers registered on JSON-RPC router: {}, {}, {}, {}, {}",
            METHOD_INITIALIZE,
            METHOD_TOOLS_LIST,
            METHOD_TOOLS_CALL,
            METHOD_RESOURCES_LIST,
            METHOD_RESOURCES_READ
        );
    }

    /// Construct the `InitializeResult` for this server.
    fn build_initialize_result(&self) -> InitializeResult {
        InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_dynamic: Some(true),
                }),
                resources: Some(ResourcesCapability {
                    list_dynamic: Some(true),
                    read_dynamic: Some(true),
                    subscribe: None,
                }),
                prompts: None,
                logging: None,
                _meta: None,
            },
            server_info: self.server_info.clone(),
            instructions: Some("Nabu MCP server — knowledge management tools".to_string()),
            _meta: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-method handler structs
// ---------------------------------------------------------------------------

/// Handler for the `initialize` method.
pub struct InitializeHandler {
    server: Arc<McpServer>,
}

impl InitializeHandler {
    pub fn new(server: Arc<McpServer>) -> Self {
        Self { server }
    }
}

#[async_trait]
impl RpcHandler for InitializeHandler {
    async fn handle(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let _req: InitializeParams = decode_params(params).map_err(McpError::into_jsonrpc_error)?;

        self.server.set_initialized();

        let result = self.server.build_initialize_result();
        serde_json::to_value(result)
            .map_err(|e| JsonRpcError::internal(format!("failed to serialize initialize result: {}", e)))
    }
}

/// Handler for the `tools/list` method.
pub struct ToolsListHandler {
    server: Arc<McpServer>,
}

impl ToolsListHandler {
    pub fn new(server: Arc<McpServer>) -> Self {
        Self { server }
    }
}

#[async_trait]
impl RpcHandler for ToolsListHandler {
    async fn handle(&self, _params: Option<Value>) -> Result<Value, JsonRpcError> {
        let specs = self.server.registry().specs().await;
        let tools: Vec<_> = specs
            .iter()
            .map(|s| tool_spec_to_mcp_tool(s, None))
            .collect();

        let result = ListToolsResult { tools, next_cursor: None };
        serde_json::to_value(result)
            .map_err(|e| JsonRpcError::internal(format!("failed to serialize tools list: {}", e)))
    }
}

/// Handler for the `tools/call` method.
pub struct ToolsCallHandler {
    server: Arc<McpServer>,
}

impl ToolsCallHandler {
    pub fn new(server: Arc<McpServer>) -> Self {
        Self { server }
    }
}

#[async_trait]
impl RpcHandler for ToolsCallHandler {
    async fn handle(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        if !self.server.is_initialized() {
            return Err(McpError::not_initialized().into_jsonrpc_error());
        }

        let req: CallToolParams = decode_params(params).map_err(McpError::into_jsonrpc_error)?;

        let nabu_tool_id = tool_id_from_mcp_name(&req.name);
        let tool_call = ToolCall::new(nabu_tool_id, req.arguments);

        let tool_result = self.server.registry().call(tool_call).await;
        let call_result: CallToolResult = tool_result_to_call_result(tool_result);

        serde_json::to_value(call_result)
            .map_err(|e| JsonRpcError::internal(format!("failed to serialize tool result: {}", e)))
    }
}

/// Handler for the `resources/list` method.
pub struct ResourcesListHandler {
    server: Arc<McpServer>,
}

impl ResourcesListHandler {
    pub fn new(server: Arc<McpServer>) -> Self {
        Self { server }
    }
}

#[async_trait]
impl RpcHandler for ResourcesListHandler {
    async fn handle(&self, _params: Option<Value>) -> Result<Value, JsonRpcError> {
        let result: ListResourcesResult = self.server.resource_provider().list_resources();
        serde_json::to_value(result)
            .map_err(|e| JsonRpcError::internal(format!("failed to serialize resources list: {}", e)))
    }
}

/// Handler for the `resources/read` method.
pub struct ResourcesReadHandler {
    server: Arc<McpServer>,
}

impl ResourcesReadHandler {
    pub fn new(server: Arc<McpServer>) -> Self {
        Self { server }
    }
}

#[async_trait]
impl RpcHandler for ResourcesReadHandler {
    async fn handle(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let req: ReadResourceParams = decode_params(params).map_err(McpError::into_jsonrpc_error)?;

        let result = self
            .server
            .resource_provider()
            .read_resource(&req)
            .map_err(McpError::into_jsonrpc_error)?;

        serde_json::to_value(result)
            .map_err(|e| JsonRpcError::internal(format!("failed to serialize resource: {}", e)))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{KnowledgeObject, ObjectContent, ObjectMetadata, ObjectType};
    use crate::rpc::Request;
    use serde_json::json;
    use tempfile::tempdir;

    async fn make_server() -> (Arc<McpServer>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage = Arc::new(StorageManager::new(dir.path()));
        storage.initialize().unwrap();
        storage.start().unwrap();

        let indexer = Arc::new(Indexer::with_vault_path(dir.path()));
        let _ = indexer.initialize();

        let server = Arc::new(McpServer::new(storage, indexer));
        (server, dir)
    }

    #[tokio::test]
    async fn server_starts_uninitialized() {
        let (server, _dir) = make_server().await;
        assert!(!server.is_initialized());
    }

    #[tokio::test]
    async fn register_handlers_registers_five_methods() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        assert_eq!(router.method_count().await, 5);
        assert!(router.has_method(METHOD_INITIALIZE).await);
        assert!(router.has_method(METHOD_TOOLS_LIST).await);
        assert!(router.has_method(METHOD_TOOLS_CALL).await);
        assert!(router.has_method(METHOD_RESOURCES_LIST).await);
        assert!(router.has_method(METHOD_RESOURCES_READ).await);
    }

    #[tokio::test]
    async fn register_handlers_registers_four_nabu_tools() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        assert_eq!(server.registry().tool_count().await, 4);
        assert!(server.registry().has_tool("nabu:search_note").await);
        assert!(server.registry().has_tool("nabu:read_note").await);
        assert!(server.registry().has_tool("nabu:write_note").await);
        assert!(server.registry().has_tool("nabu:list_notes").await);
    }

    #[tokio::test]
    async fn initialize_sets_server_info() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let params = Some(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "0.0.1"
            }
        }));

        let request = Request::new(1, METHOD_INITIALIZE, params);
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: InitializeResult =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(result.server_info.name, "Nabu MCP Server");
        assert!(result.capabilities.tools.is_some());
        assert!(result.capabilities.resources.is_some());
        assert!(server.is_initialized());
    }

    #[tokio::test]
    async fn tools_list_returns_registered_tools() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        // Initialize first
        let init_params = Some(json!({ "protocolVersion": MCP_PROTOCOL_VERSION }));
        let init_req = Request::new(1, METHOD_INITIALIZE, init_params);
        router.dispatch(init_req).await;

        let request = Request::new(2, METHOD_TOOLS_LIST, None);
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: ListToolsResult =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.tools.len(), 4);

        let names: Vec<&str> = result.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_note"));
        assert!(names.contains(&"read_note"));
        assert!(names.contains(&"write_note"));
        assert!(names.contains(&"list_notes"));
    }

    #[tokio::test]
    async fn tools_call_dispatches_through_registry() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        // Initialize
        let init_req = Request::new(1, METHOD_INITIALIZE, Some(json!({ "protocolVersion": MCP_PROTOCOL_VERSION })));
        router.dispatch(init_req).await;

        // Seed a note
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Hello from MCP".to_string()),
        )
        .with_metadata(ObjectMetadata {
            vault_path: Some("mcp-test.md".to_string()),
            title: Some("MCP Test".to_string()),
            ..Default::default()
        });
        server
            .resource_provider()
            .storage()
            .save(&obj)
            .unwrap();

        // Call read_note tool
        let params = Some(json!({
            "name": "read_note",
            "arguments": { "path": "mcp-test.md" }
        }));
        let request = Request::new(2, METHOD_TOOLS_CALL, params);
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            ContentBlock::Text { text, .. } => {
                assert!(text.contains("Hello from MCP"));
            }
            _ => panic!("expected text content block"),
        }
    }

    #[tokio::test]
    async fn tools_call_unknown_tool_returns_error() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        // Initialize
        let init_req = Request::new(1, METHOD_INITIALIZE, Some(json!({ "protocolVersion": MCP_PROTOCOL_VERSION })));
        router.dispatch(init_req).await;

        let params = Some(json!({
            "name": "nonexistent_tool",
            "arguments": {}
        }));
        let request = Request::new(2, METHOD_TOOLS_CALL, params);
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn tools_call_before_init_returns_error() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let params = Some(json!({
            "name": "read_note",
            "arguments": { "path": "test.md" }
        }));
        let request = Request::new(1, METHOD_TOOLS_CALL, params);
        let response = router.dispatch(request).await;

        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn resources_list_returns_static_resources() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let request = Request::new(1, METHOD_RESOURCES_LIST, None);
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: ListResourcesResult =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.resources.len(), 2);
    }

    #[tokio::test]
    async fn resources_read_notes_all() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let params = Some(json!({ "uri": "notes://all" }));
        let request = Request::new(1, METHOD_RESOURCES_READ, params);
        let response = router.dispatch(request).await;

        assert!(response.is_success());
        let result: ReadResourceResult =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].mime_type.as_deref(), Some("application/json"));
    }

    #[tokio::test]
    async fn resources_read_unknown_uri_returns_error() {
        let (server, _dir) = make_server().await;
        let router = Router::new();
        server.register_handlers(&router).await;

        let params = Some(json!({ "uri": "unknown://foo" }));
        let request = Request::new(1, METHOD_RESOURCES_READ, params);
        let response = router.dispatch(request).await;

        assert!(!response.is_success());
    }

    #[test]
    fn method_constants_match_mcp_spec() {
        assert_eq!(METHOD_INITIALIZE, "initialize");
        assert_eq!(METHOD_TOOLS_LIST, "tools/list");
        assert_eq!(METHOD_TOOLS_CALL, "tools/call");
        assert_eq!(METHOD_RESOURCES_LIST, "resources/list");
        assert_eq!(METHOD_RESOURCES_READ, "resources/read");
    }
}
