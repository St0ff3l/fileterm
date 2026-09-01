// Process-local Copilot mode state and mode controls.
fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

fn default_conversation_schema_version() -> u32 {
    CONVERSATION_SCHEMA_VERSION
}

fn default_ai_mode_state() -> StoredAiModeState {
    StoredAiModeState {
        mode: AiCopilotMode::PureConversation,
        pure_context_preference: false,
        session_generation: 0,
        dangerous_command_restrictions_enabled: true,
    }
}

fn mode_registry_lock(
) -> Result<std::sync::MutexGuard<'static, HashMap<String, StoredAiModeState>>, AppError> {
    AI_MODE_REGISTRY
        .lock()
        .map_err(|_| AppError::Command("AI Copilot 模式状态锁不可用".to_string()))
}

fn mode_state_for_window(window_label: &str) -> Result<StoredAiModeState, AppError> {
    let mut registry = mode_registry_lock()?;
    Ok(registry
        .entry(window_label.to_string())
        .or_insert_with(default_ai_mode_state)
        .clone())
}

fn effective_context_attachment(state: &StoredAiModeState) -> bool {
    state.mode.requires_l2() || state.pure_context_preference
}

fn public_mode_state(state: &StoredAiModeState) -> AiCopilotModeState {
    AiCopilotModeState {
        mode: state.mode,
        attach_terminal_context: effective_context_attachment(state),
        auto_mode_guardrails: AiAutoModeGuardrailState {
            dangerous_command_restrictions_enabled: state.dangerous_command_restrictions_enabled,
        },
    }
}

pub fn get_copilot_mode_state(window: &WebviewWindow) -> Result<AiCopilotModeState, AppError> {
    Ok(public_mode_state(&mode_state_for_window(window.label())?))
}

pub fn set_copilot_mode(
    window: &WebviewWindow,
    input: SetAiCopilotModeInput,
) -> Result<AiCopilotModeState, AppError> {
    let mut registry = mode_registry_lock()?;
    let state = registry
        .entry(window.label().to_string())
        .or_insert_with(default_ai_mode_state);
    if input.mode == AiCopilotMode::FullyAutomatic
        && state.mode != AiCopilotMode::FullyAutomatic
        && !input.confirmed
    {
        return Err(ai_error(
            "AI_MODE_CONFIRMATION_REQUIRED",
            "启用全自动模式前必须由用户确认远端命令可能不经逐次审批执行",
        ));
    }
    let mode_changed = state.mode != input.mode;
    state.mode = input.mode;
    if mode_changed {
        // The registry is process-local, so a restart also requires a new
        // full-auto opt-in.
        state.session_generation = state.session_generation.wrapping_add(1);
    }
    Ok(public_mode_state(state))
}

pub fn set_context_attach(
    window: &WebviewWindow,
    input: SetAiContextAttachInput,
) -> Result<AiCopilotModeState, AppError> {
    let mut registry = mode_registry_lock()?;
    let state = registry
        .entry(window.label().to_string())
        .or_insert_with(default_ai_mode_state);
    if state.mode.requires_l2() && !input.attach_terminal_context {
        return Err(ai_error(
            "AI_CONTEXT_LOCKED",
            "半自动和全自动模式必须附带 L2 终端上下文",
        ));
    }
    state.pure_context_preference = input.attach_terminal_context;
    Ok(public_mode_state(state))
}

pub fn set_dangerous_command_restrictions(
    window: &WebviewWindow,
    input: SetAiDangerousCommandRestrictionsInput,
) -> Result<AiCopilotModeState, AppError> {
    let mut registry = mode_registry_lock()?;
    let state = registry
        .entry(window.label().to_string())
        .or_insert_with(default_ai_mode_state);
    state.dangerous_command_restrictions_enabled = input.enabled;
    Ok(public_mode_state(state))
}
