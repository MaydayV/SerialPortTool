use super::schema::{
    ActionResult, ClearReceivedRequest, ConfigureConnectionRequest, ConnectRequest, ConnectionKind,
    DisconnectRequest, ReadReceivedRequest, SelectProtocolRequest, SendDataRequest, ToolError,
    ToolErrorCode, ToolResult, WaitForDataRequest, MAX_READ_BYTES,
};
use crate::conn::{ConnConfig, SerialConfig, TcpUdpConfig};
use crate::control::events::{McpActivityEvent, McpActivityStage};
use crate::control::{AppControlService, ControlActionOrigin as ActionOrigin};
use crate::list_ports_info;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};

pub trait ToolControlContext: Send + Sync {
    fn call(&self, name: &str, arguments: Option<&Value>) -> ToolResult;

    fn cancel_pending(&self) {}
}

pub struct AppToolControlContext<R: Runtime> {
    service: std::sync::Arc<AppControlService>,
    app: AppHandle<R>,
}

impl<R: Runtime> AppToolControlContext<R> {
    pub fn new(service: std::sync::Arc<AppControlService>, app: AppHandle<R>) -> Self {
        Self { service, app }
    }
}

impl<R: Runtime> ToolControlContext for AppToolControlContext<R> {
    fn call(&self, name: &str, arguments: Option<&Value>) -> ToolResult {
        self.emit_activity(McpActivityStage::Connected, name, "MCP 客户端已连接", None);
        self.emit_activity(McpActivityStage::Started, name, "MCP 调用处理中", None);
        let result = call_tool(&self.service, &self.app, name, arguments);
        let action_id = result
            .structured_content
            .get("action_id")
            .and_then(Value::as_str)
            .or_else(|| {
                result
                    .structured_content
                    .get("error")
                    .and_then(|error| error.get("action_id"))
                    .and_then(Value::as_str)
            })
            .map(str::to_string);
        self.emit_activity(
            if result.is_error {
                McpActivityStage::Failed
            } else {
                McpActivityStage::Finished
            },
            name,
            if result.is_error {
                "MCP 调用失败"
            } else {
                "MCP 调用完成"
            },
            action_id,
        );
        result
    }

    fn cancel_pending(&self) {
        self.service.cancel_pending_approvals();
    }
}

impl<R: Runtime> AppToolControlContext<R> {
    fn emit_activity(
        &self,
        stage: McpActivityStage,
        operation: &str,
        summary: &str,
        action_id: Option<String>,
    ) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let _ = self.app.emit(
            "mcp-activity",
            McpActivityEvent {
                stage,
                operation: operation.to_string(),
                summary: summary.to_string(),
                action_id,
                timestamp_ms,
            },
        );
    }
}

pub fn call_tool<R: Runtime>(
    service: &AppControlService,
    app: &AppHandle<R>,
    name: &str,
    arguments: Option<&Value>,
) -> ToolResult {
    match name {
        "list_ports" => list_ports(),
        "get_state" => get_state(service),
        "read_received" => read_received(service, arguments),
        "wait_for_data" => wait_for_data(service, arguments),
        "get_connection_profiles" => {
            unavailable_bridge("连接收藏仍由 Vue Pinia 持有，MCP bridge 尚未接入；不会伪造成功结果")
        }
        "configure_connection" => configure_connection(service, app, arguments),
        "connect" => connect(service, app, arguments),
        "disconnect" => disconnect(service, app, arguments),
        "send_data" => send_data(service, app, arguments),
        "clear_received" => clear_received(service, app, arguments),
        "select_protocol" => {
            let request = match parse_arguments::<SelectProtocolRequest>(arguments) {
                Ok(request) => request,
                Err(error) => return ToolResult::error(&error),
            };
            if let Err(error) = request.validate() {
                return ToolResult::error(&error);
            }
            unavailable_bridge("协议模板仍由 Vue Pinia 持有，MCP bridge 尚未接入；不会伪造成功结果")
        }
        _ => ToolResult::error(&ToolError::invalid_params(format!("unknown tool: {name}"))),
    }
}

