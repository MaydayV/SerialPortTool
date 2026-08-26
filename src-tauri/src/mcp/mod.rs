//! SerialPortTool's modern MCP server.
//!
//! Task 3 provides the transport, authentication, lifecycle, discovery, and
//! stable tool contract. Tool execution remains deliberately unwired until
//! Task 4 and therefore cannot control hardware yet.

pub mod auth;
pub mod schema;
pub mod server;
pub mod transport;

pub use schema::{
    tool_definition, tool_definitions, ActionResult, ClearReceivedRequest,
    ConfigureConnectionRequest, ConnectRequest, ConnectionKind, DataEncoding, DisconnectRequest,
    EmptyRequest, PermissionMode, ReadReceivedRequest, SelectProtocolRequest, SendDataRequest,
    TextContent, ToolAnnotations, ToolDefinition, ToolError, ToolErrorCode, ToolResult,
    WaitForDataRequest, MAX_BAUD_RATE, MAX_READ_BYTES, MAX_SEND_BYTES, MAX_TARGET_LENGTH,
    MAX_WAIT_BYTES, MAX_WAIT_TIMEOUT_MS,
};
pub use server::{
    dispatch, RpcDispatchError, MCP_PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
    SUPPORTED_PROTOCOL_VERSIONS,
};
pub use transport::{McpServer, McpServerHandle, MAX_CONCURRENT_REQUESTS, MAX_REQUEST_BODY_BYTES};
