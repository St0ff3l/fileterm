//! Lazy GTK drag-out for remote files on Linux.
//!
//! GTK asks the source for `text/uri-list` when the drop target accepts the
//! drag. We use that callback as the lazy boundary: only then do we download
//! remote files into a temporary directory and return their real `file://`
//! URIs. This keeps folder and mixed-selection support from the existing GTK
//! path while removing the old pre-download at drag start.

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};

use gtk::{
    gdk, gdk_pixbuf,
    glib::{ObjectExt, Propagation, SignalHandlerId},
    prelude::{DragContextExtManual, PixbufLoaderExt, WidgetExt, WidgetExtManual},
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tokio::sync::oneshot;

use super::{
    prepare_staged_remote_paths, remove_staging_dir_sync, schedule_staging_cleanup,
    RemoteFileDragItem,
};

const DRAG_IMAGE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/128x128.png"));

struct DragSessionCleanup {
    app: AppHandle,
    window_label: String,
    stage_root: PathBuf,
    finished: AtomicBool,
}

impl DragSessionCleanup {
    fn finish(&self, dropped: bool) {
        if self.finished.swap(true, Ordering::AcqRel) {
            return;
        }
        if dropped {
            schedule_staging_cleanup(self.stage_root.clone());
        } else {
            remove_staging_dir_sync(&self.stage_root);
            let app = self.app.clone();
            let stage_root = self.stage_root.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    crate::services::transfers::cleanup_drag_transfers_in_stage(&app, &stage_root)
                        .await
                {
                    log::warn!("清理 Linux 拖出阶段任务失败：{error}");
                }
            });
        }
        let _ = self.app.emit_to(
            &self.window_label,
            "fileterm://remote-native-drag-finished",
            (),
        );
    }
}

struct LazyRemoteUriData {
    app: AppHandle,
    tab_id: String,
    items: Vec<RemoteFileDragItem>,
    stage_root: PathBuf,
    staged_paths: OnceLock<std::result::Result<Vec<PathBuf>, String>>,
}

impl LazyRemoteUriData {
    fn new(
        app: AppHandle,
        tab_id: String,
        items: Vec<RemoteFileDragItem>,
        stage_root: PathBuf,
    ) -> Self {
        Self {
            app,
            tab_id,
            items,
            stage_root,
            staged_paths: OnceLock::new(),
        }
    }

    fn materialize(&self) -> std::result::Result<Vec<PathBuf>, String> {
        self.staged_paths
            .get_or_init(|| {
                let app = self.app.clone();
                let tab_id = self.tab_id.clone();
                let items = self.items.clone();
                let stage_root = self.stage_root.clone();
                tauri::async_runtime::block_on(async move {
                    prepare_staged_remote_paths(&app, &tab_id, &items, &stage_root)
                        .await
                        .map_err(|error| error.to_string())
                })
            })
            .clone()
    }
}

