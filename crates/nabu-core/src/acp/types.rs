//! # ACP Protocol Types (Client-Side)
//!
//! Strongly-typed request, response, notification, and event structures for the
//! Agent Communication Protocol (ACP) v1, viewed from the **client** side.
//!
//! Nabu is the ACP **client/editor**. The external process is the ACP **agent**.
//! These types map directly to the JSON wire format described in the
//! [ACP v1 specification](https://agentclientprotocol.com/protocol/v1/overview).
//!
//! All types are designed to be serialized into `serde_json::Value` for
//! transport through the existing [`crate::rpc`] JSON-RPC 2.0 infrastructure.
//!
//! ## Forward compatibility
//!
//! None of the types use `#[serde(deny_unknown_fields)]`. Unknown fields in
//! incoming JSON are silently ignored, so future ACP revisions that add fields
//! will deserialize without error. Optional fields use `#[serde(default)]`
//! and `skip_serializing_if = "Option::is_none"`.
//!
//! ## Naming convention
//!
//! ACP wire fields use `camelCase`. Rust field names use `snake_case` and
//! serde performs the conversion via `#[serde(rename_all = "camelCase")]`.
//! Discriminator values (variant tags) use `snake_case` per the ACP convention.

use crate::acp::error::AcpError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Scalar type aliases
// ---------------------------------------------------------------------------

/// The ACP protocol version identifier. A single unsigned integer; only
/// incremented for breaking changes. ACP v1 is the current stable protocol.
pub type ProtocolVersion = u16;

/// The Nabu ACP client reports this as its supported protocol version during
/// `initialize`. Nabu Phase 3a targets ACP protocol version 1 (stable v1).
pub const SUPPORTED_PROTOCOL_VERSION: ProtocolVersion = 1;

/// A unique identifier for a conversation session.
pub type SessionId = String;

/// A typed identifier for an authentication method.
pub type AuthMethodId = String;

/// A typed identifier for a message within a `session/update` stream.
/// All chunks belonging to the same message share the same `messageId`.
pub type MessageId = String;

/// A typed identifier for a tool call within a session.
pub type ToolCallId = String;

// ---------------------------------------------------------------------------
// JSON-RPC envelope (client-side view)
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 message that the ACP client can receive from the agent.
///
/// The client receives three kinds of messages:
/// - **Responses** to requests it sent (e.g. the `initialize` response)
/// - **Notifications** from the agent (e.g. `session/update`, `session/cancel`)
/// - **Requests** from the agent to the client (e.g. `fs/read_text_file`,
///   `session/request_permission`)
///
/// This enum is used by the message reader to classify incoming wire messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingMessage {
    #[serde(rename = "jsonrpc")]
    pub version: String,
    /// The request ID this is a response to (absent for notifications).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The method name (present for requests and notifications).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Parameters or result payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    /// Error object (present on error responses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Structured JSON-RPC error object as it appears on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// The numeric JSON-RPC error code (e.g. `-32601`).
    pub code: i64,
    /// A human-readable error message.
    pub message: String,
    /// Optional additional error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Lifecycle metadata
// ---------------------------------------------------------------------------

