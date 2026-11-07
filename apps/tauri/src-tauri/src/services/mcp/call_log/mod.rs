// Audit metadata only. Shell text, file contents, arguments and raw errors may contain secrets.
use super::*;
use std::path::Path;

mod store;
#[cfg(test)]
mod tests;

const MCP_CALL_LOG_FILE: &str = "mcp-calls.jsonl";
const MCP_CALL_LOG_ROTATED_FILE: &str = "mcp-calls.jsonl.1";
const MCP_CALL_LOG_MAX_BYTES: u64 = 8 * 1024 * 1024;

fn redact_params(action: &str, params: &Value) -> Value {
    // Validate the action and its parameter names against the existing tool contract.
    let tool = format!("fileterm_{action}");
    let mut result = serde_json::Map::new();
    for key in [
        "tab_id",
        "profile_id",
        "operation_id",
        "command_id",
        "transfer_id",
        "rule_id",
        "path",
        "target_path",
        "destination_path",
        "parent_path",
        "remote_path",
        "local_path",
        "local_directory",
        "remote_directory",
        "cwd",
        "encoding",
        "execution_mode",
        "target_type",
        "apply_to",
        "mode",
        "timeout_ms",
        "wait_ms",
        "max_bytes",
        "offset",
        "limit",
        "recursive",
        "wait_for_ready",
        "target_is_symlink",
    ] {
        let Some(value) = params.get(key) else {
            continue;
        };
        let mut candidate = json!({key: value});
        if action == "open_connection" {
            candidate["execution_mode"] = json!("background");
        }
        if validate_tool_arguments(&tool, &candidate).is_err() {
            continue;
        }
        let safe = match key {
            "timeout_ms" | "wait_ms" | "max_bytes" | "offset" | "limit" => {
                let Some(number) = value.as_u64() else {
                    continue;
                };
                json!(number)
            }
            "recursive" | "wait_for_ready" | "target_is_symlink" => {
                let Some(boolean) = value.as_bool() else {
                    continue;
                };
                json!(boolean)
            }
            "execution_mode" => {
                let Some(mode @ ("background" | "visible-terminal")) = value.as_str() else {
                    continue;
                };
                json!(mode)
            }
            _ => {
                let Some(text) = value.as_str() else { continue };
                Value::String(truncate_text(text, 512))
            }
        };
        result.insert(key.to_string(), safe);
    }
    Value::Object(result)
}

pub(super) struct McpCallAudit {
    path: Option<PathBuf>,
    entry: Value,
    started: std::time::Instant,
    finished: bool,
}

impl McpCallAudit {
    pub(super) fn begin(path: Result<PathBuf, AppError>, request: &BridgeRequest) -> Self {
        let entry = json!({
            "schemaVersion": 2,
            "id": uuid::Uuid::new_v4().to_string(),
            "ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "source": request.source,
            "clientId": request.client_id,
            "bridgeSessionId": request.client_id,
            "action": safe_action(&request.action),
            "approval": "pending",
            "status": "started",
            "ok": false,
            "callOk": null,
            "error": null,
            "params": redact_params(&request.action, &request.params),
            "target": null,
            "durationMs": 0,
        });
        let audit = Self {
            path: path
                .map_err(|error| {
                    tracing::warn!(category = "mcp", %error, "unable to locate MCP audit file");
                })
                .ok(),
            entry,
            started: std::time::Instant::now(),
            finished: false,
        };
        audit.persist();
        audit
    }

    pub(super) async fn resolve_target(&mut self, app: &AppHandle, params: &Value) {
        if !["tab_id", "profile_id", "operation_id", "transfer_id"]
            .iter()
            .any(|key| params.get(key).is_some())
        {
            return;
        }
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let operation = match params.get("operation_id").and_then(Value::as_str) {
            Some(id) => state.connection_operations.info(id).await.ok(),
            None => None,
        };
        let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await else {
            return;
        };
        self.entry["target"] = resolve_target(&snapshot, params, operation.as_ref());
        // Persist resolved metadata before any policy/approval/execution awaits.
        self.persist();
    }

    pub(super) fn approval(&mut self, approval: &str) {
        self.entry["approval"] = json!(approval);
    }

    pub(super) fn finish(mut self, outcome: Result<&Value, &str>) {
        let (ok, status, error, execution) = outcome_metadata(outcome);
        self.entry["ok"] = json!(ok);
        self.entry["callOk"] = json!(outcome.is_ok());
        self.entry["status"] = json!(status);
        self.entry["error"] = json!(error);
        self.entry["execution"] = execution;
        self.finished = true;
        self.persist();
    }

    fn persist(&self) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let mut entry = self.entry.clone();
        entry["durationMs"] =
            json!(u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX));
        if let Err(error) = store::append(path, &entry) {
            tracing::warn!(category = "mcp", %error, "unable to persist MCP audit metadata");
        }
    }
}

impl Drop for McpCallAudit {
    fn drop(&mut self) {
        if !self.finished {
            self.entry["status"] = json!("outcome-uncertain");
            self.entry["error"] = json!("FILETERM_CALL_INTERRUPTED");
            self.persist();
        }
    }
}

