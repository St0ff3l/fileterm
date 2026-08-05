//! Local MCP bridge for FileTerm.
//!
//! Codex and Claude launch the `fileterm mcp` subprocess over stdio. That
//! process has no credentials and only forwards a small, validated read-only
//! request set to the running desktop application over an authenticated
//! loopback socket. SSH/SFTP workers and connection secrets remain inside the
//! desktop process.

use crate::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream as StdTcpStream},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;
use tauri::AppHandle;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    time::timeout,
};

const MCP_RUNTIME_FILE: &str = "mcp-runtime.json";
const MCP_PROTOCOL_VERSION: u32 = 1;
const MCP_JSONRPC_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_BRIDGE_TIMEOUT: Duration = Duration::from_secs(5);
const MCP_MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MCP_MAX_CONCURRENT_CLIENTS: usize = 8;
const MCP_DEFAULT_PAGE_SIZE: usize = 20;
const MCP_MAX_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeDescriptor {
    protocol_version: u32,
    address: String,
    token: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeEnvelope {
    token: String,
    request: BridgeRequest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct BridgeRequest {
    action: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Starts the desktop-owned endpoint and publishes only a per-launch,
/// owner-readable descriptor. The descriptor deliberately contains no
/// connection profile information or credentials.
pub fn start_runtime(app: &AppHandle) -> Result<(), AppError> {
    let path = crate::storage::workspace_file(app, MCP_RUNTIME_FILE)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| AppError::Storage(error.to_string()))?;
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|error| {
        AppError::Command(format!("Unable to start FileTerm MCP bridge: {error}"))
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        AppError::Command(format!("Unable to configure FileTerm MCP bridge: {error}"))
    })?;
    let address = listener.local_addr().map_err(|error| {
        AppError::Command(format!("Unable to inspect FileTerm MCP bridge: {error}"))
    })?;
    let descriptor = RuntimeDescriptor {
        protocol_version: MCP_PROTOCOL_VERSION,
        address: address.to_string(),
        token: format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
    };
    let content = serde_json::to_vec(&descriptor)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&path, &content)?;

    let app_handle = app.clone();
    let limiter = Arc::new(Semaphore::new(MCP_MAX_CONCURRENT_CLIENTS));
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                crate::services::logging::error(
                    &app_handle,
                    "mcp",
                    format!("unable to activate local MCP bridge: {error}"),
                );
                remove_runtime_descriptor(&app_handle);
                return;
            }
        };
        run_runtime_listener(listener, app_handle, descriptor, limiter).await;
    });
    crate::services::logging::info(app, "mcp", "local read-only MCP bridge started");
    Ok(())
}

/// Removes the per-launch authentication descriptor once the desktop process
/// exits. A stale descriptor is harmless (the random listener is gone), but
/// removing it gives CLI clients an immediate and clear "app is not running"
/// result after a normal quit.
pub fn remove_runtime_descriptor(app: &AppHandle) {
    let Ok(path) = crate::storage::workspace_file(app, MCP_RUNTIME_FILE) else {
        return;
    };
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != io::ErrorKind::NotFound {
            crate::services::logging::warn(app, "mcp", "unable to remove local MCP descriptor");
        }
    }
}

async fn run_runtime_listener(
    listener: TcpListener,
    app: AppHandle,
    descriptor: RuntimeDescriptor,
    limiter: Arc<Semaphore>,
) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            break;
        };
        let app = app.clone();
        let descriptor = descriptor.clone();
        let limiter = limiter.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = limiter.try_acquire_owned() else {
                let _ = write_bridge_response(stream, BridgeResponse::busy()).await;
                return;
            };
            if let Err(error) =
                handle_runtime_connection(stream, peer, app.clone(), descriptor).await
            {
                crate::services::logging::warn(
                    &app,
                    "mcp",
                    format!("local MCP request rejected: {error}"),
                );
            }
        });
    }
}

