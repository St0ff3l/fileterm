impl ActionApprovalDecision {
    pub fn rejection_message(self, source: ActionApprovalSource) -> &'static str {
        match (source, self) {
            (_, Self::Approved) => "",
            (_, Self::DelegatedToTerminal) => {
                "Copilot command was delegated to the visible terminal"
            }
            (ActionApprovalSource::Cli | ActionApprovalSource::Mcp, Self::Rejected) => {
                "FileTerm external operation was rejected by the user"
            }
            (ActionApprovalSource::Cli | ActionApprovalSource::Mcp, Self::Dismissed) => {
                "FileTerm external approval dialog was closed"
            }
            (ActionApprovalSource::Cli | ActionApprovalSource::Mcp, Self::TimedOut) => {
                "FileTerm external approval timed out; the operation was not started"
            }
            (ActionApprovalSource::AiCopilot, Self::Rejected) => {
                "Copilot tool call was rejected by the user"
            }
            (ActionApprovalSource::AiCopilot, Self::Dismissed) => "Copilot approval was dismissed",
            (ActionApprovalSource::AiCopilot, Self::TimedOut) => {
                "Copilot approval timed out; the command was not started"
            }
        }
    }
}

/// Queue a one-time visible approval. The caller decides how a denied or
/// timed-out decision should be represented to its own user (MCP returns an
/// error; Copilot persists a tool result). A Copilot call may also return
/// `DelegatedToTerminal`, which explicitly skips the background exec path.
pub async fn request_action_approval(
    app: &AppHandle,
    source: ActionApprovalSource,
    operation: impl Into<String>,
    details: ActionApprovalDetails,
) -> Result<ActionApprovalDecision, AppError> {
    let request_id = format!("action-approval-{}", uuid::Uuid::new_v4());
    request_action_approval_with_id(app, request_id, source, operation, details).await
}

/// Queue a one-time visible approval using a caller-supplied ID. Copilot uses
/// this to correlate the backend approval gate with the inline command card
/// that represents the same tool call in its conversation.
pub async fn request_action_approval_with_id(
    app: &AppHandle,
    request_id: String,
    source: ActionApprovalSource,
    operation: impl Into<String>,
    details: ActionApprovalDetails,
) -> Result<ActionApprovalDecision, AppError> {
    request_action_approval_with_id_and_target(app, request_id, source, operation, details, None)
        .await
}

/// Queue a one-time approval and bind it to the exact Copilot target and
/// command that may later be handed to the visible terminal. The binding is
/// kept in Rust next to the oneshot sender so a stale renderer card cannot
/// substitute a different command or tab.
pub async fn request_action_approval_with_id_and_target(
    app: &AppHandle,
    request_id: String,
    source: ActionApprovalSource,
    operation: impl Into<String>,
    details: ActionApprovalDetails,
    terminal_handoff: Option<ActionApprovalTargetBinding>,
) -> Result<ActionApprovalDecision, AppError> {
    let operation = operation.into();
    let (sender, receiver) = oneshot::channel();
    let handoff_gate = Arc::new(tokio::sync::Mutex::new(()));
    let has_terminal_handoff = terminal_handoff.is_some();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state.pending_action_approvals.write().await.insert(
        request_id.clone(),
        crate::services::workspace::PendingActionApproval {
            sender,
            terminal_handoff,
            handoff_gate: handoff_gate.clone(),
        },
    );

    if matches!(
        source,
        ActionApprovalSource::Cli | ActionApprovalSource::Mcp
    ) {
        // External CLI and MCP approvals must return to the main window so a
        // hidden or unfocused desktop window cannot leave the caller waiting
        // invisibly. Keep the original source in the event payload for the
        // renderer and audit log.
        crate::show_main_window(app);
    }

    let payload = ActionApprovalRequest {
        request_id: request_id.clone(),
        source,
        operation: operation.clone(),
        title: details.title,
        summary: details.summary,
        target: details.target,
        details: details.details,
        destructive: details.destructive,
        requires_risk_acknowledgement: details.requires_risk_acknowledgement,
    };
    if let Err(error) = app.emit("action:approval-request", payload) {
        state
            .pending_action_approvals
            .write()
            .await
            .remove(&request_id);
        return Err(AppError::Command(format!(
            "Unable to publish action approval request: {error}"
        )));
    }

    let decision = if has_terminal_handoff {
        wait_for_terminal_handoff_approval(&state, &request_id, receiver, handoff_gate).await
    } else {
        match timeout(ACTION_APPROVAL_TIMEOUT, receiver).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => ActionApprovalDecision::Dismissed,
            Err(_) => ActionApprovalDecision::TimedOut,
        }
    };
    state
        .pending_action_approvals
        .write()
        .await
        .remove(&request_id);

    let outcome = match decision {
        ActionApprovalDecision::Approved => "granted",
        ActionApprovalDecision::Rejected => "denied",
        ActionApprovalDecision::Dismissed => "dismissed",
        ActionApprovalDecision::TimedOut => "timed-out",
        ActionApprovalDecision::DelegatedToTerminal => "delegated-to-terminal",
    };
    crate::services::logging::info(
        app,
        "action-review",
        format!("approval {outcome} source={source:?} operation={operation}"),
    );
    Ok(decision)
}

/// Resolve an in-app approval exactly once. An unknown ID is intentionally a
/// no-op: it may have already timed out or been dismissed while the renderer
/// was transitioning windows.
pub async fn resolve_action_approval(
    app: &AppHandle,
    request_id: &str,
    approved: bool,
) -> Result<(), AppError> {
    resolve_action_approval_decision(
        app,
        request_id,
        if approved {
            ActionApprovalDecision::Approved
        } else {
            ActionApprovalDecision::Rejected
        },
    )
    .await
}

