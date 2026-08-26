//! Streamable HTTP transport for the modern MCP 2026-07-28 profile.
//!
//! There is exactly one POST endpoint (`/mcp`). No session identifier is
//! created, GET/DELETE are rejected, and the endpoint only listens on the
//! loopback interface when started by `McpServer`.

use super::auth::{AuthFailure, LocalBearerAuth};
use super::server::{dispatch, SUPPORTED_PROTOCOL_VERSIONS};
use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::any;
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::net::TcpListener;
use tokio_stream::once;
use tokio_util::sync::CancellationToken;

pub const MCP_PATH: &str = "/mcp";
pub const MAX_REQUEST_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CONCURRENT_REQUESTS: usize = 32;
const LOCAL_ORIGINS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

#[derive(Clone)]
struct ServerState {
    auth: LocalBearerAuth,
    shutdown: CancellationToken,
    active_requests: Arc<AtomicUsize>,
}

pub struct McpServer {
    auth: LocalBearerAuth,
}

pub struct McpServerHandle {
    inner: Arc<ServerInner>,
}

struct ServerInner {
    address: SocketAddr,
    auth: LocalBearerAuth,
    shutdown: CancellationToken,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl McpServer {
    pub fn new() -> Result<Self, getrandom::Error> {
        Ok(Self {
            auth: LocalBearerAuth::new()?,
        })
    }

    pub fn start(self) -> std::io::Result<McpServerHandle> {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let shutdown = CancellationToken::new();
        let state = Arc::new(ServerState {
            auth: self.auth.clone(),
            shutdown: shutdown.clone(),
            active_requests: Arc::new(AtomicUsize::new(0)),
        });
        let router = router(state);
        let server_shutdown = shutdown.clone();
        let join = std::thread::Builder::new()
            .name("serialporttool-mcp".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => return,
                };
                runtime.block_on(async move {
                    let Ok(listener) = TcpListener::from_std(listener) else {
                        return;
                    };
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(server_shutdown.cancelled_owned())
                        .await;
                });
            })?;
        Ok(McpServerHandle {
            inner: Arc::new(ServerInner {
                address,
                auth: self.auth,
                shutdown,
                join: Mutex::new(Some(join)),
            }),
        })
    }
}

impl McpServerHandle {
    pub fn address(&self) -> SocketAddr {
        self.inner.address
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}{}", self.address(), MCP_PATH)
    }

    /// Explicit pairing/export API. Callers must avoid logging this value.
    pub fn local_pairing_token(&self) -> String {
        self.inner.auth.token()
    }

    /// Explicit reset/revocation API for the future UI.
    pub fn reset_local_pairing_token(&self) -> Result<String, getrandom::Error> {
        self.inner.auth.reset()
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        let join = self
            .inner
            .join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(join) = join {
            if join.thread().id() != std::thread::current().id() {
                let _ = join.join();
            }
        }
    }
}

impl Drop for McpServerHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            self.shutdown();
        }
    }
}

pub fn router_for_tests(auth: LocalBearerAuth) -> Router {
    let state = Arc::new(ServerState {
        auth,
        shutdown: CancellationToken::new(),
        active_requests: Arc::new(AtomicUsize::new(0)),
    });
    router(state)
}

fn router(state: Arc<ServerState>) -> Router {
    Router::new()
        .route(MCP_PATH, any(handle_mcp))
        .with_state(state)
}

