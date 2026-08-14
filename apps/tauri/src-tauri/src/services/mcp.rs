//! Local MCP bridge for FileTerm.
//!
//! Codex and Claude launch the `fileterm mcp` subprocess over stdio. That
//! process has no credentials and forwards a validated request set to the
//! running desktop application over an authenticated loopback socket. SSH,
//! SFTP workers and connection secrets remain inside the desktop process.

use crate::services::action_review::{
    request_action_approval, ActionApprovalDecision, ActionApprovalDetails, ActionApprovalSource,
    ACTION_APPROVAL_TIMEOUT,
};
use crate::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
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
const MCP_CLIENT_TIMEOUT: Duration = Duration::from_secs(130);
const MCP_TRANSFER_WAIT_TIMEOUT: Duration = Duration::from_secs(125);
const MCP_MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;
const MCP_MAX_CONCURRENT_CLIENTS: usize = 8;
const MCP_DEFAULT_PAGE_SIZE: usize = 20;
const MCP_MAX_PAGE_SIZE: usize = 100;
const MCP_MAX_FILE_CONTENT_BYTES: usize = 512 * 1024;
const MCP_TRANSFER_WAIT_DEFAULT_MS: u64 = 30_000;
const MCP_TRANSFER_WAIT_MAX_MS: u64 = 120_000;
const MCP_TRANSFER_NOT_FOUND: &str = "FILETERM_TRANSFER_NOT_FOUND";
const MCP_POLICY_READ_ONLY: &str = "MCP_POLICY_READ_ONLY";
const MCP_SCOPE_DENIED: &str = "MCP_SCOPE_DENIED";

#[derive(Clone, Debug)]
struct McpAccessPolicy {
    connection_scope: String,
    operation_policy: String,
    default_profile_id: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum McpVisibilityScope {
    AllSavedConnections,
    ActiveSession,
    DefaultConnection,
}

#[derive(Clone, Debug)]
struct McpVisibility {
    scope: McpVisibilityScope,
    profile_ids: HashSet<String>,
    tab_ids: HashSet<String>,
}

impl McpVisibility {
    fn all_saved_connections() -> Self {
        Self {
            scope: McpVisibilityScope::AllSavedConnections,
            profile_ids: HashSet::new(),
            tab_ids: HashSet::new(),
        }
    }

    fn allows_profile(&self, profile_id: Option<&str>) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::ActiveSession | McpVisibilityScope::DefaultConnection => {
                profile_id.is_some_and(|profile_id| self.profile_ids.contains(profile_id))
            }
        }
    }

    fn allows_tab(&self, tab_id: Option<&str>) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::ActiveSession => {
                tab_id.is_some_and(|tab_id| self.tab_ids.contains(tab_id))
            }
            McpVisibilityScope::DefaultConnection => {
                tab_id.is_some_and(|tab_id| self.tab_ids.contains(tab_id))
            }
        }
    }

    fn allows_transfer_value(&self, transfer: &Value) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::ActiveSession => {
                self.allows_tab(transfer.get("tabId").and_then(Value::as_str))
            }
            McpVisibilityScope::DefaultConnection => {
                self.allows_profile(transfer.get("profileId").and_then(Value::as_str))
            }
        }
    }

    fn allows_transfer_task(&self, task: &crate::services::transfers::TransferTask) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::ActiveSession => self.allows_tab(task.tab_id.as_deref()),
            McpVisibilityScope::DefaultConnection => {
                self.allows_profile(task.profile_id.as_deref())
            }
        }
    }
}

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
    #[serde(default)]
    requires_approval: bool,
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
    crate::services::logging::info(app, "mcp", "local MCP bridge started");
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

    let request_timeout = bridge_request_timeout(&envelope.request);
    let response = match timeout(
        request_timeout,
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

fn bridge_request_timeout(request: &BridgeRequest) -> Duration {
    if request.action == "wait_for_transfer" {
        // This is a read-only, bounded observation call. Keep its bridge
        // timeout slightly above the public 120-second wait ceiling so the
        // stdio client receives the final task snapshot instead of a socket
        // timeout at the boundary.
        MCP_TRANSFER_WAIT_TIMEOUT
    } else if request.requires_approval && action_requires_approval(&request.action) {
        ACTION_APPROVAL_TIMEOUT + MCP_BRIDGE_TIMEOUT
    } else {
        MCP_BRIDGE_TIMEOUT
    }
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
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FileTerm MCP bridge response exceeds the size limit",
        ));
    }
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
    enforce_mcp_access_policy(app, &request).await?;
    if request.requires_approval && action_requires_approval(&request.action) {
        request_mcp_approval(app, &request.action, &request.params).await?;
    }

    match request.action.as_str() {
        "list_connections" => list_connections(app, &request.params).await,
        "get_session_context" => get_session_context(app, &request.params).await,
        "get_command_templates" => get_command_templates(app, &request.params).await,
        "list_remote_directory" => list_remote_directory(app, &request.params).await,
        "read_remote_file" => read_remote_file(app, &request.params).await,
        "list_transfers" => list_transfers(app, &request.params).await,
        "wait_for_transfer" => wait_for_transfer(app, &request.params).await,
        "list_ssh_tunnels" => list_ssh_tunnels(app, &request.params).await,
        "open_connection" => open_connection(app, &request.params).await,
        "activate_session" => activate_session(app, &request.params).await,
        "reconnect_session" => reconnect_session(app, &request.params).await,
        "disconnect_session" => disconnect_session(app, &request.params).await,
        "close_session" => close_session(app, &request.params).await,
        "execute_remote_command" => execute_remote_command(app, &request.params).await,
        "execute_command_template" => execute_command_template(app, &request.params).await,
        "write_remote_file" => write_remote_file(app, &request.params).await,
        "create_remote_directory" => create_remote_directory(app, &request.params).await,
        "create_remote_file" => create_remote_file(app, &request.params).await,
        "copy_remote_path" => copy_remote_path(app, &request.params).await,
        "move_remote_path" => move_remote_path(app, &request.params).await,
        "rename_remote_path" => rename_remote_path(app, &request.params).await,
        "delete_remote_path" => delete_remote_path(app, &request.params).await,
        "change_remote_permissions" => change_remote_permissions(app, &request.params).await,
        "set_remote_file_access_mode" => set_remote_file_access_mode(app, &request.params).await,
        "upload_file" => upload_file(app, &request.params).await,
        "download_file" => download_file(app, &request.params).await,
        "download_remote_directory" => download_remote_directory(app, &request.params).await,
        "pause_transfer" => transfer_action(app, &request.params, "pause").await,
        "resume_transfer" => transfer_action(app, &request.params, "resume").await,
        "discard_transfer" => transfer_action(app, &request.params, "discard").await,
        "clear_transfers" => clear_transfers(app, &request.params).await,
        "create_ssh_tunnel" => create_ssh_tunnel(app, &request.params).await,
        "start_ssh_tunnel" => tunnel_action(app, &request.params, "start").await,
        "stop_ssh_tunnel" => tunnel_action(app, &request.params, "stop").await,
        "delete_ssh_tunnel" => tunnel_action(app, &request.params, "delete").await,
        _ => Err("Unsupported FileTerm MCP action".to_string()),
    }
}

/// External Agents never get a wider capability than the policy selected in
/// FileTerm settings. This check belongs on the desktop bridge rather than in
/// the stdio MCP child process so MCP, the explicit CLI and future local
/// clients share the same decision point.
async fn enforce_mcp_access_policy(app: &AppHandle, request: &BridgeRequest) -> Result<(), String> {
    let policy = mcp_access_policy(app)?;
    if policy.operation_policy == "read-only" && action_requires_approval(&request.action) {
        return Err(format!(
            "{MCP_POLICY_READ_ONLY}: FileTerm is configured to allow only read-only Agent operations"
        ));
    }

    match policy.connection_scope.as_str() {
        "all-saved-connections" => Ok(()),
        "active-session" => enforce_active_session_scope(app, request).await,
        "default-connection" => enforce_default_connection_scope(app, request, &policy).await,
        _ => Err(format!(
            "{MCP_SCOPE_DENIED}: invalid saved connection scope"
        )),
    }
}

fn mcp_access_policy(app: &AppHandle) -> Result<McpAccessPolicy, String> {
    let preferences =
        crate::commands::app_get_ui_preferences(app.clone()).map_err(public_app_error)?;
    Ok(McpAccessPolicy {
        connection_scope: preferences.mcp_agent.connection_scope,
        operation_policy: preferences.mcp_agent.operation_policy,
        default_profile_id: preferences.mcp_agent.default_profile_id,
    })
}

async fn enforce_active_session_scope(
    app: &AppHandle,
    request: &BridgeRequest,
) -> Result<(), String> {
    if matches!(
        request.action.as_str(),
        "get_command_templates"
            | "list_transfers"
            | "wait_for_transfer"
            | "pause_transfer"
            | "resume_transfer"
            | "discard_transfer"
            | "clear_transfers"
    ) {
        return Ok(());
    }
    if request.action == "list_connections" {
        return Ok(());
    }
    if request.action == "open_connection" {
        return Err(format!(
            "{MCP_SCOPE_DENIED}: active-session scope cannot open another saved connection"
        ));
    }
    let active_tab_id = active_session_tab_id(app).await?;
    let requested_tab_id = optional_string(&request.params, "tab_id", 256)?;
    match requested_tab_id {
        Some(tab_id) if tab_id == active_tab_id => Ok(()),
        Some(_) => Err(format!(
            "{MCP_SCOPE_DENIED}: this Agent is limited to FileTerm's active session"
        )),
        None if request.action == "get_session_context" => Ok(()),
        None => Err(format!(
            "{MCP_SCOPE_DENIED}: this operation requires the active FileTerm session"
        )),
    }
}

