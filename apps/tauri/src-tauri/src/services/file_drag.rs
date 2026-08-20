use std::collections::HashSet;
use std::path::Path;
#[cfg(not(target_os = "macos"))]
use std::path::PathBuf;
#[cfg(not(target_os = "macos"))]
use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use tauri::{Emitter, Manager, WebviewWindow};
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use tokio::sync::oneshot;

use crate::AppError;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;

#[cfg(target_os = "windows")]
#[path = "windows_drag.rs"]
mod windows_drag;

#[cfg(target_os = "linux")]
#[path = "linux_drag.rs"]
mod linux_drag;

#[cfg(not(target_os = "macos"))]
const DRAG_STAGING_CLEANUP_DELAY: Duration = Duration::from_secs(300);
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DRAG_IMAGE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/128x128.png"));

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileDragItem {
    pub path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
}

/// Shell 拖拽图像（Windows DragImageBits 同源）：PNG data URL 由 renderer
/// 按物理像素密度渲染，尺寸/偏移均为物理像素，偏移为光标热点在图像内的
/// 位置（负值表示图像位于光标右下方）。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDragImage {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

/// Start a native drag for selected remote items.
///
/// macOS uses `NSFilePromiseProvider`, so the drag session starts immediately
/// and the transfer begins only after Finder (or another drop target) gives us
/// its destination URL. Windows and Linux use lazy native data providers: the
/// drag session starts without downloading bytes, and the first request for
/// native file data prepares a temporary local path. This keeps the transfer
/// after the user has actually dropped into Explorer/Nautilus or FileTerm.
pub async fn start_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
    drag_image: Option<RemoteDragImage>,
) -> Result<(), AppError> {
    if items.is_empty() {
        return Err(AppError::Command("没有可拖出的远程文件".to_string()));
    }
    validate_remote_drag_items(&items)?;

    #[cfg(target_os = "macos")]
    {
        return macos::start_remote_file_drag(app, window_label, tab_id, items, drag_image).await;
    }

    #[cfg(target_os = "windows")]
    {
        return windows_drag::start_remote_file_drag(app, window_label, tab_id, items, drag_image)
            .await;
    }

    #[cfg(target_os = "linux")]
    {
        return linux_drag::start_remote_file_drag(app, window_label, tab_id, items, drag_image)
            .await;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    start_staged_remote_file_drag(app, window_label, tab_id, items).await
}

/// Compatibility fallback for platforms without a file-promise drag provider.
///
/// This is intentionally isolated from the macOS path so it cannot silently
/// become the default there again.
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
async fn start_staged_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
) -> Result<(), AppError> {
    let stage_root =
        std::env::temp_dir().join(format!("fileterm-remote-drag-{}", uuid::Uuid::new_v4()));
    let absolute_paths = prepare_staged_remote_paths(app, tab_id, &items, &stage_root).await?;

    let window = match app.get_webview_window(window_label) {
        Some(window) => window,
        None => {
            if let Err(cleanup_error) =
                crate::services::transfers::cleanup_drag_transfers_in_stage(app, &stage_root).await
            {
                log::warn!("清理拖出阶段任务失败：{cleanup_error}");
            }
            remove_staging_dir(&stage_root).await;
            return Err(AppError::Command("拖出窗口不存在".to_string()));
        }
    };
    if let Err(error) = start_native_drag(
        app.clone(),
        window,
        window_label,
        absolute_paths,
        stage_root.clone(),
    )
    .await
    {
        if let Err(cleanup_error) =
            crate::services::transfers::cleanup_drag_transfers_in_stage(app, &stage_root).await
        {
            log::warn!("清理拖出阶段任务失败：{cleanup_error}");
        }
        remove_staging_dir(&stage_root).await;
        return Err(error);
    }

    Ok(())
}