async fn handle_mcp(State(state): State<Arc<ServerState>>, request: Request<Body>) -> Response {
    if request.method() != Method::POST {
        return response_with_status(
            StatusCode::METHOD_NOT_ALLOWED,
            json!({
                "error": "only POST is supported for the modern Streamable HTTP MCP endpoint"
            }),
        );
    }

    if let Err(status) = validate_origin(request.headers()) {
        return response_with_status(status, json!({"error": "invalid Origin"}));
    }
    if let Err(failure) = state.auth.authorize(request.headers()) {
        let status = match failure {
            AuthFailure::Missing | AuthFailure::Malformed | AuthFailure::Invalid => {
                StatusCode::UNAUTHORIZED
            }
        };
        let mut response = response_with_status(status, json!({"error": "unauthorized"}));
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    }
    let request_cancel = state.shutdown.child_token();
    if request_cancel.is_cancelled() {
        return response_with_status(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "error": "MCP server is shutting down"
            }),
        );
    }
    if state.active_requests.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_REQUESTS {
        state.active_requests.fetch_sub(1, Ordering::AcqRel);
        return response_with_status(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "error": "too many concurrent MCP requests"
            }),
        );
    }
    let _request_guard = ActiveRequestGuard {
        active: state.active_requests.clone(),
    };

    if let Some(length) = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
    {
        if length > MAX_REQUEST_BODY_BYTES {
            return response_with_status(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({
                    "error": "MCP request body exceeds the size limit"
                }),
            );
        }
    }
    if let Some(content_type) = request.headers().get(header::CONTENT_TYPE) {
        let content_type = content_type.to_str().unwrap_or_default();
        if !content_type
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        {
            return response_with_status(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                json!({
                    "error": "MCP requests must use application/json"
                }),
            );
        }
    }
    let headers = request.headers().clone();
    let accept = response_format(&headers);
    let body = match to_bytes(request.into_body(), MAX_REQUEST_BODY_BYTES + 1).await {
        Ok(body) if body.len() <= MAX_REQUEST_BODY_BYTES => body,
        Ok(_) | Err(_) => {
            return response_with_status(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({
                    "error": "MCP request body exceeds the size limit"
                }),
            );
        }
    };
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            return rpc_error_response(None, -32700, format!("invalid JSON: {error}"), None, accept)
        }
    };
    let request = match validate_request(value, &headers) {
        Ok(request) => request,
        Err(error) => {
            return rpc_error_response(error.id, error.code, error.message, error.data, accept)
        }
    };
    let id = request.id.clone();
    if request.notification {
        if dispatch(&request.method, request.params.as_ref()).is_err() {
            // JSON-RPC notifications never receive a response, including errors.
        }
        return Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(Body::empty())
            .unwrap_or_else(|_| Response::new(Body::empty()));
    }

    let result = dispatch(&request.method, request.params.as_ref());
    match result {
        Ok(result) => rpc_success_response(id, result, accept),
        Err(error) => rpc_error_response(id, error.code, error.message, error.data, accept),
    }
}

struct ValidatedRequest {
    id: Option<Value>,
    notification: bool,
    method: String,
    params: Option<Value>,
}

struct ValidationError {
    id: Option<Value>,
    code: i32,
    message: String,
    data: Option<Value>,
}