pub async fn start_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
    _drag_image: Option<super::RemoteDragImage>,
) -> std::result::Result<(), crate::AppError> {
    let window = app
        .get_webview_window(window_label)
        .ok_or_else(|| crate::AppError::Command("拖出窗口不存在".to_string()))?;
    let (result_sender, result_receiver) = oneshot::channel::<std::result::Result<(), String>>();
    let app_for_main = app.clone();
    let window_label = window_label.to_string();
    let tab_id = tab_id.to_string();
    let window_for_main = window.clone();
    window
        .run_on_main_thread(move || {
            let result = start_remote_file_drag_on_main_thread(
                app_for_main,
                window_label,
                tab_id,
                window_for_main,
                items,
            );
            let _ = result_sender.send(result);
        })
        .map_err(|error| crate::AppError::Command(format!("切换到原生拖出线程失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| crate::AppError::Command("原生拖出线程未响应".to_string()))?
        .map_err(crate::AppError::Command)
}

fn start_remote_file_drag_on_main_thread(
    app: AppHandle,
    window_label: String,
    tab_id: String,
    window: WebviewWindow,
    items: Vec<RemoteFileDragItem>,
) -> std::result::Result<(), String> {
    let gtk_window = window.gtk_window().map_err(|error| error.to_string())?;
    let stage_root =
        std::env::temp_dir().join(format!("fileterm-remote-drag-{}", uuid::Uuid::new_v4()));
    let lazy_data = Arc::new(LazyRemoteUriData::new(
        app.clone(),
        tab_id,
        items,
        stage_root.clone(),
    ));
    let handler_ids: Arc<Mutex<Vec<SignalHandlerId>>> = Arc::new(Mutex::new(Vec::new()));
    let drag_action = gdk::DragAction::COPY;
    let cleanup = Arc::new(DragSessionCleanup {
        app: app.clone(),
        window_label,
        stage_root: stage_root.clone(),
        finished: AtomicBool::new(false),
    });

    gtk_window.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &[], drag_action);
    gtk_window.drag_source_add_uri_targets();

    let data_for_get = lazy_data.clone();
    let cleanup_for_data = cleanup.clone();
    let data_handler_id = gtk_window.connect_drag_data_get(move |_, _, selection_data, _, _| {
        match data_for_get.materialize() {
            Ok(paths) => {
                let uris: Vec<String> = match paths
                    .iter()
                    .map(|path| url::Url::from_file_path(path).map(|uri| uri.to_string()))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(uris) => uris,
                    Err(_) => {
                        log::error!("准备 Linux 原生拖出 URI 失败：暂存路径无效");
                        cleanup_for_data.finish(false);
                        selection_data.set_uris(&[]);
                        return;
                    }
                };
                let uri_refs: Vec<&str> = uris.iter().map(String::as_str).collect();
                selection_data.set_uris(&uri_refs);
            }
            Err(error) => {
                log::error!("准备 Linux 原生拖出文件失败：{error}");
                selection_data.set_uris(&[]);
            }
        }
    });
    if let Err((error, handler_id)) = push_signal_handler(&handler_ids, data_handler_id) {
        gtk_window.disconnect(handler_id);
        gtk_window.drag_source_unset();
        cleanup.finish(false);
        return Err(error);
    }

    let Some(target_list) = gtk_window.drag_source_get_target_list() else {
        cleanup_signal_handlers(&handler_ids, &gtk_window);
        cleanup.finish(false);
        return Err("没有可用的 Linux 原生拖放目标".to_string());
    };
    let Some(drag_context) = gtk_window.drag_begin_with_coordinates(
        &target_list,
        drag_action,
        gdk::ffi::GDK_BUTTON1_MASK as i32,
        None,
        -1,
        -1,
    ) else {
        cleanup_signal_handlers(&handler_ids, &gtk_window);
        cleanup.finish(false);
        return Err("无法启动 Linux 原生拖出".to_string());
    };

    if let Some(icon) = image_binary_to_pixbuf(DRAG_IMAGE) {
        drag_context.drag_set_icon_pixbuf(&icon, 0, 0);
    }

    let window_for_failed = gtk_window.clone();
    let handler_ids_for_failed = handler_ids.clone();
    let cleanup_for_failed = cleanup.clone();
    let failed_handler_id = gtk_window.connect_drag_failed(move |_, _, _| {
        cleanup_signal_handlers(&handler_ids_for_failed, &window_for_failed);
        cleanup_for_failed.finish(false);
        Propagation::Proceed
    });
    if let Err((error, handler_id)) = push_signal_handler(&handler_ids, failed_handler_id) {
        gtk_window.disconnect(handler_id);
        cleanup_signal_handlers(&handler_ids, &gtk_window);
        cleanup.finish(false);
        return Err(error);
    }

    let window_for_end = gtk_window.clone();
    let handler_ids_for_end = handler_ids.clone();
    let cleanup_for_end = cleanup.clone();
    let end_handler_id = gtk_window.connect_drag_end(move |_, _| {
        cleanup_signal_handlers(&handler_ids_for_end, &window_for_end);
        cleanup_for_end.finish(false);
    });
    if let Err((error, handler_id)) = push_signal_handler(&handler_ids, end_handler_id) {
        gtk_window.disconnect(handler_id);
        cleanup_signal_handlers(&handler_ids, &gtk_window);
        cleanup.finish(false);
        return Err(error);
    }

    let window_for_drop = gtk_window.clone();
    let handler_ids_for_drop = handler_ids.clone();
    let cleanup_for_drop = cleanup.clone();
    drag_context.connect_drop_performed(move |context, _| {
        cleanup_signal_handlers(&handler_ids_for_drop, &window_for_drop);
        log::debug!("Linux 原生拖出落点：{:?}", context.selected_action());
        cleanup_for_drop.finish(true);
    });

    Ok(())
}

fn image_binary_to_pixbuf(data: &[u8]) -> Option<gdk_pixbuf::Pixbuf> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader
        .write(data)
        .and_then(|_| loader.close())
        .map_err(|_| ())
        .and_then(|_| loader.pixbuf().ok_or(()))
        .ok()
}

fn cleanup_signal_handlers(
    handler_ids: &Arc<Mutex<Vec<SignalHandlerId>>>,
    window: &gtk::ApplicationWindow,
) {
    if let Ok(mut handler_ids) = handler_ids.lock() {
        for handler_id in handler_ids.drain(..) {
            window.disconnect(handler_id);
        }
    }
    window.drag_source_unset();
}

fn push_signal_handler(
    handler_ids: &Arc<Mutex<Vec<SignalHandlerId>>>,
    handler_id: SignalHandlerId,
) -> std::result::Result<(), (String, SignalHandlerId)> {
    let mut registered_handlers = match handler_ids.lock() {
        Ok(registered_handlers) => registered_handlers,
        Err(_) => return Err(("拖放处理器状态损坏".to_string(), handler_id)),
    };
    registered_handlers.push(handler_id);
    Ok(())
}
