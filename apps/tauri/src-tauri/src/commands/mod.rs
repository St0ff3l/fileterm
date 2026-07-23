use crate::sessions::WorkerCmd;
use crate::storage::read_json_array;
use crate::AppError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, Emitter, Manager, WebviewWindow};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// 等待 worker 接收命令的最大时间。worker 主循环被 SFTP init / shell
/// channel 写阻塞 时，mpsc 一旦满，send 会永久 await，导致前端 invoke
/// 链路整体卡死（多窗口发送后续 tab 全部排队、Cmd+Q 退出无法完成）。
/// 超时后返回显式 busy 错误，绝不静默吞掉输入。SSH 终端输入已经走
/// 独立 channel；这里仍作为 Telnet / Serial 和通用 worker 命令的保护。
const WORKER_CMD_SEND_TIMEOUT: Duration = Duration::from_millis(500);

/// 文件/会话级操作（list/read/write/重连等）容忍更长延迟，但同样不能
/// 永久阻塞——一旦 worker 卡死，应当让前端拿到明确错误。
const WORKER_FILE_CMD_SEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Worker 已接收命令后也必须在有限时间内答复。之前仅限制了 mpsc send，
/// 但某个后台 SFTP/exec task 丢失 reply 时，oneshot 会一直 await，导致
/// 删除、打开目录和 Root 弹窗永久 loading。
const WORKER_FILE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// 退出时给 worker 的 Disconnect 命令留 1 秒，超时直接放弃发送：worker
/// 主循环卡死时 channel 满，send 不进去；强行 await 会让 Cmd+Q 整个
/// 退出链路 hang 住，用户只能强制杀进程。drop sender 后 worker 的
/// `cmd_rx.recv()` 会返回 None，自然走清理路径。
const WORKER_DISCONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// Let a child-window close command resolve its IPC callback before destroying
/// the calling WebView. Destroying synchronously makes WebView2 report a
/// missing callback id and can leave renderer cleanup half-finished.
const CHILD_WINDOW_DESTROY_DELAY: Duration = Duration::from_millis(25);