fn validate_request(
    value: Value,
    headers: &HeaderMap,
) -> Result<ValidatedRequest, ValidationError> {
    let header_version = headers
        .get("MCP-Protocol-Version")
        .and_then(|value| value.to_str().ok());
    let Some(header_version) = header_version else {
        return Err(ValidationError {
            id: value.get("id").cloned(),
            code: -32020,
            message: "MCP-Protocol-Version header is required".into(),
            data: Some(json!({"supportedVersions": SUPPORTED_PROTOCOL_VERSIONS})),
        });
    };
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&header_version) {
        return Err(ValidationError {
            id: value.get("id").cloned(),
            code: -32022,
            message: "UnsupportedProtocolVersion".into(),
            data: Some(json!({
                "supported": SUPPORTED_PROTOCOL_VERSIONS,
            })),
        });
    }
    let object = value.as_object().ok_or_else(|| ValidationError {
        id: None,
        code: -32600,
        message: "MCP request must be a single JSON object".into(),
        data: None,
    })?;
    if object.contains_key("result") || object.contains_key("error") {
        return Err(ValidationError {
            id: object.get("id").cloned(),
            code: -32600,
            message: "MCP POST body must contain a request or notification, not a response".into(),
            data: None,
        });
    }
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(ValidationError {
            id: object.get("id").cloned(),
            code: -32600,
            message: "jsonrpc must be \"2.0\"".into(),
            data: None,
        });
    }
    if let Some(id) = object.get("id") {
        if !(id.is_string() || id.is_number() || id.is_null()) {
            return Err(ValidationError {
                id: None,
                code: -32600,
                message: "JSON-RPC id must be a string, number, or null".into(),
                data: None,
            });
        }
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .filter(|method| !method.is_empty())
        .ok_or_else(|| ValidationError {
            id: object.get("id").cloned(),
            code: -32600,
            message: "JSON-RPC request requires a method".into(),
            data: None,
        })?;
    let params = object.get("params").cloned();
    let metadata = object
        .get("_meta")
        .or_else(|| params.as_ref().and_then(|params| params.get("_meta")))
        .ok_or_else(|| ValidationError {
            id: object.get("id").cloned(),
            code: -32602,
            message: "MCP request requires _meta".into(),
            data: None,
        })?;
    let metadata_version = validate_metadata(metadata, object.get("id").cloned())?;
    if metadata_version != header_version {
        return Err(ValidationError {
            id: object.get("id").cloned(),
            code: -32020,
            message: "MCP-Protocol-Version header does not match request metadata".into(),
            data: Some(json!({"supportedVersions": SUPPORTED_PROTOCOL_VERSIONS})),
        });
    }
    validate_standard_headers(headers, method, params.as_ref(), object.get("id").cloned())?;

    Ok(ValidatedRequest {
        id: object.get("id").cloned(),
        notification: !object.contains_key("id"),
        method: method.to_string(),
        params,
    })
}

fn validate_metadata(metadata: &Value, id: Option<Value>) -> Result<&str, ValidationError> {
    let Some(metadata) = metadata.as_object() else {
        return Err(ValidationError {
            id,
            code: -32602,
            message: "MCP _meta must be an object".into(),
            data: None,
        });
    };
    let protocol_version = metadata
        .get("io.modelcontextprotocol/protocolVersion")
        .and_then(Value::as_str);
    let Some(protocol_version) = protocol_version else {
        return Err(ValidationError {
            id,
            code: -32602,
            message: "_meta is missing io.modelcontextprotocol/protocolVersion".into(),
            data: Some(json!({"supportedVersions": SUPPORTED_PROTOCOL_VERSIONS})),
        });
    };
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version) {
        return Err(ValidationError {
            id,
            code: -32022,
            message: "UnsupportedProtocolVersion".into(),
            data: Some(json!({
                "supported": SUPPORTED_PROTOCOL_VERSIONS,
            })),
        });
    }
    let Some(client_capabilities) = metadata.get("io.modelcontextprotocol/clientCapabilities")
    else {
        return Err(ValidationError {
            id,
            code: -32602,
            message: "_meta is missing io.modelcontextprotocol/clientCapabilities".into(),
            data: None,
        });
    };
    if !client_capabilities.is_object() {
        return Err(ValidationError {
            id,
            code: -32602,
            message: "io.modelcontextprotocol/clientCapabilities must be an object".into(),
            data: None,
        });
    }
    Ok(protocol_version)
}

fn validate_standard_headers(
    headers: &HeaderMap,
    method: &str,
    params: Option<&Value>,
    id: Option<Value>,
) -> Result<(), ValidationError> {
    let method_header = headers
        .get("Mcp-Method")
        .and_then(|value| value.to_str().ok());
    if method_header != Some(method) {
        return Err(ValidationError {
            id,
            code: -32020,
            message: "Mcp-Method header is missing or does not match the request method".into(),
            data: None,
        });
    }

    if matches!(method, "tools/call" | "resources/read" | "prompts/get") {
        let expected_name = params
            .and_then(Value::as_object)
            .and_then(|params| params.get("name").or_else(|| params.get("uri")))
            .and_then(Value::as_str);
        let actual_name = headers
            .get("Mcp-Name")
            .and_then(|value| value.to_str().ok());
        if expected_name.is_none() || actual_name != expected_name {
            return Err(ValidationError {
                id,
                code: -32020,
                message: "Mcp-Name header is missing or does not match the request name".into(),
                data: None,
            });
        }
    }
    Ok(())
}

