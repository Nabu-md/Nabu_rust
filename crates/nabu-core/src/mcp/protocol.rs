//! # MCP Protocol Types
//!
//! Strongly-typed message structures for the Model Context Protocol (MCP) server.
//! These types map directly to the MCP JSON wire format and are used by the
//! [`McpServer`](super::server::McpServer) handlers when serializing
//! [`Request`](crate::rpc::Request) params and deserializing responses.
//!
//! The types are pure data models — no transport, no I/O. They are designed
//! for forward compatibility: no `#[serde(deny_unknown_fields)]`, optional
//! fields use `#[serde(default)]` and `skip_serializing_if = "Option::is_none"`.

use serde::{Deserialize, Serialize};

/// The MCP protocol version implemented by this server.
///
/// See: <https://spec.modelcontextprotocol.io/specification>
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP server implementation info returned in the `initialize` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// Client capabilities advertised during `initialize`.
///
/// This is a permissive, forward-compatible container — unknown capability
/// types are ignored (the struct only tracks the top-level flags the MCP
/// server actually uses).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootCapabilities {
    #[serde(default)]
    pub list_dynamic: bool,
}

/// Server capabilities advertised in the `initialize` response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_dynamic: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_dynamic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_dynamic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_dynamic: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingCapability {}

/// `initialize` request parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ClientCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<Implementation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// `initialize` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Implementation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// tools/list / tools/call
// ---------------------------------------------------------------------------

