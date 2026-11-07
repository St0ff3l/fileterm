// Local audit log for external MCP/CLI bridge calls.
//
// Every dispatched bridge request is appended to `mcp-calls.jsonl` in the
// FileTerm data directory: one JSON object per line with the connection
// identity, the requested action, redacted parameters, and the outcome. The
// log is unconditional (independent of the diagnostic-log switches) so an
// operator can always answer "what did the Agent run on which machine".
// Passwords and other secret-shaped values are removed before writing, and
// long string values are truncated to keep one call to a bounded line.

const MCP_CALL_LOG_FILE: &str = "mcp-calls.jsonl";
const MCP_CALL_LOG_ROTATED_FILE: &str = "mcp-calls.jsonl.1";
/// Roughly 10k calls per shard; rotation is best-effort and keeps one
/// previous shard only.
const MCP_CALL_LOG_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Cap for one serialized `params` value in the log entry.
const MCP_CALL_LOG_PARAMS_LIMIT: usize = 16 * 1024;
/// Cap for a single string value inside `params`.
const MCP_CALL_LOG_STRING_LIMIT: usize = 2 * 1024;
/// Maximum number of array items copied into the log entry.
const MCP_CALL_LOG_ARRAY_LIMIT: usize = 64;
/// Maximum object depth copied into the log entry.
const MCP_CALL_LOG_DEPTH_LIMIT: usize = 8;

/// Secret-shaped parameter keys are dropped entirely. The match is
/// deliberately broad: `password`, `secret`, `credential`, and anything
/// containing `token` cover current and future parameters without
/// maintaining an exact list.
fn mcp_call_log_key_is_secret(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("password")
        || key.contains("secret")
        || key.contains("credential")
        || key.contains("token")
}

fn truncate_mcp_call_log_text(value: &str) -> String {
    truncate_text(value, MCP_CALL_LOG_STRING_LIMIT)
}

fn redact_mcp_call_log_params(params: &Value) -> Value {
    redact_mcp_call_log_value(params, 0)
}

fn redact_mcp_call_log_value(value: &Value, depth: usize) -> Value {
    if depth >= MCP_CALL_LOG_DEPTH_LIMIT {
        return Value::String("…".to_string());
    }
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !mcp_call_log_key_is_secret(key))
                .map(|(key, value)| (key.clone(), redact_mcp_call_log_value(value, depth + 1)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(MCP_CALL_LOG_ARRAY_LIMIT)
                .map(|item| redact_mcp_call_log_value(item, depth + 1))
                .collect(),
        ),
        Value::String(text) => Value::String(truncate_mcp_call_log_text(text)),
        other => other.clone(),
    }
}

fn bound_mcp_call_log_params(params: &Value) -> Value {
    let redacted = redact_mcp_call_log_params(params);
    match serde_json::to_string(&redacted) {
        Ok(encoded) if encoded.len() <= MCP_CALL_LOG_PARAMS_LIMIT => redacted,
        _ => Value::String(format!(
            "<parameters truncated: {} bytes>",
            encoded_len_or_unknown(params)
        )),
    }
}

fn encoded_len_or_unknown(params: &Value) -> String {
    serde_json::to_string(params)
        .map(|encoded| encoded.len().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Append one call record to the audit log. Best-effort: a failed write is
/// reported once through the diagnostic log but never fails the tool call.
fn record_mcp_call_entry(app: &AppHandle, entry: Value) {
    let Ok(path) = crate::storage::workspace_file(app, MCP_CALL_LOG_FILE) else {
        return;
    };
    let mut line = serde_json::to_string(&entry).unwrap_or_default();
    if line.is_empty() {
        return;
    }
    line.push('\n');
    if let Ok(metadata) = fs::metadata(&path) {
        if metadata.len() + line.len() as u64 > MCP_CALL_LOG_MAX_BYTES {
            let rotated = path.with_file_name(MCP_CALL_LOG_ROTATED_FILE);
            // On failure keep appending to the oversized shard rather than
            // losing records; the next successful rotation clears it.
            let _ = fs::rename(&path, rotated);
        }
    }
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            if let Err(error) = file.write_all(line.as_bytes()) {
                crate::services::logging::warn(
                    app,
                    "mcp",
                    format!("unable to append to the MCP call audit log: {error}"),
                );
            }
        }
        Err(error) => {
            crate::services::logging::warn(
                app,
                "mcp",
                format!("unable to open the MCP call audit log: {error}"),
            );
        }
    }
}

/// Build and persist the audit entry for one dispatched bridge request.
/// `approval` records how the approval boundary was satisfied for this call.
fn record_mcp_call(
    app: &AppHandle,
    request: &BridgeRequest,
    approval: &str,
    started: std::time::Instant,
    outcome: &Result<Value, String>,
) {
    let (ok, error) = match outcome {
        Ok(_) => (true, None),
        Err(error) => (false, Some(truncate_mcp_call_log_text(error))),
    };
    let entry = json!({
        "ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "source": request.source,
        "clientId": request.client_id,
        "action": request.action,
        "approval": approval,
        "ok": ok,
        "error": error,
        "params": bound_mcp_call_log_params(&request.params),
        "durationMs": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    });
    record_mcp_call_entry(app, entry);
}

/// One page of the call audit log for the in-app viewer. Entries are the
/// already-redacted JSON objects as written to `mcp-calls.jsonl`, newest
/// first.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallLogPage {
    pub items: Vec<Value>,
    pub total: usize,
    pub offset: usize,
    pub has_more: bool,
}

/// Read a page of the audit log for the renderer viewer. Malformed lines are
/// skipped, matching the append-only writer's best-effort contract.
pub fn read_call_log(app: &AppHandle, limit: usize, offset: usize) -> Result<McpCallLogPage, AppError> {
    let path = crate::storage::workspace_file(app, MCP_CALL_LOG_FILE)?;
    let mut entries = Vec::new();
    match fs::read_to_string(&path) {
        Ok(content) => {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<Value>(line) {
                    entries.push(entry);
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(AppError::Storage(format!(
                "Unable to read the MCP call audit log: {error}"
            )))
        }
    }
    // The writer appends chronologically; the viewer shows newest first.
    entries.reverse();
    let total = entries.len();
    let items = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let has_more = offset + items.len() < total;
    Ok(McpCallLogPage {
        items,
        total,
        offset,
        has_more,
    })
}
