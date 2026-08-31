use std::sync::{atomic::Ordering, Arc};

use tauri::{AppHandle, Emitter, Manager};

pub const LOCAL_TERMINAL_STARTUP_TRANSCRIPT: &str = "Starting local shell...\r\n";

pub fn local_terminal_startup_transcript() -> &'static str {
    if cfg!(target_os = "windows") {
        ""
    } else {
        LOCAL_TERMINAL_STARTUP_TRANSCRIPT
    }
}

pub fn decode_terminal(bytes: &[u8], encoding: &str) -> String {
    match encoding.trim().to_lowercase().as_str() {
        "gbk" | "gb18030" => encoding_rs::GB18030.decode(bytes).0.into_owned(),
        "big5" | "cp950" => encoding_rs::BIG5.decode(bytes).0.into_owned(),
        "shift-jis" | "shift_jis" | "sjis" => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
        "euc-jp" => encoding_rs::EUC_JP.decode(bytes).0.into_owned(),
        "euc-kr" | "cp949" => encoding_rs::EUC_KR.decode(bytes).0.into_owned(),
        "windows-1252" | "cp1252" | "latin1" | "iso-8859-1" => {
            encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

pub fn encode_terminal(value: &str, encoding: &str) -> Vec<u8> {
    match encoding.trim().to_lowercase().as_str() {
        "gbk" | "gb18030" => encoding_rs::GB18030.encode(value).0.into_owned(),
        "big5" | "cp950" => encoding_rs::BIG5.encode(value).0.into_owned(),
        "shift-jis" | "shift_jis" | "sjis" => encoding_rs::SHIFT_JIS.encode(value).0.into_owned(),
        "euc-jp" => encoding_rs::EUC_JP.encode(value).0.into_owned(),
        "euc-kr" | "cp949" => encoding_rs::EUC_KR.encode(value).0.into_owned(),
        "windows-1252" | "cp1252" | "latin1" | "iso-8859-1" => {
            encoding_rs::WINDOWS_1252.encode(value).0.into_owned()
        }
        _ => value.as_bytes().to_vec(),
    }
}

fn truncate_transcript(value: &mut String) {
    const LIMIT: usize = 200_000;
    const RETAIN: usize = 180_000;
    if value.len() <= LIMIT {
        return;
    }
    let mut start = value.len() - RETAIN;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    *value = value[start..].to_string();
}

pub async fn emit_terminal_data(app: &AppHandle, tab_id: &str, chunk: &str) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state.publish_terminal_output(tab_id, chunk);
    crate::services::session_logs::append_chunk(app, tab_id, chunk).await;
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(tab_id) {
        session.terminal_transcript.push_str(chunk);
        truncate_transcript(&mut session.terminal_transcript);
    }
}

/// Publish local PTY output only while its runtime is still the owner of the
/// tab. The gate also serializes the final output chunk with shutdown so an old
/// reader cannot publish after a reconnect has installed a new shell.
pub async fn emit_local_terminal_data(
    app: &AppHandle,
    tab_id: &str,
    runtime_id: &str,
    gate: &Arc<crate::services::workspace::LocalTerminalRuntimeGate>,
    chunk: &str,
) -> bool {
    if chunk.is_empty() {
        return true;
    }

    let _emit_guard = gate.emit_lock.lock().await;
    if !gate.active.load(Ordering::Acquire) {
        crate::services::logging::debug(
            app,
            "local",
            format!(
                "discarding PTY batch tab={} runtime={} bytes={} reason=runtime-inactive",
                tab_id,
                runtime_id,
                chunk.len()
            ),
        );
        return false;
    }

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let owns_runtime = state
        .local_terminal_runtime_ids
        .read()
        .await
        .get(tab_id)
        .is_some_and(|current_id| current_id == runtime_id);
    if !owns_runtime {
        crate::services::logging::warn(
            app,
            "local",
            format!(
                "discarding PTY batch tab={} runtime={} bytes={} reason=runtime-not-owner",
                tab_id,
                runtime_id,
                chunk.len()
            ),
        );
        return false;
    }

    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(tab_id) {
        session.terminal_transcript.push_str(chunk);
        truncate_transcript(&mut session.terminal_transcript);
    }
    state.publish_terminal_output(tab_id, chunk);
    true
}

/// Store an OSC 7 working-directory update from a local PTY without exposing
/// the launch environment. The local tab currently has no remote file pane,
/// so this is metadata-only; a future local file pane can opt into the same
/// `follow_shell_cwd` behavior used by SSH sessions.
pub async fn update_local_terminal_cwd(
    app: &AppHandle,
    tab_id: &str,
    runtime_id: &str,
    gate: &Arc<crate::services::workspace::LocalTerminalRuntimeGate>,
    cwd: String,
) -> bool {
    let _emit_guard = gate.emit_lock.lock().await;
    if !gate.active.load(Ordering::Acquire) {
        return false;
    }

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let owns_runtime = state
        .local_terminal_runtime_ids
        .read()
        .await
        .get(tab_id)
        .is_some_and(|current_id| current_id == runtime_id);
    if !owns_runtime {
        return false;
    }

    let changed = {
        let mut sessions = state.sessions.write().await;
        let Some(session) = sessions.get_mut(tab_id) else {
            return false;
        };
        if session.shell_cwd.as_deref() == Some(cwd.as_str()) {
            false
        } else {
            session.shell_cwd = Some(cwd.clone());
            if session.follow_shell_cwd {
                session.remote_path = cwd;
            }
            true
        }
    };
    if changed {
        state.touch_ai_session_revision(tab_id).await;
        if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    true
}

pub async fn set_terminal_state(
    app: &AppHandle,
    tab_id: &str,
    summary: String,
    status: crate::services::WorkspaceTabStatus,
) {
    set_terminal_state_with_snapshot(app, tab_id, summary, status, true).await;
}

/// Update a terminal state while allowing a compound workspace mutation to
/// defer its snapshot until the full invariant has been written. This is used
/// when a newly spawned local PTY is immediately inserted into a pane tree:
/// broadcasting the transient standalone tab would make the renderer flash a
/// second top-level tab before the split root is installed.
pub async fn set_terminal_state_without_snapshot(
    app: &AppHandle,
    tab_id: &str,
    summary: String,
    status: crate::services::WorkspaceTabStatus,
) {
    set_terminal_state_with_snapshot(app, tab_id, summary, status, false).await;
}

async fn set_terminal_state_with_snapshot(
    app: &AppHandle,
    tab_id: &str,
    summary: String,
    status: crate::services::WorkspaceTabStatus,
    emit_workspace_snapshot: bool,
) {
    let connected = status.is_connected();
    let (transcript, target_changed) = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        // 显式分块获取 tabs 与 sessions 锁，避免依赖 NBL 隐式释放；
        // 与 ssh.rs::update_tab_status_and_emit 保持一致，防止未来在 if let
        // 块内引入对 sessions 的访问导致两把写锁被同时持有。
        {
            let mut tabs = state.tabs.write().await;
            if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                tab.status = status;
            }
        }
        let mut sessions = state.sessions.write().await;
        let Some(session) = sessions.get_mut(tab_id) else {
            return;
        };
        let target_changed = session.connected != connected;
        session.summary = summary.clone();
        session.connected = connected;
        (session.terminal_transcript.clone(), target_changed)
    };
    if target_changed {
        app.state::<crate::services::workspace::WorkspaceState>()
            .touch_ai_session_revision(tab_id)
            .await;
    }
    let operation_state = if connected {
        crate::services::connection_operations::ConnectionOperationState::Connected
    } else if matches!(
        status,
        crate::services::WorkspaceTabStatus::Error | crate::services::WorkspaceTabStatus::Closed
    ) {
        crate::services::connection_operations::ConnectionOperationState::Failed {
            code: crate::services::connection_operations::FILETERM_CONNECTION_FAILED.to_string(),
        }
    } else {
        crate::services::connection_operations::ConnectionOperationState::Connecting
    };
    app.state::<crate::services::workspace::WorkspaceState>()
        .connection_operations
        .publish_for_tab(tab_id, operation_state)
        .await;
    let _ = app.emit(
        "terminal:state",
        serde_json::json!({
            "tabId": tab_id,
            "summary": summary,
            "transcript": transcript,
            "connected": connected,
            "status": status,
        }),
    );
    if emit_workspace_snapshot {
        if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
}