fn list_ports() -> ToolResult {
    match list_ports_info() {
        Ok(ports) => ToolResult::success(&ports, format!("已列出 {} 个串口", ports.len())),
        Err(error) => ToolResult::error(&tool_error_from_string(&error)),
    }
}

fn get_state(service: &AppControlService) -> ToolResult {
    let state = service.state();
    let result = json!({
        "connection": state.connection,
        "rx_buffer_records": state.rx_buffer_records,
        "rx_buffer_bytes": state.rx_buffer_bytes,
        "latest_cursor": service.latest_rx_cursor(),
        "action_events": service.action_events(),
    });
    ToolResult::success(&result, "已读取当前连接和接收状态")
}

fn read_received(service: &AppControlService, arguments: Option<&Value>) -> ToolResult {
    let request = match parse_arguments::<ReadReceivedRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return ToolResult::error(&error),
    };
    if let Err(error) = request.validate() {
        return ToolResult::error(&error);
    }
    match service.read_received(request.cursor, request.limit, MAX_READ_BYTES) {
        Ok(result) => ToolResult::success(
            &result,
            format!("已读取 {} 条接收记录", result.records.len()),
        ),
        Err(error) => ToolResult::error(&read_error(&error)),
    }
}

fn wait_for_data(service: &AppControlService, arguments: Option<&Value>) -> ToolResult {
    let request = match parse_arguments::<WaitForDataRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return ToolResult::error(&error),
    };
    if let Err(error) = request.validate() {
        return ToolResult::error(&error);
    }
    match service.wait_for_data(Duration::from_millis(request.timeout_ms), request.max_bytes) {
        Ok(result) => ToolResult::success(&result, format!("收到 {} 字节数据", result.bytes)),
        Err(error) => ToolResult::error(&wait_error(&error)),
    }
}

fn configure_connection<R: Runtime>(
    service: &AppControlService,
    app: &AppHandle<R>,
    arguments: Option<&Value>,
) -> ToolResult {
    let request = match parse_arguments::<ConfigureConnectionRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return ToolResult::error(&error),
    };
    if let Err(error) = request.validate() {
        return ToolResult::error(&error);
    }
    let config = match conn_config(&request) {
        Ok(config) => config,
        Err(error) => return ToolResult::error(&error),
    };
    match service.configure(config, app.clone(), ActionOrigin::Mcp) {
        Ok(action) => action_result(action, "连接配置已暂存"),
        Err(error) => ToolResult::error(&tool_error_from_string(&error)),
    }
}

fn connect<R: Runtime>(
    service: &AppControlService,
    app: &AppHandle<R>,
    arguments: Option<&Value>,
) -> ToolResult {
    if let Err(error) = parse_arguments::<ConnectRequest>(arguments) {
        return ToolResult::error(&error);
    }
    match service.connect(app.clone(), ActionOrigin::Mcp) {
        Ok(action) => action_result(action, "连接已建立"),
        Err(error) => ToolResult::error(&tool_error_from_string(&error)),
    }
}

fn disconnect<R: Runtime>(
    service: &AppControlService,
    app: &AppHandle<R>,
    arguments: Option<&Value>,
) -> ToolResult {
    if let Err(error) = parse_arguments::<DisconnectRequest>(arguments) {
        return ToolResult::error(&error);
    }
    match service.close(app.clone(), ActionOrigin::Mcp) {
        Ok(action) => action_result(action, "连接已关闭"),
        Err(error) => ToolResult::error(&tool_error_from_string(&error)),
    }
}

fn send_data<R: Runtime>(
    service: &AppControlService,
    app: &AppHandle<R>,
    arguments: Option<&Value>,
) -> ToolResult {
    let request = match parse_arguments::<SendDataRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return ToolResult::error(&error),
    };
    let data = match request.validate_and_decode() {
        Ok(data) => data,
        Err(error) => return ToolResult::error(&error),
    };
    if data.is_empty() {
        return ToolResult::error(&ToolError::invalid_params("decoded data must not be empty"));
    }
    match service.send(data, app.clone(), ActionOrigin::Mcp) {
        Ok(action) => {
            let result = ActionResult {
                action_id: action.action_id,
                summary: action.summary,
                result: json!({ "bytes_sent": action.result }),
            };
            ToolResult::success(&result, "数据发送完成")
        }
        Err(error) => ToolResult::error(&tool_error_from_string(&error)),
    }
}