async fn handle_runtime_connection(
    stream: TcpStream,
    peer: SocketAddr,
    app: AppHandle,
    descriptor: RuntimeDescriptor,
) -> Result<(), String> {
    if !peer.ip().is_loopback() {
        return Err("non-loopback MCP client was rejected".to_string());
    }

    let (reader, mut writer) = stream.into_split();
    let mut reader = AsyncBufReader::new(reader);
    let line = read_bridge_line(&mut reader).await?;
    let envelope: BridgeEnvelope = serde_json::from_str(&line)
        .map_err(|_| "invalid FileTerm MCP bridge request".to_string())?;
    if !bool::from(descriptor.token.as_bytes().ct_eq(envelope.token.as_bytes())) {
        write_bridge_response_to_writer(
            &mut writer,
            BridgeResponse::error("FileTerm MCP authentication failed"),
        )
        .await
        .map_err(|error| error.to_string())?;
        return Err("invalid MCP bridge token".to_string());
    }

    let response = match timeout(
        MCP_BRIDGE_TIMEOUT,
        dispatch_bridge_request(&app, envelope.request),
    )
    .await
    {
        Ok(Ok(result)) => BridgeResponse::success(result),
        Ok(Err(error)) => BridgeResponse::error(error),
        Err(_) => BridgeResponse::error(
            "FileTerm MCP request timed out; retry after checking the session",
        ),
    };
    write_bridge_response_to_writer(&mut writer, response)
        .await
        .map_err(|error| error.to_string())
}

async fn read_bridge_line(
    reader: &mut AsyncBufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<String, String> {
    let mut line = String::new();
    let count = timeout(MCP_BRIDGE_TIMEOUT, reader.read_line(&mut line))
        .await
        .map_err(|_| "FileTerm MCP bridge request timed out".to_string())?
        .map_err(|_| "Unable to read FileTerm MCP bridge request".to_string())?;
    if count == 0 {
        return Err("FileTerm MCP bridge client closed without a request".to_string());
    }
    if line.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP bridge request exceeds the size limit".to_string());
    }
    Ok(line)
}

async fn write_bridge_response(stream: TcpStream, response: BridgeResponse) -> io::Result<()> {
    let (_, mut writer) = stream.into_split();
    write_bridge_response_to_writer(&mut writer, response).await
}

async fn write_bridge_response_to_writer(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    response: BridgeResponse,
) -> io::Result<()> {
    let payload =
        serde_json::to_string(&response).map_err(|error| io::Error::other(error.to_string()))?;
    timeout(MCP_BRIDGE_TIMEOUT, async {
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "FileTerm MCP bridge response timed out",
        )
    })?
}

impl BridgeResponse {
    fn success(result: Value) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(message.into()),
        }
    }

    fn busy() -> Self {
        Self::error("FileTerm MCP bridge is busy; retry shortly")
    }
}

async fn dispatch_bridge_request(app: &AppHandle, request: BridgeRequest) -> Result<Value, String> {
    match request.action.as_str() {
        "list_connections" => list_connections(app, &request.params).await,
        "get_session_context" => get_session_context(app, &request.params).await,
        "list_remote_directory" => list_remote_directory(app, &request.params).await,
        _ => Err("Unsupported FileTerm MCP action".to_string()),
    }
}

async fn list_connections(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let library = crate::commands::app_get_connection_library(app.clone())
        .await
        .map_err(public_app_error)?;
    let profiles = library
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned an invalid connection library".to_string())?;
    let total = profiles.len();
    let items = profiles
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_offset = offset + items.len();
    Ok(json!({
        "total": total,
        "count": items.len(),
        "offset": offset,
        "items": items,
        "hasMore": next_offset < total,
        "nextOffset": (next_offset < total).then_some(next_offset),
    }))
}

async fn get_session_context(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let profile_id = optional_string(params, "profile_id", 256)?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let tabs = snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned invalid workspace tabs".to_string())?;
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_object)
        .ok_or_else(|| "FileTerm returned invalid workspace sessions".to_string())?;

    let items = tabs
        .iter()
        .filter(|tab| tab.get("paneRootTabId").is_none())
        .filter(|tab| {
            profile_id.as_deref().is_none_or(|profile_id| {
                tab.get("profileId").and_then(Value::as_str) == Some(profile_id)
            })
        })
        .filter_map(|tab| {
            let tab_id = tab.get("id").and_then(Value::as_str)?;
            let session = sessions.get(tab_id)?;
            Some(json!({
                "tabId": tab_id,
                "profileId": tab.get("profileId"),
                "title": tab.get("title"),
                "sessionType": tab.get("sessionType"),
                "status": tab.get("status"),
                "connected": session.get("connected"),
                "remotePath": session.get("remotePath"),
                "capabilities": session.get("capabilities"),
            }))
        })
        .collect::<Vec<_>>();
    Ok(json!({ "items": items }))
}

