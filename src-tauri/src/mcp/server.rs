//! MCP JSON-RPC method dispatch and the modern server capability surface.

use super::schema::{tool_definitions, ToolError, ToolErrorCode, ToolResult};
use serde_json::{json, Value};

pub const MCP_PROTOCOL_VERSION: &str = "2026-07-28";
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[MCP_PROTOCOL_VERSION];
pub const SERVER_NAME: &str = "SerialPortTool";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TOOLS_PAGE_SIZE_DEFAULT: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub enum DispatchResult {
    Response(Value),
    Notification,
}

pub fn dispatch(method: &str, params: Option<&Value>) -> Result<Value, RpcDispatchError> {
    match method {
        "server/discover" => Ok(json!({
            "supportedVersions": SUPPORTED_PROTOCOL_VERSIONS,
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            },
            "capabilities": {
                "tools": {"listChanged": false}
            }
        })),
        "tools/list" => tools_list(params),
        "tools/call" => tools_call(params),
        _ => Err(RpcDispatchError::method_not_found(method)),
    }
}

fn tools_list(params: Option<&Value>) -> Result<Value, RpcDispatchError> {
    let params = params.unwrap_or(&Value::Null);
    if !params.is_null() && !params.is_object() {
        return Err(RpcDispatchError::invalid_params(
            "tools/list params must be an object",
        ));
    }

    let cursor = params
        .get("cursor")
        .and_then(Value::as_str)
        .map(parse_cursor)
        .transpose()?;
    let offset = cursor.unwrap_or(0);
    let mut tools = tool_definitions();
    tools.sort_by(|left, right| left.name.cmp(&right.name));
    if offset > tools.len() {
        return Err(RpcDispatchError::invalid_params(
            "invalid tools/list cursor",
        ));
    }
    let end = offset
        .saturating_add(TOOLS_PAGE_SIZE_DEFAULT)
        .min(tools.len());
    let next_cursor = (end < tools.len()).then(|| end.to_string());
    Ok(json!({
        "tools": &tools[offset..end],
        "nextCursor": next_cursor,
    }))
}

fn tools_call(params: Option<&Value>) -> Result<Value, RpcDispatchError> {
    let params = params
        .and_then(Value::as_object)
        .ok_or_else(|| RpcDispatchError::invalid_params("tools/call params must be an object"))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcDispatchError::invalid_params("tools/call requires a string name"))?;
    if super::schema::tool_definition(name).is_none() {
        return Err(RpcDispatchError::invalid_params(format!(
            "unknown tool: {name}"
        )));
    }

    // Task 3 intentionally exposes the stable contract but does not call the
    // application control service. Task 4 will replace this result with the
    // typed AppControlService operation while preserving ToolResult semantics.
    let error = ToolError::new(
        ToolErrorCode::TransportError,
        "tool execution is not wired in Task 3",
    );
    serde_json::to_value(ToolResult::error(&error)).map_err(|error| {
        RpcDispatchError::internal(format!("failed to serialize tool result: {error}"))
    })
}

fn parse_cursor(cursor: &str) -> Result<usize, RpcDispatchError> {
    cursor
        .parse::<usize>()
        .map_err(|_| RpcDispatchError::invalid_params("invalid tools/list cursor"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcDispatchError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcDispatchError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }
}
