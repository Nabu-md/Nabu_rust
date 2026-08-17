//! # ACP Client-Side Types
//!
//! Strongly-typed request and response structures for ACP client-side methods
//! — operations that an ACP agent may invoke *on the client* (Nabu).
//!
//! These types map directly to the ACP protocol wire format but live in the
//! `tool_calling` layer rather than the `acp` layer. The `acp` module owns
//! the server-side (agent) protocol surface; this submodule owns the
//! client-side method types that Nabu advertises via `ClientCapabilities`
//! and handles through its JSON-RPC router.
//!
//! ## Methods
//!
//! | Method | Capability | Description |
//! |--------|-----------|-------------|
//! | `fs/read_text_file` | `fs.readTextFile` | Read a text file from the vault |
//! | `fs/write_text_file` | `fs.writeTextFile` | Write content to a file in the vault |
//! | `terminal/create` | `terminal` | Execute a command in a new terminal |
//! | `terminal/kill` | `terminal` | Kill a terminal without releasing it |
//! | `terminal/output` | `terminal` | Read current terminal output |
//! | `terminal/wait_for_exit` | `terminal` | Wait for terminal command to exit |
//! | `terminal/release` | `terminal` | Release a terminal and free resources |
//! | `elicitation/create` | `elicitation` | Request structured user input |
//! | `session/request_permission` | (none) | Request permission for a tool call |
//!
//! ## Forward compatibility
//!
//! None of these types use `#[serde(deny_unknown_fields)]`. Unknown fields
//! in incoming JSON are silently ignored, so future ACP revisions that add
//! fields will deserialize without error. Optional fields use `#[serde(default)]`.

use crate::acp::types::{EnvVariable, SessionId};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// A unique identifier for a terminal instance.
pub type TerminalId = String;

/// A unique identifier for an elicitation request.
pub type ElicitationId = String;

/// A unique identifier for a permission option.
pub type PermissionOptionId = String;

/// A unique identifier for a tool call.
pub type ToolCallId = String;

// ---------------------------------------------------------------------------
// Client capabilities (extends acp::ClientCapabilities)
// ---------------------------------------------------------------------------

/// Elicitation capabilities a client may support.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationCapabilities {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Client-side capabilities that Nabu advertises during `initialize`.
///
/// This extends the `acp::ClientCapabilities` with elicitation support
/// (which is not present in the ACP types module) and provides a unified
/// structure for constructing the `client_capabilities` field of
/// [`InitializeRequest`](crate::acp::InitializeRequest).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpClientCapabilities {
    /// File-system read/write capabilities.
    #[serde(default)]
    pub fs_read: bool,
    /// Whether the client supports `fs/write_text_file`.
    #[serde(default)]
    pub fs_write: bool,
    /// Whether the client supports terminal operations.
    #[serde(default)]
    pub terminal: bool,
    /// Whether the client supports elicitations.
    #[serde(default)]
    pub elicitation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

impl AcpClientCapabilities {
    /// Build the corresponding `acp::ClientCapabilities` for wire serialization.
    pub fn to_acp_client_capabilities(&self) -> crate::acp::ClientCapabilities {
        crate::acp::ClientCapabilities {
            fs: Some(crate::acp::FileSystemCapabilities {
                read_text_file: self.fs_read,
                write_text_file: self.fs_write,
                _meta: None,
            }),
            terminal: self.terminal,
            session: None,
            _meta: self._meta.clone(),
        }
    }

