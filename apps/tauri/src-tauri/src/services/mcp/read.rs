// Read-only connection, session, file, transfer, and tunnel queries.
async fn list_connections(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let visibility = mcp_visibility(app).await?;
    let library = crate::commands::app_get_connection_library(app.clone())
        .await
        .map_err(public_app_error)?;
    let profiles = library
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned an invalid connection library".to_string())?;
    let profiles = profiles
        .iter()
        .filter(|profile| visibility.allows_profile(profile.get("id").and_then(Value::as_str)))
        .collect::<Vec<_>>();
    let total = profiles.len();
    let items = profiles
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_offset = offset + items.len();
    Ok(json!({
        "total": total,
        "count": items.len(),
        "offset": offset,
        "items": items,
        "hasMore": next_offset < total,
        "nextOffset": (next_offset < total).then_some(next_offset),
    }))
}

async fn get_session_context(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let profile_id = optional_string(params, "profile_id", 256)?;
    let visibility = mcp_visibility(app).await?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let tabs = snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned invalid workspace tabs".to_string())?;
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_object)
        .ok_or_else(|| "FileTerm returned invalid workspace sessions".to_string())?;

    let items = tabs
        .iter()
        .filter(|tab| tab.get("paneRootTabId").is_none())
        .filter(|tab| visibility.allows_tab(tab.get("id").and_then(Value::as_str)))
        .filter(|tab| visibility.allows_profile(tab.get("profileId").and_then(Value::as_str)))
        .filter(|tab| {
            profile_id.as_deref().is_none_or(|profile_id| {
                tab.get("profileId").and_then(Value::as_str) == Some(profile_id)
            })
        })
        .filter_map(|tab| {
            let tab_id = tab.get("id").and_then(Value::as_str)?;
            let session = sessions.get(tab_id)?;
            Some(compact_session(tab, session, tab_id))
        })
        .collect::<Vec<_>>();
    Ok(json!({ "items": items }))
}

async fn list_remote_commands(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let (limit, offset) = pagination(params)?;
    let page = app
        .state::<crate::services::workspace::WorkspaceState>()
        .background_remote_commands
        .list(&tab_id, limit, offset)
        .await;
    serde_json::to_value(page).map_err(|error| error.to_string())
}

async fn get_command_templates(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let templates = snapshot
        .get("commandTemplates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = templates.len();
    let items = templates
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_offset = offset + items.len();
    Ok(json!({
        "total": total,
        "count": items.len(),
        "offset": offset,
        "items": items,
        "hasMore": next_offset < total,
        "nextOffset": (next_offset < total).then_some(next_offset),
    }))
}

async fn list_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = optional_string(params, "path", 4_096)?;
    let (limit, offset) = pagination(params)?;
    let snapshot = crate::commands::mcp_list_remote_directory(app.clone(), tab_id, path)
        .await
        .map_err(public_app_error)?;
    let items = snapshot
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = items.len();
    let items = items
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_offset = offset + items.len();
    Ok(json!({
        "tabId": snapshot.get("tabId"),
        "path": snapshot.get("path"),
        "total": total,
        "count": items.len(),
        "offset": offset,
        "items": items,
        "hasMore": next_offset < total,
        "nextOffset": (next_offset < total).then_some(next_offset),
    }))
}

async fn read_remote_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = required_string(params, "path", 4_096)?;
    let encoding = optional_string(params, "encoding", 64)?;
    let content = crate::commands::app_read_remote_file(
        app.clone(),
        tab_id.clone(),
        path.clone(),
        encoding.clone(),
    )
    .await
    .map_err(public_app_error)?;
    let (content, truncated) = truncate_text_with_flag(&content, MCP_MAX_FILE_CONTENT_BYTES);
    Ok(json!({
        "tabId": tab_id,
        "path": path,
        "encoding": encoding.unwrap_or_else(|| "utf-8".to_string()),
        "content": content,
        "truncated": truncated,
    }))
}

async fn list_transfers(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let (limit, offset) = pagination(params)?;
    let visibility = mcp_visibility(app).await?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let transfers = snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .ok_or_else(|| "FileTerm returned invalid transfer state".to_string())?;
    let transfers = transfers
        .iter()
        .filter(|transfer| visibility.allows_transfer_value(transfer))
        .collect::<Vec<_>>();
    let total = transfers.len();
    let items = transfers
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_offset = offset + items.len();
    Ok(json!({
        "total": total,
        "count": items.len(),
        "offset": offset,
        "items": items,
        "hasMore": next_offset < total,
        "nextOffset": (next_offset < total).then_some(next_offset),
    }))
}

/// Wait locally for a transfer to complete, without forcing an MCP client to
/// repeatedly poll the desktop bridge. A timeout is a successful observation
/// result: the task is returned in its latest known state so an Agent can make
/// an explicit next decision instead of mistaking a still-running transfer for
/// a failed request.
async fn wait_for_transfer(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let transfer_id = required_string(params, "transfer_id", 256)?;
    let timeout_ms = optional_u64(params, "timeout_ms")?.unwrap_or(MCP_TRANSFER_WAIT_DEFAULT_MS);
    if !(1_000..=MCP_TRANSFER_WAIT_MAX_MS).contains(&timeout_ms) {
        return Err(format!(
            "timeout_ms must be between 1000 and {MCP_TRANSFER_WAIT_MAX_MS}"
        ));
    }

    let visibility = mcp_visibility(app).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let task = crate::services::transfers::list(app)
            .await
            .map_err(public_app_error)?
            .into_iter()
            .find(|task| task.id == transfer_id)
            .ok_or_else(|| format!("{MCP_TRANSFER_NOT_FOUND}: transfer was not found"))?;
        if !visibility.allows_transfer_task(&task) {
            return Err(format!(
                "{MCP_SCOPE_DENIED}: this Agent cannot access the requested transfer"
            ));
        }
        let terminal = matches!(task.status.as_str(), "done" | "failed" | "canceled");
        if terminal {
            return Ok(json!({
                "transferId": transfer_id,
                "transfer": task,
                "timedOut": false,
            }));
        }

        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(json!({
                "transferId": transfer_id,
                "transfer": task,
                "timedOut": true,
            }));
        }
        // Transfer workers already update their durable task state at a
        // throttled cadence. Waiting here keeps that cadence local to
        // FileTerm and avoids external Agent-side polling loops.
        tokio::time::sleep((deadline - now).min(Duration::from_millis(250))).await;
    }
}

async fn list_ssh_tunnels(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let items = crate::commands::app_list_ssh_tunnels(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(json!({ "tabId": tab_id, "items": items }))
}