async fn send_terminal_input(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    data: String,
) -> Result<(), AppError> {
    if let Some(sender) = state.terminal_inputs.read().await.get(tab_id).cloned() {
        return sender
            .send(data)
            .map_err(|_| AppError::Storage("Terminal session closed".to_string()));
    }

    // Telnet and serial still use their protocol worker queue. SSH owns the
    // dedicated low-latency input channel above.
    let sender = state
        .workers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .ok_or_else(|| AppError::Storage("Terminal session not found".to_string()))?;
    timeout(
        WORKER_CMD_SEND_TIMEOUT,
        sender.send(WorkerCmd::WriteTerminal(data)),
    )
    .await
    .map_err(|_| AppError::Storage("Terminal worker busy".to_string()))?
    .map_err(|error| AppError::Storage(error.to_string()))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UiPreferences {
    pub theme: String,
    pub locale: String,
}

#[derive(Deserialize, Debug)]
pub struct UiPreferencesInput {
    pub theme: Option<String>,
    pub locale: Option<String>,
}

const DEFAULT_UI_THEME: &str = "default-dark";
const DEFAULT_UI_LOCALE: &str = "zhCN";

fn normalize_ui_preferences(mut preferences: UiPreferences) -> UiPreferences {
    if !matches!(preferences.theme.as_str(), "default-dark" | "default-light") {
        preferences.theme = DEFAULT_UI_THEME.to_string();
    }
    if !matches!(preferences.locale.as_str(), "zhCN" | "enUS") {
        preferences.locale = DEFAULT_UI_LOCALE.to_string();
    }
    preferences
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandHistoryEntry {
    pub command: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSendPreferences {
    pub remember_selection: bool,
    pub send_scope: String,
    pub selected_tab_ids: Vec<String>,
}

impl Default for CommandSendPreferences {
    fn default() -> Self {
        Self {
            remember_selection: false,
            send_scope: "current".to_string(),
            selected_tab_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSshKeyInput {
    pub source_path: Option<String>,
    pub note: Option<String>,
}

fn write_json_object(app: &AppHandle, name: &str, value: &Value) -> Result<(), AppError> {
    let path = crate::storage::workspace_file(app, name)?;
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    std::fs::write(&temporary, content).map_err(|error| AppError::Storage(error.to_string()))?;
    crate::storage::replace_file_atomically(&temporary, &path)
}

#[tauri::command]
pub fn app_get_platform() -> String {
    std::env::consts::OS.to_string()
}

fn canonical_arch(arch: &str) -> String {
    match arch {
        "aarch64" => "arm64".to_string(),
        "x86_64" => "x64".to_string(),
        other => other.to_string(),
    }
}

fn resolve_native_arch(platform: &str, process_arch: &str, macos_arm64_capable: bool) -> String {
    if platform == "macos" && macos_arm64_capable {
        return "arm64".to_string();
    }

    canonical_arch(process_arch)
}

#[cfg(target_os = "macos")]
fn macos_arm64_capable() -> bool {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.optional.arm64"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|value| value.trim() == "1")
}

#[cfg(not(target_os = "macos"))]
fn macos_arm64_capable() -> bool {
    false
}

#[tauri::command]
pub fn app_get_arch() -> String {
    resolve_native_arch(
        std::env::consts::OS,
        std::env::consts::ARCH,
        macos_arm64_capable(),
    )
}

#[tauri::command]
pub fn app_get_runtime_version() -> String {
    tauri::VERSION.to_string()
}

#[tauri::command]
pub fn app_read_clipboard_text() -> Result<String, AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| AppError::Clipboard(error.to_string()))?;
    clipboard
        .get_text()
        .map_err(|error| AppError::Clipboard(error.to_string()))
}

#[tauri::command]
pub fn app_write_clipboard_text(text: String) -> Result<(), AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| AppError::Clipboard(error.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|error| AppError::Clipboard(error.to_string()))
}

#[tauri::command]
pub fn app_open_external_url(url: String) -> Result<(), AppError> {
    let parsed = validate_external_url(&url)?;
    open::that(parsed.as_str()).map_err(|error| AppError::Command(error.to_string()))
}

fn validate_external_url(url: &str) -> Result<url::Url, AppError> {
    let parsed = url::Url::parse(url)
        .map_err(|error| AppError::Command(format!("外部链接无效: {error}")))?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(parsed)
    } else {
        Err(AppError::Command(
            "仅允许打开 http 或 https 外部链接".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn app_get_update_status(app: AppHandle) -> Result<serde_json::Value, AppError> {
    Ok(crate::services::updates::get_status(&app).await)
}

#[tauri::command]
pub async fn app_check_for_updates(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::updates::check(&app).await
}

#[tauri::command]
pub async fn app_download_update(app: AppHandle) -> Result<(), AppError> {
    crate::services::updates::download(&app).await
}

#[tauri::command]
pub async fn app_install_update(app: AppHandle) -> Result<(), AppError> {
    crate::services::updates::install(&app).await
}

#[tauri::command]
pub fn app_open_logs_directory(app: AppHandle) -> Result<(), AppError> {
    let log_directory = crate::storage::state_path(&app)?.with_file_name("logs");
    std::fs::create_dir_all(&log_directory)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    open::that(log_directory).map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub fn app_get_ui_preferences(app: AppHandle) -> Result<UiPreferences, AppError> {
    let path = crate::storage::state_path(&app)?;
    if path.exists() {
        let content =
            std::fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
        let preferences: UiPreferences = serde_json::from_str(&content)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        Ok(normalize_ui_preferences(preferences))
    } else {
        Ok(UiPreferences {
            theme: DEFAULT_UI_THEME.to_string(),
            locale: DEFAULT_UI_LOCALE.to_string(),
        })
    }
}

#[tauri::command]
pub fn app_set_ui_preferences(
    app: AppHandle,
    input: UiPreferencesInput,
) -> Result<UiPreferences, AppError> {
    let path = crate::storage::state_path(&app)?;
    let mut preferences = app_get_ui_preferences(app.clone())?;
    if let Some(theme) = input.theme {
        preferences.theme = theme;
    }
    if let Some(locale) = input.locale {
        preferences.locale = locale;
    }
    let preferences = normalize_ui_preferences(preferences);
    let content = serde_json::to_string_pretty(&preferences)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    std::fs::write(path, content).map_err(|error| AppError::Storage(error.to_string()))?;
    if let Err(error) =
        crate::install_localized_application_menu(&app, preferences.locale == "enUS")
    {
        // Preferences are already durable at this point. Do not report the
        // whole save as failed (and invite a duplicate retry) merely because
        // native menu refresh failed on the current platform.
        crate::services::logging::warn(
            &app,
            "ui-preferences",
            format!("failed to refresh native menu: {error}"),
        );
    }
    if let Err(error) = crate::install_localized_tray_menu(&app, preferences.locale == "enUS") {
        crate::services::logging::warn(
            &app,
            "ui-preferences",
            format!("failed to refresh tray menu: {error}"),
        );
    }
    let _ = app.emit("app:ui-preferences-changed", &preferences);
    Ok(preferences)
}

fn normalize_ui_state(value: Value) -> Result<serde_json::Map<String, Value>, AppError> {
    match value {
        Value::Object(mut object) => {
            let mut states = object
                .remove("values")
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            object.remove("version");
            states.extend(object);
            Ok(states)
        }
        Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|item| {
                let key = item.get("key")?.as_str()?.to_string();
                let value = item.get("value")?.clone();
                Some((key, value))
            })
            .collect()),
        _ => Err(AppError::Serialization("UI 状态文件格式无效".to_string())),
    }
}

fn read_ui_state(app: &AppHandle) -> Result<serde_json::Map<String, Value>, AppError> {
    normalize_ui_state(crate::storage::read_json_object(app, "ui-state.json")?)
}

fn write_ui_state(app: &AppHandle, states: serde_json::Map<String, Value>) -> Result<(), AppError> {
    write_json_object(app, "ui-state.json", &Value::Object(states))
}

#[tauri::command]
pub fn app_get_ui_state_item(app: AppHandle, key: String) -> Result<Option<String>, AppError> {
    Ok(read_ui_state(&app)?
        .get(&key)
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

#[tauri::command]
pub fn app_set_ui_state_item(app: AppHandle, key: String, value: String) -> Result<(), AppError> {
    let mut states = read_ui_state(&app)?;
    states.insert(key, Value::String(value));
    write_ui_state(&app, states)
}

#[tauri::command]
pub fn app_remove_ui_state_item(app: AppHandle, key: String) -> Result<(), AppError> {
    let mut states = read_ui_state(&app)?;
    states.remove(&key);
    write_ui_state(&app, states)
}

#[tauri::command]
pub fn app_get_terminal_command_history(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<TerminalCommandHistoryEntry>, AppError> {
    let value = crate::storage::read_json_object(&app, "command-history.json")?;
    Ok(value
        .get(&profile_id)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<TerminalCommandHistoryEntry>(entry).ok())
        .filter(|entry| !entry.command.trim().is_empty())
        .collect())
}

#[tauri::command]
pub fn app_set_terminal_command_history(
    app: AppHandle,
    profile_id: String,
    entries: Vec<TerminalCommandHistoryEntry>,
) -> Result<(), AppError> {
    let mut value = crate::storage::read_json_object(&app, "command-history.json")?;
    let sanitized = entries
        .into_iter()
        .filter(|entry| !entry.command.trim().is_empty())
        .take(500)
        .collect::<Vec<_>>();
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization("命令历史文件格式无效".to_string()))?;
    object.insert(
        profile_id,
        serde_json::to_value(sanitized)
            .map_err(|error| AppError::Serialization(error.to_string()))?,
    );
    write_json_object(&app, "command-history.json", &value)
}

#[tauri::command]
pub fn app_get_command_send_preferences(
    app: AppHandle,
) -> Result<CommandSendPreferences, AppError> {
    let value = crate::storage::read_json_object(&app, "command-send-preferences.json")?;
    let preferences = serde_json::from_value::<CommandSendPreferences>(value).unwrap_or_default();
    Ok(CommandSendPreferences {
        send_scope: match preferences.send_scope.as_str() {
            "current" | "all-ssh" | "selected-ssh" => preferences.send_scope,
            _ => "current".to_string(),
        },
        ..preferences
    })
}

#[tauri::command]
pub fn app_set_command_send_preferences(
    app: AppHandle,
    preferences: CommandSendPreferences,
) -> Result<(), AppError> {
    if !matches!(
        preferences.send_scope.as_str(),
        "current" | "all-ssh" | "selected-ssh"
    ) {
        return Err(AppError::Command("命令发送范围无效".to_string()));
    }
    let selected_tab_ids = preferences
        .selected_tab_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .take(200)
        .collect::<Vec<_>>();
    write_json_object(
        &app,
        "command-send-preferences.json",
        &serde_json::to_value(CommandSendPreferences {
            selected_tab_ids,
            ..preferences
        })
        .map_err(|error| AppError::Serialization(error.to_string()))?,
    )
}

async fn lock_library_after_transfer_hydration(
    app: &AppHandle,
) -> Result<tokio::sync::OwnedMutexGuard<()>, AppError> {
    // Transfer hydration can emit a cleanup snapshot. Finish it before taking
    // the library lock so that nested snapshot cannot wait on this same lock.
    crate::services::transfers::ensure_loaded(app).await?;
    Ok(app
        .state::<crate::services::workspace::WorkspaceState>()
        .library_mutation
        .clone()
        .lock_owned()
        .await)
}

#[tauri::command]
pub async fn app_get_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_get_connection_library(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let library_mutation = app
        .state::<crate::services::workspace::WorkspaceState>()
        .library_mutation
        .clone();
    let _guard = library_mutation.lock().await;
    let (profiles_with_secrets, folders) =
        crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let profiles = profiles_with_secrets
        .iter()
        .map(crate::services::profile_ops::strip_secret_fields_public)
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "profiles": profiles,
        "folders": folders,
    }))
}

#[tauri::command]
pub fn app_list_ssh_keys(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    crate::services::ssh_keys::list(&app)
}

#[tauri::command]
pub async fn app_select_ssh_key_file(
    app: AppHandle,
) -> Result<Option<serde_json::Value>, AppError> {
    crate::services::ssh_keys::select_file(&app).await
}

#[tauri::command]
pub fn app_import_ssh_key(
    app: AppHandle,
    input: Option<ImportSshKeyInput>,
) -> Result<Option<serde_json::Value>, AppError> {
    let input = input.unwrap_or(ImportSshKeyInput {
        source_path: None,
        note: None,
    });
    let result = crate::services::ssh_keys::import(&app, input.source_path, input.note)?;
    if result.is_some() {
        emit_ssh_keys_changed(&app)?;
    }
    Ok(result)
}

#[tauri::command]
pub fn app_update_ssh_key_note(
    app: AppHandle,
    key_id: String,
    note: String,
) -> Result<serde_json::Value, AppError> {
    let updated = crate::services::ssh_keys::update_note(&app, &key_id, note)?;
    emit_ssh_keys_changed(&app)?;
    Ok(updated)
}

#[tauri::command]
pub fn app_delete_ssh_key(app: AppHandle, key_id: String) -> Result<(), AppError> {
    crate::services::ssh_keys::delete(&app, &key_id)?;
    emit_ssh_keys_changed(&app)
}

fn emit_ssh_keys_changed(app: &AppHandle) -> Result<(), AppError> {
    app.emit("sshKeys:changed", crate::services::ssh_keys::list(app)?)
        .map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub async fn app_preview_connection_import(
    app: AppHandle,
    source: Option<String>,
) -> Result<Option<serde_json::Value>, AppError> {
    let dialog = rfd::AsyncFileDialog::new()
        .add_filter("Connection files", &["json", "config", "txt"])
        .set_title("选择连接配置或目录");
    let paths = match source.as_deref() {
        Some("folder") => dialog
            .pick_folder()
            .await
            .map(|folder| vec![folder.path().to_path_buf()]),
        Some("files") | None => dialog.pick_files().await.map(|files| {
            files
                .into_iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        }),
        _ => return Err(AppError::Command("导入来源无效".to_string())),
    };
    let Some(paths) = paths else {
        return Ok(None);
    };
    crate::services::connections::create_import_plan_from_paths(&app, paths)
        .await
        .map(Some)
}

#[tauri::command]
pub async fn app_commit_connection_json_import(
    app: AppHandle,
    plan_id: String,
    options: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let selected_ids = options
        .get("selectedItemIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let strategy = options
        .get("conflictStrategy")
        .and_then(Value::as_str)
        .unwrap_or("skip");
    crate::services::connections::commit_import_plan(&app, &plan_id, &selected_ids, strategy).await
}

#[tauri::command]
pub async fn app_export_connections(app: AppHandle, format: String) -> Result<bool, AppError> {
    let extension = if format == "compatible" {
        "json"
    } else {
        "fileterm.json"
    };
    let Some(target) = rfd::AsyncFileDialog::new()
        .set_file_name(format!("fileterm-connections.{extension}"))
        .add_filter("JSON", &["json"])
        .save_file()
        .await
    else {
        return Ok(false);
    };
    let bytes = crate::services::connections::export_bundle(&app, &format)?;
    tokio::fs::write(target.path(), bytes)
        .await
        .map_err(|error| AppError::Storage(format!("无法写入导出文件: {error}")))?;
    Ok(true)
}

#[tauri::command]
pub async fn app_export_connections_as_files(
    app: AppHandle,
    format: String,
) -> Result<bool, AppError> {
    let Some(target) = rfd::AsyncFileDialog::new().pick_folder().await else {
        return Ok(false);
    };
    let (profiles, _) = crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let mut used_names = std::collections::HashSet::new();
    for profile in profiles {
        let id = profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("connection");
        let name = profile.get("name").and_then(Value::as_str).unwrap_or(id);
        let filename = format!(
            "{}.json",
            crate::services::connections::export_filename(name, id, &mut used_names)
        );
        let payload = if format == "compatible" {
            serde_json::json!({
                "id": profile.get("id"), "name": profile.get("name"),
                "description": profile.get("note"), "conection_type": profile.get("type"),
                "host": profile.get("host"), "port": profile.get("port"),
                "user_name": profile.get("username"), "terminal_encoding": profile.get("encoding"),
                "authentication_type": profile.get("authType"), "password": profile.get("password"),
                "private_key_path": profile.get("privateKeyPath"), "passphrase": profile.get("passphrase"),
                "exec_channel_enable": profile.get("enableExecChannel"),
                "port_forwarding_list": profile.get("forwards"),
            })
        } else {
            serde_json::json!({
                "schemaVersion": 1,
                "generatedAt": crate::services::webdav::export_timestamp(),
                "profiles": [profile],
            })
        };
        let bytes = serde_json::to_vec_pretty(&payload)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        tokio::fs::write(target.path().join(filename), bytes)
            .await
            .map_err(|error| AppError::Storage(format!("无法写入单连接导出: {error}")))?;
    }
    Ok(true)
}

#[tauri::command]
pub fn app_get_webdav_sync_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::get_config(&app)
}

#[tauri::command]
pub fn app_set_webdav_sync_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::save_config(&app, input)
}

#[tauri::command]
pub async fn app_upload_webdav_sync(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::upload(&app).await
}

#[tauri::command]
pub async fn app_download_webdav_sync(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let result = crate::services::webdav::download(&app).await?;
    let changed = result.get("imported").and_then(Value::as_u64).unwrap_or(0)
        + result.get("updated").and_then(Value::as_u64).unwrap_or(0);
    if changed > 0 {
        if let Ok(snapshot) = get_workspace_snapshot_unlocked(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn app_workspace_mutation(
    app: AppHandle,
    operation: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    match operation.as_str() {
        "create-profile" => {
            if let Some(input) = payload.get("input").cloned() {
                crate::services::profile_ops::create_profile(&app, input)?;
            }
        }
        "create-folder" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("新建分类");
            let parent_id = payload.get("parentId").and_then(|id| id.as_str());
            crate::services::profile_ops::create_folder(&app, name, parent_id)?;
        }
        "create-command-folder" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("新建命令分类");
            let parent_id = payload.get("parentId").and_then(|id| id.as_str());
            crate::services::profile_ops::create_command_folder(&app, name, parent_id)?;
        }
        "create-command" => {
            if let Some(input) = payload.get("input").cloned() {
                crate::services::profile_ops::create_command_template(&app, input)?;
            }
        }
        _ => {
            return Err(AppError::Command(format!(
                "Unsupported operation: {operation}"
            )))
        }
    }
    get_workspace_snapshot_and_emit(&app).await
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenWindowInput {
    pub kind: String,
    pub mode: Option<String>,
    pub profile_id: Option<String>,
    pub command_id: Option<String>,
    pub folder_id: Option<String>,
    pub command: Option<String>,
    pub source: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub tab_id: Option<String>,
    pub encoding: Option<String>,
}

#[tauri::command]
pub async fn app_open_window(app: AppHandle, input: OpenWindowInput) -> Result<(), AppError> {
    // WebView2 can deadlock when WebviewWindowBuilder is used from a
    // synchronous Tauri command on Windows. Keep the command asynchronous and
    // perform the blocking builder call on a worker thread so the native event
    // loop remains able to finish WebView2 initialization and service every
    // other invoke request.
    tauri::async_runtime::spawn_blocking(move || crate::open_child_window(&app, input))
        .await
        .map_err(|error| AppError::Window(format!("子窗口创建任务失败: {error}")))?
}

fn renderer_approved_close_should_destroy(window_label: &str) -> bool {
    window_label != "main"
}

fn destroy_child_window_after_invoke_reply(window: WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CHILD_WINDOW_DESTROY_DELAY).await;
        let _ = window.destroy();
    });
}

#[tauri::command]
pub async fn app_window_action(
    app: AppHandle,
    window: WebviewWindow,
    action: String,
) -> Result<(), AppError> {
    match action.as_str() {
        "show" => {
            window
                .show()
                .map_err(|error| AppError::Window(error.to_string()))?;
            window
                .set_focus()
                .map_err(|error| AppError::Window(error.to_string()))?;
        }
        "minimize" => {
            let _ = window.minimize();
        }
        "toggle-maximize" => {
            if let Ok(true) = window.is_maximized() {
                let _ = window.unmaximize();
            } else {
                let _ = window.maximize();
            }
            let _ = app.emit(
                "app:window-maximized-change",
                window.is_maximized().unwrap_or(false),
            );
        }
        "close" => {
            if !renderer_approved_close_should_destroy(window.label()) {
                // Match Electron: closing the last workspace item requests a
                // normal main-window close. The CloseRequested guard decides
                // whether to hide to tray, quit, or cancel.
                let _ = window.close();
            } else {
                // A child renderer has already approved this close. Destroy it
                // after this command's invoke reply so WebView2 does not try to
                // resolve the callback in an already-destroyed renderer.
                crate::resolve_file_editor_close(&app, &window);
                destroy_child_window_after_invoke_reply(window);
            }
        }
        "hide" => {
            crate::hide_main_window_and_children(&app);
        }
        "request-quit" => {
            crate::request_main_window_close(&app, true);
        }
        "quit" => {
            let quit_registry = app.state::<crate::QuitPreparationRegistry>();
            if !quit_registry.try_begin() {
                return Ok(());
            }
            let editors_approved = match crate::request_file_editors_for_quit(&app).await {
                Ok(approved) => approved,
                Err(error) => {
                    quit_registry.cancel();
                    return Err(error);
                }
            };
            if !editors_approved {
                quit_registry.cancel();
                return Ok(());
            }
            // Quit the entire app. Used by the renderer when the user
            // confirms a Cmd+Q / tray-quit request. Persist paused transfer
            // checkpoints before exiting so a restart never silently loses a
            // resumable file.
            if let Err(error) = crate::services::transfers::shutdown(&app).await {
                quit_registry.cancel();
                return Err(error);
            }
            shutdown_session_workers(&app).await;
            app.exit(0);
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
pub fn app_is_window_maximized(window: WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false)
}

#[tauri::command]
pub fn app_cancel_file_editor_close(app: AppHandle, window: WebviewWindow) {
    crate::cancel_file_editor_close(&app, &window);
}

#[tauri::command]
pub fn app_show_window_menu(
    app: AppHandle,
    window: WebviewWindow,
    menu_type: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let kind = crate::WindowMenuKind::try_from(menu_type.as_str())?;
    crate::show_window_context_menu(&app, &window, kind, x, y)
}

// ==========================================
// Phase 3 commands implementation
// ==========================================

pub(crate) async fn get_workspace_snapshot_unlocked(
    app: AppHandle,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    let tabs = state.tabs.read().await.clone();
    let active_tab_id = state.active_tab_id.read().await.clone();
    let sessions = state.sessions.read().await.clone();
    let transfers = state.transfers.read().await.clone();
    let active_pane_tab_id_by_root = state.active_pane_tab_id_by_root.read().await.clone();

    // Read + heal profiles, then strip secrets before exposing in snapshot.
    let (profiles_with_secrets, folders) =
        crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let profiles: Vec<serde_json::Value> = profiles_with_secrets
        .iter()
        .map(crate::services::profile_ops::strip_secret_fields_public)
        .collect();
    let (command_folders, commands) =
        crate::services::profile_ops::read_and_heal_command_library(&app)?;

    Ok(serde_json::json!({
        "profiles": profiles,
        "folders": folders,
        "commandFolders": command_folders,
        "commandTemplates": commands,
        "tabs": tabs,
        "activeTabId": active_tab_id,
        "transfers": transfers,
        "sessions": sessions,
        "activePaneTabIdByRoot": active_pane_tab_id_by_root,
    }))
}

pub async fn get_workspace_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    get_workspace_snapshot_unlocked(app).await
}

async fn get_workspace_snapshot_and_emit(app: &AppHandle) -> Result<serde_json::Value, AppError> {
    let snapshot = get_workspace_snapshot_unlocked(app.clone()).await?;
    if let Err(error) = app.emit("workspace:snapshot", snapshot.clone()) {
        // Persistence has already succeeded. A failed best-effort broadcast
        // must not turn a successful mutation into a retryable renderer error
        // that can create duplicate folders/commands/profiles.
        crate::services::logging::warn(
            app,
            "workspace",
            format!("failed to broadcast workspace snapshot: {error}"),
        );
    }
    Ok(snapshot)
}

async fn send_worker_cmd<T>(
    app: &AppHandle,
    tab_id: &str,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let workers = state.workers.read().await;
    let sender = workers
        .get(tab_id)
        .ok_or_else(|| AppError::Storage("Session not found".to_string()))?
        .clone();
    drop(workers);

    let (tx, rx) = oneshot::channel();
    let cmd = make_cmd(tx);
    // 不持有 workers 读锁跨 await：clone sender 后立即释放，避免后续写锁死锁。
    // send 必须超时，worker 卡死时前端能拿到明确错误而不是永久 hang。
    timeout(WORKER_FILE_CMD_SEND_TIMEOUT, sender.send(cmd))
        .await
        .map_err(|_| AppError::Storage("Worker busy: command send timeout".to_string()))?
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let res = timeout(WORKER_FILE_RESPONSE_TIMEOUT, rx)
        .await
        .map_err(|_| AppError::Storage("远程文件操作超时，请检查连接后重试".to_string()))?
        .map_err(|e| AppError::Storage(e.to_string()))?
        .map_err(AppError::Storage)?;
    Ok(res)
}

async fn refresh_remote_files(app: &AppHandle, tab_id: &str, path: &str) -> Result<(), AppError> {
    let files = send_worker_cmd(app, tab_id, |tx| WorkerCmd::ListRemoteFiles {
        path: path.to_string(),
        respond_to: tx,
    })
    .await?;

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(tab_id) {
        session.remote_files = files;
    }
    Ok(())
}

fn create_tab_layout(profile_type: &str) -> String {
    match profile_type {
        "ssh" => "terminal-file".to_string(),
        "ftp" => "file-only".to_string(),
        _ => "terminal-only".to_string(),
    }
}

fn start_session_worker(
    tab_id: String,
    profile: serde_json::Value,
    receiver: mpsc::Receiver<WorkerCmd>,
    terminal_input_receiver: Option<mpsc::UnboundedReceiver<String>>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    match profile.get("type").and_then(Value::as_str).unwrap_or("ssh") {
        "ftp" => crate::sessions::ftp::start_ftp_worker(tab_id, profile, receiver, app),
        "telnet" => crate::sessions::telnet::start_telnet_worker(tab_id, profile, receiver, app),
        "serial" => crate::sessions::serial::start_serial_worker(tab_id, profile, receiver, app),
        _ => crate::sessions::ssh::start_ssh_worker(
            tab_id,
            profile,
            receiver,
            terminal_input_receiver.expect("SSH worker requires a terminal input channel"),
            app,
            cancellation,
        ),
    }
}

async fn stop_session_worker(state: &crate::services::workspace::WorkspaceState, tab_id: &str) {
    if let Some(control) = state.worker_controls.write().await.remove(tab_id) {
        // Cancel first: a command sender cannot wake a worker which is inside
        // an SSH read/metrics parse. This also prevents a stale worker from
        // emitting state over a replacement connection after reconnect.
        control.cancel();
    }
    state.terminal_inputs.write().await.remove(tab_id);
    let sender = state.workers.write().await.remove(tab_id);
    if let Some(sender) = sender {
        // 超时即放弃：worker 主循环卡死时 channel 已满，send 不进去；
        // 但 sender 已经从 workers map 移除并即将 drop，worker 的
        // `cmd_rx.recv()` 会返回 None 走清理路径，无需依赖这条 Disconnect。
        let _ = timeout(
            WORKER_DISCONNECT_TIMEOUT,
            sender.send(WorkerCmd::Disconnect),
        )
        .await;
    }
}

pub async fn shutdown_session_workers(app: &AppHandle) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let controls = state
        .worker_controls
        .write()
        .await
        .drain()
        .map(|(_, control)| control)
        .collect::<Vec<_>>();
    for control in controls {
        control.cancel();
    }
    state.terminal_inputs.write().await.clear();
    let senders = state
        .workers
        .write()
        .await
        .drain()
        .map(|(_, sender)| sender)
        .collect::<Vec<_>>();
    for sender in senders {
        // Cmd+Q 退出链路：任何单个卡死 worker 都不能阻塞整体退出。
        // 超时后直接 drop sender，worker 收到 recv()==None 自动清理。
        let _ = timeout(
            WORKER_DISCONNECT_TIMEOUT,
            sender.send(WorkerCmd::Disconnect),
        )
        .await;
    }
}

/// 为指定 profile 创建并启动一个新的 session（tab + session + worker）。
/// 返回新 tab_id。调用者负责更新 active_tab_id、paneRoot 以及 emit snapshot。
///
/// 抽取自 `app_open_profile`，供 `app_split_tab` 复用：分屏时基于当前 profile
/// 新建一个独立 session，不共享 PTY。
async fn spawn_session_for_profile(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    profile: &serde_json::Value,
    profile_id: &str,
    pane_root_tab_id: Option<String>,
) -> Result<String, AppError> {
    let profile_type = profile
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("ssh");
    let name = profile
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("SSH Session");

    let tab_id = format!("tab-{}", uuid::Uuid::new_v4());
    let capabilities =
        crate::services::workspace::ConnectionCapabilities::for_session_type(profile_type);
    let new_tab = crate::services::WorkspaceTab {
        id: tab_id.clone(),
        profile_id: profile_id.to_string(),
        session_type: profile_type.to_string(),
        title: name.to_string(),
        layout: create_tab_layout(profile_type),
        status: crate::services::WorkspaceTabStatus::Connecting,
        pane_root: None,
        pane_root_tab_id,
    };

    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| profile.get("devicePath").and_then(Value::as_str))
        .unwrap_or("127.0.0.1");
    let port = profile.get("port").and_then(|p| p.as_i64()).unwrap_or(22) as u16;
    let username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root");
    let initial_remote_path = crate::services::workspace::initial_remote_path_for_profile(profile);

    {
        let mut tabs = state.tabs.write().await;
        tabs.push(new_tab);
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            tab_id.clone(),
            crate::services::SessionSnapshot {
                profile_id: profile_id.to_string(),
                access_host: format!("{}:{}", host, port),
                summary: format!("{}@{}", username, host),
                terminal_transcript: "连接主机...\r\n".to_string(),
                remote_path: initial_remote_path,
                shell_cwd: None,
                follow_shell_cwd: true,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                has_reusable_sudo_auth: false,
                login_user: None,
                shell_user: None,
                connected: false,
                system_metrics: None,
                capabilities,
                reconnect_mode: crate::services::workspace::reconnect_mode_for_profile(profile),
            },
        );
    }

    let (tx, rx) = mpsc::channel(100);
    let (terminal_input_tx, terminal_input_rx) = if profile_type == "ssh" {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Some(sender), Some(receiver))
    } else {
        (None, None)
    };
    let worker_control = CancellationToken::new();
    {
        let mut workers = state.workers.write().await;
        workers.insert(tab_id.clone(), tx);
    }
    if let Some(sender) = terminal_input_tx {
        state
            .terminal_inputs
            .write()
            .await
            .insert(tab_id.clone(), sender);
    }
    state
        .worker_controls
        .write()
        .await
        .insert(tab_id.clone(), worker_control.clone());

    start_session_worker(
        tab_id.clone(),
        profile.clone(),
        rx,
        terminal_input_rx,
        app.clone(),
        worker_control,
    );

    Ok(tab_id)
}

