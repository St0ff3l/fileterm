//! Renderer-owned, one-shot password prompts for remote backup operations.

use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tokio::time::timeout;
use zeroize::Zeroizing;

use crate::services::workspace::{BackupPasswordResponse, PendingBackupPassword};
use crate::AppError;

const PROMPT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(crate) async fn request(
    app: &AppHandle,
    operation: &'static str,
    provider: &'static str,
) -> Result<Zeroizing<String>, AppError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel::<BackupPasswordResponse>();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if !state
        .insert_pending_backup_password(request_id.clone(), PendingBackupPassword { sender })
        .await
    {
        return Err(AppError::Window(
            "无法打开远程备份密码输入框，请确认主窗口仍在运行。".to_string(),
        ));
    }

    if app
        .emit(
            "backup:password-request",
            json!({
                "requestId": request_id,
                "operation": operation,
                "provider": provider,
            }),
        )
        .is_err()
    {
        state
            .pending_backup_passwords
            .write()
            .await
            .remove(&request_id);
        return Err(AppError::Window("无法显示远程备份密码输入框。".to_string()));
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }

    let response = match timeout(PROMPT_TIMEOUT, receiver).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) | Err(_) => {
            state
                .pending_backup_passwords
                .write()
                .await
                .remove(&request_id);
            return Err(AppError::Command(
                "远程备份密码输入已取消或超时。".to_string(),
            ));
        }
    };
    if response.cancelled {
        return Err(AppError::Command("远程备份操作已取消。".to_string()));
    }
    let value = response
        .value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Command("请输入远程备份主密码。".to_string()))?;
    Ok(Zeroizing::new(value))
}