async fn enforce_default_connection_scope(
    app: &AppHandle,
    request: &BridgeRequest,
    policy: &McpAccessPolicy,
) -> Result<(), String> {
    let Some(default_profile_id) = policy.default_profile_id.as_deref() else {
        return Err(format!(
            "{MCP_SCOPE_DENIED}: choose a default connection in FileTerm settings first"
        ));
    };
    if matches!(
        request.action.as_str(),
        "get_command_templates"
            | "list_transfers"
            | "wait_for_transfer"
            | "pause_transfer"
            | "resume_transfer"
            | "discard_transfer"
            | "clear_transfers"
    ) {
        return Ok(());
    }
    if request.action == "list_connections" {
        return Ok(());
    }
    if request.action == "open_connection" {
        let requested_profile = required_string(&request.params, "profile_id", 256)?;
        return (requested_profile == default_profile_id)
            .then_some(())
            .ok_or_else(|| {
                format!(
                    "{MCP_SCOPE_DENIED}: this Agent is limited to FileTerm's default connection"
                )
            });
    }
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let allowed_tab_ids = session_tab_ids_for_profile(&snapshot, default_profile_id);
    if request.action == "get_session_context" {
        let requested_profile = optional_string(&request.params, "profile_id", 256)?;
        return requested_profile
            .is_none_or(|profile_id| profile_id == default_profile_id)
            .then_some(())
            .ok_or_else(|| {
                format!(
                    "{MCP_SCOPE_DENIED}: this Agent is limited to FileTerm's default connection"
                )
            });
    }
    let requested_tab_id = required_string(&request.params, "tab_id", 256)?;
    allowed_tab_ids
        .iter()
        .any(|tab_id| tab_id == &requested_tab_id)
        .then_some(())
        .ok_or_else(|| {
            format!("{MCP_SCOPE_DENIED}: this Agent is limited to FileTerm's default connection")
        })
}

async fn active_session_tab_id(app: &AppHandle) -> Result<String, String> {
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    active_session_tab_id_from_snapshot(&snapshot)
}

fn active_session_tab_id_from_snapshot(snapshot: &Value) -> Result<String, String> {
    let active_root = snapshot
        .get("activeTabId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{MCP_SCOPE_DENIED}: no active FileTerm session"))?;
    Ok(snapshot
        .get("activePaneTabIdByRoot")
        .and_then(Value::as_object)
        .and_then(|values| values.get(active_root))
        .and_then(Value::as_str)
        .unwrap_or(active_root)
        .to_string())
}

fn session_tab_ids_for_profile(snapshot: &Value, profile_id: &str) -> Vec<String> {
    snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|tab| tab.get("profileId").and_then(Value::as_str) == Some(profile_id))
        .filter_map(|tab| tab.get("id").and_then(Value::as_str).map(ToOwned::to_owned))
        .collect()
}

async fn mcp_visibility(app: &AppHandle) -> Result<McpVisibility, String> {
    let policy = mcp_access_policy(app)?;
    match policy.connection_scope.as_str() {
        "all-saved-connections" => Ok(McpVisibility::all_saved_connections()),
        "active-session" => {
            let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                .await
                .map_err(public_app_error)?;
            let active_tab_id = active_session_tab_id_from_snapshot(&snapshot)?;
            let mut visibility = McpVisibility {
                scope: McpVisibilityScope::ActiveSession,
                profile_ids: HashSet::new(),
                tab_ids: HashSet::from([active_tab_id.clone()]),
            };
            if let Some(profile_id) = snapshot
                .get("tabs")
                .and_then(Value::as_array)
                .and_then(|tabs| {
                    tabs.iter().find(|tab| {
                        tab.get("id").and_then(Value::as_str) == Some(active_tab_id.as_str())
                    })
                })
                .and_then(|tab| tab.get("profileId"))
                .and_then(Value::as_str)
            {
                visibility.profile_ids.insert(profile_id.to_string());
            }
            Ok(visibility)
        }
        "default-connection" => {
            let Some(default_profile_id) = policy.default_profile_id else {
                return Err(format!(
                    "{MCP_SCOPE_DENIED}: choose a default connection in FileTerm settings first"
                ));
            };
            let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                .await
                .map_err(public_app_error)?;
            Ok(McpVisibility {
                scope: McpVisibilityScope::DefaultConnection,
                profile_ids: HashSet::from([default_profile_id.clone()]),
                tab_ids: session_tab_ids_for_profile(&snapshot, &default_profile_id)
                    .into_iter()
                    .collect(),
            })
        }
        _ => Err(format!(
            "{MCP_SCOPE_DENIED}: invalid saved connection scope"
        )),
    }
}

fn action_requires_approval(action: &str) -> bool {
    matches!(
        action,
        "open_connection"
            | "reconnect_session"
            | "disconnect_session"
            | "close_session"
            | "execute_remote_command"
            | "execute_command_template"
            | "write_remote_file"
            | "create_remote_directory"
            | "create_remote_file"
            | "copy_remote_path"
            | "move_remote_path"
            | "rename_remote_path"
            | "delete_remote_path"
            | "change_remote_permissions"
            | "set_remote_file_access_mode"
            | "upload_file"
            | "download_file"
            | "download_remote_directory"
            | "pause_transfer"
            | "resume_transfer"
            | "discard_transfer"
            | "clear_transfers"
            | "create_ssh_tunnel"
            | "start_ssh_tunnel"
            | "stop_ssh_tunnel"
            | "delete_ssh_tunnel"
    )
}

async fn request_mcp_approval(app: &AppHandle, action: &str, params: &Value) -> Result<(), String> {
    let details = approval_details(app, action, params).await?;
    let decision = request_action_approval(app, ActionApprovalSource::Mcp, action, details)
        .await
        .map_err(public_app_error)?;
    match decision {
        ActionApprovalDecision::Approved => Ok(()),
        decision => Err(decision
            .rejection_message(ActionApprovalSource::Mcp)
            .to_string()),
    }
}