/// Resolve an in-app approval with a specific outcome. This is kept separate
/// from the boolean approval API so the visible-terminal handoff cannot be
/// persisted or reported as a user rejection.
pub async fn resolve_action_approval_decision(
    app: &AppHandle,
    request_id: &str,
    decision: ActionApprovalDecision,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid action approval request".to_string(),
        ));
    }
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = {
        let mut pending = state.pending_action_approvals.write().await;
        pending.remove(request_id)
    };
    if let Some(pending) = sender {
        let _ = pending.sender.send(decision);
    }
    Ok(())
}

/// Wait for a Copilot approval while making timeout and visible-terminal
/// handoff claim the same per-request gate. The timeout branch keeps the
/// oneshot receiver alive until it has acquired that gate and removed the
/// pending entry; if a handoff already owns the gate, it must finish (or
/// reject) before the approval waiter can conclude that the request expired.
async fn wait_for_terminal_handoff_approval(
    state: &crate::services::workspace::WorkspaceState,
    request_id: &str,
    mut receiver: oneshot::Receiver<ActionApprovalDecision>,
    handoff_gate: Arc<tokio::sync::Mutex<()>>,
) -> ActionApprovalDecision {
    let approval_timeout = sleep(ACTION_APPROVAL_TIMEOUT);
    tokio::pin!(approval_timeout);
    tokio::select! {
        response = &mut receiver => response.unwrap_or(ActionApprovalDecision::Dismissed),
        _ = &mut approval_timeout => {
            let _handoff_guard = handoff_gate.lock().await;
            let removed = state
                .pending_action_approvals
                .write()
                .await
                .remove(request_id);
            if removed.is_some() {
                ActionApprovalDecision::TimedOut
            } else {
                // A normal approval resolver or the handoff already owns the
                // request and will send its decision through this receiver.
                drop(_handoff_guard);
                receiver.await.unwrap_or(ActionApprovalDecision::Dismissed)
            }
        }
    }
}

/// Atomically consume a Copilot approval and send its exact command to the
/// currently active visible terminal. The Rust side validates the approval's
/// target, command, active pane, and session revision before writing to the
/// PTY; the renderer cannot execute first and resolve the approval later.
pub async fn execute_ai_terminal_handoff(
    app: &AppHandle,
    request_id: &str,
    raw_tab_id: &str,
    raw_command: &str,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid action approval request".to_string(),
        ));
    }
    let tab_id = validate_remote_exec_tab_id(raw_tab_id)?;
    let command = validate_visible_terminal_command(raw_command)?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    // Look up the gate before claiming the map entry, then hold it through
    // every validation and the final PTY enqueue. The timeout path follows
    // the same map-read -> gate -> map-write order, so only one side can win
    // the expiry/handoff race.
    let handoff_gate = state
        .pending_action_approvals
        .read()
        .await
        .get(request_id)
        .map(|pending| pending.handoff_gate.clone())
        .ok_or_else(|| AppError::Command(AI_TERMINAL_HANDOFF_NOT_PENDING.to_string()))?;
    let _handoff_guard = handoff_gate.lock().await;
    let pending = state
        .pending_action_approvals
        .write()
        .await
        .remove(request_id)
        .ok_or_else(|| AppError::Command(AI_TERMINAL_HANDOFF_NOT_PENDING.to_string()))?;

    let Some(binding) = pending.terminal_handoff.clone() else {
        let _ = pending.sender.send(ActionApprovalDecision::Rejected);
        return Err(AppError::Command(
            "Copilot approval is not eligible for terminal handoff".to_string(),
        ));
    };
    if binding.tab_id != tab_id || binding.command != command {
        let _ = pending.sender.send(ActionApprovalDecision::Rejected);
        return Err(AppError::Command(
            "Copilot terminal handoff target no longer matches the approved command".to_string(),
        ));
    }

    if let Err(error) = ensure_visible_terminal_session_active(app, &tab_id).await {
        let _ = pending.sender.send(ActionApprovalDecision::Rejected);
        return Err(error);
    }
    let current_revision = state.ai_session_revision(&tab_id).await.to_string();
    if current_revision != binding.session_revision {
        let _ = pending.sender.send(ActionApprovalDecision::Rejected);
        return Err(AppError::Command("AI_AUTO_MODE_TARGET_CHANGED".to_string()));
    }

    if let Err(error) = crate::commands::send_exact_active_terminal_input(
        &state,
        &tab_id,
        Some(&binding.session_revision),
        format!("{command}\r"),
    )
    .await
    {
        let _ = pending.sender.send(ActionApprovalDecision::Rejected);
        return Err(error);
    }

    let _ = pending
        .sender
        .send(ActionApprovalDecision::DelegatedToTerminal);
    Ok(())
}

#[derive(Clone)]
pub struct RemoteExecRequest {
    pub tab_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    /// Optional identity binding used by Copilot. External callers leave it
    /// unset; a bound request is rejected if the SSH target changes before
    /// the isolated exec channel starts.
    pub expected_session_revision: Option<String>,
    /// One-shot values supplied by a trusted local caller. They are never
    /// logged, persisted, or returned to the caller.
    pub sudo_password: Option<String>,
    pub su_password: Option<String>,
    pub save_sudo_password: bool,
    pub save_su_password: bool,
    /// Whether a missing privileged credential may be resolved through the
    /// local FileTerm password prompt.
    pub allow_local_privileged_prompt: bool,
    /// Optional progress hook used by AI Copilot and the MCP/CLI bridge to show
    /// that the tool call is waiting for the user in the foreground FileTerm
    /// window.
    pub privileged_prompt_notice: Option<PrivilegedPromptNotice>,
}