#[tauri::command]
pub async fn app_open_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;
    let profiles = read_json_array(&app, "profiles.json")?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;

    // Match Electron's open lifecycle: recency is about the user's intent to
    // open a connection, not whether the later network handshake succeeds.
    crate::services::profile_ops::touch_profile(&app, &profile_id)?;

    let tab_id = spawn_session_for_profile(&app, &state, profile, &profile_id, None).await?;

    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(tab_id);
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// 分屏：基于当前 profile 新建一个独立 session，并在 pane tree 中把 source leaf
/// 替换为 split(source_leaf, new_leaf)。
///
/// - `direction = "row"`：左右分（垂直分屏），新 pane 在右
/// - `direction = "column"`：上下分（水平分屏），新 pane 在下
///
/// 只支持 SSH session。FTP / Telnet / Serial 暂不支持分屏。
#[tauri::command]
pub async fn app_split_tab(
    app: AppHandle,
    source_tab_id: String,
    direction: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;

    let split_direction = match direction.as_str() {
        "row" => crate::services::SplitDirection::Row,
        "column" => crate::services::SplitDirection::Column,
        _ => {
            return Err(AppError::Storage(format!(
                "Invalid split direction: {}",
                direction
            )))
        }
    };

    // 找到 source tab，并解析其所属的顶层 workspace tab。分屏 leaf 始终
    // 归属一个 root，而不是第二个顶栏 tab。
    let (profile_id, pane_root_tab_id) = {
        let tabs = state.tabs.read().await;
        let source = tabs
            .iter()
            .find(|t| t.id == source_tab_id)
            .ok_or_else(|| AppError::Storage(format!("Tab not found: {}", source_tab_id)))?;
        // 只支持 SSH 分屏
        if source.session_type != "ssh" {
            return Err(AppError::Storage(format!(
                "Split pane is only supported for SSH sessions, got: {}",
                source.session_type
            )));
        }
        let root_tab_id = source.pane_root_tab_id.clone().unwrap_or_else(|| {
            if source.pane_root.is_some() {
                source.id.clone()
            } else {
                tabs.iter()
                    .find(|tab| {
                        tab.pane_root
                            .as_ref()
                            .map(|root| root.leaf_tab_ids().iter().any(|id| id == &source_tab_id))
                            .unwrap_or(false)
                    })
                    .map(|tab| tab.id.clone())
                    .unwrap_or_else(|| source.id.clone())
            }
        });
        (source.profile_id.clone(), root_tab_id)
    };

    // 读 profile
    let profiles = read_json_array(&app, "profiles.json")?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;

    // 创建新 session（不 touch_profile，分屏不算独立打开）
    let new_tab_id =
        spawn_session_for_profile(&app, &state, profile, &profile_id, Some(pane_root_tab_id))
            .await?;

    // 更新 paneRoot，并保留承载该分屏树的 root tab id。
    let root_tab_id = {
        let mut tabs = state.tabs.write().await;

        // 先找 source 是否已经是 root（有 paneRoot）
        let root_idx = tabs
            .iter()
            .position(|t| t.id == source_tab_id && t.pane_root.is_some());

        if let Some(idx) = root_idx {
            // source 是 root，在其 paneRoot 里替换 source leaf
            let root_tab = &mut tabs[idx];
            if let Some(ref mut pane_root) = root_tab.pane_root {
                let replacement = crate::services::PaneNode::Split {
                    direction: split_direction,
                    children: vec![
                        crate::services::PaneNode::Leaf {
                            tab_id: source_tab_id.clone(),
                        },
                        crate::services::PaneNode::Leaf {
                            tab_id: new_tab_id.clone(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                };
                pane_root.replace_leaf(&source_tab_id, replacement);
            }
            source_tab_id.clone()
        } else {
            // source 不是 root，可能是独立 tab 或某个 root 的 leaf
            // 先检查它是否是某个 root 的 leaf
            let containing_root_idx = tabs.iter().position(|t| {
                t.pane_root
                    .as_ref()
                    .map(|root| root.leaf_tab_ids().iter().any(|id| id == &source_tab_id))
                    .unwrap_or(false)
            });

            if let Some(idx) = containing_root_idx {
                // source 是某个 root 的 leaf，在该 root 的 paneRoot 里替换
                let root_tab = &mut tabs[idx];
                if let Some(ref mut pane_root) = root_tab.pane_root {
                    let replacement = crate::services::PaneNode::Split {
                        direction: split_direction,
                        children: vec![
                            crate::services::PaneNode::Leaf {
                                tab_id: source_tab_id.clone(),
                            },
                            crate::services::PaneNode::Leaf {
                                tab_id: new_tab_id.clone(),
                            },
                        ],
                        weights: vec![0.5, 0.5],
                    };
                    pane_root.replace_leaf(&source_tab_id, replacement);
                }
                root_tab.id.clone()
            } else {
                // source 是独立 tab，变成 root
                let source_idx = tabs
                    .iter()
                    .position(|t| t.id == source_tab_id)
                    .ok_or_else(|| AppError::Storage("Source tab vanished".to_string()))?;
                tabs[source_idx].pane_root = Some(crate::services::PaneNode::Split {
                    direction: split_direction,
                    children: vec![
                        crate::services::PaneNode::Leaf {
                            tab_id: source_tab_id.clone(),
                        },
                        crate::services::PaneNode::Leaf {
                            tab_id: new_tab_id.clone(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                });
                source_tab_id.clone()
            }
        }
    };

    // 无论当前 source 是 root、leaf 还是独立 tab，顶层都必须停留在 root。
    // 新 session 只作为 active pane，不得成为一个新的顶栏 tab。
    {
        let mut active_tab = state.active_tab_id.write().await;
        *active_tab = Some(root_tab_id.clone());

        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.insert(root_tab_id, new_tab_id.clone());
    }

    get_workspace_snapshot_and_emit(&app).await
}

#[derive(Debug, PartialEq, Eq)]
struct PaneCloseOutcome {
    root_tab_id: String,
    remaining_pane_tab_ids: Vec<String>,
    keeps_split: bool,
}

/// Remove a leaf from a split root while preserving the invariant that a
/// top-level tab is always backed by a live session. When the original root
/// leaf is closed, a surviving leaf is promoted to be the new root rather
/// than leaving a tree whose container points at a closed terminal.
fn remove_split_pane_from_tabs(
    tabs: &mut Vec<crate::services::WorkspaceTab>,
    root_tab_id: &str,
    pane_tab_id: &str,
) -> Result<PaneCloseOutcome, AppError> {
    let root_idx = tabs
        .iter()
        .position(|tab| {
            tab.id == root_tab_id
                || tab
                    .pane_root
                    .as_ref()
                    .map(|r| r.leaf_tab_ids().iter().any(|id| id == pane_tab_id))
                    .unwrap_or(false)
        })
        .ok_or_else(|| AppError::Storage(format!("Root tab not found: {root_tab_id}")))?;
    let actual_root_tab_id = tabs[root_idx].id.clone();
    let mut next_pane_root = tabs[root_idx]
        .pane_root
        .clone()
        .ok_or_else(|| AppError::Storage("Tab does not have a split pane layout".to_string()))?;
    let before = next_pane_root.leaf_tab_ids();
    if before.len() < 2 {
        return Err(AppError::Storage(
            "Cannot close the only pane through the split-pane command".to_string(),
        ));
    }
    if !before.iter().any(|id| id == pane_tab_id) {
        return Err(AppError::Storage(format!("Pane not found: {pane_tab_id}")));
    }

    next_pane_root.remove_leaf(pane_tab_id);
    let remaining_pane_tab_ids = next_pane_root.leaf_tab_ids();
    let keeps_split = remaining_pane_tab_ids.len() > 1;

    if pane_tab_id != actual_root_tab_id {
        let root_tab = &mut tabs[root_idx];
        root_tab.pane_root = keeps_split.then_some(next_pane_root);
        tabs.retain(|tab| tab.id != pane_tab_id);
        return Ok(PaneCloseOutcome {
            root_tab_id: actual_root_tab_id,
            remaining_pane_tab_ids,
            keeps_split,
        });
    }

    // The original root session is itself a leaf. If it is the pane being
    // closed, turn the first surviving session into the new top-level tab and
    // repoint all remaining leaves at it. This mirrors Ghostty's separation of
    // tab container and terminal surface without requiring us to rename a
    // running SSH worker.
    let promoted_root_tab_id = remaining_pane_tab_ids
        .first()
        .cloned()
        .ok_or_else(|| AppError::Storage("No surviving pane to promote as root tab".to_string()))?;

    let _root_tab = tabs.remove(root_idx);
    let promoted_idx = tabs
        .iter()
        .position(|tab| tab.id == promoted_root_tab_id)
        .ok_or_else(|| {
            AppError::Storage(format!("Promoted pane tab not found: {promoted_root_tab_id}"))
        })?;

    let mut promoted_tab = tabs.remove(promoted_idx);
    promoted_tab.pane_root_tab_id = None;
    promoted_tab.pane_root = keeps_split.then_some(next_pane_root);

    for tab in tabs.iter_mut() {
        if tab.pane_root_tab_id.as_deref() == Some(actual_root_tab_id.as_str()) {
            tab.pane_root_tab_id = Some(promoted_root_tab_id.clone());
        }
    }

    let insert_idx = root_idx.min(tabs.len());
    tabs.insert(insert_idx, promoted_tab);

    Ok(PaneCloseOutcome {
        root_tab_id: promoted_root_tab_id,
        remaining_pane_tab_ids,
        keeps_split,
    })
}

/// 关闭分屏中的单个 pane。若 pane tree 只剩一个 leaf，退化回普通 tab。
#[tauri::command]
pub async fn app_close_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;

    // Validate before stopping the session. Closing the final pane is a
    // top-level-tab action and must go through its confirmation flow instead.
    let resolved_root_tab_id = {
        let tabs = state.tabs.read().await;
        let root_tab = tabs
            .iter()
            .find(|tab| {
                tab.id == root_tab_id
                    || tab
                        .pane_root
                        .as_ref()
                        .map(|r| r.leaf_tab_ids().iter().any(|id| id == &pane_tab_id))
                        .unwrap_or(false)
            })
            .ok_or_else(|| AppError::Storage(format!("Root tab not found: {root_tab_id}")))?;
        let leaves = root_tab
            .pane_root
            .as_ref()
            .map(crate::services::PaneNode::leaf_tab_ids)
            .ok_or_else(|| {
                AppError::Storage("Tab does not have a split pane layout".to_string())
            })?;
        if leaves.len() < 2 {
            return Err(AppError::Storage(
                "Cannot close the only pane through the split-pane command".to_string(),
            ));
        }
        if !leaves.iter().any(|id| id == &pane_tab_id) {
            return Err(AppError::Storage(format!("Pane not found: {pane_tab_id}")));
        }
        root_tab.id.clone()
    };

    let previous_active_pane = state
        .active_pane_tab_id_by_root
        .read()
        .await
        .get(&resolved_root_tab_id)
        .cloned();

    // 暂停该 pane 关联的传输，并清理对应 worker。
    let _ = crate::services::transfers::pause_for_tab(
        &app,
        &pane_tab_id,
        "Pane 关闭后已暂停，可在重连后继续传输",
    )
    .await;
    stop_session_worker(&state, &pane_tab_id).await;

    let outcome = {
        let mut tabs = state.tabs.write().await;
        remove_split_pane_from_tabs(&mut tabs, &resolved_root_tab_id, &pane_tab_id)?
    };

    state.sessions.write().await.remove(&pane_tab_id);

    {
        let mut active_tab = state.active_tab_id.write().await;
        if active_tab.as_deref() == Some(root_tab_id.as_str()) {
            *active_tab = Some(outcome.root_tab_id.clone());
        }
    }

    {
        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.remove(&root_tab_id);
        if outcome.keeps_split {
            let next_active_pane = previous_active_pane
                .filter(|id| id != &pane_tab_id && outcome.remaining_pane_tab_ids.contains(id))
                .unwrap_or_else(|| outcome.remaining_pane_tab_ids[0].clone());
            active_panes.insert(outcome.root_tab_id, next_active_pane);
        }
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// 设置分屏中的活跃 pane。
#[tauri::command]
pub async fn app_set_active_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.insert(root_tab_id, pane_tab_id);
    }
    get_workspace_snapshot(app).await
}

/// 持久化分屏 weights（拖拽 resize 结束时调用）。
#[tauri::command]
pub async fn app_set_pane_weights(
    app: AppHandle,
    root_tab_id: String,
    pane_path: Vec<usize>,
    weights: Vec<f32>,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut tabs = state.tabs.write().await;
        let root_idx = tabs
            .iter()
            .position(|t| t.id == root_tab_id)
            .ok_or_else(|| AppError::Storage(format!("Root tab not found: {}", root_tab_id)))?;
        let root_tab = &mut tabs[root_idx];
        if let Some(ref mut pane_root) = root_tab.pane_root {
            if !pane_root.set_split_weights_at_path(&pane_path, &weights) {
                return Err(AppError::Storage(
                    "Split pane path or weights are invalid".to_string(),
                ));
            }
        } else {
            return Err(AppError::Storage(
                "Tab does not have a split pane layout".to_string(),
            ));
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_activate_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    // A pane leaf owns a session but never owns a top-level tab. Normalizing
    // here makes the invariant hold even for stale UI events.
    let top_level_tab_id = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .and_then(|tab| tab.pane_root_tab_id.clone())
        .unwrap_or(tab_id);
    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(top_level_tab_id);
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_reconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let profile_id = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.profile_id.clone())
    };

    if let Some(pid) = profile_id {
        let profiles = read_json_array(&app, "profiles.json")?;
        if let Some(profile) = profiles
            .iter()
            .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&pid))
        {
            // Claim the reconnect before awaiting worker shutdown. Tauri can
            // dispatch Enter/button/auto-reconnect commands concurrently; a
            // status check performed after an await lets each caller replace
            // the worker and append another reconnect banner.
            let should_start = {
                let mut tabs = state.tabs.write().await;
                claim_reconnect_tab(&mut tabs, &tab_id)
            };
            if !should_start {
                return get_workspace_snapshot(app).await;
            }

            // Terminate existing worker
            stop_session_worker(&state, &tab_id).await;

            // Set connecting status. Preserve the existing transcript so the
            // renderer can re-hydrate the terminal with prior history on
            // reconnect (mirrors Electron's BoundedTextBuffer retention).
            // We only append a separator + "连接主机..." notice so the user
            // sees that a reconnect is in progress.
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    session.connected = false;
                    session.remote_files_loading = false;
                    session.shell_user = None;
                    session.file_access_mode = "user".to_string();
                    session.has_reusable_sudo_auth = false;
                    session.reconnect_mode =
                        crate::services::workspace::reconnect_mode_for_profile(profile);
                    // Append a reconnect separator instead of wiping history.
                    if !session.terminal_transcript.is_empty() {
                        session
                            .terminal_transcript
                            .push_str("\r\n--- 重新连接 ---\r\n");
                    }
                    session.terminal_transcript.push_str("连接主机...\r\n");
                    // Cap to 200k chars (matches Electron's BoundedTextBuffer).
                    if session.terminal_transcript.len() > 200_000 {
                        let mut cut = session.terminal_transcript.len() - 180_000;
                        while cut < session.terminal_transcript.len()
                            && !session.terminal_transcript.is_char_boundary(cut)
                        {
                            cut += 1;
                        }
                        session.terminal_transcript =
                            session.terminal_transcript[cut..].to_string();
                    }
                    session.remote_files = Vec::new();
                    session.system_metrics = None;
                }
            }

            // Renderer-triggered reconnects apply the returned snapshot, but
            // auto-reconnect is initiated by the worker and has no renderer
            // caller to apply it. Broadcast the connecting snapshot for both
            // paths so the terminal/file panes cannot remain on stale state.
            if let Ok(snapshot) = get_workspace_snapshot(app.clone()).await {
                let _ = app.emit("workspace:snapshot", snapshot);
            }

            let (tx, rx) = mpsc::channel(100);
            let profile_type = profile.get("type").and_then(Value::as_str).unwrap_or("ssh");
            let (terminal_input_tx, terminal_input_rx) = if profile_type == "ssh" {
                let (sender, receiver) = mpsc::unbounded_channel();
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
            let worker_control = CancellationToken::new();
            {
                let mut workers = state.workers.write().await;
                workers.insert(tab_id.clone(), tx);
            }
            if let Some(sender) = terminal_input_tx {
                state
                    .terminal_inputs
                    .write()
                    .await
                    .insert(tab_id.clone(), sender);
            }
            state
                .worker_controls
                .write()
                .await
                .insert(tab_id.clone(), worker_control.clone());

            start_session_worker(
                tab_id,
                profile.clone(),
                rx,
                terminal_input_rx,
                app.clone(),
                worker_control,
            );
        }
    }

    get_workspace_snapshot(app).await
}

fn claim_reconnect_tab(tabs: &mut [crate::services::WorkspaceTab], tab_id: &str) -> bool {
    let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) else {
        return false;
    };
    if tab.status == crate::services::WorkspaceTabStatus::Connecting {
        return false;
    }
    tab.status = crate::services::WorkspaceTabStatus::Connecting;
    true
}