async fn approval_details(
    app: &AppHandle,
    action: &str,
    params: &Value,
) -> Result<ActionApprovalDetails, String> {
    let tab_id = optional_string(params, "tab_id", 256)?;
    let target = match action {
        "open_connection" => optional_string(params, "profile_id", 256)?,
        "write_remote_file"
        | "delete_remote_path"
        | "change_remote_permissions"
        | "set_remote_file_access_mode" => optional_string(params, "path", 4_096)?,
        "copy_remote_path" | "move_remote_path" => {
            optional_string(params, "destination_path", 4_096)?
        }
        "rename_remote_path" => optional_string(params, "target_path", 4_096)?,
        "upload_file" => optional_string(params, "local_path", 4_096)?,
        "download_file" | "download_remote_directory" => {
            optional_string(params, "remote_path", 4_096)?
        }
        "pause_transfer" | "resume_transfer" | "discard_transfer" => {
            optional_string(params, "transfer_id", 256)?
        }
        "clear_transfers" => params.get("transfer_ids").map(|value| {
            truncate_text(
                &serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
                4_096,
            )
        }),
        "start_ssh_tunnel" | "stop_ssh_tunnel" | "delete_ssh_tunnel" => {
            optional_string(params, "rule_id", 256)?
        }
        "create_remote_directory" | "create_remote_file" => Some(format!(
            "父目录：{}\n名称：{}",
            required_string(params, "parent_path", 4_096)?,
            required_string(params, "name", 512)?
        )),
        "create_ssh_tunnel" => params
            .get("rule")
            .and_then(Value::as_object)
            .and_then(|rule| rule.get("name"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        _ => tab_id.clone(),
    };
    let details = match action {
        "execute_remote_command" => Some(truncate_text(
            &required_text(params, "command", 64 * 1024)?,
            4 * 1024,
        )),
        "execute_command_template" => {
            let command_id = required_string(params, "command_id", 256)?;
            let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                .await
                .map_err(public_app_error)?;
            let template = snapshot
                .get("commandTemplates")
                .and_then(Value::as_array)
                .and_then(|templates| {
                    templates.iter().find(|template| {
                        template.get("id").and_then(Value::as_str) == Some(command_id.as_str())
                    })
                });
            let template_text = template
                .and_then(|template| template.get("command"))
                .and_then(Value::as_str)
                .map(|command| truncate_text(command, 4 * 1024))
                .unwrap_or_else(|| "未找到命令模板内容".to_string());
            Some(format!(
                "命令模板：{}\n命令：{}\n参数：{}",
                command_id,
                template_text,
                serde_json::to_string(params.get("args").unwrap_or(&Value::Null))
                    .unwrap_or_else(|_| "null".to_string())
            ))
        }
        "write_remote_file" => {
            let content = required_text(params, "content", MCP_MAX_FILE_CONTENT_BYTES)?;
            Some(format!(
                "写入 {} 字节{}",
                content.len(),
                if content.is_empty() {
                    String::new()
                } else {
                    format!("\n内容预览：{}", truncate_text(&content, 1_000))
                }
            ))
        }
        "upload_file" => Some(format!(
            "本地源：{}\n远端目录：{}",
            required_string(params, "local_path", 4_096)?,
            required_string(params, "remote_directory", 4_096)?
        )),
        "download_file" | "download_remote_directory" => Some(format!(
            "远端源：{}\n本地目录：{}",
            required_string(params, "remote_path", 4_096)?,
            required_string(params, "local_directory", 4_096)?
        )),
        "copy_remote_path" | "move_remote_path" => Some(format!(
            "源路径：{}\n目标路径：{}",
            required_string(params, "target_path", 4_096)?,
            required_string(params, "destination_path", 4_096)?
        )),
        "rename_remote_path" => Some(format!(
            "原路径：{}\n新名称：{}",
            required_string(params, "target_path", 4_096)?,
            required_string(params, "new_name", 512)?
        )),
        "change_remote_permissions" => Some(format!(
            "模式：{}\n递归：{}\n应用范围：{}",
            required_string(params, "mode", 4)?,
            optional_bool(params, "recursive")?.unwrap_or(false),
            optional_string(params, "apply_to", 32)?.unwrap_or_else(|| "all".to_string())
        )),
        "set_remote_file_access_mode" => Some(format!(
            "访问模式：{}",
            required_string(params, "mode", 16)?
        )),
        "clear_transfers" => Some(format!(
            "传输任务：{}",
            serde_json::to_string(params.get("transfer_ids").unwrap_or(&Value::Null))
                .unwrap_or_else(|_| "null".to_string())
        )),
        "create_ssh_tunnel" => Some(format!(
            "规则：{}",
            truncate_text(
                &serde_json::to_string(params.get("rule").unwrap_or(&Value::Null))
                    .unwrap_or_else(|_| "null".to_string()),
                4 * 1024
            )
        )),
        _ => None,
    };
    let summary = match action {
        "open_connection" => "打开 FileTerm 连接".to_string(),
        "reconnect_session" => "重新连接 FileTerm 会话".to_string(),
        "disconnect_session" => "断开 FileTerm 会话".to_string(),
        "close_session" => "关闭 FileTerm 标签页".to_string(),
        "execute_remote_command" => "在远程 SSH 主机执行命令".to_string(),
        "execute_command_template" => "执行 FileTerm 命令模板".to_string(),
        "write_remote_file" => "写入远程文件".to_string(),
        "create_remote_directory" => "创建远程目录".to_string(),
        "create_remote_file" => "创建远程文件".to_string(),
        "copy_remote_path" => "复制远程文件或目录".to_string(),
        "move_remote_path" => "移动远程文件或目录".to_string(),
        "rename_remote_path" => "重命名远程文件或目录".to_string(),
        "delete_remote_path" => "删除远程文件或目录".to_string(),
        "change_remote_permissions" => "修改远程文件权限".to_string(),
        "set_remote_file_access_mode" => "切换远程文件访问身份".to_string(),
        "upload_file" => "上传本地文件或目录".to_string(),
        "download_file" => "下载远程文件".to_string(),
        "download_remote_directory" => "下载远程目录".to_string(),
        "pause_transfer" => "暂停传输任务".to_string(),
        "resume_transfer" => "继续传输任务".to_string(),
        "discard_transfer" => "丢弃传输任务断点".to_string(),
        "clear_transfers" => "清理传输历史".to_string(),
        "create_ssh_tunnel" => "创建 SSH 隧道".to_string(),
        "start_ssh_tunnel" => "启动 SSH 隧道".to_string(),
        "stop_ssh_tunnel" => "停止 SSH 隧道".to_string(),
        "delete_ssh_tunnel" => "删除 SSH 隧道".to_string(),
        _ => return Err("Unsupported FileTerm MCP approval action".to_string()),
    };
    Ok(ActionApprovalDetails {
        title: "MCP 外部操作需要确认".to_string(),
        summary,
        target: target.or(tab_id),
        details,
        destructive: matches!(
            action,
            "write_remote_file"
                | "delete_remote_path"
                | "change_remote_permissions"
                | "set_remote_file_access_mode"
                | "discard_transfer"
                | "clear_transfers"
                | "delete_ssh_tunnel"
        ),
        requires_risk_acknowledgement: false,
    })
}

async fn list_connections(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let visibility = mcp_visibility(app).await?;
    let library = crate::commands::app_get_connection_library(app.clone())
        .await
        .map_err(public_app_error)?;
    let profiles = library
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned an invalid connection library".to_string())?;
    let profiles = profiles
        .iter()
        .filter(|profile| visibility.allows_profile(profile.get("id").and_then(Value::as_str)))
        .collect::<Vec<_>>();
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
    let visibility = mcp_visibility(app).await?;
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
        .filter(|tab| {
            matches!(visibility.scope, McpVisibilityScope::ActiveSession)
                || tab.get("paneRootTabId").is_none()
        })
        .filter(|tab| visibility.allows_tab(tab.get("id").and_then(Value::as_str)))
        .filter(|tab| visibility.allows_profile(tab.get("profileId").and_then(Value::as_str)))
        .filter(|tab| {
            profile_id.as_deref().is_none_or(|profile_id| {
                tab.get("profileId").and_then(Value::as_str) == Some(profile_id)
            })
        })
        .filter_map(|tab| {
            let tab_id = tab.get("id").and_then(Value::as_str)?;
            let session = sessions.get(tab_id)?;
            Some(compact_session(tab, session, tab_id))
        })
        .collect::<Vec<_>>();
    Ok(json!({ "items": items }))
}

async fn get_command_templates(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let templates = snapshot
        .get("commandTemplates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = templates.len();
    let items = templates
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

async fn list_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = optional_string(params, "path", 4_096)?;
    let (limit, offset) = pagination(params)?;
    let snapshot = crate::commands::mcp_list_remote_directory(app.clone(), tab_id, path)
        .await
        .map_err(public_app_error)?;
    let items = snapshot
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = items.len();
    let items = items
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_offset = offset + items.len();
    Ok(json!({
        "tabId": snapshot.get("tabId"),
        "path": snapshot.get("path"),
        "total": total,
        "count": items.len(),
        "offset": offset,
        "items": items,
        "hasMore": next_offset < total,
        "nextOffset": (next_offset < total).then_some(next_offset),
    }))
}

async fn read_remote_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = required_string(params, "path", 4_096)?;
    let encoding = optional_string(params, "encoding", 64)?;
    let content = crate::commands::app_read_remote_file(
        app.clone(),
        tab_id.clone(),
        path.clone(),
        encoding.clone(),
    )
    .await
    .map_err(public_app_error)?;
    let (content, truncated) = truncate_text_with_flag(&content, MCP_MAX_FILE_CONTENT_BYTES);
    Ok(json!({
        "tabId": tab_id,
        "path": path,
        "encoding": encoding.unwrap_or_else(|| "utf-8".to_string()),
        "content": content,
        "truncated": truncated,
    }))
}

async fn list_transfers(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let visibility = mcp_visibility(app).await?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let transfers = snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned invalid transfer state".to_string())?;
    let transfers = transfers
        .iter()
        .filter(|transfer| visibility.allows_transfer_value(transfer))
        .collect::<Vec<_>>();
    let total = transfers.len();
    let items = transfers
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

/// Wait locally for a transfer to complete, without forcing an MCP client to
/// repeatedly poll the desktop bridge. A timeout is a successful observation
/// result: the task is returned in its latest known state so an Agent can make
/// an explicit next decision instead of mistaking a still-running transfer for
/// a failed request.
async fn wait_for_transfer(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let transfer_id = required_string(params, "transfer_id", 256)?;
    let timeout_ms = optional_u64(params, "timeout_ms")?.unwrap_or(MCP_TRANSFER_WAIT_DEFAULT_MS);
    if !(1_000..=MCP_TRANSFER_WAIT_MAX_MS).contains(&timeout_ms) {
        return Err(format!(
            "timeout_ms must be between 1000 and {MCP_TRANSFER_WAIT_MAX_MS}"
        ));
    }

    let visibility = mcp_visibility(app).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let task = crate::services::transfers::list(app)
            .await
            .map_err(public_app_error)?
            .into_iter()
            .find(|task| task.id == transfer_id)
            .ok_or_else(|| format!("{MCP_TRANSFER_NOT_FOUND}: transfer was not found"))?;
        if !visibility.allows_transfer_task(&task) {
            return Err(format!(
                "{MCP_SCOPE_DENIED}: this Agent cannot access the requested transfer"
            ));
        }
        let terminal = matches!(task.status.as_str(), "done" | "failed" | "canceled");
        if terminal {
            return Ok(json!({
                "transferId": transfer_id,
                "transfer": task,
                "timedOut": false,
            }));
        }

        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(json!({
                "transferId": transfer_id,
                "transfer": task,
                "timedOut": true,
            }));
        }
        // Transfer workers already update their durable task state at a
        // throttled cadence. Waiting here keeps that cadence local to
        // FileTerm and avoids external Agent-side polling loops.
        tokio::time::sleep((deadline - now).min(Duration::from_millis(250))).await;
    }
}

async fn list_ssh_tunnels(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let items = crate::commands::app_list_ssh_tunnels(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(json!({ "tabId": tab_id, "items": items }))
}

async fn open_connection(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let profile_id = required_string(params, "profile_id", 256)?;
    let snapshot = crate::commands::app_open_profile(app.clone(), profile_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(&snapshot, None, "open_connection"))
}

async fn activate_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_activate_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "activate_session",
    ))
}

async fn reconnect_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_reconnect_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "reconnect_session",
    ))
}

async fn disconnect_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_disconnect_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "disconnect_session",
    ))
}

async fn close_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_close_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(&snapshot, Some(&tab_id), "close_session"))
}

async fn execute_remote_command(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let command = required_text(params, "command", 64 * 1024)?;
    let cwd = optional_string(params, "cwd", 4_096)?;
    let timeout_ms = optional_u64(params, "timeout_ms")?;
    let sudo_password = optional_secret_string(params, "sudo_password", 4 * 1024)?;
    let su_password = optional_secret_string(params, "su_password", 4 * 1024)?;
    let save_sudo_password = optional_bool(params, "save_sudo_password")?.unwrap_or(false);
    let save_su_password = optional_bool(params, "save_su_password")?.unwrap_or(false);
    crate::commands::app_execute_remote_command(
        app.clone(),
        tab_id.clone(),
        command,
        cwd,
        timeout_ms,
        sudo_password,
        su_password,
        Some(save_sudo_password),
        Some(save_su_password),
    )
    .await
    .map(|result| json!({ "tabId": tab_id, "result": result }))
    .map_err(public_app_error)
}

async fn execute_command_template(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let command_id = required_string(params, "command_id", 256)?;
    let args = optional_string_array(params, "args", 64, 4_096)?;
    let options = params.get("options").cloned();
    crate::commands::app_execute_command_template(
        app.clone(),
        tab_id.clone(),
        command_id,
        args,
        options,
    )
    .await
    .map(|result| json!({ "tabId": tab_id, "result": result }))
    .map_err(public_app_error)
}

async fn write_remote_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = required_string(params, "path", 4_096)?;
    let content = required_text(params, "content", MCP_MAX_FILE_CONTENT_BYTES)?;
    let encoding = optional_string(params, "encoding", 64)?;
    let snapshot = crate::commands::app_write_remote_file(
        app.clone(),
        tab_id.clone(),
        path,
        content,
        encoding,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "write_remote_file",
    ))
}

async fn create_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let parent_path = required_string(params, "parent_path", 4_096)?;
    let name = required_string(params, "name", 512)?;
    let snapshot = crate::commands::app_create_remote_directory(
        app.clone(),
        tab_id.clone(),
        parent_path,
        name,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "create_remote_directory",
    ))
}

async fn create_remote_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let parent_path = required_string(params, "parent_path", 4_096)?;
    let name = required_string(params, "name", 512)?;
    let snapshot =
        crate::commands::app_create_remote_file(app.clone(), tab_id.clone(), parent_path, name)
            .await
            .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "create_remote_file",
    ))
}

async fn copy_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let destination_path = required_string(params, "destination_path", 4_096)?;
    let target_type = required_target_type(params)?;
    let snapshot = crate::commands::app_copy_remote_path(
        app.clone(),
        tab_id.clone(),
        target_path,
        destination_path,
        target_type,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "copy_remote_path",
    ))
}