    /// Returns `true` if the given ACP method is supported by these capabilities.
    pub fn supports_method(&self, method: &str) -> bool {
        match method {
            "fs/read_text_file" => self.fs_read,
            "fs/write_text_file" => self.fs_write,
            "terminal/create" | "terminal/kill" | "terminal/output" | "terminal/wait_for_exit"
            | "terminal/release" => self.terminal,
            "elicitation/create" => self.elicitation,
            "session/request_permission" => true,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// File-system methods
// ---------------------------------------------------------------------------

/// Request to read content from a text file.
///
/// The `path` must be absolute and must resolve within the vault root.
/// Path traversal (e.g. `../`) is rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileRequest {
    /// Absolute path to the file to read.
    pub path: String,
    /// Line number to start reading from (1-based). `null` starts from line 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    /// Maximum number of lines to read. `null` reads to end of file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response containing the contents of a text file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileResponse {
    /// Content payload returned by this response.
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request to write content to a text file.
///
/// The `path` must be absolute and must resolve within the vault root.
/// Path traversal is rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTextFileRequest {
    /// The text content to write to the file.
    pub content: String,
    /// Absolute path to the file to write.
    pub path: String,
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response to `fs/write_text_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTextFileResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Terminal methods
// ---------------------------------------------------------------------------

/// Exit status of a terminal command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitStatus {
    /// The process exit code (may be null if terminated by signal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    /// The signal that terminated the process (may be null if exited normally).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request to create a new terminal and execute a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalRequest {
    /// Array of command arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// The command to execute.
    pub command: String,
    /// Working directory for the command. Must be an absolute path.
    /// `null` uses the session's working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables for the command.
    #[serde(default)]
    pub env: Vec<EnvVariable>,
    /// Maximum number of output bytes to retain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_byte_limit: Option<u64>,
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response containing the ID of the created terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalResponse {
    /// The unique identifier for the created terminal.
    pub terminal_id: TerminalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request to kill a terminal without releasing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillTerminalRequest {
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    /// The ID of the terminal to kill.
    #[serde(alias = "terminal_id")]
    pub terminal_id: TerminalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response to `terminal/kill` method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillTerminalResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request to read current terminal output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputRequest {
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    /// The ID of the terminal to query.
    #[serde(alias = "terminal_id")]
    pub terminal_id: TerminalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response containing the terminal's current output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputResponse {
    /// Exit status if the command has completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<TerminalExitStatus>,
    /// The terminal output captured so far.
    pub output: String,
    /// Whether the output was truncated due to byte limits.
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request to wait for a terminal command to exit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForTerminalExitRequest {
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    /// The ID of the terminal to wait for.
    #[serde(alias = "terminal_id")]
    pub terminal_id: TerminalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response containing the exit status of a terminal command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForTerminalExitResponse {
    /// The process exit code (may be null if terminated by signal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    /// The signal that terminated the process (may be null if exited normally).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request to release a terminal and free its resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseTerminalRequest {
    /// The session ID for this request.
    #[serde(default)]
    pub session_id: SessionId,
    /// The ID of the terminal to release.
    #[serde(alias = "terminal_id")]
    pub terminal_id: TerminalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Response to `terminal/release` method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseTerminalResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Elicitation methods
// ---------------------------------------------------------------------------

/// A JSON Schema property for elicitation form fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationPropertySchema {
    /// The JSON type of this property (e.g. "string", "number", "boolean").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Human-readable name/title for this property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Type-safe elicitation schema for requesting structured user input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationSchema {
    /// Optional description of what this schema represents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Property definitions (must be primitive types).
    #[serde(default)]
    pub properties: std::collections::HashMap<String, ElicitationPropertySchema>,
    /// List of required property names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// Optional title for the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Type discriminator. Always `"object"`.
    #[serde(default = "default_object_type")]
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

impl Default for ElicitationSchema {
    fn default() -> Self {
        Self {
            description: None,
            properties: std::collections::HashMap::new(),
            required: None,
            title: None,
            r#type: default_object_type(),
            _meta: None,
        }
    }
}

fn default_object_type() -> String {
    "object".to_string()
}

/// Response to an elicitation request, provided by the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResponse {
    /// The ID of the elicitation that completed.
    pub elicitation_id: ElicitationId,
    /// The structured response data from the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The result of handling an elicitation request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ElicitationOutcome {
    /// The user provided a response.
    Provided {
        response: serde_json::Value,
    },
    /// The user cancelled the elicitation.
    Cancelled,
}

// ---------------------------------------------------------------------------
// Permission methods
// ---------------------------------------------------------------------------

/// The kind of permission option being presented to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

/// An option presented to the user when requesting permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    /// Hint about the nature of this permission option.
    pub kind: PermissionOptionKind,
    /// Human-readable label to display to the user.
    pub name: String,
    /// Unique identifier for this permission option.
    pub option_id: PermissionOptionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A file location being accessed or modified by a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallLocation {
    /// Optional line number within the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    /// The absolute file path being accessed or modified.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The kind of tool (agent tool vs MCP tool).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Agent,
    Mcp,
}

/// Execution status of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// An update to an existing tool call, used for permission requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdate {
    /// The ID of the tool call being updated.
    pub tool_call_id: ToolCallId,
    /// Update the execution status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolCallStatus>,
    /// Update the human-readable title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The tool kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Request for user permission to execute a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionRequest {
    /// Available permission options for the user to choose from.
    pub options: Vec<PermissionOption>,
    /// The session ID for this request.
    pub session_id: SessionId,
    /// Details about the tool call requiring permission.
    pub tool_call: ToolCallUpdate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// The outcome of a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RequestPermissionOutcome {
    /// The prompt turn was cancelled before the user responded.
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
    /// The user selected one of the provided options.
    Selected {
        /// The ID of the option the user selected.
        option_id: PermissionOptionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
}

/// Response to a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionResponse {
    pub outcome: RequestPermissionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Shared method-name constants
// ---------------------------------------------------------------------------

/// JSON-RPC method names for ACP client-side methods.
///
/// These are the method names that an ACP agent will send to Nabu as
/// JSON-RPC requests when Nabu has advertised the corresponding capabilities.
pub mod method {
    pub const FS_READ_TEXT_FILE: &str = "fs/read_text_file";
    pub const FS_WRITE_TEXT_FILE: &str = "fs/write_text_file";
    pub const TERMINAL_CREATE: &str = "terminal/create";
    pub const TERMINAL_KILL: &str = "terminal/kill";
    pub const TERMINAL_OUTPUT: &str = "terminal/output";
    pub const TERMINAL_WAIT_FOR_EXIT: &str = "terminal/wait_for_exit";
    pub const TERMINAL_RELEASE: &str = "terminal/release";
    pub const ELICITATION_CREATE: &str = "elicitation/create";
    pub const SESSION_REQUEST_PERMISSION: &str = "session/request_permission";
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Decode ACP client-side request params into a typed request struct.
///
/// Delegates to [`crate::acp::types::decode_params`] for deserialization,
/// mapping failures to [`crate::acp::AcpError`].
pub fn decode_params<T>(params: Option<serde_json::Value>) -> Result<T, crate::acp::AcpError>
where
    T: DeserializeOwned,
{
    crate::acp::types::decode_params(params)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::types::Implementation;

    #[test]
    fn client_capabilities_to_acp_roundtrip() {
        let caps = AcpClientCapabilities {
            fs_read: true,
            fs_write: false,
            terminal: true,
            elicitation: false,
            _meta: None,
        };
        let acp = caps.to_acp_client_capabilities();
        assert!(acp.fs.is_some());
        let fs = acp.fs.unwrap();
        assert!(fs.read_text_file);
        assert!(!fs.write_text_file);
        assert!(acp.terminal);
    }

    #[test]
    fn client_capabilities_supports_method() {
        let caps = AcpClientCapabilities {
            fs_read: true,
            fs_write: false,
            terminal: true,
            elicitation: true,
            _meta: None,
        };
        assert!(caps.supports_method("fs/read_text_file"));
        assert!(!caps.supports_method("fs/write_text_file"));
        assert!(caps.supports_method("terminal/create"));
        assert!(caps.supports_method("terminal/kill"));
        assert!(caps.supports_method("terminal/output"));
        assert!(caps.supports_method("terminal/wait_for_exit"));
        assert!(caps.supports_method("terminal/release"));
        assert!(caps.supports_method("elicitation/create"));
        assert!(caps.supports_method("session/request_permission"));
        assert!(!caps.supports_method("unknown/method"));
    }

    #[test]
    fn read_text_file_request_roundtrips() {
        let req = ReadTextFileRequest {
            path: "/vault/note.md".to_string(),
            line: Some(1),
            limit: Some(100),
            session_id: "sess_1".to_string(),
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ReadTextFileRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "/vault/note.md");
        assert_eq!(back.line, Some(1));
        assert_eq!(back.limit, Some(100));
    }

    #[test]
    fn write_text_file_request_roundtrips() {
        let req = WriteTextFileRequest {
            content: "hello world".to_string(),
            path: "/vault/out.md".to_string(),
            session_id: "sess_1".to_string(),
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: WriteTextFileRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "hello world");
        assert_eq!(back.path, "/vault/out.md");
    }

    #[test]
    fn create_terminal_request_roundtrips() {
        let req = CreateTerminalRequest {
            args: vec!["--help".to_string()],
            command: "ls".to_string(),
            cwd: Some("/tmp".to_string()),
            env: vec![EnvVariable {
                name: "FOO".to_string(),
                value: "bar".to_string(),
                _meta: None,
            }],
            output_byte_limit: Some(1024),
            session_id: "sess_1".to_string(),
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateTerminalRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.command, "ls");
        assert_eq!(back.args, vec!["--help".to_string()]);
        assert_eq!(back.cwd, Some("/tmp".to_string()));
        assert_eq!(back.output_byte_limit, Some(1024));
    }

    #[test]
    fn wait_for_exit_response_roundtrips() {
        let resp = WaitForTerminalExitResponse {
            exit_code: Some(0),
            signal: None,
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: WaitForTerminalExitResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.exit_code, Some(0));
        assert_eq!(back.signal, None);
    }

    #[test]
    fn permission_option_roundtrips() {
        let opt = PermissionOption {
            kind: PermissionOptionKind::AllowOnce,
            name: "Allow".to_string(),
            option_id: "opt_1".to_string(),
            _meta: None,
        };
        let json = serde_json::to_string(&opt).unwrap();
        let back: PermissionOption = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, PermissionOptionKind::AllowOnce);
        assert_eq!(back.option_id, "opt_1");
    }

    #[test]
    fn request_permission_response_selected_roundtrips() {
        let resp = RequestPermissionResponse {
            outcome: RequestPermissionOutcome::Selected {
                option_id: "opt_allow".to_string(),
                _meta: None,
            },
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RequestPermissionResponse = serde_json::from_str(&json).unwrap();
        match back.outcome {
            RequestPermissionOutcome::Selected { option_id, .. } => {
                assert_eq!(option_id, "opt_allow");
            }
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn request_permission_response_cancelled_roundtrips() {
        let resp = RequestPermissionResponse {
            outcome: RequestPermissionOutcome::Cancelled { _meta: None },
            _meta: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RequestPermissionResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.outcome, RequestPermissionOutcome::Cancelled { .. }));
    }

    #[test]
    fn terminal_exit_status_roundtrips() {
        let status = TerminalExitStatus {
            exit_code: Some(1),
            signal: Some("SIGTERM".to_string()),
            _meta: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        let back: TerminalExitStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back.exit_code, Some(1));
        assert_eq!(back.signal, Some("SIGTERM".to_string()));
    }

    #[test]
    fn elicitation_schema_defaults_type_to_object() {
        let schema = ElicitationSchema::default();
        assert_eq!(schema.r#type, "object");
        assert!(schema.properties.is_empty());
    }

    #[test]
    fn tool_call_update_roundtrips() {
        let update = ToolCallUpdate {
            tool_call_id: "tc_1".to_string(),
            status: Some(ToolCallStatus::InProgress),
            title: None,
            kind: Some(ToolKind::Agent),
            _meta: None,
        };
        let json = serde_json::to_string(&update).unwrap();
        let back: ToolCallUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tool_call_id, "tc_1");
        assert_eq!(back.status, Some(ToolCallStatus::InProgress));
    }

    #[test]
    fn implementation_used_in_acp_types() {
        let _impl = Implementation {
            name: "nabu".to_string(),
            title: None,
            version: "0.1.0".to_string(),
            _meta: None,
        };
    }

    #[test]
    fn read_text_file_request_ignores_unknown_fields() {
        let json = r#"{"path":"/vault/note.md","sessionId":"s1","futureField":42}"#;
        let req: ReadTextFileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.path, "/vault/note.md");
        assert_eq!(req.session_id, "s1");
    }

    #[test]
    fn request_permission_request_roundtrips() {
        let req = RequestPermissionRequest {
            options: vec![PermissionOption {
                kind: PermissionOptionKind::AllowOnce,
                name: "Allow".to_string(),
                option_id: "allow_once".to_string(),
                _meta: None,
            }],
            session_id: "sess_1".to_string(),
            tool_call: ToolCallUpdate {
                tool_call_id: "tc_1".to_string(),
                status: None,
                title: None,
                kind: None,
                _meta: None,
            },
            _meta: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RequestPermissionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess_1");
        assert_eq!(back.options.len(), 1);
        assert_eq!(back.tool_call.tool_call_id, "tc_1");
    }
}
