use serde_json::json;
use serialporttool_lib::control::{AppControlService, ControlActionOrigin};
use serialporttool_lib::mcp::server::dispatch_with_context;
use serialporttool_lib::mcp::tools::{call_tool, AppToolControlContext, ToolControlContext};
use serialporttool_lib::mcp::{McpServer, PermissionMode, ToolErrorCode, MCP_PROTOCOL_VERSION};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn mock_app() -> tauri::AppHandle<tauri::test::MockRuntime> {
    tauri::test::mock_app().handle().clone()
}

fn http_tool_call(
    server: &serialporttool_lib::mcp::McpServerHandle,
    name: &str,
) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": {}},
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": MCP_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientInfo": {"name": "mcp-tools-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    });
    let encoded = serde_json::to_vec(&body).unwrap();
    let address = server.address();
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nMCP-Protocol-Version: {MCP_PROTOCOL_VERSION}\r\nMcp-Method: tools/call\r\nMcp-Name: {name}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        server.local_pairing_token(),
        encoded.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(&encoded).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let body_start = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    serde_json::from_slice(&response[body_start..]).unwrap()
}

#[test]
fn tools_call_uses_shared_control_service_and_mcp_action_origin() {
    let service = Arc::new(AppControlService::new());
    service.set_permission_mode(PermissionMode::Full);
    let app = mock_app();
    let context = AppToolControlContext::new(service.clone(), app.clone());
    let request = json!({
        "name": "configure_connection",
        "arguments": {
            "kind": "tcp",
            "target": "127.0.0.1",
            "port": 2345
        }
    });

    let result = dispatch_with_context(
        "tools/call",
        Some(&request),
        Some(&context as &dyn ToolControlContext),
    )
    .expect("tools/call dispatch failed");

    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["result"]["kind"], "tcp");
    assert_eq!(
        service.snapshot().config.unwrap().endpoint.unwrap(),
        "127.0.0.1:2345"
    );
    assert!(service
        .action_events()
        .iter()
        .any(|event| event.origin == ControlActionOrigin::Mcp));
}

#[test]
fn send_data_returns_tool_error_when_not_connected() {
    let service = AppControlService::new();
    service.set_permission_mode(PermissionMode::Full);
    let app = mock_app();
    let result = call_tool(
        &service,
        &app,
        "send_data",
        Some(&json!({"encoding": "hex", "data": "AA 55"})),
    );

    assert!(result.is_error);
    assert_eq!(result.structured_content["error"]["code"], "not_connected");
    assert!(result.structured_content["error"]["action_id"].is_string());
}

#[test]
fn read_and_clear_use_the_same_bounded_receive_buffer() {
    let service = AppControlService::new();
    service.set_permission_mode(PermissionMode::Full);
    let app = mock_app();
    service
        .rx_buffer()
        .push(b"hello", 1000.0, Some("peer#1".into()));

    let read = call_tool(
        &service,
        &app,
        "read_received",
        Some(&json!({"cursor": 0, "limit": 10})),
    );
    assert!(!read.is_error);
    assert_eq!(
        read.structured_content["records"][0]["data"],
        json!([104, 101, 108, 108, 111])
    );
    assert_eq!(read.structured_content["records"][0]["peer"], "peer#1");

    let clear = call_tool(&service, &app, "clear_received", Some(&json!({})));
    assert!(!clear.is_error);
    assert_eq!(clear.structured_content["result"]["records"], 1);
    assert_eq!(service.rx_buffer().len(), 0);
}

#[test]
fn unsupported_frontend_bridges_fail_explicitly_without_fake_success() {
    let service = AppControlService::new();
    let app = mock_app();
    let result = call_tool(
        &service,
        &app,
        "select_protocol",
        Some(&json!({"protocol_id": "template-1"})),
    );

    assert!(result.is_error);
    assert_eq!(
        result.structured_content["error"]["code"],
        "transport_error"
    );
    assert_eq!(
        result.structured_content["error"]["details"]["implemented"],
        false
    );
    assert_eq!(ToolErrorCode::TransportError.to_string(), "transport_error");
}

#[test]
fn http_tools_call_reaches_the_shared_control_service() {
    let service = Arc::new(AppControlService::new());
    let server = McpServer::new()
        .unwrap()
        .with_control(service.clone(), mock_app())
        .start()
        .unwrap();

    let result = http_tool_call(&server, "get_state");
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(
        result["result"]["structuredContent"]["connection"]["status"],
        "closed"
    );
    assert!(service.action_events().is_empty());
    server.shutdown();
}

#[test]
fn configure_connect_send_and_read_complete_a_real_tcp_loopback() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 64];
        let count = stream.read(&mut buf).unwrap();
        stream.write_all(&buf[..count]).unwrap();
        buf[..count].to_vec()
    });

    let service = Arc::new(AppControlService::new());
    service.set_permission_mode(PermissionMode::Full);
    let app = mock_app();
    let configured = call_tool(
        &service,
        &app,
        "configure_connection",
        Some(&json!({
            "kind": "tcp",
            "target": "127.0.0.1",
            "port": port
        })),
    );
    assert!(!configured.is_error);

    let connected = call_tool(&service, &app, "connect", Some(&json!({})));
    assert!(
        !connected.is_error,
        "connect result: {:?}",
        connected.structured_content
    );

    let sent = call_tool(
        &service,
        &app,
        "send_data",
        Some(&json!({"encoding": "text", "data": "mcp-ping"})),
    );
    assert!(!sent.is_error, "send result: {:?}", sent.structured_content);
    assert_eq!(sent.structured_content["result"]["bytes_sent"], 8);

    let received = loop {
        let result = call_tool(
            &service,
            &app,
            "read_received",
            Some(&json!({"cursor": 0, "limit": 10})),
        );
        let has_records = result.structured_content["records"]
            .as_array()
            .is_some_and(|records| !records.is_empty());
        if !result.is_error && has_records {
            break result;
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        received.structured_content["records"][0]["data"],
        json!([109, 99, 112, 45, 112, 105, 110, 103])
    );

    let disconnected = call_tool(&service, &app, "disconnect", Some(&json!({})));
    assert!(!disconnected.is_error);
    assert_eq!(server.join().unwrap(), b"mcp-ping");
}