fn safe_action(action: &str) -> &str {
    let arguments = if action == "open_connection" {
        json!({"execution_mode": "background"})
    } else {
        json!({})
    };
    if validate_tool_arguments(&format!("fileterm_{action}"), &arguments).is_ok() {
        action
    } else {
        "unsupported-action"
    }
}

fn resolve_target(
    snapshot: &Value,
    params: &Value,
    operation: Option<&crate::services::connection_operations::ConnectionOperationInfo>,
) -> Value {
    let transfer = snapshot
        .get("transfers")
        .and_then(Value::as_array)
        .and_then(|items| {
            let id = params.get("transfer_id")?.as_str()?;
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
        });
    let tab_id = params
        .get("tab_id")
        .and_then(Value::as_str)
        .or_else(|| operation.and_then(|info| info.tab_id.as_deref()))
        .or_else(|| transfer?.get("tabId")?.as_str());
    let tab = snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .and_then(|tabs| {
            tabs.iter()
                .find(|tab| tab.get("id").and_then(Value::as_str) == tab_id)
        });
    let profile_id = tab
        .and_then(|tab| tab.get("profileId"))
        .and_then(Value::as_str)
        .or_else(|| operation.map(|info| info.profile_id.as_str()))
        .or_else(|| {
            transfer
                .and_then(|task| task.get("profileId"))
                .and_then(Value::as_str)
        })
        .or_else(|| params.get("profile_id").and_then(Value::as_str));
    let profile = snapshot
        .get("profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| {
            profiles
                .iter()
                .find(|profile| profile.get("id").and_then(Value::as_str) == profile_id)
        });
    if tab.is_none() && profile.is_none() && operation.is_none() {
        return Value::Null;
    }
    json!({
        "tabId": tab.and_then(|tab| tab.get("id")),
        "profileId": profile_id,
        "host": profile.and_then(|profile| profile.get("host")),
        "port": profile.and_then(|profile| profile.get("port")),
        "protocol": profile.and_then(|profile| profile.get("type")),
    })
}

fn outcome_metadata(
    outcome: Result<&Value, &str>,
) -> (bool, &'static str, Option<&'static str>, Value) {
    let response = match outcome {
        Ok(response) => response,
        Err("FileTerm MCP request cancelled") => {
            return (
                false,
                "outcome-uncertain",
                Some("FILETERM_CALL_CANCELLED"),
                Value::Null,
            );
        }
        Err(error) => {
            let code = mcp_error_code(error);
            let status = if code == "FILETERM_REQUEST_TIMEOUT" {
                "outcome-uncertain"
            } else {
                "call-failed"
            };
            return (false, status, Some(code), Value::Null);
        }
    };
    let result = response.get("result").unwrap_or(response);
    let execution = json!({
        "exitCode": result.get("exitCode"),
        "exitSignal": result.get("exitSignal").and_then(Value::as_str).map(|_| "signalled"),
        "timedOut": result.get("timedOut").and_then(Value::as_bool).unwrap_or(false),
        "cancelled": result.get("cancelled").and_then(Value::as_bool).unwrap_or(false),
        "running": result.get("running").and_then(Value::as_bool).unwrap_or(false),
        "inputRequired": result.get("inputRequired").and_then(Value::as_bool).unwrap_or(false),
    });
    if execution["timedOut"] == true
        || execution["cancelled"] == true
        || execution["inputRequired"] == true
    {
        return (
            false,
            "outcome-uncertain",
            Some("FILETERM_EXECUTION_INCOMPLETE"),
            execution,
        );
    }
    if result
        .get("exitCode")
        .and_then(Value::as_u64)
        .is_some_and(|code| code != 0)
        || result
            .get("exitSignal")
            .is_some_and(|signal| !signal.is_null())
    {
        return (
            false,
            "command-failed",
            Some("FILETERM_COMMAND_FAILED"),
            execution,
        );
    }
    let status = if result.get("exitCode").and_then(Value::as_u64) == Some(0) {
        "command-succeeded"
    } else if result.get("running") == Some(&Value::Bool(true))
        || response.get("accepted") == Some(&Value::Bool(true))
        || response
            .get("agent")
            .and_then(|agent| agent.get("state"))
            .and_then(Value::as_str)
            .is_some_and(|state| {
                matches!(
                    state,
                    "command-accepted"
                        | "visible-command-accepted"
                        | "connection-starting"
                        | "connection-waiting"
                )
            })
    {
        "accepted"
    } else {
        "call-succeeded"
    };
    (true, status, None, execution)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallLogPage {
    pub items: Vec<Value>,
    pub total: usize,
    pub offset: usize,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

pub fn read_call_log(
    app: &AppHandle,
    limit: usize,
    offset: usize,
    before: Option<&str>,
) -> Result<McpCallLogPage, AppError> {
    let path = crate::storage::workspace_file(app, MCP_CALL_LOG_FILE)?;
    store::read_page(&path, limit.clamp(1, 200), offset, before)
        .map_err(|error| AppError::Storage(format!("Unable to read MCP audit metadata: {error}")))
}