async fn move_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let destination_path = required_string(params, "destination_path", 4_096)?;
    let snapshot = crate::commands::app_move_remote_path(
        app.clone(),
        tab_id.clone(),
        target_path,
        destination_path,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "move_remote_path",
    ))
}

async fn rename_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let new_name = required_string(params, "new_name", 512)?;
    let snapshot =
        crate::commands::app_rename_remote_path(app.clone(), tab_id.clone(), target_path, new_name)
            .await
            .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "rename_remote_path",
    ))
}

async fn delete_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let target_type = required_target_type(params)?;
    let snapshot = crate::commands::app_delete_remote_path(
        app.clone(),
        tab_id.clone(),
        target_path,
        target_type,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "delete_remote_path",
    ))
}

async fn change_remote_permissions(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "path", 4_096)?;
    let mode = required_string(params, "mode", 4)?;
    let options = serde_json::from_value(json!({
        "mode": mode,
        "recursive": optional_bool(params, "recursive")?.unwrap_or(false),
        "applyTo": optional_string(params, "apply_to", 32)?,
    }))
    .map_err(|error| format!("Invalid remote permission options: {error}"))?;
    let snapshot = crate::commands::app_change_remote_permissions(
        app.clone(),
        tab_id.clone(),
        target_path,
        options,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "change_remote_permissions",
    ))
}

async fn set_remote_file_access_mode(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let mode = required_string(params, "mode", 16)?;
    if !matches!(mode.as_str(), "user" | "root") {
        return Err("mode must be user or root".to_string());
    }
    let snapshot =
        crate::commands::app_set_remote_file_access_mode(app.clone(), tab_id.clone(), mode, None)
            .await
            .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "set_remote_file_access_mode",
    ))
}

fn transfer_options(params: &Value) -> Result<Option<Value>, String> {
    Ok(optional_string(params, "target_name", 512)?
        .map(|target_name| json!({ "targetName": target_name })))
}

async fn upload_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let local_path = required_string(params, "local_path", 4_096)?;
    let remote_directory = required_string(params, "remote_directory", 4_096)?;
    let snapshot = crate::commands::app_upload_file(
        app.clone(),
        tab_id.clone(),
        local_path,
        remote_directory,
        transfer_options(params)?,
    )
    .await
    .map_err(public_app_error)?;
    Ok(transfer_snapshot(
        &snapshot,
        &tab_id,
        "upload_file",
        "upload",
    ))
}

async fn download_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let remote_path = required_string(params, "remote_path", 4_096)?;
    let local_directory = required_string(params, "local_directory", 4_096)?;
    let snapshot = crate::commands::app_download_file(
        app.clone(),
        tab_id.clone(),
        remote_path,
        local_directory,
        transfer_options(params)?,
    )
    .await
    .map_err(public_app_error)?;
    Ok(transfer_snapshot(
        &snapshot,
        &tab_id,
        "download_file",
        "download",
    ))
}

async fn download_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let remote_path = required_string(params, "remote_path", 4_096)?;
    let local_directory = required_string(params, "local_directory", 4_096)?;
    let snapshot = crate::commands::app_download_remote_path(
        app.clone(),
        tab_id.clone(),
        remote_path,
        "folder".to_string(),
        local_directory,
        transfer_options(params)?,
    )
    .await
    .map_err(public_app_error)?;
    Ok(transfer_snapshot(
        &snapshot,
        &tab_id,
        "download_remote_directory",
        "download",
    ))
}

async fn transfer_action(app: &AppHandle, params: &Value, action: &str) -> Result<Value, String> {
    let transfer_id = required_string(params, "transfer_id", 256)?;
    ensure_transfer_in_mcp_scope(app, &transfer_id).await?;
    let snapshot = match action {
        "pause" => crate::commands::app_pause_transfer(app.clone(), transfer_id.clone()).await,
        "resume" => crate::commands::app_resume_transfer(app.clone(), transfer_id.clone()).await,
        "discard" => crate::commands::app_discard_transfer(app.clone(), transfer_id.clone()).await,
        _ => return Err("Unsupported transfer action".to_string()),
    }
    .map_err(public_app_error)?;
    Ok(json!({
        "operation": format!("{action}_transfer"),
        "transferId": transfer_id,
        "transfer": find_transfer(&snapshot, &transfer_id),
    }))
}

async fn clear_transfers(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let transfer_ids = required_string_array(params, "transfer_ids", 256, 256)?;
    for transfer_id in &transfer_ids {
        ensure_transfer_in_mcp_scope(app, transfer_id).await?;
    }
    let snapshot = crate::commands::app_clear_transfers(app.clone(), transfer_ids.clone())
        .await
        .map_err(public_app_error)?;
    Ok(json!({
        "operation": "clear_transfers",
        "transferIds": transfer_ids,
        "remaining": snapshot.get("transfers").cloned().unwrap_or_else(|| json!([])),
    }))
}

async fn ensure_transfer_in_mcp_scope(app: &AppHandle, transfer_id: &str) -> Result<(), String> {
    let visibility = mcp_visibility(app).await?;
    let task = crate::services::transfers::list(app)
        .await
        .map_err(public_app_error)?
        .into_iter()
        .find(|task| task.id == transfer_id)
        .ok_or_else(|| format!("{MCP_TRANSFER_NOT_FOUND}: transfer was not found"))?;
    visibility
        .allows_transfer_task(&task)
        .then_some(())
        .ok_or_else(|| {
            format!("{MCP_SCOPE_DENIED}: this Agent cannot access the requested transfer")
        })
}

async fn create_ssh_tunnel(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let rule = params
        .get("rule")
        .cloned()
        .ok_or_else(|| "rule is required".to_string())?;
    if !rule.is_object() {
        return Err("rule must be an object".to_string());
    }
    let items = crate::commands::app_create_ssh_tunnel(app.clone(), tab_id.clone(), rule)
        .await
        .map_err(public_app_error)?;
    Ok(json!({ "tabId": tab_id, "items": items }))
}

async fn tunnel_action(app: &AppHandle, params: &Value, action: &str) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let rule_id = required_string(params, "rule_id", 256)?;
    let items = match action {
        "start" => {
            crate::commands::app_start_ssh_tunnel(app.clone(), tab_id.clone(), rule_id).await
        }
        "stop" => crate::commands::app_stop_ssh_tunnel(app.clone(), tab_id.clone(), rule_id).await,
        "delete" => {
            crate::commands::app_delete_ssh_tunnel(app.clone(), tab_id.clone(), rule_id).await
        }
        _ => return Err("Unsupported SSH tunnel action".to_string()),
    }
    .map_err(public_app_error)?;
    Ok(json!({ "tabId": tab_id, "items": items }))
}

fn compact_session(tab: &Value, session: &Value, tab_id: &str) -> Value {
    json!({
        "tabId": tab_id,
        "rootTabId": tab.get("paneRootTabId").cloned().unwrap_or_else(|| Value::String(tab_id.to_string())),
        "profileId": tab.get("profileId"),
        "title": tab.get("title"),
        "sessionType": tab.get("sessionType"),
        "status": tab.get("status"),
        "connected": session.get("connected"),
        "remotePath": session.get("remotePath"),
        "shellCwd": session.get("shellCwd"),
        "shellUser": session.get("shellUser"),
        "sessionRevision": session.get("aiSessionRevision"),
        "fileAccessMode": session.get("fileAccessMode"),
        "capabilities": session.get("capabilities"),
    })
}

fn compact_snapshot(snapshot: &Value, tab_id: Option<&str>, operation: &str) -> Value {
    let tab_id = tab_id.or_else(|| snapshot.get("activeTabId").and_then(Value::as_str));
    let session = tab_id.and_then(|id| {
        let tab = snapshot
            .get("tabs")
            .and_then(Value::as_array)
            .and_then(|tabs| {
                tabs.iter()
                    .find(|tab| tab.get("id").and_then(Value::as_str) == Some(id))
            })?;
        let session = snapshot.get("sessions")?.get(id)?;
        Some(compact_session(tab, session, id))
    });
    json!({
        "operation": operation,
        "activeTabId": snapshot.get("activeTabId"),
        "session": session,
    })
}

fn transfer_snapshot(snapshot: &Value, tab_id: &str, operation: &str, direction: &str) -> Value {
    let transfer = snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .and_then(|transfers| {
            transfers.iter().rev().find(|transfer| {
                transfer.get("tabId").and_then(Value::as_str) == Some(tab_id)
                    && transfer.get("direction").and_then(Value::as_str) == Some(direction)
            })
        });
    json!({
        "operation": operation,
        "tabId": tab_id,
        "transfer": transfer,
    })
}

fn find_transfer(snapshot: &Value, transfer_id: &str) -> Value {
    snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .and_then(|transfers| {
            transfers
                .iter()
                .find(|transfer| transfer.get("id").and_then(Value::as_str) == Some(transfer_id))
        })
        .cloned()
        .unwrap_or(Value::Null)
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

fn optional_u64(params: &Value, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .ok_or_else(|| format!("{key} must be a non-negative integer"))
        .map(Some)
}

fn optional_bool(params: &Value, key: &str) -> Result<Option<bool>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    value
        .as_bool()
        .ok_or_else(|| format!("{key} must be a boolean"))
        .map(Some)
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

/// Parse a one-shot secret without trimming it. Passwords may legitimately
/// contain leading/trailing spaces; control characters are rejected later by
/// the execution service before the value reaches SSH stdin.
fn optional_secret_string(
    params: &Value,
    key: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?;
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{key} exceeds the FileTerm MCP limit"));
    }
    Ok(Some(value.to_string()))
}

fn required_text(params: &Value, key: &str, maximum: usize) -> Result<String, String> {
    let value = params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required and must be a string"))?;
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{key} exceeds the FileTerm MCP limit"));
    }
    Ok(value.to_string())
}

fn optional_string_array(
    params: &Value,
    key: &str,
    maximum_items: usize,
    maximum_item_bytes: usize,
) -> Result<Option<Vec<String>>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("{key} must be an array of strings"))?;
    if items.len() > maximum_items {
        return Err(format!("{key} has too many items"));
    }
    items
        .iter()
        .map(|item| {
            let value = item
                .as_str()
                .ok_or_else(|| format!("{key} must contain only strings"))?;
            if value.len() > maximum_item_bytes {
                return Err(format!("{key} item exceeds the FileTerm MCP limit"));
            }
            Ok(value.to_string())
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Some)
}