/// Metadata about a client or agent implementation.
///
/// Sent in the `initialize` request (`clientInfo`) and response (`agentInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    /// Intended for programmatic or logical use.
    pub name: String,
    /// Intended for UI / end-user display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Version of the implementation.
    pub version: String,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// File-system capabilities a client may support.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemCapabilities {
    #[serde(default)]
    pub read_text_file: bool,
    #[serde(default)]
    pub write_text_file: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Prompt content capabilities an agent supports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilities {
    #[serde(default)]
    pub image: bool,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub embedded_context: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// MCP transport capabilities an agent supports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilities {
    #[serde(default)]
    pub http: bool,
    #[serde(default)]
    pub sse: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities for the `session/list` method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities for the `session/delete` method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDeleteCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities for the `session/resume` method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResumeCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities for the `session/close` method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCloseCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Whether the agent supports `additionalDirectories` on session lifecycle
/// requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAdditionalDirectoriesCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Session-related capabilities advertised by the agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<SessionListCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<SessionDeleteCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<SessionResumeCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close: Option<SessionCloseCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_directories: Option<SessionAdditionalDirectoriesCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities for session config options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionConfigOptionsCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Session-related capabilities supported by the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSessionCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options: Option<SessionConfigOptionsCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities supported by the agent, returned in the `initialize` response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Whether the agent supports `session/load`.
    #[serde(default)]
    pub load_session: bool,
    /// Content types the agent can process in prompts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_capabilities: Option<PromptCapabilities>,
    /// MCP transports the agent supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_capabilities: Option<McpCapabilities>,
    /// Optional session lifecycle sub-capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_capabilities: Option<SessionCapabilities>,
    /// Optional authentication extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthCapabilities>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Authentication-related capabilities an agent may advertise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logout: Option<LogoutCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities for the `logout` method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogoutCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Capabilities supported by the client, sent in the `initialize` request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs: Option<FileSystemCapabilities>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<ClientSessionCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

/// A description of an authentication method the agent supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMethod {
    pub id: AuthMethodId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

/// A key/value environment variable for an MCP server subprocess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVariable {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// An HTTP header for MCP servers using HTTP/SSE transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Configuration for an MCP server using stdio transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStdio {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<EnvVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Configuration for an MCP server using HTTP transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerHttp {
    pub name: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Configuration for an MCP server using SSE transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSse {
    pub name: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Configuration for connecting to an MCP server.
///
/// The transport is discriminated by the `type` field on the wire
/// (`"http"`, `"sse"`, `"stdio"`). When the `type` field is absent the
/// server defaults to the **stdio** transport, matching the ACP specification
/// where stdio is the baseline and most common transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpServer {
    Http(McpServerHttp),
    Sse(McpServerSse),
    Stdio(McpServerStdio),
}

impl Default for McpServer {
    fn default() -> Self {
        Self::Stdio(McpServerStdio {
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: Vec::new(),
            _meta: None,
        })
    }
}

impl Serialize for McpServer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (tag, value) = match self {
            Self::Http(inner) => ("http", serde_json::to_value(inner)),
            Self::Sse(inner) => ("sse", serde_json::to_value(inner)),
            Self::Stdio(inner) => ("stdio", serde_json::to_value(inner)),
        };
        let mut value = value.map_err(serde::ser::Error::custom)?;
        if let serde_json::Value::Object(ref mut obj) = value {
            obj.insert(
                "type".to_string(),
                serde_json::Value::String(tag.to_string()),
            );
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for McpServer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let transport = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("stdio");

        match transport {
            "http" => McpServerHttp::deserialize(value)
                .map(Self::Http)
                .map_err(serde::de::Error::custom),
            "sse" => McpServerSse::deserialize(value)
                .map(Self::Sse)
                .map_err(serde::de::Error::custom),
            _ => McpServerStdio::deserialize(value)
                .map(Self::Stdio)
                .map_err(serde::de::Error::custom),
        }
    }
}

// ---------------------------------------------------------------------------
// Content blocks
// ---------------------------------------------------------------------------

/// The sender or recipient of messages and data in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Assistant,
    User,
}

