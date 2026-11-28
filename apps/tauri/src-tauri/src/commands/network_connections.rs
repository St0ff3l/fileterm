pub async fn app_get_network_connections(app: AppHandle, tab_id: String) -> Result<Value, AppError> {
    {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let sessions = state.sessions.read().await;
        let session = sessions.get(&tab_id).ok_or_else(|| AppError::Command("NETWORK_SESSION_UNAVAILABLE".into()))?;
        if !session.connected || !session.capabilities.resource_monitoring {
            return Err(AppError::Command("NETWORK_SESSION_UNAVAILABLE".into()));
        }
    }
    let result = send_worker_cmd(&app, &tab_id, |respond_to| WorkerCmd::ReadNetworkConnections {
        respond_to,
    }).await?;
    if result["timedOut"] == true {
        return Err(AppError::Command("NETWORK_SNAPSHOT_TIMEOUT".into()));
    }
    let output = result["output"].as_str().unwrap_or_default();
    if result["exitCode"].as_u64() != Some(0) {
        let message = if output.contains("FILETERM_NETWORK_UNSUPPORTED") {
            "NETWORK_PLATFORM_UNSUPPORTED"
        } else if output.contains("FILETERM_NETWORK_SS_REQUIRED") {
            "NETWORK_SS_REQUIRED"
        } else {
            "NETWORK_SNAPSHOT_FAILED"
        };
        return Err(AppError::Command(message.into()));
    }
    crate::sessions::network_connections::parse(output, result["outputTruncated"] == true)
        .map_err(AppError::Command)
}