/// Tool definition returned by `tools/list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    /// JSON Schema (draft 7) for the tool's arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// `tools/list` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<McpTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `tools/call` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A single content block returned by a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
    Image {
        #[serde(rename = "data")]
        data_b64: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
    Audio {
        #[serde(rename = "data")]
        data_b64: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
    Resource {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource: Option<Resource>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        _meta: Option<serde_json::Value>,
    },
}

/// Result of `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub structured_content: serde_json::Value,
    #[serde(rename = "isError", default, skip_serializing_if = "is_false")]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

fn is_false(b: &bool) -> bool {
    !b
}

// ---------------------------------------------------------------------------
// resources/list / resources/read
// ---------------------------------------------------------------------------

/// Resource definition returned by `resources/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// `resources/list` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesResult {
    pub resources: Vec<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `resources/read` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceParams {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// A single resource content block returned by `resources/read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContents {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _meta: Option<serde_json::Value>,
}

/// `resources/read` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContents>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Decode MCP method params into a typed structure.
pub fn decode_params<T>(params: Option<serde_json::Value>) -> Result<T, super::error::McpError>
where
    T: serde::de::DeserializeOwned,
{
    let value = params.unwrap_or(serde_json::Value::Null);
    serde_json::from_value(value)
        .map_err(|e| super::error::McpError::invalid_params(e.to_string()))
}

/// Convert a Nabu [`ToolSpec`](crate::tool_calling::ToolSpec) into an MCP
/// [`McpTool`], translating the parameter declarations into a JSON-Schema
/// `inputSchema` object.
pub fn tool_spec_to_mcp_tool(
    spec: &crate::tool_calling::ToolSpec,
    tool_display_name: Option<String>,
) -> McpTool {
    let properties: serde_json::Map<String, serde_json::Value> = spec
        .parameters
        .iter()
        .filter_map(|p| {
            let mut schema = serde_json::Map::new();
            if let Some(t) = &p.schema.r#type {
                schema.insert("type".to_string(), serde_json::Value::String(t.clone()));
            }
            if let Some(d) = &p.schema.description {
                schema.insert(
                    "description".to_string(),
                    serde_json::Value::String(d.clone()),
                );
            }
            if let Some(e) = &p.schema.enum_values {
                schema.insert(
                    "enum".to_string(),
                    serde_json::Value::Array(
                        e.iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
            }
            Some((
                p.name.clone(),
                serde_json::Value::Object(schema),
            ))
        })
        .collect();

    let required: Vec<String> = spec
        .parameters
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.clone())
        .collect();

    let mut schema_obj = serde_json::Map::new();
    schema_obj.insert(
        "type".to_string(),
        serde_json::Value::String("object".to_string()),
    );
    if !properties.is_empty() {
        schema_obj.insert(
            "properties".to_string(),
            serde_json::Value::Object(properties),
        );
    }
    if !required.is_empty() {
        schema_obj.insert(
            "required".to_string(),
            serde_json::Value::Array(
                required
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
        );
    }

    let input_schema = serde_json::Value::Object(schema_obj);

    let mcp_name = mcp_tool_name(&spec.id.0);
    let display_name = tool_display_name.unwrap_or_else(|| spec.name.clone());

    McpTool {
        name: mcp_name,
        description: spec.description.clone(),
        input_schema,
        title: Some(display_name),
        _meta: None,
    }
}

/// Translate a Nabu tool ID (e.g. `nabu:search_note`) into an MCP-safe tool
/// name (e.g. `search_note`).  MCP tool names must match `^[a-zA-Z0-9._-]+$`,
/// so the `nabu:` namespace prefix is stripped.
pub fn mcp_tool_name(tool_id: &str) -> String {
    tool_id
        .strip_prefix("nabu:")
        .unwrap_or(tool_id)
        .to_string()
}

/// Translate an MCP tool name back to a Nabu tool ID.
pub fn tool_id_from_mcp_name(name: &str) -> String {
    if name.starts_with("nabu:") {
        name.to_string()
    } else {
        format!("nabu:{}", name)
    }
}

/// Build a `CallToolResult` from a [`ToolResult`](crate::tool_calling::ToolResult).
pub fn tool_result_to_call_result(
    result: crate::tool_calling::ToolResult,
) -> CallToolResult {
    match result.status {
        crate::tool_calling::ToolResultStatus::Success => {
            let result_json = result.result.unwrap_or(serde_json::Value::Null);
            let text = serde_json::to_string_pretty(&result_json)
                .unwrap_or_else(|_| result_json.to_string());
            CallToolResult {
                content: vec![ContentBlock::Text {
                    text,
                    annotations: None,
                    _meta: None,
                }],
                structured_content: result_json,
                is_error: false,
                _meta: None,
            }
        }
        _ => {
            let err_msg = result
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| format!("Tool failed with status: {:?}", result.status));
            let detail = result
                .error
                .as_ref()
                .and_then(|e| e.detail.clone())
                .unwrap_or_default();
            let text = if detail.is_empty() {
                err_msg
            } else {
                format!("{}: {}", err_msg, detail)
            };
            CallToolResult {
                content: vec![ContentBlock::Text {
                    text,
                    annotations: None,
                    _meta: None,
                }],
                structured_content: serde_json::Value::Null,
                is_error: true,
                _meta: None,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_calling::{ToolParam, ToolParamSchema, ToolSpec};

    #[test]
    fn mcp_tool_name_strips_nabu_prefix() {
        assert_eq!(mcp_tool_name("nabu:search_note"), "search_note");
        assert_eq!(mcp_tool_name("search_note"), "search_note");
    }

    #[test]
    fn tool_id_from_mcp_name_adds_prefix() {
        assert_eq!(tool_id_from_mcp_name("search_note"), "nabu:search_note");
        assert_eq!(
            tool_id_from_mcp_name("nabu:search_note"),
            "nabu:search_note"
        );
    }

    #[test]
    fn tool_spec_to_mcp_tool_generates_valid_schema() {
        let spec = ToolSpec::new("nabu:read_note", "Read Note", "Read a note by path")
            .with_param(ToolParam::required(
                "path",
                ToolParamSchema::of_type("string"),
            ))
            .with_param(ToolParam::optional(
                "encoding",
                ToolParamSchema::of_type("string"),
            ));

        let tool = tool_spec_to_mcp_tool(&spec, None);
        assert_eq!(tool.name, "read_note");
        assert_eq!(tool.title.as_deref(), Some("Read Note"));
        assert_eq!(tool.description, "Read a note by path");

        let schema = &tool.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["path"]));
    }

    #[test]
    fn decode_params_roundtrips() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct TestReq {
            name: String,
            count: u32,
        }
        let params = Some(serde_json::json!({ "name": "test", "count": 42 }));
        let req: TestReq = decode_params(params).unwrap();
        assert_eq!(req, TestReq {
            name: "test".to_string(),
            count: 42,
        });
    }

    #[test]
    fn decode_params_rejects_missing_fields() {
        #[derive(Deserialize)]
        struct TestReq {
            name: String,
        }
        let params = Some(serde_json::json!({ "other": 1 }));
        assert!(decode_params::<TestReq>(params).is_err());
    }
}