fn required_string_array(
    params: &Value,
    key: &str,
    maximum_items: usize,
    maximum_item_bytes: usize,
) -> Result<Vec<String>, String> {
    optional_string_array(params, key, maximum_items, maximum_item_bytes)?
        .ok_or_else(|| format!("{key} is required"))
}

fn required_target_type(params: &Value) -> Result<String, String> {
    let target_type = required_string(params, "target_type", 16)?;
    if !matches!(target_type.as_str(), "file" | "folder") {
        return Err("target_type must be file or folder".to_string());
    }
    Ok(target_type)
}

fn truncate_text(value: &str, maximum: usize) -> String {
    truncate_text_with_flag(value, maximum).0
}

fn truncate_text_with_flag(value: &str, maximum: usize) -> (String, bool) {
    if value.len() <= maximum {
        return (value.to_string(), false);
    }
    let mut end = maximum;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (
        format!("{}\n[… output truncated by FileTerm …]", &value[..end]),
        true,
    )
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
        println!("Usage: fileterm mcp\n\nRun the FileTerm MCP server over stdio. FileTerm must be running.");
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
        if line.len() > MCP_MAX_MESSAGE_BYTES {
            let response = jsonrpc_error(Value::Null, -32600, "Request exceeds the size limit");
            serde_json::to_writer(&mut stdout, &response)
                .map_err(|error| format!("Unable to encode MCP response: {error}"))?;
            stdout
                .write_all(b"\n")
                .map_err(|error| format!("Unable to write MCP response: {error}"))?;
            stdout
                .flush()
                .map_err(|error| format!("Unable to flush MCP response: {error}"))?;
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

/// Entry point for the small FileTerm CLI. The CLI intentionally
/// shares the MCP bridge and returns JSON so shell scripts and agents can use
/// the same capability boundary without duplicating authorization logic.
pub fn run_cli(arguments: &[String]) -> Result<(), String> {
    let command_index = usize::from(arguments.first().is_some_and(|argument| argument == "cli"));
    let Some(command) = arguments.get(command_index).map(String::as_str) else {
        print_cli_help();
        return Ok(());
    };
    let options = &arguments[command_index + 1..];

    match command {
        "help" | "-h" | "--help" => {
            print_cli_help();
            Ok(())
        }
        "-V" | "--version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "connections" => {
            if has_cli_help(options) {
                print_cli_command_help("connections");
                return Ok(());
            }
            let values = parse_cli_options(options, &["limit", "offset"])?;
            let mut params = serde_json::Map::new();
            if let Some(limit) = values.get("limit") {
                params.insert("limit".to_string(), json!(parse_cli_usize("limit", limit)?));
            }
            if let Some(offset) = values.get("offset") {
                params.insert(
                    "offset".to_string(),
                    json!(parse_cli_usize("offset", offset)?),
                );
            }
            print_cli_result(call_desktop_bridge(cli_bridge_request(
                "list_connections",
                Value::Object(params),
            ))?)
        }
        "sessions" => {
            if has_cli_help(options) {
                print_cli_command_help("sessions");
                return Ok(());
            }
            let values = parse_cli_options(options, &["profile-id"])?;
            let mut params = serde_json::Map::new();
            if let Some(profile_id) = values.get("profile-id") {
                params.insert("profile_id".to_string(), json!(profile_id));
            }
            print_cli_result(call_desktop_bridge(cli_bridge_request(
                "get_session_context",
                Value::Object(params),
            ))?)
        }
        "directory" | "ls" => {
            if has_cli_help(options) {
                print_cli_command_help("directory");
                return Ok(());
            }
            let values = parse_cli_options(options, &["tab-id", "path", "limit", "offset"])?;
            let tab_id = values
                .get("tab-id")
                .ok_or_else(|| "directory requires --tab-id <TAB_ID>".to_string())?;
            let mut params = serde_json::Map::new();
            params.insert("tab_id".to_string(), json!(tab_id));
            if let Some(path) = values.get("path") {
                params.insert("path".to_string(), json!(path));
            }
            if let Some(limit) = values.get("limit") {
                params.insert("limit".to_string(), json!(parse_cli_usize("limit", limit)?));
            }
            if let Some(offset) = values.get("offset") {
                params.insert(
                    "offset".to_string(),
                    json!(parse_cli_usize("offset", offset)?),
                );
            }
            print_cli_result(call_desktop_bridge(cli_bridge_request(
                "list_remote_directory",
                Value::Object(params),
            ))?)
        }
        "commands" | "command-templates" => {
            cli_action("get_command_templates", options, &["limit", "offset"], &[])
        }
        "read" | "cat" => cli_action(
            "read_remote_file",
            options,
            &["tab-id", "path", "encoding"],
            &["tab-id", "path"],
        ),
        "transfers" => cli_action("list_transfers", options, &["limit", "offset"], &[]),
        "wait-transfer" => cli_action(
            "wait_for_transfer",
            options,
            &["transfer-id", "timeout-ms"],
            &["transfer-id"],
        ),
        "tunnels" => cli_action("list_ssh_tunnels", options, &["tab-id"], &["tab-id"]),
        "open" => cli_action("open_connection", options, &["profile-id"], &["profile-id"]),
        "activate" => cli_action("activate_session", options, &["tab-id"], &["tab-id"]),
        "reconnect" => cli_action("reconnect_session", options, &["tab-id"], &["tab-id"]),
        "disconnect" => cli_action("disconnect_session", options, &["tab-id"], &["tab-id"]),
        "close" => cli_action("close_session", options, &["tab-id"], &["tab-id"]),
        "exec" | "execute" => cli_action(
            "execute_remote_command",
            options,
            &[
                "tab-id",
                "command",
                "cwd",
                "timeout-ms",
                "sudo-password",
                "su-password",
                "save-sudo-password",
                "save-su-password",
            ],
            &["tab-id", "command"],
        ),
        "command-template" => cli_action(
            "execute_command_template",
            options,
            &["tab-id", "command-id", "args-json", "options-json"],
            &["tab-id", "command-id"],
        ),
        "write" => cli_action(
            "write_remote_file",
            options,
            &["tab-id", "path", "content", "encoding"],
            &["tab-id", "path", "content"],
        ),
        "mkdir" => cli_action(
            "create_remote_directory",
            options,
            &["tab-id", "parent-path", "name"],
            &["tab-id", "parent-path", "name"],
        ),
        "touch" => cli_action(
            "create_remote_file",
            options,
            &["tab-id", "parent-path", "name"],
            &["tab-id", "parent-path", "name"],
        ),
        "copy" => cli_action(
            "copy_remote_path",
            options,
            &["tab-id", "target-path", "destination-path", "target-type"],
            &["tab-id", "target-path", "destination-path", "target-type"],
        ),
        "move" => cli_action(
            "move_remote_path",
            options,
            &["tab-id", "target-path", "destination-path"],
            &["tab-id", "target-path", "destination-path"],
        ),
        "rename" => cli_action(
            "rename_remote_path",
            options,
            &["tab-id", "target-path", "new-name"],
            &["tab-id", "target-path", "new-name"],
        ),
        "delete" => cli_action(
            "delete_remote_path",
            options,
            &["tab-id", "target-path", "target-type"],
            &["tab-id", "target-path", "target-type"],
        ),
        "chmod" => cli_action(
            "change_remote_permissions",
            options,
            &["tab-id", "path", "mode", "recursive", "apply-to"],
            &["tab-id", "path", "mode"],
        ),
        "access" => cli_action(
            "set_remote_file_access_mode",
            options,
            &["tab-id", "mode"],
            &["tab-id", "mode"],
        ),
        "upload" => cli_action(
            "upload_file",
            options,
            &["tab-id", "local-path", "remote-directory", "target-name"],
            &["tab-id", "local-path", "remote-directory"],
        ),
        "download" => cli_action(
            "download_file",
            options,
            &["tab-id", "remote-path", "local-directory", "target-name"],
            &["tab-id", "remote-path", "local-directory"],
        ),
        "download-directory" => cli_action(
            "download_remote_directory",
            options,
            &["tab-id", "remote-path", "local-directory", "target-name"],
            &["tab-id", "remote-path", "local-directory"],
        ),
        "pause-transfer" => cli_action(
            "pause_transfer",
            options,
            &["transfer-id"],
            &["transfer-id"],
        ),
        "resume-transfer" => cli_action(
            "resume_transfer",
            options,
            &["transfer-id"],
            &["transfer-id"],
        ),
        "discard-transfer" | "cancel-transfer" => cli_action(
            "discard_transfer",
            options,
            &["transfer-id"],
            &["transfer-id"],
        ),
        "clear-transfers" => cli_action(
            "clear_transfers",
            options,
            &["transfer-ids"],
            &["transfer-ids"],
        ),
        "create-tunnel" => cli_action(
            "create_ssh_tunnel",
            options,
            &["tab-id", "rule-json"],
            &["tab-id", "rule-json"],
        ),
        "start-tunnel" => cli_action(
            "start_ssh_tunnel",
            options,
            &["tab-id", "rule-id"],
            &["tab-id", "rule-id"],
        ),
        "stop-tunnel" => cli_action(
            "stop_ssh_tunnel",
            options,
            &["tab-id", "rule-id"],
            &["tab-id", "rule-id"],
        ),
        "delete-tunnel" => cli_action(
            "delete_ssh_tunnel",
            options,
            &["tab-id", "rule-id"],
            &["tab-id", "rule-id"],
        ),
        "call" => cli_call_action(options),
        _ => Err(format!(
            "Unknown FileTerm CLI command: {command}. Run `fileterm --help` for usage."
        )),
    }
}

fn has_cli_help(arguments: &[String]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == "-h" || argument == "--help")
}

fn cli_bridge_request(action: &str, params: Value) -> BridgeRequest {
    BridgeRequest {
        action: action.to_string(),
        params,
        requires_approval: false,
    }
}

fn cli_action(
    action: &str,
    arguments: &[String],
    allowed: &[&str],
    required: &[&str],
) -> Result<(), String> {
    if has_cli_help(arguments) {
        print_cli_command_help(action);
        return Ok(());
    }
    let values = parse_cli_options(arguments, allowed)?;
    for key in required {
        if !values.contains_key(*key) {
            return Err(format!("{action} requires --{key} <value>"));
        }
    }
    let params = cli_values_to_params(&values)?;
    print_cli_result(call_desktop_bridge(cli_bridge_request(action, params))?)
}

