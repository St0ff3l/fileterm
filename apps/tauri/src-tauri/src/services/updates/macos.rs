//! Keep a separate backup: the upstream installer can discard its own backup on failure.

use std::{os::unix::fs::DirBuilderExt, path::Path};

use tauri::{AppHandle, Manager};

use crate::AppError;

fn validate_app_path(app: &Path) -> Result<(), String> {
    if app.extension().is_none_or(|extension| extension != "app")
        || app.starts_with("/Volumes")
        || app
            .components()
            .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err("请先将 FileTerm 拖入 Applications 文件夹，再运行应用内更新。".to_string());
    }
    Ok(())
}

fn install_with_backup(
    app: &Path,
    install: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    validate_app_path(app)?;
    let parent = app.parent().ok_or("无法确定应用目录")?;
    // A sibling backup also checks write access before the updater can request elevation.
    let backup = parent.join(format!(".fileterm-update-backup-{}", uuid::Uuid::new_v4()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&backup)
        .map_err(|error| format!("应用目录不可写，请从 GitHub 下载 DMG 手动更新: {error}"))?;
    let original = backup.join("FileTerm.app");
    let result = std::process::Command::new("/usr/bin/ditto")
        .arg(app)
        .arg(&original)
        .output()
        .map_err(|error| {
            let _ = std::fs::remove_dir_all(&backup);
            format!("无法备份当前应用: {error}")
        })?;
    if !result.status.success() {
        let _ = std::fs::remove_dir_all(&backup);
        return Err(format!(
            "无法备份当前应用: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let installed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(install))
        .unwrap_or_else(|_| Err("更新安装器异常退出".to_string()));
    if let Err(error) = installed {
        // Keep both copies until restoration succeeds, including a partially installed app.
        let restore = (|| -> std::io::Result<()> {
            if app.exists() {
                std::fs::rename(app, backup.join("failed.app"))?;
            }
            std::fs::rename(&original, app)
        })();
        return match restore {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&backup);
                Err(format!("更新失败，已恢复旧版应用: {error}"))
            }
            Err(restore_error) => Err(format!(
                "更新失败: {error}；恢复失败: {restore_error}。旧版应用保留在 {}，请手动恢复。",
                original.display()
            )),
        };
    }
    let _ = std::fs::remove_dir_all(&backup);
    Ok(())
}

pub(super) async fn install(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _operation = state.update_operation.lock().await;
    if super::current_update_mode(&super::get_status(app).await) == super::RELEASE_PAGE_UPDATE_MODE
    {
        return super::open_release_page(app).await;
    }
    if !crate::request_file_editors_for_quit(app).await? {
        return Ok(());
    }
    let pending = state.downloaded_update.lock().await.take();
    let Some(pending) = pending else {
        super::set_status(
            app,
            super::status_error(
                app,
                "in-app",
                &super::update_channel(app),
                "没有已验证的更新包，请重新检查更新。".to_string(),
            ),
        )
        .await;
        return Ok(());
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let bundle = tauri_plugin_updater::extract_path_from_executable(&executable)
            .map_err(|error| error.to_string())?;
        install_with_backup(&bundle, || {
            pending
                .update
                .install(pending.bytes)
                .map_err(|error| error.to_string())
        })
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    if let Err(error) = result {
        super::set_status(
            app,
            super::status_error(app, "in-app", &super::update_channel(app), error),
        )
        .await;
        return Ok(());
    }
    // Match normal quit so resumable transfers are checkpointed before restarting.
    if let Err(error) = crate::services::transfers::shutdown(app).await {
        super::set_status(app, super::status_error(app, "in-app", &super::update_channel(app), format!("新版已安装，但保存传输断点失败，请保存工作后手动退出并重新打开 FileTerm: {error}"))).await;
        return Ok(());
    }
    crate::commands::shutdown_session_workers(app).await;
    app.request_restart();
    Ok(())
}

#[cfg(test)]
mod tests;
