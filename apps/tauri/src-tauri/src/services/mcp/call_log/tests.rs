use super::*;

pub(super) struct AuditDirectory(PathBuf);

impl AuditDirectory {
    pub(super) fn new() -> Self {
        let path = env::temp_dir().join(format!("fileterm-audit-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(super) fn path(&self) -> PathBuf {
        self.0.join(MCP_CALL_LOG_FILE)
    }
}

impl Drop for AuditDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request(action: &str, params: Value) -> BridgeRequest {
    BridgeRequest {
        action: action.to_string(),
        params,
        source: WorkspaceSessionSource::Mcp,
        requires_approval: true,
        progress_token: None,
        client_id: Some(format!("bridge-{}", uuid::Uuid::new_v4().simple())),
    }
}

fn record(path: &Path, action: &str, params: Value, approval: &str, result: Result<Value, String>) {
    let mut audit = McpCallAudit::begin(Ok(path.to_path_buf()), &request(action, params));
    audit.approval(approval);
    audit.finish(result.as_ref().map_err(String::as_str));
}

#[test]
fn audit_omits_unstructured_secrets_even_for_rejected_calls() {
    let dir = AuditDirectory::new();
    for action in [
        "execute_remote_command",
        "write_remote_file",
        "execute_command_template",
    ] {
        record(
            &dir.path(),
            action,
            json!({
                "tab_id": "tab-1", "path": "/.env",
                "command": "curl -H 'Authorization: Bearer sensitive-value'",
                "content": "API_KEY=sensitive-value",
                "args": ["sensitive-value"],
                "rule": {"privateKey": "sensitive-value"},
                "sudo_password": "sensitive-value",
                "unknown": "sensitive-value",
            }),
            "rejected",
            Err("approval rejected: sensitive-value".to_string()),
        );
    }
    let disk = fs::read_to_string(dir.path()).unwrap();
    assert!(!disk.contains("sensitive-value"));
    let page = store::read_page(&dir.path(), 50, 0, None).unwrap();
    assert_eq!(page.items.len(), 3);
    assert!(page
        .items
        .iter()
        .all(|entry| entry["approval"] == "rejected" && entry["ok"] == false));
    assert_eq!(page.items[1]["params"]["path"], "/.env");
    assert_eq!(
        redact_params("unsupported", &json!({"path": "sensitive-value"})),
        json!({})
    );
    assert_eq!(
        redact_params(
            "open_connection",
            &json!({"profile_id": "p1", "execution_mode": "visible-terminal"})
        )["profile_id"],
        "p1"
    );
    assert_eq!(safe_action("open_connection"), "open_connection");
}

#[test]
fn dropped_and_cancelled_dispatches_retain_an_uncertain_record() {
    let dir = AuditDirectory::new();
    let audit = McpCallAudit::begin(
        Ok(dir.path()),
        &request("execute_remote_command", json!({"tab_id":"t1"})),
    );
    let page = store::read_page(&dir.path(), 50, 0, None).unwrap();
    assert_eq!(page.items[0]["status"], "started");
    drop(audit);
    let page = store::read_page(&dir.path(), 50, 0, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0]["status"], "outcome-uncertain");
    assert_eq!(page.items[0]["error"], "FILETERM_CALL_INTERRUPTED");
}

#[tokio::test]
async fn aborting_a_polled_dispatch_runs_audit_cleanup() {
    let dir = AuditDirectory::new();
    let path = dir.path();
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _audit = McpCallAudit::begin(
            Ok(path),
            &request("delete_remote_path", json!({"tab_id":"t1"})),
        );
        let _ = ready.send(());
        std::future::pending::<()>().await;
    });
    started.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let page = store::read_page(&dir.path(), 50, 0, None).unwrap();
    assert_eq!(page.items[0]["status"], "outcome-uncertain");
}

#[test]
fn audit_distinguishes_command_failure_timeout_acceptance_and_call_success() {
    for (result, expected_ok, expected_status) in [
        (json!({"result":{"exitCode":1}}), false, "command-failed"),
        (json!({"result":{"exitCode":0}}), true, "command-succeeded"),
        (
            json!({"result":{"exitCode":null,"timedOut":true}}),
            false,
            "outcome-uncertain",
        ),
        (
            json!({"result":{"inputRequired":true}}),
            false,
            "outcome-uncertain",
        ),
        (json!({"running":true,"exitCode":null}), true, "accepted"),
        (json!({"accepted":true}), true, "accepted"),
        (json!({"session":{}}), true, "call-succeeded"),
    ] {
        let (ok, status, _, _) = outcome_metadata(Ok(&result));
        assert_eq!(ok, expected_ok);
        assert_eq!(status, expected_status);
    }
    assert_eq!(
        outcome_metadata(Err("FileTerm MCP request cancelled")).1,
        "outcome-uncertain"
    );
    assert_eq!(
        outcome_metadata(Err(
            "FileTerm MCP request timed out; retry after checking the session"
        ))
        .1,
        "outcome-uncertain"
    );
    assert_eq!(
        super::super::mcp_approval_status(ActionApprovalDecision::Rejected),
        "rejected"
    );
    assert_eq!(
        super::super::mcp_approval_status(ActionApprovalDecision::Approved),
        "user"
    );
}