fn validate_origin(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return Ok(());
    };
    let origin = origin.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
    let uri = origin
        .parse::<axum::http::Uri>()
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if uri.scheme_str() != Some("http")
        || !(uri.path().is_empty() || uri.path() == "/")
        || uri.query().is_some()
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let Some(authority) = uri.authority() else {
        return Err(StatusCode::FORBIDDEN);
    };
    if !LOCAL_ORIGINS.contains(&authority.host()) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ResponseFormat {
    Json,
    Sse,
}

fn response_format(headers: &HeaderMap) -> ResponseFormat {
    let Some(accept) = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
    else {
        return ResponseFormat::Json;
    };
    if accept
        .split(',')
        .any(|part| part.trim().split(';').next() == Some("text/event-stream"))
        && !accept
            .split(',')
            .any(|part| part.trim().split(';').next() == Some("application/json"))
    {
        ResponseFormat::Sse
    } else {
        ResponseFormat::Json
    }
}

fn rpc_success_response(id: Option<Value>, result: Value, format: ResponseFormat) -> Response {
    let payload = json!({"jsonrpc": "2.0", "id": id, "result": result});
    match format {
        ResponseFormat::Json => json_response(StatusCode::OK, payload),
        ResponseFormat::Sse => sse_response(StatusCode::OK, payload),
    }
}

fn rpc_error_response(
    id: Option<Value>,
    code: i32,
    message: String,
    data: Option<Value>,
    format: ResponseFormat,
) -> Response {
    let mut error = Map::new();
    error.insert("code".into(), json!(code));
    error.insert("message".into(), json!(message));
    if let Some(data) = data {
        error.insert("data".into(), data);
    }
    let payload = json!({"jsonrpc": "2.0", "id": id, "error": error});
    let status = match code {
        -32601 => StatusCode::NOT_FOUND,
        -32603 => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    match format {
        ResponseFormat::Json => json_response(status, payload),
        ResponseFormat::Sse => sse_response(status, payload),
    }
}

fn json_response(status: StatusCode, payload: Value) -> Response {
    let mut response = Json(payload).into_response();
    *response.status_mut() = status;
    response
}

fn sse_response(status: StatusCode, payload: Value) -> Response {
    let event = axum::response::sse::Event::default()
        .event("message")
        .data(payload.to_string());
    let mut response = Sse::new(once(Ok::<_, Infallible>(event))).into_response();
    *response.status_mut() = status;
    response
}

fn response_with_status(status: StatusCode, payload: Value) -> Response {
    json_response(status, payload)
}

struct ActiveRequestGuard {
    active: Arc<AtomicUsize>,
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_local_origins_are_allowed() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:5173"),
        );
        assert!(validate_origin(&headers).is_ok());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost.evil"),
        );
        assert_eq!(validate_origin(&headers), Err(StatusCode::FORBIDDEN));
    }

    #[test]
    fn standard_request_headers_must_match_the_json_rpc_body() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            validate_standard_headers(&headers, "tools/list", None, Some(json!(1)))
                .unwrap_err()
                .code,
            -32020
        );

        headers.insert("Mcp-Method", HeaderValue::from_static("tools/list"));
        assert!(validate_standard_headers(&headers, "tools/list", None, Some(json!(1))).is_ok());

        headers.insert("Mcp-Method", HeaderValue::from_static("tools/call"));
        let params = json!({"name": "connect"});
        assert_eq!(
            validate_standard_headers(&headers, "tools/call", Some(&params), Some(json!(2)))
                .unwrap_err()
                .code,
            -32020
        );
        headers.insert("Mcp-Name", HeaderValue::from_static("wrong"));
        assert_eq!(
            validate_standard_headers(&headers, "tools/call", Some(&params), Some(json!(2)))
                .unwrap_err()
                .code,
            -32020
        );
        headers.insert("Mcp-Name", HeaderValue::from_static("connect"));
        assert!(
            validate_standard_headers(&headers, "tools/call", Some(&params), Some(json!(2)))
                .is_ok()
        );
    }
}
