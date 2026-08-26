//! Serializable MCP tool contracts and validation primitives.
//!
//! This module deliberately contains no connection state or transport code.  It is
//! the stable boundary that an MCP adapter and the application control service
//! can share.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

pub const MAX_SEND_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_READ_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WAIT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WAIT_TIMEOUT_MS: u64 = 120_000;
pub const MAX_TARGET_LENGTH: usize = 4 * 1024;
pub const MAX_BAUD_RATE: u32 = 10_000_000;
pub const MAX_PROTOCOL_TEMPLATES: usize = 100;
pub const MAX_TEMPLATE_NAME_LENGTH: usize = 100;
pub const MAX_TEMPLATE_DESCRIPTION_LENGTH: usize = 1_000;
pub const MAX_TEMPLATE_MARKER_BYTES: usize = 1_024;
pub const MAX_GRAPH_SERIES: usize = 32;
pub const MAX_GRAPH_SERIES_NAME_LENGTH: usize = 128;
pub const MAX_GRAPH_POINTS: usize = 20_000;
pub const MAX_GRAPH_BYTES: usize = 1024 * 1024;
pub const MAX_FRONTEND_BRIDGE_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_FRAME_LENGTH: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Observe,
    #[default]
    Ask,
    Full,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorCode {
    InvalidParams,
    NotConnected,
    Busy,
    ApprovalRequired,
    PermissionDenied,
    Timeout,
    TransportError,
}