#[test]
fn audit_resolves_targets_from_saved_state_not_client_identity() {
    let snapshot = json!({
        "tabs":[{"id":"t1","profileId":"p1"}],
        "profiles":[{"id":"p1","host":"example.test","port":22,"type":"ssh","password":"sensitive-value"}],
        "transfers":[{"id":"x1","profileId":"p1"}],
    });
    let target = resolve_target(
        &snapshot,
        &json!({"tab_id":"t1","profile_id":"forged"}),
        None,
    );
    assert_eq!(target["profileId"], "p1");
    assert_eq!(target["host"], "example.test");
    assert_eq!(target["protocol"], "ssh");
    assert!(!target.to_string().contains("sensitive-value"));
    assert_eq!(
        resolve_target(&snapshot, &json!({"transfer_id":"x1"}), None)["profileId"],
        "p1"
    );
    assert_eq!(
        resolve_target(&snapshot, &json!({"tab_id":"missing"}), None),
        Value::Null
    );
}

#[test]
fn cursor_stays_stable_across_new_calls_updates_and_rotation() {
    let dir = AuditDirectory::new();
    let mut pending = McpCallAudit::begin(Ok(dir.path()), &request("list_connections", json!({})));
    pending.entry["ts"] = json!("2000-01-01T00:00:00.000000000Z");
    pending.persist();
    for _ in 0..2 {
        record(
            &dir.path(),
            "list_connections",
            json!({}),
            "not-required",
            Ok(json!({})),
        );
    }
    let first = store::read_page(&dir.path(), 2, 0, None).unwrap();
    let cursor = first.next_cursor.as_deref().unwrap();
    fs::rename(dir.path(), dir.0.join(MCP_CALL_LOG_ROTATED_FILE)).unwrap();
    record(
        &dir.path(),
        "list_connections",
        json!({}),
        "not-required",
        Ok(json!({})),
    );
    pending.finish(Ok(&json!({})));
    let second = store::read_page(&dir.path(), 2, 0, Some(cursor)).unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(!first
        .items
        .iter()
        .any(|entry| entry["id"] == second.items[0]["id"]));
    assert_eq!(second.total, 4);
    assert!(store::read_page(&dir.path(), 2, 0, Some("expired")).is_err());
    assert!(store::read_page(&dir.path(), 2, usize::MAX, None)
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn legacy_records_are_sanitized_and_partial_lines_do_not_hide_new_calls() {
    let dir = AuditDirectory::new();
    fs::write(
        dir.path(),
        concat!(
        "{\"ts\":\"2026-11-07T00:00:00Z\",\"source\":\"cli\",\"action\":\"write_remote_file\",",
        "\"clientId\":\"forged\",\"approval\":\"user\",\"ok\":true,\"error\":\"sensitive-value\",",
        "\"params\":{\"content\":\"sensitive-value\",\"path\":\"/.env\"}}\n",
        "{\"partial\":"
    ),
    )
    .unwrap();
    record(
        &dir.path(),
        "list_connections",
        json!({}),
        "not-required",
        Ok(json!({})),
    );
    let page = store::read_page(&dir.path(), 50, 0, None).unwrap();
    assert_eq!(page.items.len(), 2);
    let legacy = page
        .items
        .iter()
        .find(|entry| entry["schemaVersion"] == 1)
        .unwrap();
    assert_eq!(legacy["approval"], "legacy-unknown");
    assert_eq!(legacy["bridgeSessionId"], Value::Null);
    assert!(!serde_json::to_string(&page)
        .unwrap()
        .contains("sensitive-value"));
}

#[test]
fn oversized_legacy_shards_keep_the_bounded_recent_tail_readable() {
    let dir = AuditDirectory::new();
    let mut content = vec![b'x'; MCP_CALL_LOG_MAX_BYTES as usize + 100];
    content.extend_from_slice(b"\n");
    content.extend_from_slice(
        br#"{"ts":"2026-11-07T00:00:00Z","source":"cli","action":"list_connections","params":{}}"#,
    );
    content.extend_from_slice(b"\n");
    fs::write(dir.path(), content).unwrap();
    let page = store::read_page(&dir.path(), 50, 0, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0]["action"], "list_connections");
}