async fn list_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = optional_string(params, "path", 4_096)?;
    crate::commands::mcp_list_remote_directory(app.clone(), tab_id, path)
        .await
        .map_err(public_app_error)
}

fn pagination(params: &Value) -> Result<(usize, usize), String> {
    let limit = optional_usize(params, "limit")?.unwrap_or(MCP_DEFAULT_PAGE_SIZE);
    let offset = optional_usize(params, "offset")?.unwrap_or(0);
    if !(1..=MCP_MAX_PAGE_SIZE).contains(&limit) {
        return Err(format!("limit must be between 1 and {MCP_MAX_PAGE_SIZE}"));
    }
    Ok((limit, offset))
}

fn optional_usize(params: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_u64()
        .ok_or_else(|| format!("{key} must be a non-negative integer"))?;
    usize::try_from(value)
        .map(Some)
        .map_err(|_| format!("{key} is too large"))
}

fn required_string(params: &Value, key: &str, maximum: usize) -> Result<String, String> {
    optional_string(params, key, maximum)?.ok_or_else(|| format!("{key} is required"))
}

fn optional_string(params: &Value, key: &str, maximum: usize) -> Result<Option<String>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?
        .trim();
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{key} exceeds the FileTerm MCP limit"));
    }
    Ok(Some(value.to_string()))
}

fn public_app_error(error: AppError) -> String {
    match error {
        AppError::Storage(_) => {
            "FileTerm could not complete the request. Verify the requested session is still open."
                .to_string()
        }
        AppError::Serialization(_) => {
            "FileTerm returned an invalid response. Retry the request.".to_string()
        }
        AppError::Command(message) => message,
        AppError::Clipboard(_) | AppError::Window(_) => {
            "FileTerm could not complete the MCP request.".to_string()
        }
    }
}

/// Entry point for `fileterm mcp`. This is deliberately dependency-free: MCP
/// uses newline-delimited JSON-RPC over stdio while the local desktop bridge
/// uses the authenticated socket above.
pub fn run_stdio(arguments: &[String]) -> Result<(), String> {
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("Usage: fileterm mcp\n\nRun the FileTerm read-only MCP server over stdio. FileTerm must be running.");
        return Ok(());
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("Unable to read MCP input: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_jsonrpc_request(request),
            Err(_) => Some(jsonrpc_error(Value::Null, -32700, "Parse error")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut stdout, &response)
                .map_err(|error| format!("Unable to encode MCP response: {error}"))?;
            stdout
                .write_all(b"\n")
                .map_err(|error| format!("Unable to write MCP response: {error}"))?;
            stdout
                .flush()
                .map_err(|error| format!("Unable to flush MCP response: {error}"))?;
        }
    }
    Ok(())
}

fn handle_jsonrpc_request(request: Value) -> Option<Value> {
    if !request.is_object() {
        return Some(jsonrpc_error(Value::Null, -32600, "Invalid Request"));
    }
    let id = request.get("id").cloned()?;
    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => return Some(jsonrpc_error(id, -32600, "Invalid Request")),
    };
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => initialize_result(&params),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(&params),
        _ => return Some(jsonrpc_error(id, -32601, "Method not found")),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => jsonrpc_error(id, -32602, &error),
    })
}

fn initialize_result(_params: &Value) -> Result<Value, String> {
    Ok(json!({
        "protocolVersion": MCP_JSONRPC_PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "fileterm-mcp-server", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Use FileTerm tools only to inspect connections and already-open remote sessions. They never expose credentials. Remote writes and command execution are intentionally unavailable until FileTerm adds explicit in-app approval."
    }))
}