impl fmt::Display for ToolErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::InvalidParams => "invalid_params",
            Self::NotConnected => "not_connected",
            Self::Busy => "busy",
            Self::ApprovalRequired => "approval_required",
            Self::PermissionDenied => "permission_denied",
            Self::Timeout => "timeout",
            Self::TransportError => "transport_error",
        };
        f.write_str(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolError {
    pub code: ToolErrorCode,
    pub message: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ToolError {
    pub fn new(code: ToolErrorCode, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code,
            summary: message.clone(),
            message,
            action_id: None,
            details: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::InvalidParams, message)
    }

    pub fn with_action_id(mut self, action_id: impl Into<String>) -> Self {
        self.action_id = Some(action_id.into());
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

impl TextContent {
    pub fn json(text: impl Into<String>) -> Self {
        Self {
            content_type: "text".into(),
            text: text.into(),
        }
    }
}

/// MCP Tool Result.  `content` keeps older clients working while
/// `structured_content` is the canonical machine-readable representation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub content: Vec<TextContent>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Value,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolResult {
    pub fn success<T: Serialize>(structured: &T, summary: impl AsRef<str>) -> Self {
        let structured_content = serde_json::to_value(structured).unwrap_or_else(|_| json!({}));
        Self {
            content: vec![TextContent::json(
                serde_json::to_string(&structured_content).unwrap_or_else(|_| "{}".into()),
            )],
            structured_content,
            is_error: false,
        }
        .with_summary(summary.as_ref())
    }

    pub fn error(error: &ToolError) -> Self {
        let structured_content = json!({ "error": error });
        Self {
            content: vec![TextContent::json(
                serde_json::to_string(&structured_content).unwrap_or_else(|_| "{}".into()),
            )],
            structured_content,
            is_error: true,
        }
    }

    fn with_summary(mut self, summary: &str) -> Self {
        if let Some(text) = self.content.first_mut() {
            text.text = format!("{}\n{}", summary, text.text);
        }
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionResult<T> {
    pub action_id: String,
    pub summary: String,
    pub result: T,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnlyHint")]
    pub read_only_hint: bool,
    #[serde(rename = "destructiveHint")]
    pub destructive_hint: bool,
    #[serde(rename = "idempotentHint")]
    pub idempotent_hint: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(rename = "outputSchema", skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub annotations: ToolAnnotations,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataEncoding {
    Text,
    Hex,
    Escape,
    Base64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendDataRequest {
    pub encoding: DataEncoding,
    pub data: String,
}

impl SendDataRequest {
    pub fn validate_and_decode(&self) -> Result<Vec<u8>, ToolError> {
        let bytes = match self.encoding {
            DataEncoding::Text => self.data.as_bytes().to_vec(),
            DataEncoding::Hex => decode_hex(&self.data)?,
            DataEncoding::Escape => decode_escape(&self.data)?,
            DataEncoding::Base64 => decode_base64(&self.data)?,
        };
        if bytes.len() > MAX_SEND_BYTES {
            return Err(ToolError::invalid_params(format!(
                "decoded data exceeds the {} MiB send limit",
                MAX_SEND_BYTES / (1024 * 1024)
            )));
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadReceivedRequest {
    pub cursor: u64,
    pub limit: usize,
}

impl ReadReceivedRequest {
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.limit == 0 || self.limit > MAX_READ_BYTES {
            return Err(ToolError::invalid_params(format!(
                "limit must be between 1 and {} bytes",
                MAX_READ_BYTES
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WaitForDataRequest {
    pub timeout_ms: u64,
    pub max_bytes: usize,
}

impl WaitForDataRequest {
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.timeout_ms == 0 || self.timeout_ms > MAX_WAIT_TIMEOUT_MS {
            return Err(ToolError::invalid_params(format!(
                "timeout_ms must be between 1 and {}",
                MAX_WAIT_TIMEOUT_MS
            )));
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_WAIT_BYTES {
            return Err(ToolError::invalid_params(format!(
                "max_bytes must be between 1 and {}",
                MAX_WAIT_BYTES
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    Serial,
    Tcp,
    Udp,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConfigureConnectionRequest {
    pub kind: ConnectionKind,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baud_rate: Option<u32>,
}

impl ConfigureConnectionRequest {
    pub fn validate(&self) -> Result<(), ToolError> {
        let target = self.target.trim();
        if target.is_empty() {
            return Err(ToolError::invalid_params("target must not be empty"));
        }
        if target.len() > MAX_TARGET_LENGTH || target.contains('\0') {
            return Err(ToolError::invalid_params(
                "target is too long or contains a NUL byte",
            ));
        }
        match self.kind {
            ConnectionKind::Serial => {
                let baud_rate = self.baud_rate.ok_or_else(|| {
                    ToolError::invalid_params("baud_rate is required for serial connections")
                })?;
                if baud_rate == 0 || baud_rate > MAX_BAUD_RATE {
                    return Err(ToolError::invalid_params(format!(
                        "baud_rate must be between 1 and {}",
                        MAX_BAUD_RATE
                    )));
                }
                if self.port.is_some() {
                    return Err(ToolError::invalid_params(
                        "port is only valid for TCP or UDP connections",
                    ));
                }
            }
            ConnectionKind::Tcp | ConnectionKind::Udp => {
                let port = self.port.ok_or_else(|| {
                    ToolError::invalid_params("port is required for TCP and UDP connections")
                })?;
                if port == 0 {
                    return Err(ToolError::invalid_params(
                        "port must be between 1 and 65535",
                    ));
                }
                if self.baud_rate.is_some() {
                    return Err(ToolError::invalid_params(
                        "baud_rate is only valid for serial connections",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConnectRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DisconnectRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ClearReceivedRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectProtocolRequest {
    pub protocol_id: String,
}

impl SelectProtocolRequest {
    pub fn validate(&self) -> Result<(), ToolError> {
        let id = self.protocol_id.trim();
        if id.is_empty() {
            return Err(ToolError::invalid_params("protocol_id must not be empty"));
        }
        if id.len() > MAX_TEMPLATE_NAME_LENGTH || id.contains('\0') {
            return Err(ToolError::invalid_params(
                "protocol_id is too long or contains a NUL byte",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolLengthField {
    pub enabled: bool,
    pub offset: usize,
    pub bytes: usize,
    pub endian: String,
    #[serde(rename = "includeSelf")]
    pub include_self: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolTemplate {
    pub name: String,
    pub header: String,
    pub tail: String,
    pub length: ProtocolLengthField,
    pub checksum: String,
    #[serde(rename = "checksumRange")]
    pub checksum_range: String,
    #[serde(rename = "checksumPosition")]
    pub checksum_position: String,
    #[serde(rename = "checksumEndian")]
    pub checksum_endian: String,
    pub description: String,
}

impl ProtocolTemplate {
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() || name.len() > MAX_TEMPLATE_NAME_LENGTH || name.contains('\0') {
            return Err("协议模板名称无效".into());
        }
        if self.description.len() > MAX_TEMPLATE_DESCRIPTION_LENGTH {
            return Err("协议模板说明过长".into());
        }
        for marker in [&self.header, &self.tail] {
            let compact: String = marker
                .chars()
                .filter(|c| !c.is_ascii_whitespace())
                .collect();
            if !compact.len().is_multiple_of(2)
                || compact.len() / 2 > MAX_TEMPLATE_MARKER_BYTES
                || !compact.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err("协议模板帧头或帧尾无效".into());
            }
        }
        if ![1, 2, 4].contains(&self.length.bytes)
            || self.length.offset > MAX_FRAME_LENGTH
            || !matches!(self.length.endian.as_str(), "little" | "big")
        {
            return Err("协议模板长度字段无效".into());
        }
        if !matches!(
            self.checksum.as_str(),
            "none" | "crc16_ibm" | "crc16_modbus" | "crc16_ccitt" | "crc32" | "sum8" | "xor8"
        ) || !matches!(self.checksum_range.as_str(), "all" | "payload")
            || !matches!(self.checksum_position.as_str(), "tail" | "before_tail")
            || !matches!(self.checksum_endian.as_str(), "little" | "big")
        {
            return Err("协议模板校验字段无效".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolState {
    pub templates: Vec<ProtocolTemplate>,
    #[serde(rename = "activeName")]
    pub active_name: String,
    #[serde(rename = "rxEnabled")]
    pub rx_enabled: bool,
    #[serde(rename = "txEnabled")]
    pub tx_enabled: bool,
    #[serde(rename = "frameCount")]
    pub frame_count: u64,
    #[serde(rename = "frameErrorCount")]
    pub frame_error_count: u64,
    #[serde(rename = "frameTrashCount")]
    pub frame_trash_count: u64,
    #[serde(rename = "canDecodeActive")]
    pub can_decode_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameStatistics {
    #[serde(rename = "frameCount")]
    pub frame_count: u64,
    #[serde(rename = "frameErrorCount")]
    pub frame_error_count: u64,
    #[serde(rename = "frameTrashCount")]
    pub frame_trash_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphSeriesSummary {
    pub name: String,
    #[serde(rename = "pointCount")]
    pub point_count: usize,
    pub color: String,
    #[serde(rename = "minX", skip_serializing_if = "Option::is_none")]
    pub min_x: Option<f64>,
    #[serde(rename = "maxX", skip_serializing_if = "Option::is_none")]
    pub max_x: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphState {
    pub enabled: bool,
    pub protocol: String,
    #[serde(rename = "headerHex")]
    pub header_hex: String,
    #[serde(rename = "xRange")]
    pub x_range: f64,
    #[serde(rename = "frameCount")]
    pub frame_count: u64,
    pub series: Vec<GraphSeriesSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphDataRequest {
    #[serde(default)]
    pub series: Option<Vec<String>>,
    pub max_points: usize,
    pub max_bytes: usize,
}

impl GraphDataRequest {
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.max_points == 0 || self.max_points > MAX_GRAPH_POINTS {
            return Err(ToolError::invalid_params(format!(
                "max_points must be between 1 and {}",
                MAX_GRAPH_POINTS
            )));
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_GRAPH_BYTES {
            return Err(ToolError::invalid_params(format!(
                "max_bytes must be between 1 and {}",
                MAX_GRAPH_BYTES
            )));
        }
        if let Some(series) = &self.series {
            if series.len() > MAX_GRAPH_SERIES {
                return Err(ToolError::invalid_params(format!(
                    "series may contain at most {} names",
                    MAX_GRAPH_SERIES
                )));
            }
            if series.iter().any(|name| {
                name.trim().is_empty()
                    || name.len() > MAX_GRAPH_SERIES_NAME_LENGTH
                    || name.contains('\0')
            }) {
                return Err(ToolError::invalid_params("series contains an invalid name"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphSeriesData {
    pub name: String,
    pub points: Vec<GraphPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphData {
    pub series: Vec<GraphSeriesData>,
    #[serde(rename = "pointCount")]
    pub point_count: usize,
    #[serde(rename = "byteCount")]
    pub byte_count: usize,
    pub truncated: bool,
    #[serde(rename = "minX", skip_serializing_if = "Option::is_none")]
    pub min_x: Option<f64>,
    #[serde(rename = "maxX", skip_serializing_if = "Option::is_none")]
    pub max_x: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EmptyRequest {}

fn decode_hex(input: &str) -> Result<Vec<u8>, ToolError> {
    let compact: String = input.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if !compact.len().is_multiple_of(2) {
        return Err(ToolError::invalid_params(
            "hex data must contain complete byte pairs",
        ));
    }
    let mut bytes = Vec::with_capacity(compact.len() / 2);
    let chars: Vec<char> = compact.chars().collect();
    for pair in chars.chunks(2) {
        let high = pair[0]
            .to_digit(16)
            .ok_or_else(|| ToolError::invalid_params("hex data contains a non-hex digit"))?;
        let low = pair[1]
            .to_digit(16)
            .ok_or_else(|| ToolError::invalid_params("hex data contains a non-hex digit"))?;
        bytes.push(((high << 4) | low) as u8);
        if bytes.len() > MAX_SEND_BYTES {
            return Err(ToolError::invalid_params(
                "decoded data exceeds the 4 MiB send limit",
            ));
        }
    }
    Ok(bytes)
}

fn decode_escape(input: &str) -> Result<Vec<u8>, ToolError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut encoded = [0u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| ToolError::invalid_params("escape data ends with a backslash"))?;
        match escaped {
            'n' => bytes.push(b'\n'),
            'r' => bytes.push(b'\r'),
            't' => bytes.push(b'\t'),
            '0' => bytes.push(0),
            '\\' => bytes.push(b'\\'),
            'x' => {
                let high = chars
                    .next()
                    .and_then(|c| c.to_digit(16))
                    .ok_or_else(|| ToolError::invalid_params("\\x escape needs two hex digits"))?;
                let low = chars
                    .next()
                    .and_then(|c| c.to_digit(16))
                    .ok_or_else(|| ToolError::invalid_params("\\x escape needs two hex digits"))?;
                bytes.push(((high << 4) | low) as u8);
            }
            _ => {
                return Err(ToolError::invalid_params(
                    "unsupported escape; use \\n, \\r, \\t, \\0, \\\\, or \\xHH",
                ))
            }
        }
        if bytes.len() > MAX_SEND_BYTES {
            return Err(ToolError::invalid_params(
                "decoded data exceeds the 4 MiB send limit",
            ));
        }
    }
    Ok(bytes)
}

fn decode_base64(input: &str) -> Result<Vec<u8>, ToolError> {
    let compact: String = input.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.is_empty() {
        return Ok(Vec::new());
    }
    if !compact.len().is_multiple_of(4) {
        return Err(ToolError::invalid_params(
            "base64 data length must be a multiple of 4",
        ));
    }
    let mut bytes = Vec::with_capacity(compact.len() / 4 * 3);
    let chars: Vec<u8> = compact.bytes().collect();
    for (index, chunk) in chars.chunks(4).enumerate() {
        let final_chunk = index == chars.len() / 4 - 1;
        let values = [
            base64_value(chunk[0])
                .ok_or_else(|| ToolError::invalid_params("invalid base64 data"))?,
            base64_value(chunk[1])
                .ok_or_else(|| ToolError::invalid_params("invalid base64 data"))?,
            base64_value_or_padding(chunk[2])?,
            base64_value_or_padding(chunk[3])?,
        ];
        if (!final_chunk && (chunk[2] == b'=' || chunk[3] == b'='))
            || (chunk[2] == b'=' && chunk[3] != b'=')
            || (chunk[2] == b'=' && values[1] & 0x0f != 0)
            || (chunk[3] == b'=' && chunk[2] != b'=' && values[2] & 0x03 != 0)
        {
            return Err(ToolError::invalid_params("invalid base64 padding"));
        }
        bytes.push((values[0] << 2) | (values[1] >> 4));
        if chunk[2] != b'=' {
            bytes.push((values[1] << 4) | (values[2] >> 2));
        }
        if chunk[3] != b'=' {
            bytes.push((values[2] << 6) | values[3]);
        }
        if bytes.len() > MAX_SEND_BYTES {
            return Err(ToolError::invalid_params(
                "decoded data exceeds the 4 MiB send limit",
            ));
        }
    }
    Ok(bytes)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn base64_value_or_padding(byte: u8) -> Result<u8, ToolError> {
    if byte == b'=' {
        Ok(0)
    } else {
        base64_value(byte).ok_or_else(|| ToolError::invalid_params("invalid base64 data"))
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn empty_schema() -> Value {
    object_schema(json!({}), &[])
}

fn action_output_schema(result: Value) -> Value {
    json!({
        "type": "object",
        "required": ["action_id", "summary", "result"],
        "properties": {
            "action_id": {"type": "string", "minLength": 1},
            "summary": {"type": "string"},
            "result": result
        },
        "additionalProperties": false
    })
}

fn error_schema() -> Value {
    json!({
        "type": "object",
        "required": ["error"],
        "properties": {
            "error": {
                "type": "object",
                "required": ["code", "message", "summary"],
                "properties": {
                    "code": {"type": "string", "enum": [
                        "invalid_params", "not_connected", "busy", "approval_required",
                        "permission_denied", "timeout", "transport_error"
                    ]},
                    "message": {"type": "string"},
                    "summary": {"type": "string"},
                    "action_id": {"type": "string"},
                    "details": {}
                }
            }
        }
    })
}

fn standard_output_schema(result: Value) -> Value {
    json!({
        "oneOf": [result, error_schema()]
    })
}

fn schema_for(name: &str) -> (Value, Value) {
    match name {
        "list_ports" => (
            empty_schema(),
            standard_output_schema(json!({
                "type": "array",
                "items": {"type": "object"}
            })),
        ),
        "get_state" => (
            empty_schema(),
            standard_output_schema(json!({"type": "object"})),
        ),
        "read_received" => (
            object_schema(
                json!({
                    "cursor": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_READ_BYTES}
                }),
                &["cursor", "limit"],
            ),
            standard_output_schema(json!({"type": "object"})),
        ),
        "wait_for_data" => (
            object_schema(
                json!({
                    "timeout_ms": {"type": "integer", "minimum": 1, "maximum": MAX_WAIT_TIMEOUT_MS},
                    "max_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_WAIT_BYTES}
                }),
                &["timeout_ms", "max_bytes"],
            ),
            standard_output_schema(json!({"type": "object"})),
        ),
        "get_connection_profiles" => (
            empty_schema(),
            standard_output_schema(json!({
                "type": "object",
                "required": ["profiles"],
                "properties": {"profiles": {"type": "array", "maxItems": 100, "items": {"type": "object"}}}
            })),
        ),
        "configure_connection" => (
            object_schema(
                json!({
                    "kind": {"type": "string", "enum": ["serial", "tcp", "udp"]},
                    "target": {"type": "string", "minLength": 1, "maxLength": MAX_TARGET_LENGTH},
                    "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                    "baud_rate": {"type": "integer", "minimum": 1, "maximum": MAX_BAUD_RATE}
                }),
                &["kind", "target"],
            ),
            standard_output_schema(action_output_schema(json!({"type": "object"}))),
        ),
        "connect" | "disconnect" | "clear_received" => (
            empty_schema(),
            standard_output_schema(action_output_schema(json!({"type": "object"}))),
        ),
        "send_data" => (
            object_schema(
                json!({
                    "encoding": {"type": "string", "enum": ["text", "hex", "escape", "base64"]},
                    "data": {"type": "string", "maxLength": MAX_SEND_BYTES * 4}
                }),
                &["encoding", "data"],
            ),
            standard_output_schema(action_output_schema(json!({"type": "object"}))),
        ),
        "select_protocol" => (
            object_schema(
                json!({
                    "protocol_id": {"type": "string", "minLength": 1, "maxLength": MAX_TEMPLATE_NAME_LENGTH}
                }),
                &["protocol_id"],
            ),
            standard_output_schema(action_output_schema(json!({"type": "object"}))),
        ),
        "get_protocol_templates" => (
            empty_schema(),
            standard_output_schema(json!({
                "type": "object",
                "required": ["templates"],
                "properties": {"templates": {"type": "array", "maxItems": MAX_PROTOCOL_TEMPLATES}}
            })),
        ),
        "get_protocol_state" => (
            empty_schema(),
            standard_output_schema(json!({"type": "object"})),
        ),
        "get_frame_statistics" => (
            empty_schema(),
            standard_output_schema(json!({"type": "object"})),
        ),
        "get_graph_state" => (
            empty_schema(),
            standard_output_schema(json!({"type": "object"})),
        ),
        "get_graph_data" => (
            object_schema(
                json!({
                    "series": {"type": "array", "maxItems": MAX_GRAPH_SERIES, "items": {"type": "string", "minLength": 1, "maxLength": MAX_GRAPH_SERIES_NAME_LENGTH}},
                    "max_points": {"type": "integer", "minimum": 1, "maximum": MAX_GRAPH_POINTS},
                    "max_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_GRAPH_BYTES}
                }),
                &["max_points", "max_bytes"],
            ),
            standard_output_schema(json!({"type": "object"})),
        ),
        "clear_graph" => (
            empty_schema(),
            standard_output_schema(action_output_schema(json!({"type": "object"}))),
        ),
        _ => (
            empty_schema(),
            standard_output_schema(json!({"type": "object"})),
        ),
    }
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    let entries = [
        (
            "list_ports",
            "List ports",
            "List currently available serial ports and their USB metadata.",
            true,
            false,
            true,
        ),
        (
            "get_state",
            "Get state",
            "Read the current connection, protocol, receive, and AI-control state.",
            true,
            false,
            true,
        ),
        (
            "read_received",
            "Read received data",
            "Read bounded received data from a cursor without changing connection state.",
            true,
            false,
            false,
        ),
        (
            "wait_for_data",
            "Wait for data",
            "Wait for received data with a bounded timeout and byte limit.",
            true,
            false,
            false,
        ),
        (
            "get_connection_profiles",
            "Get connection profiles",
            "List saved connection profiles without changing the active connection.",
            true,
            false,
            true,
        ),
        (
            "configure_connection",
            "Configure connection",
            "Validate and select serial, TCP, or UDP connection parameters.",
            false,
            false,
            true,
        ),
        (
            "connect",
            "Connect",
            "Open the currently configured connection after permission checks.",
            false,
            false,
            false,
        ),
        (
            "disconnect",
            "Disconnect",
            "Close the active connection after permission checks.",
            false,
            false,
            true,
        ),
        (
            "send_data",
            "Send data",
            "Decode and send a bounded text, hex, escape, or base64 payload.",
            false,
            false,
            false,
        ),
        (
            "clear_received",
            "Clear received data",
            "Clear received display and buffer data; this is destructive and requires permission.",
            false,
            true,
            true,
        ),
        (
            "select_protocol",
            "Select protocol",
            "Select an existing protocol template by stable identifier.",
            false,
            false,
            true,
        ),
        (
            "get_protocol_templates",
            "Get protocol templates",
            "Read the authoritative protocol templates from the Vue protocol store.",
            true,
            false,
            true,
        ),
        (
            "get_protocol_state",
            "Get protocol state",
            "Read the active protocol, RX/TX protocol switches, templates, and decode counters.",
            true,
            false,
            true,
        ),
        (
            "get_frame_statistics",
            "Get frame statistics",
            "Read frame, invalid-frame, and discarded-byte counters from the active protocol store.",
            true,
            false,
            true,
        ),
        (
            "get_graph_state",
            "Get graph state",
            "Read waveform parser settings and bounded series summaries from the Vue graph store.",
            true,
            false,
            true,
        ),
        (
            "get_graph_data",
            "Get graph data",
            "Read bounded waveform points with explicit series, point, and byte limits.",
            true,
            false,
            false,
        ),
        (
            "clear_graph",
            "Clear graph",
            "Clear waveform points and parser buffers after permission checks.",
            false,
            true,
            true,
        ),
    ];
    entries
        .into_iter()
        .map(
            |(name, title, description, read_only, destructive, idempotent)| {
                let (input_schema, output_schema) = schema_for(name);
                ToolDefinition {
                    name: name.into(),
                    title: title.into(),
                    description: description.into(),
                    input_schema,
                    output_schema: Some(output_schema),
                    annotations: ToolAnnotations {
                        read_only_hint: read_only,
                        destructive_hint: destructive,
                        idempotent_hint: idempotent,
                    },
                }
            },
        )
        .collect()
}

pub fn tool_definition(name: &str) -> Option<ToolDefinition> {
    tool_definitions()
        .into_iter()
        .find(|tool| tool.name == name)
}