/// Optional annotations that help clients decide how to display content.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<Role>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Text content for a prompt message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Image content for a prompt message (requires `image` prompt capability).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub data: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Audio content for a prompt message (requires `audio` prompt capability).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioContent {
    pub data: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A resource link — a URI the agent can access.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Text-based embedded resource contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextResourceContents {
    pub uri: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Binary-based embedded resource contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobResourceContents {
    pub uri: String,
    pub blob: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Either a text or binary resource payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddedResourceResource {
    Text(TextResourceContents),
    Blob(BlobResourceContents),
}

/// An embedded resource included directly in a prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    pub resource: EmbeddedResourceResource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A single block of content in a prompt message.
///
/// The `type` discriminator selects the variant. The baseline variants
/// (`text`, `resource_link`) MUST always be supported by ACP agents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text(TextContent),
    Image(ImageContent),
    Audio(AudioContent),
    ResourceLink(ResourceLink),
    Resource(EmbeddedResource),
}

/// Reasons an agent may stop processing a prompt turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The turn ended successfully — the agent produced a complete response.
    EndTurn,
    /// The agent reached its maximum token budget.
    MaxTokens,
    /// The agent reached the maximum number of allowed agent requests
    /// between user turns.
    MaxTurnRequests,
    /// The agent refused to continue.
    Refusal,
    /// The client interrupted the turn via cancellation.
    Cancelled,
}

// ---------------------------------------------------------------------------
// Agent → client request types (agent sends these to the client)
// ---------------------------------------------------------------------------

/// Request parameters for the `fs/read_text_file` method (agent → client).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileRequest {
    pub path: String,
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from `fs/read_text_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileResponse {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request parameters for the `fs/write_text_file` method (agent → client).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTextFileRequest {
    pub path: String,
    pub content: String,
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from `fs/write_text_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTextFileResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A permission option presented to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The type of permission option being presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

/// A tool call reference in a permission request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdate {
    pub tool_call_id: ToolCallId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolCallStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ToolCallContent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ToolCallLocation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Categories of tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    Other,
}

/// Execution status of a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// Content produced by a tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallContent {
    Content(serde_json::Value),
    Diff(DiffContent),
    Terminal(TerminalContent),
}

