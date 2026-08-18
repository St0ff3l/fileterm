#[cfg(not(target_os = "macos"))]
use std::collections::HashSet;
use std::path::Path;
#[cfg(not(target_os = "macos"))]
use std::path::PathBuf;
#[cfg(not(target_os = "macos"))]
use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;
#[cfg(not(target_os = "macos"))]
use tauri::{Manager, WebviewWindow};
#[cfg(not(target_os = "macos"))]
use tokio::sync::oneshot;

use crate::AppError;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;

#[cfg(not(target_os = "macos"))]
const DRAG_STAGING_CLEANUP_DELAY: Duration = Duration::from_secs(300);
#[cfg(not(target_os = "macos"))]
const DRAG_IMAGE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/128x128.png"));

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileDragItem {
    pub path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
}

/// Start a native drag for selected remote items.
///
/// macOS uses `NSFilePromiseProvider`, so the drag session starts immediately
/// and the transfer begins only after Finder (or another drop target) gives us
/// its destination URL. Other platforms currently use the local-path fallback
/// below until their virtual-file drag APIs are implemented.
pub async fn start_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
) -> Result<(), AppError> {
    if items.is_empty() {
        return Err(AppError::Command("没有可拖出的远程文件".to_string()));
    }

    #[cfg(target_os = "macos")]
    {
        return macos::start_remote_file_drag(app, window_label, tab_id, items).await;
    }

    #[cfg(not(target_os = "macos"))]
    start_staged_remote_file_drag(app, window_label, tab_id, items).await
}

/// Compatibility fallback for platforms without a file-promise drag provider.
///
/// This is intentionally isolated from the macOS path so it cannot silently
/// become the default there again.
#[cfg(not(target_os = "macos"))]
async fn start_staged_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
) -> Result<(), AppError> {
    let stage_root =
        std::env::temp_dir().join(format!("fileterm-remote-drag-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&stage_root)
        .await
        .map_err(|error| AppError::Command(format!("创建拖出临时目录失败：{error}")))?;

    let mut names = HashSet::new();
    let mut transfer_ids = Vec::with_capacity(items.len());
    let mut staged_paths = Vec::with_capacity(items.len());

    for item in items {
        let name = match safe_drag_name(&item.name) {
            Ok(name) => name,
            Err(error) => {
                cancel_drag_transfers(app, &transfer_ids).await;
                remove_staging_dir(&stage_root).await;
                return Err(error);
            }
        };
        if !names.insert(name.clone()) {
            cancel_drag_transfers(app, &transfer_ids).await;
            remove_staging_dir(&stage_root).await;
            return Err(AppError::Command(format!("拖出项目名称重复：{name}")));
        }

        let local_directory = stage_root.to_string_lossy().into_owned();
        let transfer_result = match item.item_type.as_str() {
            "file" => {
                crate::services::transfers::create_download(
                    app,
                    tab_id.to_string(),
                    item.path,
                    local_directory,
                    Some(name.clone()),
                )
                .await
            }
            "folder" => {
                crate::services::transfers::create_download_directory(
                    app,
                    tab_id.to_string(),
                    item.path,
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
                cancel_drag_transfers(app, &transfer_ids).await;
                remove_staging_dir(&stage_root).await;
                return Err(error);
            }
        };
        transfer_ids.push(transfer_id);
        staged_paths.push(stage_root.join(name));
    }

    for transfer_id in &transfer_ids {
        if let Err(error) = crate::services::transfers::wait_for_transfer(app, transfer_id).await {
            cancel_drag_transfers(app, &transfer_ids).await;
            remove_staging_dir(&stage_root).await;
            return Err(error);
        }
    }

    let mut absolute_paths = Vec::with_capacity(staged_paths.len());
    for path in staged_paths {
        let absolute_path = match std::fs::canonicalize(&path) {
            Ok(path) => path,
            Err(error) => {
                remove_staging_dir(&stage_root).await;
                return Err(AppError::Command(format!(
                    "拖出文件准备失败：{}：{error}",
                    path.display()
                )));
            }
        };
        absolute_paths.push(absolute_path);
    }

    let window = app
        .get_webview_window(window_label)
        .ok_or_else(|| AppError::Command("拖出窗口不存在".to_string()))?;
    if let Err(error) = start_native_drag(window, absolute_paths, stage_root.clone()).await {
        remove_staging_dir(&stage_root).await;
        return Err(error);
    }

    Ok(())
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
    Ok(name.to_string())
}

#[cfg(not(target_os = "macos"))]
async fn cancel_drag_transfers(app: &AppHandle, transfer_ids: &[String]) {
    for transfer_id in transfer_ids {
        let _ = crate::services::transfers::discard(app, transfer_id.clone()).await;
    }
}

#[cfg(not(target_os = "macos"))]
async fn remove_staging_dir(path: &Path) {
    let _ = tokio::fs::remove_dir_all(path).await;
}

#[cfg(not(target_os = "macos"))]
fn remove_staging_dir_sync(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

#[cfg(not(target_os = "macos"))]
fn schedule_staging_cleanup(path: PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(DRAG_STAGING_CLEANUP_DELAY);
        remove_staging_dir_sync(&path);
    });
}

#[cfg(not(target_os = "macos"))]
async fn start_native_drag(
    window: WebviewWindow,
    paths: Vec<PathBuf>,
    staging_root: PathBuf,
) -> Result<(), AppError> {
    let (result_sender, result_receiver) = oneshot::channel::<Result<(), String>>();
    let window_for_main = window.clone();
    window
        .run_on_main_thread(move || {
            let result = start_native_drag_on_main_thread(window_for_main, paths, staging_root);
            let _ = result_sender.send(result);
        })
        .map_err(|error| AppError::Command(format!("切换到原生拖出线程失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| AppError::Command("原生拖出线程未响应".to_string()))?
        .map_err(AppError::Command)
}

#[cfg(not(target_os = "macos"))]
fn start_native_drag_on_main_thread(
    window: WebviewWindow,
    paths: Vec<PathBuf>,
    staging_root: PathBuf,
) -> Result<(), String> {
    let callback_root = staging_root.clone();
    let callback = move |_, _| schedule_staging_cleanup(callback_root.clone());
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