#[tauri::command]
pub async fn app_disconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::pause_for_tab(&app, &tab_id, "连接断开，可在重连后继续传输")
        .await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let was_connected = state
        .sessions
        .read()
        .await
        .get(&tab_id)
        .map(|session| session.connected)
        .unwrap_or(false);
    stop_session_worker(&state, &tab_id).await;
    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = crate::services::WorkspaceTabStatus::Closed;
        }
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.connected = false;
            session.remote_files_loading = false;
            session.remote_files = Vec::new();
            session.shell_user = None;
            session.file_access_mode = "user".to_string();
            session.has_reusable_sudo_auth = false;
            session.system_metrics = None;
        }
    }

    // Cancelling an SSH worker intentionally suppresses its normal worker
    // shutdown callback. Emit the same terminal notice/state that a network
    // disconnect would have emitted, otherwise the renderer only receives a
    // workspace snapshot and keeps showing the last shell prompt forever.
    if was_connected {
        crate::sessions::terminal::emit_terminal_data(&app, &tab_id, "\r\n连接已断开\r\n").await;
    }
    crate::sessions::terminal::set_terminal_state(
        &app,
        &tab_id,
        "连接已断开".to_string(),
        crate::services::WorkspaceTabStatus::Closed,
    )
    .await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_close_tab(app: AppHandle, tab_id: String) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    // 检查是否是分屏 root：如果是，关闭所有 leaf 的 worker 和传输
    let pane_leaf_ids: Vec<String> = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.pane_root.as_ref())
            .map(|root| root.leaf_tab_ids())
            .unwrap_or_default()
    };

    // 检查是否是某个 root 的 leaf
    let containing_root_id: Option<String> = if pane_leaf_ids.is_empty() {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| {
                t.pane_root
                    .as_ref()
                    .map(|root| root.leaf_tab_ids().iter().any(|id| id == &tab_id))
                    .unwrap_or(false)
            })
            .map(|t| t.id.clone())
    } else {
        None
    };

    if let Some(root_id) = containing_root_id {
        // tab_id 是某个 root 的 leaf，等价于 close_pane
        // 暂停传输
        let _ = crate::services::transfers::pause_for_tab(
            &app,
            &tab_id,
            "Pane 关闭后已暂停，可在重连后继续传输",
        )
        .await;
        stop_session_worker(&state, &tab_id).await;
        {
            let mut tabs = state.tabs.write().await;
            let root_idx = tabs
                .iter()
                .position(|t| t.id == root_id)
                .ok_or_else(|| AppError::Storage(format!("Root tab not found: {}", root_id)))?;
            {
                let root_tab = &mut tabs[root_idx];
                if let Some(ref mut pane_root) = root_tab.pane_root {
                    pane_root.remove_leaf(&tab_id);
                    if let crate::services::PaneNode::Leaf { .. } = pane_root {
                        root_tab.pane_root = None;
                    }
                }
            }
            tabs.retain(|t| t.id != tab_id);
            let mut sessions = state.sessions.write().await;
            sessions.remove(&tab_id);
            let mut active_panes = state.active_pane_tab_id_by_root.write().await;
            if let Some(root_tab) = tabs.get(root_idx) {
                if root_tab.pane_root.is_none() {
                    active_panes.remove(&root_id);
                } else if let Some(ref pane_root) = root_tab.pane_root {
                    let leaves = pane_root.leaf_tab_ids();
                    if active_panes
                        .get(&root_id)
                        .map(|id| id == &tab_id || !leaves.contains(id))
                        .unwrap_or(true)
                    {
                        if let Some(first) = leaves.first() {
                            active_panes.insert(root_id.clone(), first.clone());
                        }
                    }
                }
            }
        }
    } else {
        // 普通关闭（可能是独立 tab 或分屏 root）
        let all_ids_to_close = if pane_leaf_ids.is_empty() {
            vec![tab_id.clone()]
        } else {
            pane_leaf_ids
        };

        for id in &all_ids_to_close {
            crate::services::transfers::pause_for_tab(
                &app,
                id,
                "标签关闭后已暂停，可在重连后继续传输",
            )
            .await?;
            stop_session_worker(&state, id).await;
        }
        {
            let mut tabs = state.tabs.write().await;
            tabs.retain(|t| !all_ids_to_close.contains(&t.id));

            let mut active = state.active_tab_id.write().await;
            if *active == Some(tab_id.clone()) {
                *active = tabs.last().map(|t| t.id.clone());
            }

            let mut sessions = state.sessions.write().await;
            for id in &all_ids_to_close {
                sessions.remove(id);
            }
            let mut active_panes = state.active_pane_tab_id_by_root.write().await;
            active_panes.remove(&tab_id);
        }
    }

    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_write_terminal(
    app: AppHandle,
    tab_id: String,
    data: String,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    send_terminal_input(&state, &tab_id, data).await
}

