// Terminal input enqueue and active-pane safety guards.

pub(crate) async fn send_terminal_input(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    data: String,
) -> Result<(), AppError> {
    if state
        .serial_transfer_cancellations
        .read()
        .await
        .contains_key(tab_id)
    {
        return Err(AppError::Command("serial transfer active".to_string()));
    }
    if let Some(sender) = state.terminal_inputs.read().await.get(tab_id).cloned() {
        return sender
            .send(data)
            .map_err(|_| AppError::Storage("Terminal session closed".to_string()));
    }

    // Telnet and serial still use their protocol worker queue. SSH owns the
    // dedicated low-latency input channel above.
    let sender = state
        .workers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .ok_or_else(|| AppError::Storage("Terminal session not found".to_string()))?;
    timeout(
        WORKER_CMD_SEND_TIMEOUT,
        sender.send(WorkerCmd::WriteTerminal(data)),
    )
    .await
    .map_err(|_| AppError::Storage("Terminal worker busy".to_string()))?
    .map_err(|error| AppError::Storage(error.to_string()))
}

/// Send an exact command only while the requested session is still the active
/// terminal pane. Keep the active-tab read guards through the bounded worker
/// enqueue so a tab switch cannot land between validation and the write.
pub(crate) async fn send_exact_active_terminal_input(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    expected_session_revision: Option<&str>,
    data: String,
) -> Result<(), AppError> {
    // Keep the tab read lock through validation and enqueue. Reconnect and
    // close first claim the tab by writing this collection; if they already
    // won that race, the status check below fails closed instead of sending
    // into a worker that is about to be replaced.
    let tabs = state.tabs.read().await;
    let tab = tabs
        .iter()
        .find(|tab| tab.id == tab_id)
        .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
    if !tab.status.is_connected() {
        return Err(AppError::Command(
            "FileTerm SSH session is not connected".to_string(),
        ));
    }
    let root_tab_id = tab
        .pane_root_tab_id
        .clone()
        .unwrap_or_else(|| tab.id.clone());
    let active_tab = state.active_tab_id.read().await;
    if active_tab.as_deref() != Some(root_tab_id.as_str()) {
        return Err(AppError::Command(
            crate::services::action_review::VISIBLE_TERMINAL_SESSION_NOT_ACTIVE.to_string(),
        ));
    }
    let active_panes = state.active_pane_tab_id_by_root.read().await;
    let active_pane_matches = active_panes
        .get(&root_tab_id)
        .map_or(root_tab_id == tab_id, |active_id| active_id == tab_id);
    if !active_pane_matches {
        return Err(AppError::Command(
            crate::services::action_review::VISIBLE_TERMINAL_SESSION_NOT_ACTIVE.to_string(),
        ));
    }

    let sessions = state.sessions.read().await;
    if !sessions
        .get(tab_id)
        .is_some_and(|session| session.connected)
    {
        return Err(AppError::Command(
            "FileTerm SSH session is not connected".to_string(),
        ));
    }

    // Keep the identity read lock through the worker enqueue. A reconnect
    // increments this revision before installing the replacement worker; if
    // it is allowed to pass between validation and send, an approved command
    // can land in the replacement PTY with the same tab ID.
    let session_revisions = state.ai_session_revisions.read().await;
    if let Some(expected_session_revision) = expected_session_revision {
        let current_session_revision = session_revisions
            .get(tab_id)
            .copied()
            .unwrap_or_default()
            .to_string();
        if current_session_revision != expected_session_revision {
            return Err(AppError::Command("AI_AUTO_MODE_TARGET_CHANGED".to_string()));
        }
    }

    let workers = state.workers.read().await;
    let sender = workers
        .get(tab_id)
        .cloned()
        .ok_or_else(|| AppError::Storage("Terminal session not found".to_string()))?;
    timeout(
        WORKER_CMD_SEND_TIMEOUT,
        sender.send(WorkerCmd::WriteTerminal(data)),
    )
    .await
    .map_err(|_| AppError::Storage("Terminal worker busy".to_string()))?
    .map_err(|error| AppError::Storage(error.to_string()))
}
