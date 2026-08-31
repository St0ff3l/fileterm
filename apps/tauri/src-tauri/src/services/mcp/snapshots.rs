// Snapshot compaction, pagination, parameter validation, and public errors.
fn compact_session(tab: &Value, session: &Value, tab_id: &str) -> Value {
    json!({
        "sessionId": tab_id,
        "tabId": tab_id,
        "background": tab.get("isBackground").and_then(Value::as_bool).unwrap_or(false),
        "source": tab.get("source"),
        "rootTabId": tab.get("paneRootTabId").cloned().unwrap_or_else(|| Value::String(tab_id.to_string())),
        "profileId": tab.get("profileId"),
        "title": tab.get("title"),
        "sessionType": tab.get("sessionType"),
        "status": tab.get("status"),
        "connected": session.get("connected"),
        "deviceMode": session.get("deviceMode"),
        "remotePath": session.get("remotePath"),
        "shellCwd": session.get("shellCwd"),
        "shellUser": session.get("shellUser"),
        "sessionRevision": session.get("aiSessionRevision"),
        "fileAccessMode": session.get("fileAccessMode"),
        "capabilities": session.get("capabilities"),
    })
}

fn compact_snapshot(snapshot: &Value, tab_id: Option<&str>, operation: &str) -> Value {
    let tab_id = tab_id.or_else(|| snapshot.get("activeTabId").and_then(Value::as_str));
    let session = tab_id.and_then(|id| {
        let tab = snapshot
            .get("tabs")
            .and_then(Value::as_array)
            .and_then(|tabs| {
                tabs.iter()
                    .find(|tab| tab.get("id").and_then(Value::as_str) == Some(id))
            })?;
        let session = snapshot.get("sessions")?.get(id)?;
        Some(compact_session(tab, session, id))
    });
    json!({
        "operation": operation,
        "activeTabId": snapshot.get("activeTabId"),
        "session": session,
    })
}

fn transfer_snapshot(snapshot: &Value, tab_id: &str, operation: &str, direction: &str) -> Value {
    let transfer = snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .and_then(|transfers| {
            transfers.iter().rev().find(|transfer| {
                transfer.get("tabId").and_then(Value::as_str) == Some(tab_id)
                    && transfer.get("direction").and_then(Value::as_str) == Some(direction)
            })
        });
    json!({
        "operation": operation,
        "tabId": tab_id,
        "transfer": transfer,
    })
}

fn find_transfer(snapshot: &Value, transfer_id: &str) -> Value {
    snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .and_then(|transfers| {
            transfers
                .iter()
                .find(|transfer| transfer.get("id").and_then(Value::as_str) == Some(transfer_id))
        })
        .cloned()
        .unwrap_or(Value::Null)
}

fn pagination(params: &Value) -> Result<(usize, usize), String> {
    let limit = optional_usize(params, "limit")?.unwrap_or(MCP_DEFAULT_PAGE_SIZE);
    let offset = optional_usize(params, "offset")?.unwrap_or(0);
    if !(1..=MCP_MAX_PAGE_SIZE).contains(&limit) {
        return Err(format!("limit must be between 1 and {MCP_MAX_PAGE_SIZE}"));
    }
    Ok((limit, offset))
}

fn optional_usize(params: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_u64()
        .ok_or_else(|| format!("{key} must be a non-negative integer"))?;
    usize::try_from(value)
        .map(Some)
        .map_err(|_| format!("{key} is too large"))
}

fn optional_u64(params: &Value, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .ok_or_else(|| format!("{key} must be a non-negative integer"))
        .map(Some)
}

fn optional_bool(params: &Value, key: &str) -> Result<Option<bool>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    value
        .as_bool()
        .ok_or_else(|| format!("{key} must be a boolean"))
        .map(Some)
}

fn required_string(params: &Value, key: &str, maximum: usize) -> Result<String, String> {
    optional_string(params, key, maximum)?.ok_or_else(|| format!("{key} is required"))
}

fn optional_string(params: &Value, key: &str, maximum: usize) -> Result<Option<String>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?
        .trim();
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{key} exceeds the FileTerm MCP limit"));
    }
    Ok(Some(value.to_string()))
}

/// Parse a one-shot secret without trimming it. Passwords may legitimately
/// contain leading/trailing spaces; control characters are rejected later by
/// the execution service before the value reaches SSH stdin.
fn optional_secret_string(
    params: &Value,
    key: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?;
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{key} exceeds the FileTerm MCP limit"));
    }
    Ok(Some(value.to_string()))
}

fn required_text(params: &Value, key: &str, maximum: usize) -> Result<String, String> {
    let value = params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required and must be a string"))?;
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{key} exceeds the FileTerm MCP limit"));
    }
    Ok(value.to_string())
}

fn optional_string_array(
    params: &Value,
    key: &str,
    maximum_items: usize,
    maximum_item_bytes: usize,
) -> Result<Option<Vec<String>>, String> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("{key} must be an array of strings"))?;
    if items.len() > maximum_items {
        return Err(format!("{key} has too many items"));
    }
    items
        .iter()
        .map(|item| {
            let value = item
                .as_str()
                .ok_or_else(|| format!("{key} must contain only strings"))?;
            if value.len() > maximum_item_bytes {
                return Err(format!("{key} item exceeds the FileTerm MCP limit"));
            }
            Ok(value.to_string())
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Some)
}

fn required_string_array(
    params: &Value,
    key: &str,
    maximum_items: usize,
    maximum_item_bytes: usize,
) -> Result<Vec<String>, String> {
    optional_string_array(params, key, maximum_items, maximum_item_bytes)?
        .ok_or_else(|| format!("{key} is required"))
}

fn required_target_type(params: &Value) -> Result<String, String> {
    let target_type = required_string(params, "target_type", 16)?;
    if !matches!(target_type.as_str(), "file" | "folder") {
        return Err("target_type must be file or folder".to_string());
    }
    Ok(target_type)
}

fn truncate_text(value: &str, maximum: usize) -> String {
    truncate_text_with_flag(value, maximum).0
}

fn truncate_text_with_flag(value: &str, maximum: usize) -> (String, bool) {
    if value.len() <= maximum {
        return (value.to_string(), false);
    }
    let mut end = maximum;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (
        format!("{}\n[… output truncated by FileTerm …]", &value[..end]),
        true,
    )
}

fn public_app_error(error: AppError) -> String {
    match error {
        AppError::Storage(_) => {
            "FileTerm could not complete the request. Verify the requested session is still open."
                .to_string()
        }
        AppError::Serialization(_) => {
            "FileTerm returned an invalid response. Retry the request.".to_string()
        }
        AppError::Command(message) => message,
        AppError::Clipboard(_) | AppError::Window(_) => {
            "FileTerm could not complete the MCP request.".to_string()
        }
    }
}