/// Download remote drag items into a private temporary directory.
///
/// This helper is deliberately called by the platform data provider, rather
/// than by `start_remote_file_drag`, on Windows/Linux. As a result, creating
/// the drag session itself never starts a transfer; the helper runs only when
/// the native drop target asks for actual file data.
#[cfg(not(target_os = "macos"))]
pub(crate) async fn prepare_staged_remote_paths(
    app: &AppHandle,
    tab_id: &str,
    items: &[RemoteFileDragItem],
    stage_root: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    if let Err(error) = tokio::fs::create_dir_all(stage_root).await {
        remove_staging_dir(stage_root).await;
        return Err(AppError::Command(format!("创建拖出临时目录失败：{error}")));
    }

    let mut names = HashSet::new();
    let mut transfer_ids = Vec::with_capacity(items.len());
    let mut staged_paths = Vec::with_capacity(items.len());

    for item in items {
        let name = match safe_drag_name(&item.name) {
            Ok(name) => name,
            Err(error) => {
                cleanup_drag_transfers(app, &transfer_ids).await;
                remove_staging_dir(stage_root).await;
                return Err(error);
            }
        };
        if !names.insert(drag_name_key(&name)) {
            cleanup_drag_transfers(app, &transfer_ids).await;
            remove_staging_dir(stage_root).await;
            return Err(AppError::Command(format!("拖出项目名称重复：{name}")));
        }

        let local_directory = stage_root.to_string_lossy().into_owned();
        let transfer_result = match item.item_type.as_str() {
            "file" => {
                crate::services::transfers::create_download(
                    app,
                    tab_id.to_string(),
                    item.path.clone(),
                    local_directory,
                    Some(name.clone()),
                )
                .await
            }
            "folder" => {
                crate::services::transfers::create_download_directory(
                    app,
                    tab_id.to_string(),
                    item.path.clone(),
                    local_directory,
                    Some(name.clone()),
                )
                .await
            }
            _ => Err(AppError::Command("远程拖出项目类型无效".to_string())),
        };

        let transfer_id = match transfer_result {
            Ok(transfer_id) => transfer_id,
            Err(error) => {
                cleanup_drag_transfers(app, &transfer_ids).await;
                remove_staging_dir(stage_root).await;
                return Err(error);
            }
        };
        transfer_ids.push(transfer_id);
        staged_paths.push(stage_root.join(name));
    }

    for transfer_id in &transfer_ids {
        if let Err(error) = crate::services::transfers::wait_for_transfer(app, transfer_id).await {
            cleanup_drag_transfers(app, &transfer_ids).await;
            remove_staging_dir(stage_root).await;
            return Err(error);
        }
    }

    let mut absolute_paths = Vec::with_capacity(staged_paths.len());
    for path in staged_paths {
        match std::fs::canonicalize(&path) {
            Ok(path) => absolute_paths.push(path),
            Err(error) => {
                cleanup_drag_transfers(app, &transfer_ids).await;
                remove_staging_dir(stage_root).await;
                return Err(AppError::Command(format!(
                    "拖出文件准备失败：{}：{error}",
                    path.display()
                )));
            }
        }
    }
    Ok(absolute_paths)
}

fn validate_remote_drag_items(items: &[RemoteFileDragItem]) -> Result<(), AppError> {
    let mut names = HashSet::new();
    for item in items {
        let name = safe_drag_name(&item.name)?;
        if item.path.trim().is_empty() || item.path.contains('\0') {
            return Err(AppError::Command("远程拖出路径无效".to_string()));
        }
        if !matches!(item.item_type.as_str(), "file" | "folder") {
            return Err(AppError::Command("远程拖出项目类型无效".to_string()));
        }
        if !names.insert(drag_name_key(&name)) {
            return Err(AppError::Command(format!("拖出项目名称重复：{name}")));
        }
    }
    Ok(())
}

fn drag_name_key(name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        name.to_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        name.to_string()
    }
}

fn safe_drag_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('\0')
        || name.contains('/')
        || name.contains('\\')
        || Path::new(name).is_absolute()
    {
        return Err(AppError::Command("远程拖出文件名无效".to_string()));
    }

    #[cfg(target_os = "windows")]
    {
        if name.ends_with(['.', ' '])
            || name.chars().any(|character| {
                character.is_control()
                    || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            })
            || is_windows_reserved_name(name)
        {
            return Err(AppError::Command(
                "远程拖出文件名在 Windows 上无效".to_string(),
            ));
        }
    }

    Ok(name.to_string())
}

