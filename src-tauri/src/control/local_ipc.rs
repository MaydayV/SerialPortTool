//! Local IPC used by the stdio MCP adapter and the running GUI.
//!
//! The wire format is deliberately transport-only: one big-endian u32 length
//! followed by one JSON-RPC value. The adapter forwards the value unchanged so
//! request metadata remains per-request metadata owned by the MCP client.

use crate::mcp::{
    dispatch_with_context, ToolControlContext, MAX_FRAME_LENGTH as MCP_MAX_FRAME_LENGTH,
};
use serde_json::{json, Value};
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const IPC_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const IPC_IO_TIMEOUT: Duration = Duration::from_secs(130);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const IPC_FRAME_READ_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_ENV: &str = "SERIALPORTTOOL_MCP_SOCKET";

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    TooLarge { length: usize, maximum: usize },
    UnexpectedEof,
    InvalidLength,
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "IPC framing I/O error: {error}"),
            Self::TooLarge { length, maximum } => {
                write!(
                    formatter,
                    "IPC frame is {length} bytes; maximum is {maximum}"
                )
            }
            Self::UnexpectedEof => formatter.write_str("IPC frame ended before its payload"),
            Self::InvalidLength => formatter.write_str("IPC frame length is invalid"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Write exactly one bounded length-prefixed frame.
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), FrameError> {
    if payload.is_empty() {
        return Err(FrameError::InvalidLength);
    }
    if payload.len() > MCP_MAX_FRAME_LENGTH {
        return Err(FrameError::TooLarge {
            length: payload.len(),
            maximum: MCP_MAX_FRAME_LENGTH,
        });
    }
    let length = u32::try_from(payload.len()).map_err(|_| FrameError::InvalidLength)?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

/// Read exactly one bounded length-prefixed frame.
///
/// `Ok(None)` means clean EOF before a new frame. A partial header or payload
/// is an error and is never silently treated as a completed request.
pub fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, FrameError> {
    let mut header = [0_u8; 4];
    let mut read = 0;
    while read < header.len() {
        match reader.read(&mut header[read..])? {
            0 if read == 0 => return Ok(None),
            0 => return Err(FrameError::UnexpectedEof),
            count => read += count,
        }
    }
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 {
        return Err(FrameError::InvalidLength);
    }
    if length > MCP_MAX_FRAME_LENGTH {
        return Err(FrameError::TooLarge {
            length,
            maximum: MCP_MAX_FRAME_LENGTH,
        });
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

#[cfg(unix)]
fn read_frame_with_timeout<R: Read>(
    reader: &mut R,
    timeout: Duration,
) -> Result<Option<Vec<u8>>, FrameError> {
    let deadline = Instant::now() + timeout;
    let mut header = [0_u8; 4];
    let mut read = 0;
    while read < header.len() {
        match reader.read(&mut header[read..]) {
            Ok(0) if read == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::UnexpectedEof),
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(FrameError::Io(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "IPC frame header read timed out",
                    )));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(FrameError::Io(error)),
        }
    }
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 {
        return Err(FrameError::InvalidLength);
    }
    if length > MCP_MAX_FRAME_LENGTH {
        return Err(FrameError::TooLarge {
            length,
            maximum: MCP_MAX_FRAME_LENGTH,
        });
    }
    let mut payload = vec![0_u8; length];
    let mut read = 0;
    while read < payload.len() {
        match reader.read(&mut payload[read..]) {
            Ok(0) => return Err(FrameError::UnexpectedEof),
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(FrameError::Io(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "IPC frame payload read timed out",
                    )));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(FrameError::Io(error)),
        }
    }
    Ok(Some(payload))
}

#[derive(Debug)]
pub enum LocalIpcError {
    Unsupported,
    Frame(FrameError),
    Json(serde_json::Error),
    Io(io::Error),
    Timeout,
    Protocol(String),
}

impl fmt::Display for LocalIpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str("local IPC is not implemented on Windows yet"),
            Self::Frame(error) => error.fmt(formatter),
            Self::Json(error) => write!(formatter, "invalid local IPC JSON: {error}"),
            Self::Io(error) => write!(formatter, "local IPC I/O error: {error}"),
            Self::Timeout => formatter.write_str("local IPC connection timed out"),
            Self::Protocol(message) => write!(formatter, "local IPC protocol error: {message}"),
        }
    }
}

impl std::error::Error for LocalIpcError {}

impl From<FrameError> for LocalIpcError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<serde_json::Error> for LocalIpcError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<io::Error> for LocalIpcError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(unix)]
pub fn default_endpoint() -> PathBuf {
    if let Ok(path) = std::env::var(SOCKET_ENV) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    // macOS temp_dir() contains a long per-process path that can exceed the
    // Unix-domain socket limit. Keep the default short; mode 0600 protects it.
    PathBuf::from("/tmp/serialporttool-mcp.sock")
}

