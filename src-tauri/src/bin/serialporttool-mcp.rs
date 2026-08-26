//! stdio MCP proxy for a running SerialPortTool GUI.
//!
//! stdout is reserved for JSON-RPC responses. Diagnostics, startup failures,
//! and transport failures are written to stderr. The proxy does not implement
//! tools; it forwards modern MCP requests over the GUI's local IPC endpoint.

use serde_json::{json, Value};
use serialporttool_lib::control::local_ipc::{default_endpoint, LocalIpcClient};
use serialporttool_lib::mcp::{MAX_FRAME_LENGTH, MCP_PROTOCOL_VERSION};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const CONNECT_TIMEOUT_ENV: &str = "SERIALPORTTOOL_MCP_CONNECT_TIMEOUT_MS";
const GUI_COMMAND_ENV: &str = "SERIALPORTTOOL_MCP_GUI_COMMAND";

struct Config {
    socket: std::path::PathBuf,
    start_gui: Option<std::path::PathBuf>,
    connect_timeout: Duration,
}

struct StartedGui(Option<Child>);

impl Drop for StartedGui {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn main() {
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("serialporttool-mcp: {error}");
            std::process::exit(2);
        }
    };
    let _started_gui = match start_gui_if_explicit(&config) {
        Ok(child) => StartedGui(child),
        Err(error) => {
            eprintln!("serialporttool-mcp: {error}");
            std::process::exit(2);
        }
    };
    let client = LocalIpcClient::new(config.socket.clone())
        .with_timeouts(config.connect_timeout, Duration::from_secs(130));
    if config.start_gui.is_some() {
        if let Err(error) = client.wait_until_available() {
            eprintln!("serialporttool-mcp: configured GUI did not expose local IPC: {error}");
            std::process::exit(1);
        }
    }

    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Err(error) = process_line(&line, &client, &mut output) {
                    eprintln!("serialporttool-mcp: {error}");
                    let _ = output.flush();
                }
            }
            Err(error) => {
                eprintln!("serialporttool-mcp: stdin read failed: {error}");
                break;
            }
        }
    }
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut socket = default_endpoint();
        let mut start_gui = None;
        let mut connect_timeout = Duration::from_secs(3);
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--socket" => {
                    index += 1;
                    socket = std::path::PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--socket requires a path".to_string())?,
                    );
                }
                "--start-gui" => {
                    index += 1;
                    start_gui = Some(std::path::PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--start-gui requires an executable path".to_string())?,
                    ));
                }
                "--help" | "-h" => {
                    eprintln!(
                        "serialporttool-mcp [--socket PATH] [--start-gui EXECUTABLE]\n\n\
                         Without --start-gui the proxy never launches a GUI.\n\
                         The SERIALPORTTOOL_MCP_GUI_COMMAND environment variable is an\n\
                         explicit equivalent of --start-gui."
                    );
                    std::process::exit(0);
                }
                value => return Err(format!("unknown argument: {value}")),
            }
            index += 1;
        }
        if start_gui.is_none() {
            start_gui = std::env::var_os(GUI_COMMAND_ENV).map(std::path::PathBuf::from);
        }
        if let Ok(value) = std::env::var(CONNECT_TIMEOUT_ENV) {
            let millis = value
                .parse::<u64>()
                .map_err(|_| format!("{CONNECT_TIMEOUT_ENV} must be an integer in milliseconds"))?;
            if millis == 0 {
                return Err(format!("{CONNECT_TIMEOUT_ENV} must be greater than zero"));
            }
            connect_timeout = Duration::from_millis(millis);
        }
        Ok(Self {
            socket,
            start_gui,
            connect_timeout,
        })
    }
}