fn clear_received<R: Runtime>(
    service: &AppControlService,
    app: &AppHandle<R>,
    arguments: Option<&Value>,
) -> ToolResult {
    if let Err(error) = parse_arguments::<ClearReceivedRequest>(arguments) {
        return ToolResult::error(&error);
    }
    match service.clear_received(app.clone(), ActionOrigin::Mcp) {
        Ok(action) => action_result(action, "接收缓冲已清空"),
        Err(error) => ToolResult::error(&tool_error_from_string(&error)),
    }
}

fn action_result<T: Serialize>(action: ActionResult<T>, summary: &str) -> ToolResult {
    let result = ActionResult {
        action_id: action.action_id,
        summary: action.summary,
        result: action.result,
    };
    ToolResult::success(&result, summary)
}

fn parse_arguments<T: DeserializeOwned>(arguments: Option<&Value>) -> Result<T, ToolError> {
    let value = arguments.cloned().unwrap_or_else(|| json!({}));
    if !value.is_object() {
        return Err(ToolError::invalid_params(
            "tool arguments must be an object",
        ));
    }
    serde_json::from_value(value).map_err(|error| ToolError::invalid_params(error.to_string()))
}

fn conn_config(request: &ConfigureConnectionRequest) -> Result<ConnConfig, ToolError> {
    match request.kind {
        ConnectionKind::Serial => Ok(ConnConfig::Serial(SerialConfig {
            port: request.target.trim().to_string(),
            baudrate: request.baud_rate.unwrap_or_default(),
            ..SerialConfig::default()
        })),
        ConnectionKind::Tcp | ConnectionKind::Udp => {
            let port = request.port.unwrap_or_default();
            let target = network_target(request.target.trim(), port);
            Ok(ConnConfig::TcpUdp(TcpUdpConfig {
                protocol: match request.kind {
                    ConnectionKind::Tcp => "tcp",
                    ConnectionKind::Udp => "udp",
                    ConnectionKind::Serial => unreachable!(),
                }
                .into(),
                mode: "client".into(),
                target,
                port,
                ..TcpUdpConfig::default()
            }))
        }
    }
}

fn network_target(target: &str, port: u16) -> String {
    let has_port = target.starts_with('[')
        || target
            .rsplit_once(':')
            .and_then(|(_, value)| value.parse::<u16>().ok())
            .is_some();
    if has_port {
        target.to_string()
    } else {
        format!("{target}:{port}")
    }
}

fn unavailable_bridge(message: &str) -> ToolResult {
    ToolResult::error(
        &ToolError::new(ToolErrorCode::TransportError, message)
            .with_details(json!({ "bridge": "frontend_pinia", "implemented": false })),
    )
}

fn read_error(error: &str) -> ToolError {
    ToolError::new(ToolErrorCode::InvalidParams, error)
}

fn wait_error(error: &str) -> ToolError {
    let code = if error.contains("超时") || error.to_ascii_lowercase().contains("timeout") {
        ToolErrorCode::Timeout
    } else {
        ToolErrorCode::InvalidParams
    };
    ToolError::new(code, error)
}

fn tool_error_from_string(error: &str) -> ToolError {
    let code = if error.contains("未连接")
        || error.contains("未建立")
        || error.contains("未打开")
        || error.contains("未配置")
    {
        ToolErrorCode::NotConnected
    } else if error.contains("权限")
        || error.contains("拒绝")
        || error.to_ascii_lowercase().contains("permission")
    {
        ToolErrorCode::PermissionDenied
    } else if error.contains("超时") || error.to_ascii_lowercase().contains("timeout") {
        ToolErrorCode::Timeout
    } else {
        ToolErrorCode::TransportError
    };
    let mut result = ToolError::new(code, error);
    if let Some(start) = error.find('[') {
        if let Some(end) = error[start + 1..].find(']') {
            result = result.with_action_id(&error[start + 1..start + 1 + end]);
        }
    }
    result
}
