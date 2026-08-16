//! # ACP Protocol Types
//!
//! Strongly-typed request and response structures for the Agent Communication
//! Protocol (ACP) method surface. These types map directly to the JSON wire
//! format described in the ACP specification and are designed to be
//! serialized into `serde_json::Value` for transport through the existing
//! [`crate::rpc`] JSON-RPC 2.0 infrastructure.
//!
//! ## Forward compatibility
//!
//! None of the types in this module use `#[serde(deny_unknown_fields)]`.
//! Unknown fields in incoming JSON are silently ignored, so future ACP revisions
//! that add fields will deserialize without error. Optional fields use
//! `#[serde(default)]` and `skip_serializing_if = "Option::is_none"` so they
//! round-trip cleanly and do not bloat responses.
//!
//! ## Naming convention
//!
//! ACP wire fields use `camelCase` (except the reserved `_meta`); Rust field
//! names use `snake_case` and serde performs the conversion via
//! `#[serde(rename_all = "camelCase")]`.

use crate::acp::error::AcpError;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

// ---------------------------------------------------------------------------
// Scalar type aliases
// ---------------------------------------------------------------------------

/// The ACP protocol version. This is a single unsigned integer; the value is
/// only incremented for breaking changes.
pub type ProtocolVersion = u16;

/// The Nabu ACP implementation reports this as its supported protocol version
/// during `initialize`. Nabu Phase 3a targets ACP protocol version 1.
pub const SUPPORTED_PROTOCOL_VERSION: ProtocolVersion = 1;

/// A unique identifier for a conversation session.
pub type SessionId = String;

/// A typed identifier for an authentication method.
pub type AuthMethodId = String;

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

/// Capabilities for the `logout` method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogoutCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Authentication-related capabilities an agent may advertise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logout: Option<LogoutCapabilities>,
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
    pub auth: Option<AgentAuthCapabilities>,
    /// Reserved for ACP forward-compatibility metadata.
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
    /// Reserved for ACP forward-compatibility metadata.
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
            obj.insert("type".to_string(), serde_json::Value::String(tag.to_string()));
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
// initialize
// ---------------------------------------------------------------------------

/// Request parameters for the `initialize` method.
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

/// Response to the `initialize` method.
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
    pub config_options: Option<Vec<serde_json::Value>>,
    /// Initial mode state, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<serde_json::Value>,
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
    pub config_options: Option<Vec<serde_json::Value>>,
    /// Initial mode state, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<serde_json::Value>,
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
    pub config_options: Option<Vec<serde_json::Value>>,
    /// Initial mode state, if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<serde_json::Value>,
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
            name: "nabu-agent".to_string(),
            title: Some("Nabu Agent".to_string()),
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
        assert_eq!(back.client_info.as_ref().unwrap().name, "nabu-agent");
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
    fn initialize_request_ignores_unknown_fields() {
        let json = r#"{"protocolVersion": 1, "futureField": 42}"#;
        let req: InitializeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.protocol_version, 1);
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
    fn resume_session_response_roundtrips() {
        let resp = ResumeSessionResponse {
            config_options: None,
            modes: None,
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn new_session_response_roundtrips() {
        let resp = NewSessionResponse {
            session_id: "sess_123".to_string(),
            config_options: None,
            modes: None,
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: NewSessionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_123");
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
        assert!(Implementation::deserialize(serde_json::from_str::<serde_json::Value>(json).unwrap()).is_err());

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
}
