/// Read the local MCP/CLI call audit log for the renderer viewer.
/// Returns newest calls first, sanitizing legacy records as well.
#[tauri::command]
pub async fn app_read_mcp_call_log(
    app: AppHandle,
    limit: Option<usize>,
    offset: Option<usize>,
    before: Option<String>,
) -> Result<crate::services::mcp::McpCallLogPage, AppError> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0);
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::mcp::read_call_log(&app, limit, offset, before.as_deref())
    })
    .await
    .map_err(|error| AppError::Storage(format!("MCP audit reader failed: {error}")))?
}
