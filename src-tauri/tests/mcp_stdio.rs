#![cfg(unix)]

use serde_json::{json, Value};
use serialporttool_lib::control::local_ipc::{
    read_frame, write_frame, FrameError, LocalIpcClient, LocalIpcServer,
};
use serialporttool_lib::mcp::{
    ToolControlContext, ToolResult, MAX_FRAME_LENGTH, MCP_PROTOCOL_VERSION,
};
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn endpoint(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    PathBuf::from("/tmp").join(format!("spt-{name}-{}-{suffix}.sock", std::process::id()))
}

fn modern_request(id: u64, method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": if method == "tools/call" {
            json!({"name": "list_ports", "arguments": {}})
        } else {
            json!({})
        },
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": MCP_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientInfo": {"name": "metadata-preservation-test", "version": "9"},
            "io.modelcontextprotocol/clientCapabilities": {"custom": {"request": id}}
        }
    })
}

fn spawn_proxy(socket: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_serialporttool-mcp"))
        .arg("--socket")
        .arg(socket)
        .env("SERIALPORTTOOL_MCP_CONNECT_TIMEOUT_MS", "1000")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("stdio proxy should spawn")
}

#[test]
fn stdio_proxy_keeps_stdout_json_only_and_forwards_metadata() {
    let socket = endpoint("stdio");
    let listener = UnixListener::bind(&socket).unwrap();
    let requests = [
        modern_request(1, "server/discover"),
        modern_request(2, "tools/list"),
        modern_request(3, "tools/call"),
    ];
    let mut notification = modern_request(4, "server/discover");
    notification.as_object_mut().unwrap().remove("id");
    let expected = requests
        .iter()
        .cloned()
        .chain(std::iter::once(notification.clone()))
        .collect::<Vec<_>>();
    let worker = std::thread::spawn(move || {
        for (index, expected_request) in expected.into_iter().enumerate() {
            let (mut stream, _) = listener.accept().unwrap();
            let payload = read_frame(&mut stream).unwrap().unwrap();
            let received: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(received, expected_request);
            if index < 3 {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": received["id"],
                    "result": {"method": received["method"]}
                });
                write_frame(&mut stream, &serde_json::to_vec(&response).unwrap()).unwrap();
            }
        }
    });

    let mut proxy = spawn_proxy(&socket);
    {
        let stdin = proxy.stdin.as_mut().unwrap();
        for request in &requests {
            serde_json::to_writer(&mut *stdin, request).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
        // A notification must be forwarded without producing stdout.
        serde_json::to_writer(&mut *stdin, &notification).unwrap();
        stdin.write_all(b"\n").unwrap();
        // Parse errors are JSON-RPC errors, not log output.
        stdin.write_all(b"not-json\n").unwrap();
    }
    let output = proxy.wait_with_output().unwrap();
    worker.join().unwrap();
    let _ = std::fs::remove_file(&socket);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 4);
    for line in &lines {
        let value: Value = serde_json::from_str(line).expect("stdout must contain JSON only");
        assert_eq!(value["jsonrpc"], "2.0");
    }
    assert_eq!(
        serde_json::from_str::<Value>(lines[0]).unwrap()["result"]["method"],
        "server/discover"
    );
}

#[test]
fn framing_bounds_and_eof_are_explicit() {
    let mut encoded = Vec::new();
    write_frame(&mut encoded, br#"{"ok":true}"#).unwrap();
    let mut cursor = std::io::Cursor::new(encoded);
    assert_eq!(
        read_frame(&mut cursor).unwrap(),
        Some(br#"{"ok":true}"#.to_vec())
    );
    assert_eq!(read_frame(&mut cursor).unwrap(), None);

    let oversized = vec![0_u8; MAX_FRAME_LENGTH + 1];
    assert!(matches!(
        write_frame(&mut Vec::new(), &oversized),
        Err(FrameError::TooLarge { .. })
    ));
    let mut partial = std::io::Cursor::new(vec![0, 0, 0, 3, b'{']);
    assert!(matches!(
        read_frame(&mut partial),
        Err(FrameError::Io(_)) | Err(FrameError::UnexpectedEof)
    ));
}

struct NoopControl;

impl ToolControlContext for NoopControl {
    fn call(&self, _name: &str, _arguments: Option<&Value>) -> ToolResult {
        ToolResult::success(&json!({"ok": true}), "test")
    }
}

#[test]
fn gui_ipc_shutdown_removes_unix_socket() {
    let socket = endpoint("shutdown");
    let server = LocalIpcServer::start(&socket, Arc::new(NoopControl)).unwrap();
    let client =
        LocalIpcClient::new(&socket).with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    client.wait_until_available().unwrap();
    assert!(socket.exists());
    server.shutdown();
    assert!(!socket.exists());
}

#[test]
fn gui_ipc_shutdown_is_bounded_with_a_slow_client() {
    let socket = endpoint("slow");
    let server = LocalIpcServer::start(&socket, Arc::new(NoopControl)).unwrap();
    let client = UnixStream::connect(&socket).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let started = std::time::Instant::now();
    server.shutdown();
    assert!(started.elapsed() < Duration::from_secs(6));
    drop(client);
    assert!(!socket.exists());
}