#[tauri::command]
pub fn app_subscribe_terminal_data(app: AppHandle, channel: Channel<serde_json::Value>) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state.register_terminal_output_channel(channel);
}

#[tauri::command]
pub async fn app_resize_terminal(
    app: AppHandle,
    tab_id: String,
    cols: u32,
    rows: u32,
    width: u32,
    height: u32,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let workers = state.workers.read().await;
    if let Some(sender) = workers.get(&tab_id) {
        let _ = timeout(
            WORKER_CMD_SEND_TIMEOUT,
            sender.send(WorkerCmd::ResizeTerminal {
                cols,
                rows,
                width,
                height,
            }),
        )
        .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn app_open_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
) -> Result<serde_json::Value, AppError> {
    refresh_remote_files(&app, &tab_id, &target_path).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.remote_path = target_path;
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_set_follow_shell_cwd(
    app: AppHandle,
    tab_id: String,
    enabled: bool,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let cwd_to_follow = {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.follow_shell_cwd = enabled;
            if enabled && session.shell_cwd.as_deref() != Some(session.remote_path.as_str()) {
                session.shell_cwd.clone()
            } else {
                None
            }
        } else {
            None
        }
    };

    // Match Electron's recovery behaviour: enabling follow must immediately
    // catch the file pane up to the most recently reported shell directory.
    // Waiting for another `cd` leaves the toggle active while the pane remains
    // stale forever when the initial listing happened to fail.
    if let Some(cwd) = cwd_to_follow {
        match refresh_remote_files(&app, &tab_id, &cwd).await {
            Ok(()) => {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    if session.follow_shell_cwd
                        && session.shell_cwd.as_deref() == Some(cwd.as_str())
                    {
                        session.remote_path = cwd;
                    }
                }
            }
            Err(error) => {
                // CWD reporting is best-effort in Electron too. A directory
                // the SFTP user cannot read must not make the toggle itself
                // fail or interfere with the interactive terminal.
                crate::services::logging::ssh_debug(
                    &app,
                    &tab_id,
                    format!("CWD follow recovery failed for {cwd}: {error}"),
                );
            }
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_read_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    encoding: Option<String>,
) -> Result<String, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::ReadRemoteFile {
        path: target_path,
        encoding: enc,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_write_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    content: String,
    encoding: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::WriteRemoteFile {
        path: target_path.clone(),
        content,
        encoding: enc,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_create_remote_directory(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CreateRemoteDirectory {
        parent_path: parent_path.clone(),
        name,
        respond_to: tx,
    })
    .await?;

    let _ = refresh_remote_files(&app, &tab_id, &parent_path).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_create_remote_file(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CreateRemoteFile {
        parent_path: parent_path.clone(),
        name,
        respond_to: tx,
    })
    .await?;

    let _ = refresh_remote_files(&app, &tab_id, &parent_path).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_copy_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
    target_type: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CopyRemotePath {
        target_path,
        destination_path: destination_path.clone(),
        target_type,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&destination_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_move_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::MoveRemotePath {
        target_path: target_path.clone(),
        destination_path: destination_path.clone(),
        respond_to: tx,
    })
    .await?;

    let parent_src = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let parent_dest = std::path::Path::new(&destination_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());

    let _ = refresh_remote_files(&app, &tab_id, &parent_src).await;
    if parent_src != parent_dest {
        let _ = refresh_remote_files(&app, &tab_id, &parent_dest).await;
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_rename_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    new_name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::RenameRemotePath {
        target_path: target_path.clone(),
        new_name,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_delete_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    target_type: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::DeleteRemotePath {
        target_path: target_path.clone(),
        target_type,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PermissionApplyTarget {
    All,
    Files,
    Directories,
}

impl PermissionApplyTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Files => "files",
            Self::Directories => "directories",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePermissionChangeOptions {
    mode: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    apply_to: Option<PermissionApplyTarget>,
}

fn parse_remote_permission_mode(mode: &str) -> Result<u32, AppError> {
    let trimmed = mode.trim();
    if !(3..=4).contains(&trimmed.len())
        || !trimmed
            .chars()
            .all(|character| matches!(character, '0'..='7'))
    {
        return Err(AppError::Command(
            "权限值必须是 3 到 4 位八进制数字，例如 755".to_string(),
        ));
    }
    u32::from_str_radix(trimmed, 8).map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub async fn app_change_remote_permissions(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    options: RemotePermissionChangeOptions,
) -> Result<serde_json::Value, AppError> {
    let permissions = parse_remote_permission_mode(&options.mode)?;
    let recursive = options.recursive;
    let apply_to = options
        .apply_to
        .unwrap_or(PermissionApplyTarget::All)
        .as_str()
        .to_string();
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::ChangeRemotePermissions {
        target_path: target_path.clone(),
        permissions,
        recursive,
        apply_to,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_set_remote_file_access_mode(
    app: AppHandle,
    tab_id: String,
    mode: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let sudo_user = options
        .as_ref()
        .and_then(|o| o.get("sudoUser"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let sudo_password = options
        .as_ref()
        .and_then(|o| o.get("sudoPassword"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::SetRemoteFileAccessMode {
        mode,
        sudo_user,
        sudo_password,
        respond_to: tx,
    })
    .await?;

    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_queue_upload(
    app: AppHandle,
    file_names: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::queue_upload(&app, file_names).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_upload_file(
    app: AppHandle,
    tab_id: String,
    local_path: String,
    remote_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    crate::services::transfers::create_upload(
        &app,
        tab_id,
        local_path,
        remote_directory,
        target_name,
    )
    .await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_download_file(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    crate::services::transfers::create_download(
        &app,
        tab_id,
        remote_path,
        local_directory,
        target_name,
    )
    .await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_download_remote_path(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    target_type: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    match target_type.as_str() {
        "file" => app_download_file(app, tab_id, remote_path, local_directory, options).await,
        "folder" => {
            crate::services::transfers::create_download_directory(
                &app,
                tab_id,
                remote_path,
                local_directory,
                target_name,
            )
            .await?;
            get_workspace_snapshot(app).await
        }
        _ => Err(AppError::Command("远端传输目标类型无效".to_string())),
    }
}

#[tauri::command]
pub async fn app_cancel_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::discard(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_pause_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::pause(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_resume_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::resume(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_discard_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::discard(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_clear_transfers(
    app: AppHandle,
    transfer_ids: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::clear(&app, transfer_ids).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_list_ssh_tunnels(
    app: AppHandle,
    tab_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::ListSshTunnels {
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_create_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule: serde_json::Value,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CreateSshTunnel {
        rule,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_start_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::StartSshTunnel {
        rule_id,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_stop_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::StopSshTunnel {
        rule_id,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_delete_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::DeleteSshTunnel {
        rule_id,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_resolve_ssh_interaction(
    app: AppHandle,
    request_id: String,
    response: serde_json::Value,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = {
        let mut pending = state.pending_interactions.write().await;
        pending.remove(&request_id)
    };
    if let Some(tx) = sender {
        // Sender error means the receiver was dropped (handshake timed out
        // or the worker exited) — not actionable, ignore.
        let _ = tx.send(response);
    }
    Ok(())
}

// ==========================================
// Phase 2 commands: profile / folder / command CRUD
// ==========================================
//
// These commands delegate to `services::profile_ops`, which mirrors the
// Electron `FileProfileRepository` semantics (group/parentId self-healing,
// secrets stripping, cascade rename / delete, ordering).

#[tauri::command]
pub async fn app_create_profile(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::create_profile(&app, input)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_profile(
    app: AppHandle,
    profile_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let profile = crate::services::profile_ops::update_profile(&app, &profile_id, input)?;
    let reconnect_mode = crate::services::workspace::reconnect_mode_for_profile(&profile);
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    for session in sessions.values_mut() {
        if session.profile_id == profile_id {
            session.reconnect_mode = reconnect_mode.clone();
        }
    }
    drop(sessions);
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_profile(&app, &profile_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_folder(&app, &folder_id, updates)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_folder(&app, &folder_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_entity_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_entity_order(&app, &id, new_parent_id, new_order)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_folder(&app, &folder_id, updates)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_command_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_command_folder(&app, &folder_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_order(&app, &id, new_parent_id, new_order)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_template(
    app: AppHandle,
    command_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_template(&app, &command_id, input)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_command_template(
    app: AppHandle,
    command_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_command_template(&app, &command_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

/// Render and send a command template to an active SSH session.
///
/// This intentionally performs the rendering in the main process: the command
/// source is durable storage, while the renderer only supplies positional
/// arguments and whether the final carriage return is desired. It mirrors the
/// Electron workspace service and keeps arbitrary command text out of the IPC
/// surface.
#[tauri::command]
pub async fn app_execute_command_template(
    app: AppHandle,
    tab_id: String,
    command_id: String,
    args: Option<Vec<String>>,
    options: Option<Value>,
) -> Result<Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.session_type.clone())
    };
    if session_type.as_deref() != Some("ssh") {
        return Err(AppError::Command("只有 SSH 会话支持快捷命令".to_string()));
    }

    let commands = read_json_array(&app, "commands.json")?;
    let command = commands
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(command_id.as_str()))
        .ok_or_else(|| AppError::Storage(format!("Command not found: {command_id}")))?;
    let template = command
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Storage(format!("Command is invalid: {command_id}")))?;
    let rendered_command = render_command_template(template, args.as_deref().unwrap_or_default());
    let append_carriage_return = options
        .as_ref()
        .and_then(|value| value.get("appendCarriageReturn"))
        .and_then(Value::as_bool)
        .or_else(|| command.get("appendCarriageReturn").and_then(Value::as_bool))
        .unwrap_or(true);

    let payload = if append_carriage_return {
        format!("{rendered_command}\r")
    } else {
        rendered_command.clone()
    };
    send_terminal_input(&state, &tab_id, payload).await?;

    Ok(serde_json::json!({ "renderedCommand": rendered_command }))
}

fn render_command_template(template: &str, args: &[String]) -> String {
    // `[p#1]` is the durable command-template placeholder format shared with
    // Electron. Invalid/out-of-range references deliberately render as an
    // empty string so existing command libraries retain their behavior.
    let placeholder = Regex::new(r"\[p#(\d+)\]").expect("constant placeholder regex must compile");
    placeholder
        .replace_all(template, |captures: &regex::Captures<'_>| {
            captures
                .get(1)
                .and_then(|index| index.as_str().parse::<usize>().ok())
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| args.get(index))
                .cloned()
                .unwrap_or_default()
        })
        .into_owned()
}

#[cfg(test)]
mod command_template_tests {
    use super::render_command_template;

    #[test]
    fn renders_positional_command_template_arguments() {
        assert_eq!(
            render_command_template(
                "deploy [p#1] --region [p#2] --empty=[p#3]",
                &["api".to_string(), "cn-north".to_string(),]
            ),
            "deploy api --region cn-north --empty="
        );
    }
}

#[cfg(test)]
mod split_pane_close_tests {
    use super::remove_split_pane_from_tabs;
    use crate::services::{PaneNode, SplitDirection, WorkspaceTab, WorkspaceTabStatus};

    fn tab(id: &str, pane_root: Option<PaneNode>, pane_root_tab_id: Option<&str>) -> WorkspaceTab {
        WorkspaceTab {
            id: id.to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status: WorkspaceTabStatus::Connected,
            pane_root,
            pane_root_tab_id: pane_root_tab_id.map(str::to_string),
        }
    }

    #[test]
    fn closing_a_child_pane_keeps_the_existing_root_tab() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "child".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("child", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "child").unwrap();

        assert_eq!(outcome.root_tab_id, "root");
        assert!(!outcome.keeps_split);
        assert_eq!(outcome.remaining_pane_tab_ids, vec!["root"]);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "root");
        assert!(tabs[0].pane_root.is_none());
    }

    #[test]
    fn closing_the_original_root_leaf_promotes_a_surviving_pane() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Split {
                            direction: SplitDirection::Column,
                            children: vec![
                                PaneNode::Leaf {
                                    tab_id: "second".to_string(),
                                },
                                PaneNode::Leaf {
                                    tab_id: "third".to_string(),
                                },
                            ],
                            weights: vec![0.5, 0.5],
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("second", None, Some("root")),
            tab("third", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "root").unwrap();

        assert_eq!(outcome.root_tab_id, "second");
        assert!(outcome.keeps_split);
        assert_eq!(outcome.remaining_pane_tab_ids, vec!["second", "third"]);
        assert_eq!(tabs.len(), 2);

        let promoted_root = tabs.iter().find(|tab| tab.id == "second").unwrap();
        assert!(promoted_root.pane_root.is_some());
        assert!(promoted_root.pane_root_tab_id.is_none());
        assert_eq!(
            tabs.iter()
                .find(|tab| tab.id == "third")
                .and_then(|tab| tab.pane_root_tab_id.as_deref()),
            Some("second")
        );
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::claim_reconnect_tab;
    use crate::services::{WorkspaceTab, WorkspaceTabStatus};

    fn tab(status: WorkspaceTabStatus) -> WorkspaceTab {
        WorkspaceTab {
            id: "tab-1".to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status,
            pane_root: None,
            pane_root_tab_id: None,
        }
    }

    #[test]
    fn reconnect_can_only_be_claimed_once_while_connecting() {
        let mut tabs = vec![tab(WorkspaceTabStatus::Closed)];

        assert!(claim_reconnect_tab(&mut tabs, "tab-1"));
        assert_eq!(tabs[0].status, WorkspaceTabStatus::Connecting);
        assert!(!claim_reconnect_tab(&mut tabs, "tab-1"));
    }

    #[test]
    fn reconnect_does_not_claim_an_unknown_tab() {
        let mut tabs = vec![tab(WorkspaceTabStatus::Closed)];

        assert!(!claim_reconnect_tab(&mut tabs, "missing"));
        assert_eq!(tabs[0].status, WorkspaceTabStatus::Closed);
    }
}

#[cfg(test)]
mod architecture_tests {
    use super::resolve_native_arch;

    #[test]
    fn reports_apple_silicon_when_x64_process_runs_under_rosetta() {
        assert_eq!(resolve_native_arch("macos", "x86_64", true), "arm64");
    }

    #[test]
    fn canonicalizes_native_rust_architecture_names() {
        assert_eq!(resolve_native_arch("macos", "aarch64", true), "arm64");
        assert_eq!(resolve_native_arch("macos", "x86_64", false), "x64");
        assert_eq!(resolve_native_arch("linux", "x86_64", false), "x64");
    }
}

#[cfg(test)]
mod window_lifecycle_tests {
    use super::renderer_approved_close_should_destroy;

    #[test]
    fn main_window_close_keeps_the_lifecycle_guard() {
        assert!(!renderer_approved_close_should_destroy("main"));
        assert!(renderer_approved_close_should_destroy(
            "file-editor-local-1"
        ));
        assert!(renderer_approved_close_should_destroy("connection-manager"));
    }
}

#[cfg(test)]
mod ui_state_tests {
    use super::normalize_ui_state;

    #[test]
    fn reads_current_object_ui_state() {
        let states = normalize_ui_state(serde_json::json!({ "main.tab-ui": "tabs" })).unwrap();
        assert_eq!(
            states.get("main.tab-ui").and_then(|value| value.as_str()),
            Some("tabs")
        );
    }

    #[test]
    fn migrates_electron_and_legacy_array_ui_state() {
        let electron = normalize_ui_state(serde_json::json!({
            "version": 1,
            "values": { "ssh-key-manager-ui": "folders" }
        }))
        .unwrap();
        assert_eq!(
            electron
                .get("ssh-key-manager-ui")
                .and_then(|value| value.as_str()),
            Some("folders")
        );

        let legacy = normalize_ui_state(serde_json::json!([
            { "key": "ssh-key-manager-ui", "value": "legacy-folders" }
        ]))
        .unwrap();
        assert_eq!(
            legacy
                .get("ssh-key-manager-ui")
                .and_then(|value| value.as_str()),
            Some("legacy-folders")
        );
    }
}

#[cfg(test)]
mod ui_preferences_tests {
    use super::{normalize_ui_preferences, UiPreferences};

    #[test]
    fn falls_back_to_safe_values_for_unknown_preferences() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "unknown-theme".to_string(),
            locale: "unknown-locale".to_string(),
        });

        assert_eq!(preferences.theme, "default-dark");
        assert_eq!(preferences.locale, "zhCN");
    }

    #[test]
    fn keeps_supported_preferences_unchanged() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-light".to_string(),
            locale: "enUS".to_string(),
        });

        assert_eq!(preferences.theme, "default-light");
        assert_eq!(preferences.locale, "enUS");
    }
}

#[cfg(test)]
mod permission_contract_tests {
    use super::{
        parse_remote_permission_mode, PermissionApplyTarget, RemotePermissionChangeOptions,
    };

    #[test]
    fn reads_shared_camel_case_permission_contract() {
        let options: RemotePermissionChangeOptions = serde_json::from_value(serde_json::json!({
            "mode": "0640",
            "recursive": true,
            "applyTo": "files"
        }))
        .expect("shared permission options should deserialize");

        assert_eq!(parse_remote_permission_mode(&options.mode).unwrap(), 0o640);
        assert!(options.recursive);
        assert!(matches!(
            options.apply_to,
            Some(PermissionApplyTarget::Files)
        ));
    }

    #[test]
    fn rejects_legacy_permissions_field_instead_of_defaulting_to_0755() {
        let options = serde_json::from_value::<RemotePermissionChangeOptions>(serde_json::json!({
            "permissions": 384,
            "recursive": false
        }));
        assert!(options.is_err());
    }

    #[test]
    fn validates_octal_permission_modes() {
        assert_eq!(parse_remote_permission_mode("600").unwrap(), 0o600);
        assert_eq!(parse_remote_permission_mode("755").unwrap(), 0o755);
        assert!(parse_remote_permission_mode("888").is_err());
        assert!(parse_remote_permission_mode("75").is_err());
    }
}

#[cfg(test)]
mod external_url_tests {
    use super::validate_external_url;

    #[test]
    fn external_url_policy_accepts_only_web_links() {
        for allowed in [
            "https://github.com/St0ff3l/fileterm",
            "http://127.0.0.1/docs",
        ] {
            assert!(validate_external_url(allowed).is_ok());
        }
        for denied in [
            "file:///etc/passwd",
            "ssh://example.com",
            "javascript:alert(1)",
        ] {
            assert!(validate_external_url(denied).is_err());
        }
        assert!(validate_external_url("not a url").is_err());
    }
}
