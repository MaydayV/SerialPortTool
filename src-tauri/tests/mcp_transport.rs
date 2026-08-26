use serde_json::{json, Value};
use serialporttool_lib::mcp::{McpServer, MAX_REQUEST_BODY_BYTES, MCP_PROTOCOL_VERSION};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn rpc_request(method: &str, id: u64, _token: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": MCP_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientInfo": {"name": "transport-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    })
}

fn send_rpc(
    endpoint: &str,
    token: Option<&str>,
    body: Value,
    protocol_header: Option<&str>,
    origin: Option<&str>,
    accept: &str,
) -> HttpResponse {
    let address = endpoint
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap()
        .0;
    let encoded = serde_json::to_vec(&body).unwrap();
    let mut extra = String::new();
    if let Some(token) = token {
        extra.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(protocol) = protocol_header {
        extra.push_str(&format!("MCP-Protocol-Version: {protocol}\r\n"));
    }
    if let Some(method) = body.get("method").and_then(Value::as_str) {
        extra.push_str(&format!("Mcp-Method: {method}\r\n"));
        if matches!(method, "tools/call" | "resources/read" | "prompts/get") {
            if let Some(name) = body
                .get("params")
                .and_then(|params| params.get("name").or_else(|| params.get("uri")))
                .and_then(Value::as_str)
            {
                extra.push_str(&format!("Mcp-Name: {name}\r\n"));
            }
        }
    }
    if let Some(origin) = origin {
        extra.push_str(&format!("Origin: {origin}\r\n"));
    }
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nAccept: {accept}\r\nConnection: close\r\nContent-Length: {}\r\n{extra}\r\n",
        encoded.len()
    );
    send_raw(address, request.as_bytes(), &encoded)
}

fn send_method(endpoint: &str, token: &str, method: &str) -> HttpResponse {
    let address = endpoint
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap()
        .0;
    let request = format!(
        "{method} /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    send_raw(address, request.as_bytes(), &[])
}

fn send_raw(address: &str, headers: &[u8], body: &[u8]) -> HttpResponse {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut stream = loop {
        match address
            .to_socket_addrs()
            .unwrap()
            .next()
            .and_then(|addr| TcpStream::connect(addr).ok())
        {
            Some(stream) => break stream,
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            None => panic!("MCP server did not become ready"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let _ = stream.write_all(headers);
    let _ = stream.write_all(body);

    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "server closed before HTTP headers");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = header_text.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect::<Vec<_>>();
    let content_length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok());
    let expected = content_length.map(|length| header_end + length);
    while expected.is_none_or(|expected| bytes.len() < expected) {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => break,
            Err(error) => panic!("read response failed: {error}"),
        }
    }
    HttpResponse {
        status,
        headers,
        body: bytes[header_end..expected.unwrap_or(bytes.len()).min(bytes.len())].to_vec(),
    }
}

fn body_json(response: &HttpResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}

fn header<'a>(response: &'a HttpResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn server_binds_loopback_random_port_and_rotates_token() {
    let server = McpServer::new().unwrap().start().unwrap();
    assert_eq!(server.address().ip().to_string(), "127.0.0.1");
    assert_ne!(server.address().port(), 0);
    let first = server.local_pairing_token();
    let second = server.reset_local_pairing_token().unwrap();
    assert_ne!(first, second);
    server.shutdown();
}

#[test]
fn auth_and_origin_policy_are_enforced() {
    let server = McpServer::new().unwrap().start().unwrap();
    let request = rpc_request("server/discover", 1, "");
    assert_eq!(
        send_rpc(
            &server.endpoint(),
            None,
            request.clone(),
            Some(MCP_PROTOCOL_VERSION),
            None,
            "application/json"
        )
        .status,
        401
    );
    assert_eq!(
        send_rpc(
            &server.endpoint(),
            Some("wrong"),
            request.clone(),
            Some(MCP_PROTOCOL_VERSION),
            None,
            "application/json"
        )
        .status,
        401
    );
    assert_eq!(
        send_rpc(
            &server.endpoint(),
            Some(&server.local_pairing_token()),
            request.clone(),
            Some(MCP_PROTOCOL_VERSION),
            None,
            "application/json"
        )
        .status,
        200
    );
    assert_eq!(
        send_rpc(
            &server.endpoint(),
            Some(&server.local_pairing_token()),
            request.clone(),
            Some(MCP_PROTOCOL_VERSION),
            Some("http://localhost:5173"),
            "application/json"
        )
        .status,
        200
    );
    assert_eq!(
        send_rpc(
            &server.endpoint(),
            Some(&server.local_pairing_token()),
            request,
            Some(MCP_PROTOCOL_VERSION),
            Some("http://evil.example"),
            "application/json"
        )
        .status,
        403
    );
    server.shutdown();
}

#[test]
fn get_and_delete_are_not_legacy_sse_endpoints() {
    let server = McpServer::new().unwrap().start().unwrap();
    let token = server.local_pairing_token();
    assert_eq!(send_method(&server.endpoint(), &token, "GET").status, 405);
    assert_eq!(
        send_method(&server.endpoint(), &token, "DELETE").status,
        405
    );
    server.shutdown();
}

#[test]
fn protocol_header_and_metadata_are_required_and_consistent() {
    let server = McpServer::new().unwrap().start().unwrap();
    let token = server.local_pairing_token();
    let body = rpc_request("server/discover", 2, &token);
    let missing_header = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        body.clone(),
        None,
        None,
        "application/json",
    ));
    assert_eq!(missing_header["error"]["code"], -32020);
    let unsupported = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        body.clone(),
        Some("2025-11-25"),
        None,
        "application/json",
    ));
    assert_eq!(
        unsupported["error"]["message"],
        "UnsupportedProtocolVersion"
    );
    assert_eq!(unsupported["error"]["code"], -32022);
    let mismatched = send_rpc(
        &server.endpoint(),
        Some(&token),
        body,
        Some("2026-01-01"),
        None,
        "application/json",
    );
    assert_eq!(body_json(&mismatched)["error"]["code"], -32022);
    let mut no_meta = rpc_request("server/discover", 3, &token);
    no_meta.as_object_mut().unwrap().remove("_meta");
    let no_meta_response = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        no_meta,
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    assert_eq!(
        no_meta_response["error"]["message"],
        "MCP request requires _meta"
    );
    server.shutdown();
}