#[cfg(not(unix))]
pub fn default_endpoint() -> PathBuf {
    if let Ok(path) = std::env::var(SOCKET_ENV) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    std::env::temp_dir().join("serialporttool-mcp.sock")
}

#[derive(Clone, Debug)]
pub struct LocalIpcClient {
    endpoint: PathBuf,
    connect_timeout: Duration,
    io_timeout: Duration,
}

impl LocalIpcClient {
    pub fn new(endpoint: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.into(),
            connect_timeout: IPC_CONNECT_TIMEOUT,
            io_timeout: IPC_IO_TIMEOUT,
        }
    }

    pub fn with_timeouts(mut self, connect_timeout: Duration, io_timeout: Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self.io_timeout = io_timeout;
        self
    }

    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    /// Wait for a GUI endpoint without sending an MCP request.
    #[cfg(unix)]
    pub fn wait_until_available(&self) -> Result<(), LocalIpcError> {
        let deadline = Instant::now() + self.connect_timeout;
        loop {
            match platform::connect(&self.endpoint, self.io_timeout) {
                Ok(stream) => {
                    drop(stream);
                    return Ok(());
                }
                Err(error) if Instant::now() < deadline => {
                    let _ = error;
                    thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    #[cfg(not(unix))]
    pub fn wait_until_available(&self) -> Result<(), LocalIpcError> {
        let _ = self;
        Err(LocalIpcError::Unsupported)
    }

    /// Forward one JSON-RPC request unchanged. Notifications return `None`.
    #[cfg(unix)]
    pub fn forward(&self, request: &Value) -> Result<Option<Value>, LocalIpcError> {
        let payload = serde_json::to_vec(request)?;
        let mut stream = platform::connect(&self.endpoint, self.io_timeout)?;
        write_frame(&mut stream, &payload)?;
        if !request
            .as_object()
            .is_some_and(|object| object.contains_key("id"))
        {
            return Ok(None);
        }
        let response = read_frame(&mut stream)?.ok_or_else(|| {
            LocalIpcError::Protocol("GUI closed IPC connection without a response".into())
        })?;
        Ok(Some(serde_json::from_slice(&response)?))
    }

    #[cfg(not(unix))]
    pub fn forward(&self, _request: &Value) -> Result<Option<Value>, LocalIpcError> {
        Err(LocalIpcError::Unsupported)
    }
}

pub struct LocalIpcServerHandle {
    endpoint: PathBuf,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
    control: Arc<dyn ToolControlContext>,
}

impl LocalIpcServerHandle {
    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        self.control.cancel_pending();
        let join = self
            .join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(join) = join {
            if join.thread().id() != thread::current().id() {
                let _ = join.join();
            }
        }
    }
}

impl Drop for LocalIpcServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct LocalIpcServer;

impl LocalIpcServer {
    pub fn start(
        endpoint: impl Into<PathBuf>,
        control: Arc<dyn ToolControlContext>,
    ) -> Result<LocalIpcServerHandle, LocalIpcError> {
        let endpoint = endpoint.into();
        platform::start_server(endpoint, control)
    }
}

fn handle_request(request: Value, control: &dyn ToolControlContext) -> Option<Value> {
    let object = match request.as_object() {
        Some(object) => object,
        None => {
            return Some(json_rpc_error(
                None,
                -32600,
                "MCP request must be an object",
            ))
        }
    };
    let id = object.get("id").cloned();
    if !object.contains_key("id") {
        let _ = dispatch_with_context(
            object
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            object.get("params"),
            Some(control),
        );
        return None;
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Some(json_rpc_error(
            id,
            -32600,
            "JSON-RPC request requires a method",
        ));
    };
    match dispatch_with_context(method, object.get("params"), Some(control)) {
        Ok(result) => Some(json!({"jsonrpc": "2.0", "id": id, "result": result})),
        Err(error) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": error.code,
                "message": error.message,
                "data": error.data,
            }
        })),
    }
}

fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};

    pub fn connect(endpoint: &Path, io_timeout: Duration) -> io::Result<UnixStream> {
        let stream = UnixStream::connect(endpoint)?;
        stream.set_read_timeout(Some(io_timeout))?;
        stream.set_write_timeout(Some(io_timeout))?;
        Ok(stream)
    }

    pub fn start_server(
        endpoint: PathBuf,
        control: Arc<dyn ToolControlContext>,
    ) -> Result<LocalIpcServerHandle, LocalIpcError> {
        if endpoint.as_os_str().is_empty() {
            return Err(LocalIpcError::Protocol("IPC endpoint is empty".into()));
        }
        if endpoint.as_os_str().len() > 100 {
            return Err(LocalIpcError::Protocol(
                "Unix socket path is too long; set SERIALPORTTOOL_MCP_SOCKET to a shorter path"
                    .into(),
            ));
        }
        if let Some(parent) = endpoint.parent() {
            fs::create_dir_all(parent)?;
        }
        if endpoint.exists() {
            match UnixStream::connect(&endpoint) {
                Ok(_) => {
                    return Err(LocalIpcError::Io(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "another SerialPortTool GUI already owns the IPC endpoint",
                    )))
                }
                Err(_) => fs::remove_file(&endpoint)?,
            }
        }
        let listener = UnixListener::bind(&endpoint)?;
        fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_control = control.clone();
        let thread_endpoint = endpoint.clone();
        let join = thread::Builder::new()
            .name("serialporttool-local-mcp".into())
            .spawn(move || {
                let mut workers = Vec::new();
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = stream.set_read_timeout(Some(IPC_IO_TIMEOUT));
                            let _ = stream.set_write_timeout(Some(IPC_IO_TIMEOUT));
                            let worker_control = thread_control.clone();
                            workers.push(thread::spawn(move || {
                                let mut stream = stream;
                                let mut read_stream = match stream.try_clone() {
                                    Ok(stream) => stream,
                                    Err(error) => {
                                        eprintln!(
                                            "serialporttool-mcp IPC stream clone failed: {error}"
                                        );
                                        return;
                                    }
                                };
                                let _ = read_stream.set_nonblocking(true);
                                match read_frame_with_timeout(
                                    &mut read_stream,
                                    IPC_FRAME_READ_TIMEOUT,
                                ) {
                                    Ok(Some(payload)) => {
                                        match serde_json::from_slice::<Value>(&payload) {
                                            Ok(request) => {
                                                if let Some(response) =
                                                    handle_request(request, worker_control.as_ref())
                                                {
                                                    if let Ok(payload) =
                                                        serde_json::to_vec(&response)
                                                    {
                                                        let _ = write_frame(&mut stream, &payload);
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                let response = json_rpc_error(
                                                    None,
                                                    -32700,
                                                    &format!("invalid JSON: {error}"),
                                                );
                                                if let Ok(payload) = serde_json::to_vec(&response) {
                                                    let _ = write_frame(&mut stream, &payload);
                                                }
                                            }
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        eprintln!("serialporttool-mcp IPC frame rejected: {error}")
                                    }
                                }
                            }));
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(ACCEPT_POLL_INTERVAL);
                        }
                        Err(error) => {
                            eprintln!("serialporttool-mcp IPC listener stopped: {error}");
                            break;
                        }
                    }
                }
                thread_control.cancel_pending();
                for worker in workers {
                    let _ = worker.join();
                }
                let _ = fs::remove_file(thread_endpoint);
            })?;
        Ok(LocalIpcServerHandle {
            endpoint,
            stop,
            join: Mutex::new(Some(join)),
            control,
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::*;

    pub fn connect(_endpoint: &Path, _io_timeout: Duration) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows Named Pipe local IPC is not implemented yet",
        ))
    }

    pub fn start_server(
        _endpoint: PathBuf,
        _control: Arc<dyn ToolControlContext>,
    ) -> Result<LocalIpcServerHandle, LocalIpcError> {
        Err(LocalIpcError::Unsupported)
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;

    pub fn connect(_endpoint: &Path, _io_timeout: Duration) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "local IPC is not implemented on this platform",
        ))
    }

    pub fn start_server(
        _endpoint: PathBuf,
        _control: Arc<dyn ToolControlContext>,
    ) -> Result<LocalIpcServerHandle, LocalIpcError> {
        Err(LocalIpcError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_round_trip_and_eof_are_unambiguous() {
        let mut encoded = Vec::new();
        write_frame(&mut encoded, br#"{"jsonrpc":"2.0"}"#).unwrap();
        let mut cursor = Cursor::new(encoded);
        assert_eq!(
            read_frame(&mut cursor).unwrap(),
            Some(br#"{"jsonrpc":"2.0"}"#.to_vec())
        );
        assert_eq!(read_frame(&mut cursor).unwrap(), None);
    }

    #[test]
    fn frame_bounds_reject_oversized_and_partial_payloads() {
        let oversized = vec![0_u8; MCP_MAX_FRAME_LENGTH + 1];
        assert!(matches!(
            write_frame(&mut Vec::new(), &oversized),
            Err(FrameError::TooLarge { .. })
        ));
        let mut partial = Cursor::new(vec![0, 0, 0, 4, b'a']);
        assert!(matches!(
            read_frame(&mut partial),
            Err(FrameError::Io(_)) | Err(FrameError::UnexpectedEof)
        ));
    }
}