fn cli_call_action(arguments: &[String]) -> Result<(), String> {
    let action = arguments
        .first()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| "call requires an action name".to_string())?;
    let values = parse_cli_options(&arguments[1..], &["params-json"])?;
    let params_json = values
        .get("params-json")
        .ok_or_else(|| "call requires --params-json JSON".to_string())?;
    let params = serde_json::from_str::<Value>(params_json)
        .map_err(|error| format!("--params-json must be valid JSON: {error}"))?;
    if !params.is_object() {
        return Err("--params-json must contain a JSON object".to_string());
    }
    print_cli_result(call_desktop_bridge(cli_bridge_request(action, params))?)
}

fn cli_values_to_params(values: &HashMap<String, String>) -> Result<Value, String> {
    let mut params = serde_json::Map::new();
    for (key, value) in values {
        let parameter = match key.as_str() {
            "rule-json" => "rule".to_string(),
            "args-json" => "args".to_string(),
            "options-json" => "options".to_string(),
            "transfer-ids" => "transfer_ids".to_string(),
            _ => key.replace('-', "_"),
        };
        let converted = match key.as_str() {
            "rule-json" => serde_json::from_str::<Value>(value)
                .map_err(|error| format!("--rule-json must be valid JSON: {error}"))?,
            "args-json" => serde_json::from_str::<Value>(value)
                .map_err(|error| format!("--args-json must be valid JSON: {error}"))?,
            "options-json" => serde_json::from_str::<Value>(value)
                .map_err(|error| format!("--options-json must be valid JSON: {error}"))?,
            "transfer-ids" => Value::Array(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(|item| Value::String(item.to_string()))
                    .collect(),
            ),
            "recursive" => Value::Bool(parse_cli_bool("recursive", value)?),
            "save-sudo-password" | "save-su-password" => Value::Bool(parse_cli_bool(key, value)?),
            "limit" | "offset" | "timeout-ms" => json!(parse_cli_usize(key, value)?),
            _ => Value::String(value.clone()),
        };
        params.insert(parameter, converted);
    }
    Ok(Value::Object(params))
}

fn parse_cli_bool(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("Option --{key} must be true or false")),
    }
}

fn parse_cli_options(
    arguments: &[String],
    allowed: &[&str],
) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        let key = argument
            .strip_prefix("--")
            .filter(|key| !key.is_empty())
            .ok_or_else(|| format!("Expected a long option, got {argument}"))?;
        if !allowed.contains(&key) {
            return Err(format!("Unknown option --{key}"));
        }
        if values.contains_key(key) {
            return Err(format!("Option --{key} may only be provided once"));
        }
        let value = arguments
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("Option --{key} requires a value"))?;
        if value.is_empty() {
            return Err(format!("Option --{key} must not be empty"));
        }
        values.insert(key.to_string(), value.clone());
        index += 2;
    }
    Ok(values)
}

fn parse_cli_usize(key: &str, value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("Option --{key} must be a non-negative integer"))
}

fn print_cli_result(result: Value) -> Result<(), String> {
    let output = serde_json::to_string_pretty(&result)
        .map_err(|error| format!("Unable to encode FileTerm CLI response: {error}"))?;
    println!("{output}");
    Ok(())
}