fn start_gui_if_explicit(config: &Config) -> Result<Option<Child>, String> {
    let Some(executable) = config.start_gui.as_ref() else {
        return Ok(None);
    };
    eprintln!("serialporttool-mcp: explicitly starting configured GUI executable");
    Command::new(executable)
        .env("SERIALPORTTOOL_MCP_SOCKET", &config.socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map(Some)
        .map_err(|error| format!("failed to start configured GUI: {error}"))
}

fn process_line(
    line: &[u8],
    client: &LocalIpcClient,
    output: &mut impl Write,
) -> Result<(), String> {
    let trimmed = line.strip_suffix(b"\n").unwrap_or(line);
    let trimmed = trimmed.strip_suffix(b"\r").unwrap_or(trimmed);
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.len() > MAX_FRAME_LENGTH {
        return write_error(
            output,
            None,
            -32600,
            "stdio JSON-RPC request exceeds the size limit",
        );
    }
    let request: Value = match serde_json::from_slice(trimmed) {
        Ok(request) => request,
        Err(error) => {
            return write_error(output, None, -32700, &format!("invalid JSON: {error}"));
        }
    };
    let Some(object) = request.as_object() else {
        return write_error(output, None, -32600, "JSON-RPC request must be an object");
    };
    let id = object.get("id").cloned();
    if let Err(error) = validate_modern_request(object) {
        if object.contains_key("id") {
            return write_error(output, id, error.0, &error.1);
        }
        return Ok(());
    }
    match client.forward(&request) {
        Ok(Some(response)) => {
            serde_json::to_writer(&mut *output, &response)
                .map_err(|error| format!("stdout JSON-RPC write failed: {error}"))?;
            output
                .write_all(b"\n")
                .map_err(|error| format!("stdout JSON-RPC newline failed: {error}"))?;
            output
                .flush()
                .map_err(|error| format!("stdout JSON-RPC flush failed: {error}"))?;
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("serialporttool-mcp: GUI IPC request failed: {error}");
            if object.contains_key("id") {
                write_error(output, id, -32001, "SerialPortTool GUI is unavailable")?;
            }
        }
    }
    Ok(())
}

fn validate_modern_request(object: &serde_json::Map<String, Value>) -> Result<(), (i32, String)> {
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or((-32600, "JSON-RPC request requires a method".into()))?;
    if !matches!(method, "server/discover" | "tools/list" | "tools/call") {
        return Err((
            -32601,
            format!("method not supported by stdio proxy: {method}"),
        ));
    }
    let metadata = object
        .get("_meta")
        .ok_or((-32602, "MCP request requires per-request _meta".into()))?;
    let metadata = metadata
        .as_object()
        .ok_or((-32602, "MCP request _meta must be an object".into()))?;
    if metadata.get("io.modelcontextprotocol/protocolVersion")
        != Some(&Value::String(MCP_PROTOCOL_VERSION.into()))
    {
        return Err((
            -32022,
            format!("unsupported MCP protocol version; use {MCP_PROTOCOL_VERSION}"),
        ));
    }
    if !metadata
        .get("io.modelcontextprotocol/clientInfo")
        .is_some_and(Value::is_object)
    {
        return Err((-32602, "MCP _meta is missing clientInfo".into()));
    }
    if !metadata
        .get("io.modelcontextprotocol/clientCapabilities")
        .is_some_and(Value::is_object)
    {
        return Err((-32602, "MCP _meta is missing clientCapabilities".into()));
    }
    Ok(())
}

fn write_error(
    output: &mut impl Write,
    id: Option<Value>,
    code: i32,
    message: &str,
) -> Result<(), String> {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    });
    serde_json::to_writer(&mut *output, &response)
        .map_err(|error| format!("stdout JSON-RPC error write failed: {error}"))?;
    output
        .write_all(b"\n")
        .map_err(|error| format!("stdout JSON-RPC error newline failed: {error}"))?;
    output
        .flush()
        .map_err(|error| format!("stdout JSON-RPC error flush failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn metadata_validation_does_not_rewrite_request() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": MCP_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientInfo": {"name": "serialporttool-mcp-stdio", "version": "test"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        });
        validate_modern_request(request.as_object().unwrap()).unwrap();
        assert_eq!(
            request["_meta"]["io.modelcontextprotocol/clientInfo"]["name"],
            "serialporttool-mcp-stdio"
        );
    }

    #[test]
    fn stdout_writer_contains_only_json_lines() {
        let mut output = Cursor::new(Vec::new());
        write_error(&mut output, Some(json!(7)), -32001, "unavailable").unwrap();
        let bytes = output.into_inner();
        let line = bytes.strip_suffix(b"\n").unwrap();
        let _: Value = serde_json::from_slice(line).unwrap();
    }
}
