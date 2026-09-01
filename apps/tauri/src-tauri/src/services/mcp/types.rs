// MCP bridge envelopes, requests, responses, and progress types.
#[derive(Clone, Debug)]
struct McpAccessPolicy {
    connection_scope: String,
    operation_policy: String,
    allowed_profile_ids: HashSet<String>,
}

#[derive(Clone, Copy, Debug)]
enum McpVisibilityScope {
    AllSavedConnections,
    SelectedConnections,
}

#[derive(Clone, Debug)]
struct McpVisibility {
    scope: McpVisibilityScope,
    profile_ids: HashSet<String>,
    tab_ids: HashSet<String>,
}

impl McpVisibility {
    fn all_saved_connections() -> Self {
        Self {
            scope: McpVisibilityScope::AllSavedConnections,
            profile_ids: HashSet::new(),
            tab_ids: HashSet::new(),
        }
    }

    fn allows_profile(&self, profile_id: Option<&str>) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::SelectedConnections => {
                profile_id.is_some_and(|profile_id| self.profile_ids.contains(profile_id))
            }
        }
    }

    fn allows_tab(&self, tab_id: Option<&str>) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::SelectedConnections => {
                tab_id.is_some_and(|tab_id| self.tab_ids.contains(tab_id))
            }
        }
    }

    fn allows_transfer_value(&self, transfer: &Value) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::SelectedConnections => {
                self.allows_tab(transfer.get("tabId").and_then(Value::as_str))
                    || self.allows_profile(transfer.get("profileId").and_then(Value::as_str))
            }
        }
    }

    fn allows_transfer_task(&self, task: &crate::services::transfers::TransferTask) -> bool {
        match self.scope {
            McpVisibilityScope::AllSavedConnections => true,
            McpVisibilityScope::SelectedConnections => {
                self.allows_tab(task.tab_id.as_deref())
                    || self.allows_profile(task.profile_id.as_deref())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeDescriptor {
    protocol_version: u32,
    address: String,
    token: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeEnvelope {
    token: String,
    request: BridgeRequest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct BridgeRequest {
    action: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    source: WorkspaceSessionSource,
    #[serde(default)]
    requires_approval: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    progress_token: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CliJsonlRequest {
    id: Value,
    action: String,
    #[serde(default = "empty_json_object")]
    params: Value,
    #[serde(default, alias = "requires_approval")]
    requires_approval: bool,
    #[serde(default)]
    progress_token: Option<Value>,
}

struct CliJsonlJob {
    request: CliJsonlRequest,
    cancellation: Arc<AtomicBool>,
    controls: CliJsonlRequestControls,
}

#[derive(Clone, Default)]
struct CliJsonlRequestControls {
    active: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl CliJsonlRequestControls {
    fn register(&self, id: &Value) -> Result<Arc<AtomicBool>, String> {
        let key = cli_jsonl_request_key(id)?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| "FileTerm CLI JSONL request registry is unavailable".to_string())?;
        if active.contains_key(&key) {
            return Err("FileTerm CLI JSONL request id is already in use".to_string());
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        active.insert(key, Arc::clone(&cancellation));
        Ok(cancellation)
    }

    fn cancel(&self, id: &Value) -> Result<bool, String> {
        let key = cli_jsonl_request_key(id)?;
        let active = self
            .active
            .lock()
            .map_err(|_| "FileTerm CLI JSONL request registry is unavailable".to_string())?;
        if let Some(cancellation) = active.get(&key) {
            cancellation.store(true, Ordering::Release);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn remove(&self, id: &Value) {
        let Ok(key) = cli_jsonl_request_key(id) else {
            return;
        };
        if let Ok(mut active) = self.active.lock() {
            active.remove(&key);
        }
    }
}

fn cli_jsonl_request_key(id: &Value) -> Result<String, String> {
    match id {
        Value::String(value) if !value.is_empty() && value.len() <= 256 => {
            if value.chars().any(char::is_control) {
                Err("FileTerm CLI JSONL request id must not contain control characters".to_string())
            } else {
                Ok(format!("s:{value}"))
            }
        }
        Value::Number(_) => serde_json::to_string(id)
            .map_err(|_| "FileTerm CLI JSONL request id must be a string or number".to_string())
            .and_then(|value| {
                if value.len() > 256 {
                    Err("FileTerm CLI JSONL request id must be at most 256 bytes".to_string())
                } else {
                    Ok(format!("n:{value}"))
                }
            }),
        Value::String(_) => Err(
            "FileTerm CLI JSONL request id must be a non-empty string of at most 256 bytes"
                .to_string(),
        ),
        _ => Err("FileTerm CLI JSONL request id must be a string or number".to_string()),
    }
}

fn validate_cli_jsonl_cancel_params(params: &Value) -> Result<Value, String> {
    let object = params
        .as_object()
        .ok_or_else(|| "cancel_request params must be a JSON object".to_string())?;
    if object.len() != 1 || !object.contains_key("request_id") {
        return Err("cancel_request params require only request_id".to_string());
    }
    let request_id = object
        .get("request_id")
        .ok_or_else(|| "cancel_request requires request_id".to_string())?;
    cli_jsonl_request_key(request_id)?;
    Ok(request_id.clone())
}

fn empty_json_object() -> Value {
    json!({})
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// A one-way status event sent before the final bridge response. It exists so
/// CLI/MCP callers can observe that FileTerm has opened a foreground secure
/// prompt while the original command remains pending for the user's input.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeProgress {
    kind: String,
    event: String,
    status: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress_token: Option<Value>,
}

impl BridgeProgress {
    fn action_approval_waiting(action: &str, progress_token: Option<Value>) -> Self {
        Self {
            kind: "progress".to_string(),
            event: "action-approval-waiting".to_string(),
            status: "input-required".to_string(),
            code: "FILETERM_ACTION_APPROVAL_REQUIRED".to_string(),
            message: format!(
                "FileTerm is waiting for confirmation before running the external operation: {action}. Confirm it in the main window; the MCP request remains pending."
            ),
            progress_token,
        }
    }

    fn privileged_password_prompt(code: &str, progress_token: Option<Value>) -> Self {
        let method = if code == SUDO_PASSWORD_NEEDED {
            "sudo"
        } else {
            "su"
        };
        Self {
            kind: "progress".to_string(),
            event: "privileged-password-prompt".to_string(),
            status: "input-required".to_string(),
            code: code.to_string(),
            message: format!(
                "FileTerm opened a secure {method} password prompt in the main window. Enter the password there; the command is waiting and will continue after submission."
            ),
            progress_token,
        }
    }

    fn connection_waiting(progress_token: Option<Value>) -> Self {
        Self {
            kind: "progress".to_string(),
            event: "connection-waiting".to_string(),
            status: "waiting".to_string(),
            code: MCP_CONNECTION_WAITING.to_string(),
            message: "FileTerm is waiting for the saved connection to become ready. If a secure SSH credential prompt appears in the FileTerm window, complete it there; the CLI/MCP request remains pending.".to_string(),
            progress_token,
        }
    }
}