#[test]
fn discover_tools_and_pagination_are_stable() {
    let server = McpServer::new().unwrap().start().unwrap();
    let token = server.local_pairing_token();
    let discover = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        rpc_request("server/discover", 4, &token),
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    assert_eq!(
        discover["result"]["supportedVersions"][0],
        MCP_PROTOCOL_VERSION
    );
    assert_eq!(discover["result"]["serverInfo"]["name"], "SerialPortTool");
    assert_eq!(
        discover["result"]["capabilities"]["tools"]["listChanged"],
        false
    );

    let mut first = rpc_request("tools/list", 5, &token);
    first["params"] = json!({});
    let first_result = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        first,
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    assert_eq!(first_result["result"]["tools"].as_array().unwrap().len(), 5);
    let cursor = first_result["result"]["nextCursor"]
        .as_str()
        .unwrap()
        .to_string();
    let mut second = rpc_request("tools/list", 6, &token);
    second["params"] = json!({"cursor": cursor});
    let second_result = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        second,
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    let mut third = rpc_request("tools/list", 7, &token);
    third["params"] = json!({"cursor": second_result["result"]["nextCursor"].as_str().unwrap()});
    let third_result = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        third,
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    let names = first_result["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second_result["result"]["tools"].as_array().unwrap())
        .chain(third_result["result"]["tools"].as_array().unwrap())
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names.len(), 11);
    assert_eq!(names, sorted);
    server.shutdown();
}

#[test]
fn malformed_requests_and_sse_are_bounded_and_distinguished() {
    let server = McpServer::new().unwrap().start().unwrap();
    let token = server.local_pairing_token();
    let endpoint = server.endpoint();
    let address = endpoint
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap()
        .0;
    let invalid = format!("POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nMCP-Protocol-Version: {MCP_PROTOCOL_VERSION}\r\nContent-Type: application/json\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot-json!");
    assert_eq!(send_raw(address, invalid.as_bytes(), &[]).status, 400);
    let response_body = json!({"jsonrpc":"2.0","id":8,"result":{}});
    let response_result = body_json(&send_rpc(
        &endpoint,
        Some(&token),
        response_body,
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    assert_eq!(response_result["error"]["code"], -32600);
    let oversized = "x".repeat(MAX_REQUEST_BODY_BYTES + 1);
    assert_eq!(
        send_rpc(
            &endpoint,
            Some(&token),
            json!({"oversized": oversized}),
            Some(MCP_PROTOCOL_VERSION),
            None,
            "application/json"
        )
        .status,
        413
    );
    let sse = send_rpc(
        &endpoint,
        Some(&token),
        rpc_request("server/discover", 9, &token),
        Some(MCP_PROTOCOL_VERSION),
        None,
        "text/event-stream",
    );
    assert_eq!(sse.status, 200);
    assert!(header(&sse, "content-type")
        .unwrap()
        .starts_with("text/event-stream"));
    server.shutdown();
}

#[test]
fn tool_calls_are_execution_errors_not_json_rpc_protocol_errors() {
    let server = McpServer::new().unwrap().start().unwrap();
    let token = server.local_pairing_token();
    let mut request = rpc_request("tools/call", 10, &token);
    request["params"] = json!({"name":"connect","arguments":{}});
    let value = body_json(&send_rpc(
        &server.endpoint(),
        Some(&token),
        request,
        Some(MCP_PROTOCOL_VERSION),
        None,
        "application/json",
    ));
    assert_eq!(value["result"]["isError"], true);
    assert_eq!(
        value["result"]["structuredContent"]["error"]["code"],
        "transport_error"
    );
    server.shutdown();
}

#[test]
fn shutdown_completes_without_deadlock() {
    let server = McpServer::new().unwrap().start().unwrap();
    let started = Instant::now();
    server.shutdown();
    assert!(started.elapsed() < Duration::from_secs(2));
}
