//! MCP contracts for SerialPortTool.
//!
//! Task 1 intentionally stops at reusable tool definitions and validation.  No
//! HTTP server, OAuth authorization implementation, or connection control lives
//! in this module.

pub mod schema;

pub use schema::{
    tool_definition, tool_definitions, ActionResult, ClearReceivedRequest,
    ConfigureConnectionRequest, ConnectRequest, ConnectionKind, DataEncoding, DisconnectRequest,
    EmptyRequest, PermissionMode, ReadReceivedRequest, SelectProtocolRequest, SendDataRequest,
    TextContent, ToolAnnotations, ToolDefinition, ToolError, ToolErrorCode, ToolResult,
    WaitForDataRequest, MAX_BAUD_RATE, MAX_READ_BYTES, MAX_SEND_BYTES, MAX_TARGET_LENGTH,
    MAX_WAIT_BYTES, MAX_WAIT_TIMEOUT_MS,
};
