/// Read the local MCP/CLI call audit log for the renderer viewer.
/// Returns newest entries first. Secrets never reach this command: they are
/// already removed at write time by the audit log writer.
#[tauri::command]
pub fn app_read_mcp_call_log(
    app: AppHandle,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<crate::services::mcp::McpCallLogPage, AppError> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0);
    crate::services::mcp::read_call_log(&app, limit, offset)
}
