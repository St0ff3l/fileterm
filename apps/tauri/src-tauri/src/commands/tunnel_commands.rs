// SSH tunnel commands.
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