fn call_tool(params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call requires a tool name".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    validate_tool_arguments(name, &arguments)?;
    let request = match name {
        "fileterm_list_connections" => BridgeRequest {
            action: "list_connections".to_string(),
            params: arguments,
        },
        "fileterm_get_session_context" => BridgeRequest {
            action: "get_session_context".to_string(),
            params: arguments,
        },
        "fileterm_list_remote_directory" => BridgeRequest {
            action: "list_remote_directory".to_string(),
            params: arguments,
        },
        _ => return Err("Unknown FileTerm tool".to_string()),
    };
    match call_desktop_bridge(request) {
        Ok(result) => Ok(tool_result(result, false)),
        Err(error) => Ok(tool_result(json!({ "error": error }), true)),
    }
}

fn validate_tool_arguments(name: &str, arguments: &Value) -> Result<(), String> {
    let allowed: &[&str] = match name {
        "fileterm_list_connections" => &["limit", "offset"],
        "fileterm_get_session_context" => &["profile_id"],
        "fileterm_list_remote_directory" => &["tab_id", "path"],
        _ => return Err("Unknown FileTerm tool".to_string()),
    };
    let object = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be an object".to_string())?;
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{name} does not support the argument {key}"));
    }
    Ok(())
}

fn tool_result(value: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": is_error,
    })
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "fileterm_list_connections",
            "title": "List FileTerm connections",
            "description": "List saved FileTerm connection profiles without credentials. Use this to identify a profile before asking the user to open it in FileTerm.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_PAGE_SIZE, "description": "Maximum profiles to return (default 20)." },
                    "offset": { "type": "integer", "minimum": 0, "description": "Profiles to skip for pagination (default 0)." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "fileterm_get_session_context",
            "title": "Get FileTerm session context",
            "description": "List currently open FileTerm workspace sessions with connection status, current remote path, and capabilities. Terminal transcripts and credentials are never returned.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profile_id": { "type": "string", "description": "Optional saved FileTerm profile ID to filter sessions." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "fileterm_list_remote_directory",
            "title": "List an open FileTerm remote directory",
            "description": "List directory entries through an already-open FileTerm file-capable session. The tool cannot open connections or modify the remote host.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "description": "Open FileTerm workspace tab ID from fileterm_get_session_context." },
                    "path": { "type": "string", "description": "Optional remote directory path; defaults to that session's current remote path." }
                },
                "required": ["tab_id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
        }),
    ]
}

fn jsonrpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn call_desktop_bridge(request: BridgeRequest) -> Result<Value, String> {
    let runtime_path = runtime_descriptor_path()?;
    let descriptor_content = fs::read_to_string(&runtime_path).map_err(|_| {
        "FileTerm desktop app is not running. Open FileTerm, then retry this MCP tool.".to_string()
    })?;
    let descriptor: RuntimeDescriptor = serde_json::from_str(&descriptor_content).map_err(|_| {
        "FileTerm MCP runtime information is invalid. Restart FileTerm, then retry this MCP tool.".to_string()
    })?;
    if descriptor.protocol_version != MCP_PROTOCOL_VERSION || descriptor.token.is_empty() {
        return Err(
            "FileTerm MCP runtime version is unsupported. Restart FileTerm and retry.".to_string(),
        );
    }
    let address: SocketAddr = descriptor.address.parse().map_err(|_| {
        "FileTerm MCP runtime address is invalid. Restart FileTerm, then retry this MCP tool."
            .to_string()
    })?;
    if !address.ip().is_loopback() {
        return Err("FileTerm MCP rejected a non-local runtime address.".to_string());
    }

    let mut stream = StdTcpStream::connect_timeout(&address, MCP_BRIDGE_TIMEOUT).map_err(|_| {
        "FileTerm desktop app is unavailable. Open or restart FileTerm, then retry this MCP tool.".to_string()
    })?;
    stream
        .set_read_timeout(Some(MCP_BRIDGE_TIMEOUT))
        .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    stream
        .set_write_timeout(Some(MCP_BRIDGE_TIMEOUT))
        .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    let envelope = BridgeEnvelope {
        token: descriptor.token,
        request,
    };
    let payload = serde_json::to_vec(&envelope)
        .map_err(|_| "Unable to encode FileTerm MCP request".to_string())?;
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP request exceeds the size limit.".to_string());
    }
    stream
        .write_all(&payload)
        .and_then(|()| stream.write_all(b"\n"))
        .and_then(|()| stream.flush())
        .map_err(|_| {
            "Unable to send the request to FileTerm. Restart FileTerm and retry.".to_string()
        })?;

    let mut response_line = String::new();
    BufReader::new(stream)
        .read_line(&mut response_line)
        .map_err(|_| "FileTerm did not respond to the MCP request. Retry shortly.".to_string())?;
    if response_line.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP response exceeds the size limit.".to_string());
    }
    let response: BridgeResponse = serde_json::from_str(&response_line).map_err(|_| {
        "FileTerm returned an invalid MCP response. Restart FileTerm and retry.".to_string()
    })?;
    if response.ok {
        response
            .result
            .ok_or_else(|| "FileTerm returned an empty MCP response.".to_string())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "FileTerm could not complete the MCP request.".to_string()))
    }
}