fn print_cli_help() {
    println!(
        "FileTerm CLI {}\n\nUsage:\n  fileterm connections [--limit N] [--offset N]\n  fileterm sessions [--profile-id PROFILE_ID]\n  fileterm directory --tab-id TAB_ID [--path REMOTE_PATH] [--limit N] [--offset N]\n  fileterm read --tab-id TAB_ID --path REMOTE_PATH [--encoding utf-8]\n  fileterm exec --tab-id TAB_ID --command COMMAND [--cwd PATH] [--timeout-ms N]\n  fileterm write --tab-id TAB_ID --path REMOTE_PATH --content TEXT\n  fileterm upload --tab-id TAB_ID --local-path PATH --remote-directory PATH\n  fileterm download --tab-id TAB_ID --remote-path REMOTE_PATH --local-directory PATH\n  fileterm transfers [--limit N] [--offset N]\n  fileterm wait-transfer --transfer-id ID [--timeout-ms N]\n  fileterm mkdir|touch|copy|move|rename|delete|chmod|access ...\n  fileterm tunnels|create-tunnel|start-tunnel|stop-tunnel|delete-tunnel ...\n  fileterm call ACTION --params-json JSON\n  fileterm mcp\n\n`exec` uses an independent non-interactive SSH channel and never writes the visible terminal transcript. If a command needs generic input such as MFA, a confirmation, or a REPL answer, it returns REMOTE_INTERACTIVE_INPUT_REQUIRED; finish that operation in the visible SSH terminal and retry. Sudo/su credentials use explicit trusted parameters, encrypted profiles, or the FileTerm main-window secure prompt. CLI operations are explicit user-invoked JSON commands and require a running FileTerm desktop app. MCP mutation tools use the in-app approval dialog.\nUse `fileterm cli <command>` as an equivalent spelling.",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_cli_command_help(command: &str) {
    match command {
        "connections" => println!("Usage: fileterm connections [--limit N] [--offset N]"),
        "sessions" => println!("Usage: fileterm sessions [--profile-id PROFILE_ID]"),
        "directory" => println!(
            "Usage: fileterm directory --tab-id TAB_ID [--path REMOTE_PATH] [--limit N] [--offset N]\n       fileterm ls --tab-id TAB_ID [--path REMOTE_PATH] [--limit N] [--offset N]"
        ),
        "read_remote_file" => println!("Usage: fileterm read --tab-id TAB_ID --path REMOTE_PATH [--encoding utf-8]"),
        "execute_remote_command" => println!("Usage: fileterm exec --tab-id TAB_ID --command COMMAND [--cwd PATH] [--timeout-ms N] [--sudo-password PASSWORD --save-sudo-password true] [--su-password PASSWORD --save-su-password true]"),
        "wait_for_transfer" => println!("Usage: fileterm wait-transfer --transfer-id ID [--timeout-ms N]"),
        "write_remote_file" => println!("Usage: fileterm write --tab-id TAB_ID --path REMOTE_PATH --content TEXT [--encoding utf-8]"),
        "upload_file" => println!("Usage: fileterm upload --tab-id TAB_ID --local-path PATH --remote-directory PATH [--target-name NAME]"),
        "download_file" => println!("Usage: fileterm download --tab-id TAB_ID --remote-path PATH --local-directory PATH [--target-name NAME]"),
        "download_remote_directory" => println!("Usage: fileterm download-directory --tab-id TAB_ID --remote-path PATH --local-directory PATH [--target-name NAME]"),
        "clear_transfers" => println!("Usage: fileterm clear-transfers --transfer-ids ID1,ID2"),
        "create_ssh_tunnel" => println!("Usage: fileterm create-tunnel --tab-id TAB_ID --rule-json JSON"),
        "call" => println!("Usage: fileterm call ACTION --params-json JSON"),
        _ => print_cli_help(),
    }
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
        "instructions": "Use FileTerm tools to inspect or operate already-saved and already-open connections. Credentials and terminal transcripts are never returned. MCP writes, remote commands, transfers, tunnels, and session state changes always pause for explicit approval in the FileTerm window and time out closed. Use fileterm_execute_remote_command for bounded non-interactive commands; saved sudo/su credentials are consumed through SSH stdin without entering command text. If no saved credential or local prompt is available, an Agent may ask the user for a sudo/su password and pass that explicit one-shot value in the matching tool field; never put it in the command text or repeat it in a result. If a command needs MFA, confirmation, an installer prompt, passwd, SSH authentication, or another generic interactive input, the tool returns REMOTE_INTERACTIVE_INPUT_REQUIRED; tell the user to finish it in the visible SSH terminal and retry. 中文规则：普通后台 exec 不接管通用交互输入；sudo/su 可使用用户明确提供的一次性密码、加密 profile 或 FileTerm 主窗口安全输入；危险密码不要写入命令文本或工具结果。"
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
    let action = name
        .strip_prefix("fileterm_")
        .ok_or_else(|| "Unknown FileTerm tool".to_string())?;
    let request = BridgeRequest {
        action: action.to_string(),
        params: arguments,
        requires_approval: true,
    };
    match call_desktop_bridge(request) {
        Ok(result) => Ok(tool_result(result, false)),
        Err(error) => Ok(tool_error_result(error)),
    }
}

fn validate_tool_arguments(name: &str, arguments: &Value) -> Result<(), String> {
    let allowed: &[&str] = match name {
        "fileterm_list_connections"
        | "fileterm_list_transfers"
        | "fileterm_get_command_templates" => &["limit", "offset"],
        "fileterm_wait_for_transfer" => &["transfer_id", "timeout_ms"],
        "fileterm_get_session_context" => &["profile_id"],
        "fileterm_list_remote_directory" => &["tab_id", "path", "limit", "offset"],
        "fileterm_read_remote_file" => &["tab_id", "path", "encoding"],
        "fileterm_list_ssh_tunnels"
        | "fileterm_activate_session"
        | "fileterm_reconnect_session"
        | "fileterm_disconnect_session"
        | "fileterm_close_session" => &["tab_id"],
        "fileterm_open_connection" => &["profile_id"],
        "fileterm_execute_remote_command" => &[
            "tab_id",
            "command",
            "cwd",
            "timeout_ms",
            "sudo_password",
            "su_password",
            "save_sudo_password",
            "save_su_password",
        ],
        "fileterm_execute_command_template" => &["tab_id", "command_id", "args", "options"],
        "fileterm_write_remote_file" => &["tab_id", "path", "content", "encoding"],
        "fileterm_create_remote_directory" | "fileterm_create_remote_file" => {
            &["tab_id", "parent_path", "name"]
        }
        "fileterm_copy_remote_path" => {
            &["tab_id", "target_path", "destination_path", "target_type"]
        }
        "fileterm_move_remote_path" => &["tab_id", "target_path", "destination_path"],
        "fileterm_rename_remote_path" => &["tab_id", "target_path", "new_name"],
        "fileterm_delete_remote_path" => &["tab_id", "target_path", "target_type"],
        "fileterm_change_remote_permissions" => {
            &["tab_id", "path", "mode", "recursive", "apply_to"]
        }
        "fileterm_set_remote_file_access_mode" => &["tab_id", "mode"],
        "fileterm_upload_file" => &["tab_id", "local_path", "remote_directory", "target_name"],
        "fileterm_download_file" | "fileterm_download_remote_directory" => {
            &["tab_id", "remote_path", "local_directory", "target_name"]
        }
        "fileterm_pause_transfer" | "fileterm_resume_transfer" | "fileterm_discard_transfer" => {
            &["transfer_id"]
        }
        "fileterm_clear_transfers" => &["transfer_ids"],
        "fileterm_create_ssh_tunnel" => &["tab_id", "rule"],
        "fileterm_start_ssh_tunnel" | "fileterm_stop_ssh_tunnel" | "fileterm_delete_ssh_tunnel" => {
            &["tab_id", "rule_id"]
        }
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

fn tool_error_result(error: String) -> Value {
    let code = mcp_error_code(&error);
    tool_result(
        json!({
            "error": {
                "code": code,
                "message": error,
                "retryable": mcp_error_is_retryable(code),
            }
        }),
        true,
    )
}

/// The desktop service still owns detailed error text, but MCP consumers need
/// a compact stable decision key. Preserve explicitly emitted runtime codes
/// first and classify the common local bridge failures without exposing
/// credentials, paths, prompts, or terminal content.
fn mcp_error_code(error: &str) -> &'static str {
    let upper = error.to_ascii_uppercase();
    for code in [
        MCP_POLICY_READ_ONLY,
        MCP_SCOPE_DENIED,
        "REMOTE_INTERACTIVE_INPUT_REQUIRED",
        MCP_TRANSFER_NOT_FOUND,
    ] {
        if upper.contains(code) {
            return code;
        }
    }
    if upper.contains("APP IS NOT RUNNING") || upper.contains("DESKTOP APP IS UNAVAILABLE") {
        "FILETERM_APP_UNAVAILABLE"
    } else if upper.contains("BRIDGE IS BUSY") {
        "FILETERM_BRIDGE_BUSY"
    } else if upper.contains("TIMED OUT") {
        "FILETERM_REQUEST_TIMEOUT"
    } else if upper.contains("REJECTED BY THE USER") || upper.contains("APPROVAL") {
        "FILETERM_OPERATION_REJECTED"
    } else if upper.contains("SESSION") && upper.contains("NOT FOUND") {
        "FILETERM_SESSION_NOT_FOUND"
    } else if upper.contains("NOT CONNECTED") {
        "FILETERM_SESSION_DISCONNECTED"
    } else {
        "FILETERM_OPERATION_FAILED"
    }
}

fn mcp_error_is_retryable(code: &str) -> bool {
    matches!(
        code,
        "FILETERM_APP_UNAVAILABLE"
            | "FILETERM_BRIDGE_BUSY"
            | "FILETERM_REQUEST_TIMEOUT"
            | "FILETERM_SESSION_DISCONNECTED"
            | "REMOTE_INTERACTIVE_INPUT_REQUIRED"
    )
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool_definition("fileterm_list_connections", "List FileTerm connections", "List saved profiles without credentials.", json!({
            "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_PAGE_SIZE },
            "offset": { "type": "integer", "minimum": 0 }
        }), &[], true, false, true, false),
        tool_definition("fileterm_get_session_context", "Get FileTerm session context", "List open sessions with status, paths and capabilities. Credentials and terminal transcripts are never returned.", json!({
            "profile_id": { "type": "string" }
        }), &[], true, false, true, false),
        tool_definition("fileterm_get_command_templates", "List command templates", "List saved FileTerm command templates that can be executed with explicit approval.", json!({
            "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_PAGE_SIZE },
            "offset": { "type": "integer", "minimum": 0 }
        }), &[], true, false, true, false),
        tool_definition("fileterm_list_remote_directory", "List a remote directory", "List entries through an already-open file-capable session. Results are paginated.", json!({
            "tab_id": { "type": "string" },
            "path": { "type": "string" },
            "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_PAGE_SIZE },
            "offset": { "type": "integer", "minimum": 0 }
        }), &["tab_id"], true, false, true, true),
        tool_definition("fileterm_read_remote_file", "Read a remote file", "Read text from an already-open remote session. Large output is bounded and marked truncated.", json!({
            "tab_id": { "type": "string" },
            "path": { "type": "string" },
            "encoding": { "type": "string", "default": "utf-8" }
        }), &["tab_id", "path"], true, false, true, true),
        tool_definition("fileterm_list_transfers", "List transfer tasks", "List FileTerm upload/download tasks and their current status.", json!({
            "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_PAGE_SIZE },
            "offset": { "type": "integer", "minimum": 0 }
        }), &[], true, false, true, false),
        tool_definition("fileterm_wait_for_transfer", "Wait for a FileTerm transfer", "Wait locally for a transfer to reach a terminal state and return its latest task snapshot. A timed-out wait returns the latest still-running task; it does not cancel the transfer.", json!({
            "transfer_id": { "type": "string" },
            "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": MCP_TRANSFER_WAIT_MAX_MS, "default": MCP_TRANSFER_WAIT_DEFAULT_MS }
        }), &["transfer_id"], true, false, true, false),
        tool_definition("fileterm_list_ssh_tunnels", "List SSH tunnels", "List tunnels attached to an open SSH session.", json!({ "tab_id": { "type": "string" } }), &["tab_id"], true, false, true, false),
        tool_definition("fileterm_open_connection", "Open a FileTerm connection", "Open a saved profile in a new FileTerm session. The user must approve the connection attempt.", json!({ "profile_id": { "type": "string" } }), &["profile_id"], false, false, false, true),
        tool_definition("fileterm_activate_session", "Activate a FileTerm session", "Make an existing session the active workspace session.", json!({ "tab_id": { "type": "string" } }), &["tab_id"], false, false, true, false),
        tool_definition("fileterm_reconnect_session", "Reconnect a FileTerm session", "Reconnect an existing session after user approval.", json!({ "tab_id": { "type": "string" } }), &["tab_id"], false, false, false, true),
        tool_definition("fileterm_disconnect_session", "Disconnect a FileTerm session", "Disconnect an open session after user approval.", json!({ "tab_id": { "type": "string" } }), &["tab_id"], false, false, true, false),
        tool_definition("fileterm_close_session", "Close a FileTerm session", "Close a workspace tab after user approval.", json!({ "tab_id": { "type": "string" } }), &["tab_id"], false, true, true, false),
        tool_definition("fileterm_execute_remote_command", "Execute a remote command", "Run a bounded command on an open SSH session through a dedicated exec channel; the visible terminal is not hijacked. Ordinary commands remain non-interactive. A command whose first token is sudo or su may use a saved profile credential through SSH stdin without exposing it to the command text. If no safe credential is available, the Agent may ask the user for the matching sudo_password or su_password and retry with that explicit one-shot value; save_* is honored only together with an explicitly supplied value. If no credential is provided, FileTerm returns SUDO_PASSWORD_NEEDED or SU_PASSWORD_NEEDED. If a normal command reports inputRequired=true, it returns REMOTE_INTERACTIVE_INPUT_REQUIRED; finish the operation in the visible SSH terminal and retry.", json!({
            "tab_id": { "type": "string" },
            "command": { "type": "string" },
            "cwd": { "type": "string" },
            "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 120000 },
            "sudo_password": { "type": "string", "description": "One-shot sudo password explicitly provided by the user after SUDO_PASSWORD_NEEDED." },
            "su_password": { "type": "string", "description": "One-shot su password explicitly provided by the user after SU_PASSWORD_NEEDED." },
            "save_sudo_password": { "type": "boolean", "description": "Persist the explicitly supplied sudo_password in the encrypted profile store after a non-authentication-failure run." },
            "save_su_password": { "type": "boolean", "description": "Persist the explicitly supplied su_password in the encrypted profile store after a non-authentication-failure run." }
        }), &["tab_id", "command"], false, false, false, true),
        tool_definition("fileterm_execute_command_template", "Execute a command template", "Execute a saved FileTerm command template with optional positional arguments after approval.", json!({
            "tab_id": { "type": "string" },
            "command_id": { "type": "string" },
            "args": { "type": "array", "items": { "type": "string" } },
            "options": { "type": "object", "properties": { "appendCarriageReturn": { "type": "boolean" } }, "additionalProperties": false }
        }), &["tab_id", "command_id"], false, false, false, true),
        tool_definition("fileterm_write_remote_file", "Write a remote file", "Write text to a remote file after showing the target and content preview for approval.", json!({
            "tab_id": { "type": "string" }, "path": { "type": "string" }, "content": { "type": "string" }, "encoding": { "type": "string" }
        }), &["tab_id", "path", "content"], false, true, false, true),
        tool_definition("fileterm_create_remote_directory", "Create a remote directory", "Create a remote directory after approval.", json!({ "tab_id": { "type": "string" }, "parent_path": { "type": "string" }, "name": { "type": "string" } }), &["tab_id", "parent_path", "name"], false, true, false, true),
        tool_definition("fileterm_create_remote_file", "Create a remote file", "Create an empty remote file after approval.", json!({ "tab_id": { "type": "string" }, "parent_path": { "type": "string" }, "name": { "type": "string" } }), &["tab_id", "parent_path", "name"], false, true, false, true),
        tool_definition("fileterm_copy_remote_path", "Copy a remote path", "Copy a remote file or directory after approval.", json!({ "tab_id": { "type": "string" }, "target_path": { "type": "string" }, "destination_path": { "type": "string" }, "target_type": { "type": "string", "enum": ["file", "folder"] } }), &["tab_id", "target_path", "destination_path", "target_type"], false, true, false, true),
        tool_definition("fileterm_move_remote_path", "Move a remote path", "Move a remote file or directory after approval.", json!({ "tab_id": { "type": "string" }, "target_path": { "type": "string" }, "destination_path": { "type": "string" } }), &["tab_id", "target_path", "destination_path"], false, true, false, true),
        tool_definition("fileterm_rename_remote_path", "Rename a remote path", "Rename a remote file or directory after approval.", json!({ "tab_id": { "type": "string" }, "target_path": { "type": "string" }, "new_name": { "type": "string" } }), &["tab_id", "target_path", "new_name"], false, true, false, true),
        tool_definition("fileterm_delete_remote_path", "Delete a remote path", "Delete a remote file or directory after approval.", json!({ "tab_id": { "type": "string" }, "target_path": { "type": "string" }, "target_type": { "type": "string", "enum": ["file", "folder"] } }), &["tab_id", "target_path", "target_type"], false, true, false, true),
        tool_definition("fileterm_change_remote_permissions", "Change remote permissions", "Change remote mode bits after approval.", json!({ "tab_id": { "type": "string" }, "path": { "type": "string" }, "mode": { "type": "string", "pattern": "^[0-7]{3,4}$" }, "recursive": { "type": "boolean" }, "apply_to": { "type": "string", "enum": ["all", "files", "directories"] } }), &["tab_id", "path", "mode"], false, true, true, true),
        tool_definition("fileterm_set_remote_file_access_mode", "Set remote file access mode", "Switch the existing session's file view between user and root mode. Root credentials are never accepted from MCP; FileTerm must already have reusable authorization or the operation fails.", json!({ "tab_id": { "type": "string" }, "mode": { "type": "string", "enum": ["user", "root"] } }), &["tab_id", "mode"], false, true, true, true),
        tool_definition("fileterm_upload_file", "Upload a local file", "Queue a resumable upload through FileTerm's transfer service after approval.", json!({ "tab_id": { "type": "string" }, "local_path": { "type": "string" }, "remote_directory": { "type": "string" }, "target_name": { "type": "string" } }), &["tab_id", "local_path", "remote_directory"], false, false, false, true),
        tool_definition("fileterm_download_file", "Download a remote file", "Queue a resumable download through FileTerm's transfer service after approval.", json!({ "tab_id": { "type": "string" }, "remote_path": { "type": "string" }, "local_directory": { "type": "string" }, "target_name": { "type": "string" } }), &["tab_id", "remote_path", "local_directory"], false, false, false, true),
        tool_definition("fileterm_download_remote_directory", "Download a remote directory", "Queue a resumable directory download through FileTerm's transfer service after approval.", json!({ "tab_id": { "type": "string" }, "remote_path": { "type": "string" }, "local_directory": { "type": "string" }, "target_name": { "type": "string" } }), &["tab_id", "remote_path", "local_directory"], false, false, false, true),
        tool_definition("fileterm_pause_transfer", "Pause a transfer", "Pause a FileTerm transfer and preserve its resumable checkpoint after approval.", json!({ "transfer_id": { "type": "string" } }), &["transfer_id"], false, false, true, false),
        tool_definition("fileterm_resume_transfer", "Resume a transfer", "Resume a paused FileTerm transfer after approval.", json!({ "transfer_id": { "type": "string" } }), &["transfer_id"], false, false, true, true),
        tool_definition("fileterm_discard_transfer", "Discard a transfer", "Discard a transfer and its checkpoint after approval.", json!({ "transfer_id": { "type": "string" } }), &["transfer_id"], false, true, true, false),
        tool_definition("fileterm_clear_transfers", "Clear transfer history", "Clear selected transfer history after approval.", json!({ "transfer_ids": { "type": "array", "items": { "type": "string" } } }), &["transfer_ids"], false, true, true, false),
        tool_definition("fileterm_create_ssh_tunnel", "Create an SSH tunnel", "Create a tunnel rule on an open SSH session after approval.", json!({ "tab_id": { "type": "string" }, "rule": { "type": "object" } }), &["tab_id", "rule"], false, false, true, true),
        tool_definition("fileterm_start_ssh_tunnel", "Start an SSH tunnel", "Start a configured SSH tunnel after approval.", json!({ "tab_id": { "type": "string" }, "rule_id": { "type": "string" } }), &["tab_id", "rule_id"], false, false, true, true),
        tool_definition("fileterm_stop_ssh_tunnel", "Stop an SSH tunnel", "Stop a running SSH tunnel after approval.", json!({ "tab_id": { "type": "string" }, "rule_id": { "type": "string" } }), &["tab_id", "rule_id"], false, false, true, false),
        tool_definition("fileterm_delete_ssh_tunnel", "Delete an SSH tunnel", "Delete an SSH tunnel rule after approval.", json!({ "tab_id": { "type": "string" }, "rule_id": { "type": "string" } }), &["tab_id", "rule_id"], false, true, true, false),
    ]
}

#[allow(clippy::too_many_arguments)] // Tool metadata stays explicit at each call site.
fn tool_definition(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
    destructive: bool,
    idempotent: bool,
    open_world: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        },
        "outputSchema": tool_output_schema(name),
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": idempotent,
            "openWorldHint": open_world
        }
    })
}