/// File modification shown as a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffContent {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    pub new_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Terminal reference in tool call content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalContent {
    pub terminal_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A file location accessed by a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallLocation {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request from agent to client for user permission on a tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionRequest {
    pub session_id: SessionId,
    pub tool_call: ToolCallUpdate,
    pub options: Vec<PermissionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The user's decision on a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedPermissionOutcome {
    pub option_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Outcome of a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionResponse {
    #[serde(flatten)]
    pub outcome: PermissionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Permission outcome variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PermissionOutcome {
    Selected(SelectedPermissionOutcome),
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
}

// ---------------------------------------------------------------------------
// session/update notification (the main bidirectional channel)
// ---------------------------------------------------------------------------

/// A plan entry in the agent's execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub content: String,
    pub priority: PlanEntryPriority,
    pub status: PlanEntryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEntryPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEntryStatus {
    Pending,
    InProgress,
    Completed,
}

/// An execution plan for accomplishing complex tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub entries: Vec<PlanEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// An available slash command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCommand {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Session mode information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMode {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The set of modes and the one currently active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModeState {
    pub current_mode_id: String,
    pub available_modes: Vec<SessionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A session configuration option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigOption {
    pub config_id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boolean: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Cost information for a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub amount: f64,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Context window and cost update for a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageUpdate {
    pub used: u64,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Update to session metadata (partial — only changed fields).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Config option update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOptionUpdate {
    pub config_options: Vec<SessionConfigOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Current mode update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentModeUpdate {
    pub current_mode_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Available commands update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCommandsUpdate {
    pub available_commands: Vec<AvailableCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A single block of content in a streamed message chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentChunk {
    pub content: ContentBlock,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A tool call initiated by the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub tool_call_id: ToolCallId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolCallStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ToolCallContent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ToolCallLocation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The discriminated union of all `session/update` variants.
///
/// The `sessionUpdate` field is the discriminator. This enum is used as the
/// `update` field of a `SessionNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "sessionUpdate", rename_all = "snake_case")]
pub enum SessionUpdate {
    /// A chunk of the user's message being streamed.
    UserMessageChunk(ContentChunk),
    /// A chunk of the agent's response being streamed.
    AgentMessageChunk(ContentChunk),
    /// A chunk of the agent's internal reasoning being streamed.
    AgentThoughtChunk(ContentChunk),
    /// A tool call has been initiated.
    ToolCall(ToolCall),
    /// Update on the status or results of a tool call.
    ToolCallUpdate(ToolCallUpdate),
    /// The agent's execution plan.
    Plan {
        entries: Vec<PlanEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
    /// Available commands are ready or have changed.
    AvailableCommandsUpdate(AvailableCommandsUpdate),
    /// The current mode of the session has changed.
    CurrentModeUpdate(CurrentModeUpdate),
    /// Session configuration options have been updated.
    ConfigOptionUpdate(ConfigOptionUpdate),
    /// Session metadata has been updated (title, timestamps, etc.).
    SessionInfoUpdate(SessionInfoUpdate),
    /// Context window and cost update for the session.
    UsageUpdate(UsageUpdate),
}

/// Notification parameters for `session/update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNotificationParams {
    pub session_id: SessionId,
    pub update: SessionUpdate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

/// Request parameters for the `initialize` method (client → agent).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    /// The latest protocol version the client supports.
    pub protocol_version: ProtocolVersion,
    /// Capabilities supported by the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_capabilities: Option<ClientCapabilities>,
    /// Implementation metadata for the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<Implementation>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response to the `initialize` method (agent → client).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    /// The negotiated protocol version.
    pub protocol_version: ProtocolVersion,
    /// Capabilities supported by the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_capabilities: Option<AgentCapabilities>,
    /// Implementation metadata for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_info: Option<Implementation>,
    /// Authentication methods the agent supports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_methods: Vec<AuthMethod>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/new
// ---------------------------------------------------------------------------

/// Request parameters for creating a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionRequest {
    /// The working directory for this session. Must be an absolute path.
    pub cwd: String,
    /// MCP servers the agent should connect to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
    /// Additional workspace roots (absolute paths).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_directories: Vec<String>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from creating a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResponse {
    /// Unique identifier for the created session.
    pub session_id: SessionId,
    /// Initial session configuration options, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options: Option<Vec<SessionConfigOption>>,
    /// Initial mode state, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModeState>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/load
// ---------------------------------------------------------------------------

/// Request parameters for loading an existing session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionRequest {
    /// The ID of the session to load.
    pub session_id: SessionId,
    /// The working directory for this session. Must be an absolute path.
    pub cwd: String,
    /// MCP servers to connect to for this session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
    /// Additional workspace roots to activate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_directories: Vec<String>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from loading an existing session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionResponse {
    /// Initial session configuration options, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options: Option<Vec<SessionConfigOption>>,
    /// Initial mode state, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModeState>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/prompt
// ---------------------------------------------------------------------------

/// Request parameters for sending a prompt to a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    /// The ID of the session to send this message to.
    pub session_id: SessionId,
    /// The blocks of content that compose the user's message.
    pub prompt: Vec<ContentBlock>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from processing a user prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResponse {
    /// Indicates why the agent stopped processing the turn.
    pub stop_reason: StopReason,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/resume
// ---------------------------------------------------------------------------

/// Request parameters for resuming an existing session without replay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeSessionRequest {
    /// The ID of the session to resume.
    pub session_id: SessionId,
    /// The working directory for this session. Must be an absolute path.
    pub cwd: String,
    /// MCP servers to connect to for this session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
    /// Additional workspace roots to activate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_directories: Vec<String>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from resuming an existing session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeSessionResponse {
    /// Initial session configuration options, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options: Option<Vec<SessionConfigOption>>,
    /// Initial mode state, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModeState>,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/close
// ---------------------------------------------------------------------------

/// Request parameters for closing an active session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    /// The ID of the session to close.
    pub session_id: SessionId,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from closing a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloseSessionResponse {
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/delete
// ---------------------------------------------------------------------------

/// Request parameters for deleting a session from the session list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionRequest {
    /// The ID of the session to delete.
    pub session_id: SessionId,
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from deleting a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeleteSessionResponse {
    /// Reserved for ACP forward-compatibility metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/list
// ---------------------------------------------------------------------------

/// Session information returned by `session/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_directories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request parameters for listing sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response from listing sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// session/cancel notification (client → agent)
// ---------------------------------------------------------------------------

/// Notification to cancel ongoing operations for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelNotification {
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Method name constants — match the ACP v1 specification exactly
// ---------------------------------------------------------------------------

/// Method name for protocol-level request cancellation notifications.
pub const CANCEL_REQUEST_METHOD: &str = "$/cancel_request";

/// Method name for the initialize request.
pub const METHOD_INITIALIZE: &str = "initialize";

/// Method name for creating a new session.
pub const METHOD_NEW_SESSION: &str = "session/new";

/// Method name for loading an existing session.
pub const METHOD_LOAD_SESSION: &str = "session/load";

/// Method name for resuming a session.
pub const METHOD_RESUME_SESSION: &str = "session/resume";

/// Method name for sending a prompt.
pub const METHOD_PROMPT: &str = "session/prompt";

/// Method name for closing a session.
pub const METHOD_CLOSE_SESSION: &str = "session/close";

/// Method name for deleting a session.
pub const METHOD_DELETE_SESSION: &str = "session/delete";

/// Method name for listing sessions.
pub const METHOD_LIST_SESSIONS: &str = "session/list";

/// Method name for canceling a prompt turn (notification).
pub const METHOD_CANCEL: &str = "session/cancel";

/// Method name for the session/update notification (agent → client).
pub const METHOD_SESSION_UPDATE: &str = "session/update";

/// Method name for fs/read_text_file (agent → client).
pub const METHOD_READ_TEXT_FILE: &str = "fs/read_text_file";

/// Method name for fs/write_text_file (agent → client).
pub const METHOD_WRITE_TEXT_FILE: &str = "fs/write_text_file";

/// Method name for session/request_permission (agent → client).
pub const METHOD_REQUEST_PERMISSION: &str = "session/request_permission";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convenience wrapper for extracting and deserializing JSON-RPC request params
/// into a strongly-typed ACP request.
///
/// Deserialization failures (missing fields, wrong types, etc.) are mapped to
/// [`AcpError::MalformedRequest`] so they surface as ACP-specific
/// `INVALID_PARAMS`-equivalent errors rather than generic internal errors.
pub fn decode_params<T>(params: Option<serde_json::Value>) -> Result<T, AcpError>
where
    T: DeserializeOwned,
{
    let value = params.unwrap_or(serde_json::Value::Null);
    serde_json::from_value(value).map_err(|e| AcpError::malformed_request(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_impl() -> Implementation {
        Implementation {
            name: "nabu".to_string(),
            title: Some("Nabu".to_string()),
            version: "0.1.0".to_string(),
            _meta: None,
        }
    }

    #[test]
    fn initialize_request_roundtrips() {
        let req = InitializeRequest {
            protocol_version: 1,
            client_capabilities: Some(ClientCapabilities::default()),
            client_info: Some(sample_impl()),
            _meta: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        let back: InitializeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.protocol_version, 1);
        assert!(back.client_capabilities.is_some());
        assert_eq!(back.client_info.as_ref().unwrap().name, "nabu");
    }

    #[test]
    fn initialize_request_accepts_missing_optional_fields() {
        let json = r#"{"protocolVersion": 1}"#;
        let req: InitializeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.protocol_version, 1);
        assert!(req.client_capabilities.is_none());
        assert!(req.client_info.is_none());
    }

    #[test]
    fn initialize_response_roundtrips() {
        let resp = InitializeResponse {
            protocol_version: 1,
            agent_capabilities: Some(AgentCapabilities {
                load_session: true,
                prompt_capabilities: None,
                mcp_capabilities: None,
                session_capabilities: None,
                auth: None,
                _meta: None,
            }),
            agent_info: Some(sample_impl()),
            auth_methods: vec![],
            _meta: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        let back: InitializeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.protocol_version, 1);
        assert!(back.agent_capabilities.as_ref().unwrap().load_session);
    }

    #[test]
    fn new_session_request_roundtrips() {
        let req = NewSessionRequest {
            cwd: "/home/user/project".to_string(),
            mcp_servers: vec![McpServer::Stdio(McpServerStdio {
                name: "fs".to_string(),
                command: "/usr/bin/fs".to_string(),
                args: vec!["--root".to_string()],
                env: vec![],
                _meta: None,
            })],
            additional_directories: vec!["/extra".to_string()],
            _meta: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        let back: NewSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cwd, "/home/user/project");
        assert_eq!(back.mcp_servers.len(), 1);
        match &back.mcp_servers[0] {
            McpServer::Stdio(s) => {
                assert_eq!(s.name, "fs");
                assert_eq!(s.command, "/usr/bin/fs");
            }
            _ => panic!("expected stdio"),
        }
    }

    #[test]
    fn mcp_server_stdio_defaults_when_type_absent() {
        let json = r#"{"name":"my-server","command":"/bin/srv","args":["--flag"],"env":[]}"#;
        let server: McpServer = serde_json::from_str(json).unwrap();
        assert!(matches!(server, McpServer::Stdio(_)));
        if let McpServer::Stdio(s) = server {
            assert_eq!(s.name, "my-server");
            assert_eq!(s.command, "/bin/srv");
        }
    }

    #[test]
    fn mcp_server_http_deserializes_from_type() {
        let json = r#"{"type":"http","name":"web","url":"https://example.com/mcp","headers":[]}"#;
        let server: McpServer = serde_json::from_str(json).unwrap();
        assert!(matches!(server, McpServer::Http(_)));
    }

    #[test]
    fn mcp_server_serializes_type_tag() {
        let server = McpServer::Http(McpServerHttp {
            name: "web".to_string(),
            url: "https://example.com/mcp".to_string(),
            headers: vec![],
            _meta: None,
        });

        let json = serde_json::to_string(&server).unwrap();
        assert!(json.contains(r#""type":"http""#));
    }

    #[test]
    fn mcp_server_roundtrips() {
        let original = McpServer::Stdio(McpServerStdio {
            name: "test-srv".to_string(),
            command: "/usr/local/bin/srv".to_string(),
            args: vec!["--opt".to_string(), "val".to_string()],
            env: vec![EnvVariable {
                name: "FOO".to_string(),
                value: "bar".to_string(),
                _meta: None,
            }],
            _meta: None,
        });

        let json = serde_json::to_string(&original).unwrap();
        let back: McpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn prompt_request_roundtrips() {
        let req = PromptRequest {
            session_id: "sess_abc".to_string(),
            prompt: vec![ContentBlock::Text(TextContent {
                text: "Hello, agent!".to_string(),
                annotations: None,
                _meta: None,
            })],
            _meta: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        let back: PromptRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_abc");
        assert_eq!(back.prompt.len(), 1);
    }

    #[test]
    fn content_block_resource_link_roundtrips() {
        let block = ContentBlock::ResourceLink(ResourceLink {
            uri: "file:///home/user/doc.md".to_string(),
            name: "doc.md".to_string(),
            title: Some("Document".to_string()),
            description: Some("a document".to_string()),
            mime_type: Some("text/markdown".to_string()),
            size: Some(1024),
            annotations: None,
            _meta: None,
        });

        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"resource_link""#));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        match back {
            ContentBlock::ResourceLink(link) => {
                assert_eq!(link.uri, "file:///home/user/doc.md");
                assert_eq!(link.name, "doc.md");
                assert_eq!(link.title.as_deref(), Some("Document"));
            }
            _ => panic!("expected ResourceLink"),
        }
    }

    #[test]
    fn stop_reason_roundtrips() {
        let cases = vec![
            (StopReason::EndTurn, "\"end_turn\""),
            (StopReason::MaxTokens, "\"max_tokens\""),
            (StopReason::MaxTurnRequests, "\"max_turn_requests\""),
            (StopReason::Refusal, "\"refusal\""),
            (StopReason::Cancelled, "\"cancelled\""),
        ];

        for (reason, expected) in cases {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, expected);
            let back: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(back, reason);
        }
    }

    #[test]
    fn prompt_response_roundtrips() {
        let resp = PromptResponse {
            stop_reason: StopReason::EndTurn,
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: PromptResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn close_session_request_roundtrips() {
        let req = CloseSessionRequest {
            session_id: "sess_xyz".to_string(),
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CloseSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_xyz");
    }

    #[test]
    fn close_session_response_is_empty_object() {
        let resp = CloseSessionResponse::default();
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn resume_session_request_roundtrips() {
        let req = ResumeSessionRequest {
            session_id: "sess_456".to_string(),
            cwd: "/workspace".to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ResumeSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_456");
        assert_eq!(back.cwd, "/workspace");
    }

    #[test]
    fn load_session_request_roundtrips() {
        let req = LoadSessionRequest {
            session_id: "sess_456".to_string(),
            cwd: "/workspace".to_string(),
            mcp_servers: vec![],
            additional_directories: vec![],
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: LoadSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_456");
        assert_eq!(back.cwd, "/workspace");
    }

    #[test]
    fn agent_capabilities_default_is_empty() {
        let caps = AgentCapabilities::default();
        assert!(!caps.load_session);
        assert!(caps.prompt_capabilities.is_none());
        assert!(caps.mcp_capabilities.is_none());
        assert!(caps.session_capabilities.is_none());
        assert!(caps.auth.is_none());
    }

    #[test]
    fn client_capabilities_default() {
        let caps = ClientCapabilities::default();
        assert!(!caps.terminal);
        assert!(caps.fs.is_none());
    }

    #[test]
    fn implementation_requires_name_and_version() {
        // Missing version → error
        let json = r#"{"name":"test"}"#;
        assert!(Implementation::deserialize(
            serde_json::from_str::<serde_json::Value>(json).unwrap()
        )
        .is_err());

        // Full
        let json = r#"{"name":"test","version":"1.0.0","title":"Test"}"#;
        let imp: Implementation = serde_json::from_str(json).unwrap();
        assert_eq!(imp.name, "test");
        assert_eq!(imp.version, "1.0.0");
    }

    #[test]
    fn embedded_resource_untagged_roundtrips() {
        let json = r#"{"text":"hello","uri":"file:///hello.txt","mimeType":"text/plain"}"#;
        let res: EmbeddedResourceResource = serde_json::from_str(json).unwrap();
        assert!(matches!(res, EmbeddedResourceResource::Text(_)));
    }

    #[test]
    fn mcp_server_absent_type_defaults_to_stdio() {
        let json = r#"{"name":"s","command":"c","args":[],"env":[]}"#;
        let srv: McpServer = serde_json::from_str(json).unwrap();
        assert!(matches!(srv, McpServer::Stdio(_)));
    }

    #[test]
    fn extra_fields_are_ignored_for_forward_compat() {
        let json = r#"{"protocolVersion": 1, "futureField": true, "another": 42}"#;
        let req: InitializeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.protocol_version, 1);
    }

    #[test]
    fn session_update_agent_message_chunk_roundtrips() {
        let json = r#"{
            "sessionUpdate": "agent_message_chunk",
            "messageId": "msg_123",
            "content": {
                "type": "text",
                "text": "Hello from agent"
            }
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                assert_eq!(chunk.message_id.as_deref(), Some("msg_123"));
                match &chunk.content {
                    ContentBlock::Text(t) => assert_eq!(t.text, "Hello from agent"),
                    _ => panic!("expected text content"),
                }
            }
            _ => panic!("expected AgentMessageChunk"),
        }
    }

    #[test]
    fn session_update_user_message_chunk_roundtrips() {
        let json = r#"{
            "sessionUpdate": "user_message_chunk",
            "content": {
                "type": "text",
                "text": "Hello from user"
            }
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        assert!(matches!(update, SessionUpdate::UserMessageChunk(_)));
    }

    #[test]
    fn session_update_plan_roundtrips() {
        let json = r#"{
            "sessionUpdate": "plan",
            "entries": [
                {
                    "content": "Check for errors",
                    "priority": "high",
                    "status": "pending"
                }
            ]
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        match update {
            SessionUpdate::Plan { entries, .. } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].content, "Check for errors");
                assert_eq!(entries[0].priority, PlanEntryPriority::High);
                assert_eq!(entries[0].status, PlanEntryStatus::Pending);
            }
            _ => panic!("expected Plan"),
        }
    }

    #[test]
    fn session_update_usage_update_roundtrips() {
        let json = r#"{
            "sessionUpdate": "usage_update",
            "used": 53000,
            "size": 200000,
            "cost": {"amount": 0.045, "currency": "USD"}
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        match update {
            SessionUpdate::UsageUpdate(u) => {
                assert_eq!(u.used, 53000);
                assert_eq!(u.size, 200000);
                assert_eq!(u.cost.as_ref().unwrap().amount, 0.045);
            }
            _ => panic!("expected UsageUpdate"),
        }
    }

    #[test]
    fn session_update_tool_call_roundtrips() {
        let json = r#"{
            "sessionUpdate": "tool_call",
            "toolCallId": "call_001",
            "title": "Reading file",
            "kind": "read",
            "status": "pending"
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        match update {
            SessionUpdate::ToolCall(tc) => {
                assert_eq!(tc.tool_call_id, "call_001");
                assert_eq!(tc.title, "Reading file");
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn session_update_tool_call_update_roundtrips() {
        let json = r#"{
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_001",
            "status": "completed"
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        match update {
            SessionUpdate::ToolCallUpdate(tcu) => {
                assert_eq!(tcu.tool_call_id, "call_001");
                assert_eq!(tcu.status, Some(ToolCallStatus::Completed));
            }
            _ => panic!("expected ToolCallUpdate"),
        }
    }

    #[test]
    fn session_notification_roundtrips() {
        let json = r#"{
            "sessionId": "sess_abc",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "messageId": "msg_123",
                "content": {
                    "type": "text",
                    "text": "Hello"
                }
            }
        }"#;
        let notif: SessionNotificationParams = serde_json::from_str(json).unwrap();
        assert_eq!(notif.session_id, "sess_abc");
        match notif.update {
            SessionUpdate::AgentMessageChunk(_) => {}
            _ => panic!("expected AgentMessageChunk"),
        }
    }

    #[test]
    fn permission_outcome_selected_roundtrips() {
        let json = r#"{"outcome":"selected","optionId":"allow_once"}"#;
        let resp: RequestPermissionResponse = serde_json::from_str(json).unwrap();
        match resp.outcome {
            PermissionOutcome::Selected(s) => assert_eq!(s.option_id, "allow_once"),
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn permission_outcome_cancelled_roundtrips() {
        let json = r#"{"outcome":"cancelled"}"#;
        let resp: RequestPermissionResponse = serde_json::from_str(json).unwrap();
        assert!(matches!(resp.outcome, PermissionOutcome::Cancelled { .. }));
    }

    #[test]
    fn delete_session_request_roundtrips() {
        let req = DeleteSessionRequest {
            session_id: "sess_del".to_string(),
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: DeleteSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_del");
    }

    #[test]
    fn list_sessions_response_roundtrips() {
        let resp = ListSessionsResponse {
            sessions: vec![SessionInfo {
                session_id: "sess_1".to_string(),
                cwd: "/home/user".to_string(),
                additional_directories: vec![],
                title: Some("My Session".to_string()),
                updated_at: Some("2025-10-29T14:22:15Z".to_string()),
                _meta: None,
            }],
            next_cursor: Some("eyJwYWdlIjogMn0=".to_string()),
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: ListSessionsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sessions.len(), 1);
        assert_eq!(back.sessions[0].session_id, "sess_1");
        assert_eq!(back.next_cursor.as_deref(), Some("eyJwYWdlIjogMn0="));
    }
}