fn runtime_descriptor_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("FILETERM_MCP_RUNTIME_FILE") {
        return Ok(PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    let path = {
        let home = env::var_os("HOME")
            .ok_or_else(|| "Unable to locate FileTerm application data.".to_string())?;
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("com.fileterm.desktop")
    };
    #[cfg(target_os = "windows")]
    let path = {
        PathBuf::from(
            env::var_os("APPDATA")
                .ok_or_else(|| "Unable to locate FileTerm application data.".to_string())?,
        )
        .join("com.fileterm.desktop")
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let path = {
        let home = env::var_os("HOME")
            .ok_or_else(|| "Unable to locate FileTerm application data.".to_string())?;
        PathBuf::from(env::var_os("XDG_DATA_HOME").unwrap_or_else(|| {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .into_os_string()
        }))
        .join("com.fileterm.desktop")
    };
    Ok(path.join(MCP_RUNTIME_FILE))
}

#[cfg(test)]
mod tests {
    use super::{
        handle_jsonrpc_request, optional_string, pagination, tool_definitions,
        validate_tool_arguments, MCP_JSONRPC_PROTOCOL_VERSION,
    };
    use serde_json::json;

    #[test]
    fn tools_are_prefixed_read_only_and_have_strict_schemas() {
        for tool in tool_definitions() {
            assert!(tool["name"].as_str().unwrap().starts_with("fileterm_"));
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["annotations"]["destructiveHint"], false);
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
    }

    #[test]
    fn pagination_enforces_a_bounded_positive_limit() {
        assert_eq!(pagination(&json!({})).unwrap(), (20, 0));
        assert!(pagination(&json!({ "limit": 0 })).is_err());
        assert!(pagination(&json!({ "limit": 101 })).is_err());
    }

    #[test]
    fn string_parameters_reject_empty_and_oversized_values() {
        assert!(optional_string(&json!({ "tab_id": "" }), "tab_id", 10).is_err());
        assert!(optional_string(&json!({ "tab_id": "01234567890" }), "tab_id", 10).is_err());
    }

    #[test]
    fn tools_list_is_returned_over_json_rpc() {
        let response = handle_jsonrpc_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        assert_eq!(response["result"]["tools"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn initialize_negotiates_the_supported_protocol_version() {
        let response = handle_jsonrpc_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2099-01-01" }
        }))
        .unwrap();
        assert_eq!(
            response["result"]["protocolVersion"],
            MCP_JSONRPC_PROTOCOL_VERSION
        );
    }

    #[test]
    fn notifications_produce_no_stdio_response() {
        assert!(handle_jsonrpc_request(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .is_none());
    }

    #[test]
    fn tool_arguments_reject_unknown_or_non_object_fields() {
        assert!(
            validate_tool_arguments("fileterm_list_connections", &json!({ "secret": true }))
                .is_err()
        );
        assert!(validate_tool_arguments("fileterm_get_session_context", &json!("bad")).is_err());
    }
}