fn tool_output_schema(name: &str) -> Value {
    match name {
        "fileterm_execute_remote_command" => {
            json!({
                "type": "object",
                "properties": {
                    "tabId": { "type": "string" },
                    "result": {
                        "type": "object",
                        "properties": {
                            "output": { "type": "string" },
                            "exitCode": { "type": ["integer", "null"], "minimum": 0 },
                            "timedOut": { "type": "boolean" },
                            "outputTruncated": { "type": "boolean" },
                            "inputRequired": { "type": "boolean" },
                            "inputKind": { "type": "string", "enum": ["secret", "text"] }
                        },
                        "required": ["output", "exitCode", "timedOut", "outputTruncated", "inputRequired"],
                        "additionalProperties": false
                    }
                },
                "required": ["tabId", "result"],
                "additionalProperties": false
            })
        }
        "fileterm_wait_for_transfer" => json!({
            "type": "object",
            "properties": {
                "transferId": { "type": "string" },
                "transfer": { "type": "object" },
                "timedOut": { "type": "boolean" }
            },
            "required": ["transferId", "transfer", "timedOut"],
            "additionalProperties": false
        }),
        "fileterm_list_connections"
        | "fileterm_get_command_templates"
        | "fileterm_list_remote_directory"
        | "fileterm_list_transfers" => json!({
            "type": "object",
            "properties": {
                "total": { "type": "integer", "minimum": 0 },
                "count": { "type": "integer", "minimum": 0 },
                "offset": { "type": "integer", "minimum": 0 },
                "items": { "type": "array" },
                "hasMore": { "type": "boolean" },
                "nextOffset": { "type": ["integer", "null"], "minimum": 0 }
            },
            "required": ["total", "count", "offset", "items", "hasMore", "nextOffset"],
            "additionalProperties": true
        }),
        "fileterm_read_remote_file" => json!({
            "type": "object",
            "properties": {
                "tabId": { "type": "string" },
                "path": { "type": "string" },
                "encoding": { "type": "string" },
                "content": { "type": "string" },
                "truncated": { "type": "boolean" }
            },
            "required": ["tabId", "path", "encoding", "content", "truncated"],
            "additionalProperties": false
        }),
        _ => json!({ "type": "object" }),
    }
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

    let request_timeout = MCP_CLIENT_TIMEOUT;
    let mut stream = StdTcpStream::connect_timeout(&address, MCP_BRIDGE_TIMEOUT).map_err(|_| {
        "FileTerm desktop app is unavailable. Open or restart FileTerm, then retry this MCP tool.".to_string()
    })?;
    stream
        .set_read_timeout(Some(request_timeout))
        .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    stream
        .set_write_timeout(Some(request_timeout))
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
        handle_jsonrpc_request, initialize_result, optional_string, pagination, tool_definitions,
        tool_error_result, validate_tool_arguments, MCP_JSONRPC_PROTOCOL_VERSION,
    };
    use serde_json::json;

    #[test]
    fn tools_are_prefixed_and_have_strict_schemas() {
        for tool in tool_definitions() {
            assert!(tool["name"].as_str().unwrap().starts_with("fileterm_"));
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
            assert_eq!(tool["outputSchema"]["type"], "object");
        }
        let read_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_read_remote_file")
            .unwrap();
        assert_eq!(read_tool["annotations"]["readOnlyHint"], true);
        let write_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_write_remote_file")
            .unwrap();
        assert_eq!(write_tool["annotations"]["readOnlyHint"], false);
        assert_eq!(write_tool["annotations"]["destructiveHint"], true);
        let remote_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_execute_remote_command")
            .unwrap();
        assert!(remote_tool["description"]
            .as_str()
            .unwrap()
            .contains("REMOTE_INTERACTIVE_INPUT_REQUIRED"));
        assert!(
            remote_tool["outputSchema"]["properties"]["result"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "inputRequired")
        );
        assert_eq!(
            remote_tool["outputSchema"]["properties"]["result"]["properties"]["inputKind"],
            json!({ "type": "string", "enum": ["secret", "text"] })
        );

        let transfer_wait_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_wait_for_transfer")
            .unwrap();
        assert_eq!(
            transfer_wait_tool["inputSchema"]["required"],
            json!(["transfer_id"])
        );
        assert_eq!(
            transfer_wait_tool["outputSchema"]["required"],
            json!(["transferId", "transfer", "timedOut"])
        );
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
        assert!(response["result"]["tools"].as_array().unwrap().len() >= 20);
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
    fn initialize_instructions_describe_credential_and_generic_input_paths() {
        let result = initialize_result(&json!({})).expect("initialize result should be valid");
        let instructions = result["instructions"].as_str().unwrap();
        assert!(instructions.contains("REMOTE_INTERACTIVE_INPUT_REQUIRED"));
        assert!(instructions.contains("visible SSH terminal"));
        assert!(instructions.contains("ask the user"));
        assert!(instructions.contains("sudo/su"));
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
        assert!(validate_tool_arguments(
            "fileterm_execute_unsupported_legacy_tool",
            &json!({ "command": "sudo id" })
        )
        .is_err());
        assert!(validate_tool_arguments(
            "fileterm_wait_for_transfer",
            &json!({
                "transfer_id": "transfer-1",
                "timeout_ms": 30_000
            })
        )
        .is_ok());
    }

    #[test]
    fn tool_errors_include_stable_codes_and_retry_semantics() {
        let unavailable = tool_error_result(
            "REMOTE_INTERACTIVE_INPUT_REQUIRED: finish the operation in the visible SSH terminal"
                .to_string(),
        );
        assert_eq!(
            unavailable["structuredContent"]["error"]["code"],
            "REMOTE_INTERACTIVE_INPUT_REQUIRED"
        );
        assert_eq!(unavailable["structuredContent"]["error"]["retryable"], true);

        let rejected = tool_error_result("MCP operation was rejected by the user".to_string());
        assert_eq!(
            rejected["structuredContent"]["error"]["code"],
            "FILETERM_OPERATION_REJECTED"
        );
        assert_eq!(rejected["structuredContent"]["error"]["retryable"], false);
    }
}