#[cfg(target_os = "windows")]
fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

#[cfg(not(target_os = "macos"))]
async fn cleanup_drag_transfers(app: &AppHandle, transfer_ids: &[String]) {
    for transfer_id in transfer_ids {
        if let Err(error) =
            crate::services::transfers::cleanup_drag_transfer(app, transfer_id).await
        {
            log::warn!("清理拖出传输任务失败 {transfer_id}: {error}");
        }
    }
}

#[cfg(not(target_os = "macos"))]
async fn remove_staging_dir(path: &Path) {
    if let Err(error) = tokio::fs::remove_dir_all(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!("清理拖出临时目录失败 {}: {error}", path.display());
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn remove_staging_dir_sync(path: &Path) {
    if let Err(error) = std::fs::remove_dir_all(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!("清理拖出临时目录失败 {}: {error}", path.display());
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn schedule_staging_cleanup(path: PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(DRAG_STAGING_CLEANUP_DELAY);
        remove_staging_dir_sync(&path);
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
async fn start_native_drag(
    app: AppHandle,
    window: WebviewWindow,
    window_label: &str,
    paths: Vec<PathBuf>,
    staging_root: PathBuf,
) -> Result<(), AppError> {
    let (result_sender, result_receiver) = oneshot::channel::<Result<(), String>>();
    let window_for_main = window.clone();
    let app_for_main = app.clone();
    let window_label = window_label.to_string();
    window
        .run_on_main_thread(move || {
            let result = start_native_drag_on_main_thread(
                app_for_main,
                window_label,
                window_for_main,
                paths,
                staging_root,
            );
            let _ = result_sender.send(result);
        })
        .map_err(|error| AppError::Command(format!("切换到原生拖出线程失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| AppError::Command("原生拖出线程未响应".to_string()))?
        .map_err(AppError::Command)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn start_native_drag_on_main_thread(
    app: AppHandle,
    window_label: String,
    window: WebviewWindow,
    paths: Vec<PathBuf>,
    staging_root: PathBuf,
) -> Result<(), String> {
    let callback_root = staging_root.clone();
    let callback = move |_, _| {
        schedule_staging_cleanup(callback_root.clone());
        let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-finished", ());
    };
    let result = {
        #[cfg(target_os = "linux")]
        {
            let gtk_window = window.gtk_window().map_err(|error| error.to_string())?;
            drag::start_drag(
                &gtk_window,
                drag::DragItem::Files(paths),
                drag::Image::Raw(DRAG_IMAGE.to_vec()),
                callback,
                drag::Options::default(),
            )
        }
        #[cfg(not(target_os = "linux"))]
        {
            drag::start_drag(
                &window,
                drag::DragItem::Files(paths),
                drag::Image::Raw(DRAG_IMAGE.to_vec()),
                callback,
                drag::Options::default(),
            )
        }
    };

    if let Err(error) = result {
        remove_staging_dir_sync(&staging_root);
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, name: &str, item_type: &str) -> RemoteFileDragItem {
        RemoteFileDragItem {
            path: path.to_string(),
            name: name.to_string(),
            item_type: item_type.to_string(),
        }
    }

    #[test]
    fn validates_mixed_file_and_folder_drag_items() {
        let items = vec![
            item("/remote/file.txt", "file.txt", "file"),
            item("/remote", "remote", "folder"),
        ];

        assert!(validate_remote_drag_items(&items).is_ok());
    }

    #[test]
    fn rejects_duplicate_names_after_normalization() {
        let items = vec![
            item("/remote/a", "a", "file"),
            item("/remote/b", " a ", "folder"),
        ];

        assert!(validate_remote_drag_items(&items).is_err());
    }

    #[test]
    fn rejects_invalid_paths_and_item_types_before_starting_drag() {
        assert!(validate_remote_drag_items(&[item("", "file.txt", "file")]).is_err());
        assert!(
            validate_remote_drag_items(&[item("/remote/file.txt", "file.txt", "link")]).is_err()
        );
        assert!(
            validate_remote_drag_items(&[item("/remote/file.txt", "../file.txt", "file")]).is_err()
        );
    }
}
