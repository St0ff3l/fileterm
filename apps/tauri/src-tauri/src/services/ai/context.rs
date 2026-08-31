// Context target resolution, redaction, preview binding, and consumption.
fn normalize_provider_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 160 {
        return Err(ai_error("AI_PROVIDER_NOT_FOUND", "AI Provider ID 无效"));
    }
    Ok(value.to_string())
}

fn resolve_chat_provider(
    app: &AppHandle,
    provider_id: &str,
) -> Result<(StoredAiProvider, Option<String>), AppError> {
    let provider_id = normalize_provider_id(provider_id)?;
    let _guard = store_lock()?;
    let (config, secrets) = read_normalized_store(app)?;
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .cloned()
        .ok_or_else(|| ai_error("AI_PROVIDER_NOT_FOUND", "找不到指定的 AI Provider"))?;
    if !provider_is_usable(&provider, &secrets) {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            "AI Provider 不可用，请检查模型、密钥和连接设置",
        ));
    }
    let api_key = secrets
        .providers
        .get(&provider.id)
        .map(|secret| secret.api_key.trim().to_string())
        .filter(|api_key| !api_key.is_empty());
    Ok((provider, api_key))
}

fn normalize_context_tab_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 160
        || value
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err(ai_error(
            "AI_CONTEXT_TARGET_CHANGED",
            "终端目标无效，请重新选择会话",
        ));
    }
    Ok(value.to_string())
}

fn normalize_context_snapshot_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(ai_error(
            "AI_CONTEXT_NOT_FOUND",
            "上下文预览无效，请重新预览",
        ));
    }
    Ok(value.to_string())
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn context_registry_lock() -> Result<std::sync::MutexGuard<'static, AiContextRegistry>, AppError> {
    AI_CONTEXT_REGISTRY
        .lock()
        .map_err(|_| AppError::Storage("AI 上下文内存锁不可用".to_string()))
}

fn prune_expired_context_snapshots(registry: &mut AiContextRegistry, now: u128) {
    let expired_snapshot_ids = registry
        .snapshots
        .iter()
        .filter_map(|(snapshot_id, snapshot)| {
            (snapshot.expires_at_millis <= now).then_some(snapshot_id.clone())
        })
        .collect::<Vec<_>>();
    for snapshot_id in expired_snapshot_ids {
        registry.snapshots.remove(&snapshot_id);
        // Keep a short-lived tombstone so a user receives the accurate
        // expired error even if another preview happened to trigger cleanup
        // before they clicked Send.
        registry.expired_snapshot_ids.insert(
            snapshot_id,
            now.saturating_add(CONTEXT_SNAPSHOT_TTL.as_millis()),
        );
    }
    registry
        .consumed_snapshot_ids
        .retain(|_, expires_at_millis| *expires_at_millis > now);
    registry
        .expired_snapshot_ids
        .retain(|_, expires_at_millis| *expires_at_millis > now);
}

fn public_context_preview(snapshot: &StoredAiContextSnapshot) -> AiContextPreview {
    AiContextPreview {
        snapshot_id: snapshot.snapshot_id.clone(),
        expires_at: snapshot.expires_at_millis.to_string(),
        mode: snapshot.mode,
        target: snapshot.target.clone(),
        preview: snapshot.preview.clone(),
        redactions: snapshot.redactions.clone(),
        truncated: snapshot.truncated,
    }
}

async fn resolve_context_target(
    app: &AppHandle,
    tab_id: &str,
    requested_root_tab_id: Option<&str>,
    include_terminal_transcript: bool,
) -> Result<(AiContextTarget, Option<String>), AppError> {
    let tab_id = normalize_context_tab_id(tab_id)?;
    let requested_root_tab_id = requested_root_tab_id
        .map(normalize_context_tab_id)
        .transpose()?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    let (tab, root_tab) = {
        let tabs = state.tabs.read().await;
        let tab = tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .cloned()
            .ok_or_else(|| ai_error("AI_CONTEXT_TARGET_CHANGED", "目标终端已关闭，请重新预览"))?;
        if !matches!(tab.session_type.as_str(), "ssh" | "local") {
            return Err(ai_error(
                "AI_CONTEXT_TARGET_CHANGED",
                "当前会话不支持 AI 终端上下文",
            ));
        }
        let root_tab = tabs
            .iter()
            .find(|candidate| {
                candidate
                    .pane_root
                    .as_ref()
                    .is_some_and(|root| root.leaf_tab_ids().iter().any(|id| id == &tab_id))
            })
            .cloned()
            .unwrap_or_else(|| tab.clone());
        (tab, root_tab)
    };

    if requested_root_tab_id
        .as_deref()
        .is_some_and(|requested| requested != root_tab.id)
    {
        return Err(ai_error(
            "AI_CONTEXT_TARGET_CHANGED",
            "分屏目标已变化，请重新预览",
        ));
    }

    if root_tab.pane_root.is_some() {
        let active_pane = state
            .active_pane_tab_id_by_root
            .read()
            .await
            .get(&root_tab.id)
            .cloned();
        if active_pane.as_deref() != Some(tab_id.as_str()) {
            return Err(ai_error(
                "AI_CONTEXT_TARGET_CHANGED",
                "当前活动分屏已变化，请重新预览",
            ));
        }
    }

    // L1 deliberately never even clones the runtime transcript. Keeping the
    // accessor behind this explicit flag protects the product boundary from a
    // future metadata-only caller accidentally reading terminal contents.
    let (access_host, shell_user, login_user, shell_cwd, remote_path, transcript, network_device) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| ai_error("AI_CONTEXT_TARGET_CHANGED", "终端会话不可用，请重新预览"))?;
        if !session.connected || !session.capabilities.terminal || !tab.status.is_connected() {
            return Err(ai_error(
                "AI_CONTEXT_TARGET_CHANGED",
                "终端未连接，请连接后重新预览",
            ));
        }
        (
            session.access_host.clone(),
            session.shell_user.clone(),
            session.login_user.clone(),
            session.shell_cwd.clone(),
            session.remote_path.clone(),
            include_terminal_transcript.then(|| session.terminal_transcript.clone()),
            session.device_mode.as_deref() == Some("network-device"),
        )
    };
    let session_revision = state.ai_session_revision(&tab_id).await.to_string();
    let display_host = if access_host.trim().is_empty() {
        tab.title.clone()
    } else {
        access_host.trim().to_string()
    };
    let user = shell_user
        .or(login_user)
        .filter(|value| !value.trim().is_empty());
    let cwd = (!network_device)
        .then(|| {
            shell_cwd
                .or_else(|| (!remote_path.trim().is_empty()).then_some(remote_path))
                .filter(|value| !value.trim().is_empty())
        })
        .flatten();

    Ok((
        AiContextTarget {
            tab_id,
            root_tab_id: root_tab.id,
            session_type: tab.session_type,
            session_revision,
            display_host,
            user,
            cwd,
            connected: true,
            network_device,
        },
        transcript,
    ))
}

fn add_redaction(
    redactions: &mut Vec<AiContextRedaction>,
    kind: AiContextRedactionKind,
    count: usize,
) {
    if count == 0 {
        return;
    }
    if let Some(existing) = redactions.iter_mut().find(|entry| entry.kind == kind) {
        existing.count = existing.count.saturating_add(count);
    } else {
        redactions.push(AiContextRedaction { kind, count });
    }
}

fn strip_terminal_controls(value: &str) -> (String, usize) {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(normalized.len());
    let mut characters = normalized.chars().peekable();
    let mut removed = 0usize;

    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            removed = removed.saturating_add(1);
            match characters.next() {
                Some('[') => {
                    for next in characters.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = characters.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && characters.next_if_eq(&'\\').is_some() {
                            break;
                        }
                    }
                }
                Some('P' | 'X' | '^' | '_') => {
                    while let Some(next) = characters.next() {
                        if next == '\u{1b}' && characters.next_if_eq(&'\\').is_some() {
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            }
            continue;
        }
        if character == '\t' {
            removed = removed.saturating_add(1);
            output.push_str("    ");
            continue;
        }
        if (character.is_control() || ('\u{7f}'..='\u{9f}').contains(&character))
            && character != '\n'
        {
            removed = removed.saturating_add(1);
            continue;
        }
        output.push(character);
    }
    (output, removed)
}

fn truncate_characters(value: &str, limit: usize) -> String {
    let mut output = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        output.push_str(" … [line truncated]");
    }
    output
}

fn sanitize_recent_terminal_output(value: &str) -> (String, Vec<AiContextRedaction>, bool) {
    let (normalized, control_count) = strip_terminal_controls(value);
    let mut redactions = Vec::new();
    add_redaction(
        &mut redactions,
        AiContextRedactionKind::ControlSequence,
        control_count,
    );

    let mut lines = Vec::new();
    let mut in_private_key = false;
    let mut private_key_count = 0usize;
    let mut authorization_count = 0usize;
    let mut credential_count = 0usize;
    let mut long_line_count = 0usize;
    for line in normalized.split('\n') {
        let upper = line.to_ascii_uppercase();
        let begins_private_key = upper.contains("-----BEGIN") && upper.contains("PRIVATE KEY-----");
        let ends_private_key = upper.contains("-----END") && upper.contains("PRIVATE KEY-----");
        if begins_private_key {
            if !in_private_key {
                private_key_count = private_key_count.saturating_add(1);
                lines.push("[REDACTED PRIVATE KEY]".to_string());
            }
            in_private_key = !ends_private_key;
            continue;
        }
        if in_private_key {
            if ends_private_key {
                in_private_key = false;
            }
            continue;
        }

        let auth_matches = AUTHORIZATION_RE.find_iter(line).count();
        authorization_count = authorization_count.saturating_add(auth_matches);
        let line = AUTHORIZATION_RE
            .replace_all(line, "${1}[REDACTED]")
            .into_owned();
        let credential_matches = CREDENTIAL_ASSIGNMENT_RE.find_iter(&line).count();
        credential_count = credential_count.saturating_add(credential_matches);
        let line = CREDENTIAL_ASSIGNMENT_RE
            .replace_all(&line, "${1}${2}[REDACTED]")
            .into_owned();
        if line.chars().count() > MAX_CONTEXT_LINE_CHARACTERS {
            long_line_count = long_line_count.saturating_add(1);
            lines.push(truncate_characters(&line, MAX_CONTEXT_LINE_CHARACTERS));
        } else {
            lines.push(line);
        }
    }
    add_redaction(
        &mut redactions,
        AiContextRedactionKind::PrivateKey,
        private_key_count,
    );
    add_redaction(
        &mut redactions,
        AiContextRedactionKind::Authorization,
        authorization_count,
    );
    add_redaction(
        &mut redactions,
        AiContextRedactionKind::CredentialAssignment,
        credential_count,
    );
    add_redaction(
        &mut redactions,
        AiContextRedactionKind::LongLine,
        long_line_count,
    );

    let mut truncated = long_line_count > 0;
    if lines.len() > MAX_CONTEXT_PREVIEW_LINES {
        let omitted = lines.len() - (MAX_CONTEXT_PREVIEW_LINES - 1);
        let retained = lines.split_off(omitted);
        lines = Vec::with_capacity(retained.len() + 1);
        lines.push(format!("[... {omitted} earlier lines omitted]"));
        lines.extend(retained);
        truncated = true;
    }
    while lines.join("\n").len() > MAX_CONTEXT_PREVIEW_BYTES && lines.len() > 1 {
        lines.remove(0);
        truncated = true;
    }
    if truncated && !lines.first().is_some_and(|line| line.starts_with("[...")) {
        lines.insert(0, "[... earlier output omitted]".to_string());
        while lines.join("\n").len() > MAX_CONTEXT_PREVIEW_BYTES && lines.len() > 1 {
            lines.remove(1);
        }
    }
    let preview = if lines.iter().all(|line| line.is_empty()) {
        "[No readable terminal output was available.]".to_string()
    } else {
        lines.join("\n")
    };
    (preview, redactions, truncated)
}

fn context_mode_reads_terminal_transcript(mode: AiContextMode) -> bool {
    mode == AiContextMode::Level2
}

pub async fn create_context_preview(
    app: &AppHandle,
    window: &WebviewWindow,
    input: CreateAiContextPreviewInput,
) -> Result<AiContextPreview, AppError> {
    let provider_id = normalize_provider_id(&input.provider_id)?;
    // Provider validation is part of the preview binding: selecting another
    // provider after review requires a fresh confirmation.
    let _ = resolve_chat_provider(app, &provider_id)?;
    let (target, transcript) = resolve_context_target(
        app,
        &input.tab_id,
        input.root_tab_id.as_deref(),
        context_mode_reads_terminal_transcript(input.mode),
    )
    .await?;
    let (preview, redactions, truncated) = match input.mode {
        // L0 deliberately creates no provider-visible payload. The target is
        // still resolved so the local snapshot contract remains uniform, but
        // host/user/CWD metadata never crosses the provider boundary.
        AiContextMode::Level0 => (String::new(), Vec::new(), false),
        AiContextMode::Level2 => {
            sanitize_recent_terminal_output(transcript.as_deref().unwrap_or_default())
        }
    };
    let expires_at_millis = now_millis().saturating_add(CONTEXT_SNAPSHOT_TTL.as_millis());
    let snapshot = StoredAiContextSnapshot {
        snapshot_id: crate::storage::new_id("ai-context"),
        expires_at_millis,
        window_label: window.label().to_string(),
        provider_id,
        mode: input.mode,
        target,
        preview,
        redactions,
        truncated,
    };
    let public = public_context_preview(&snapshot);
    let mut registry = context_registry_lock()?;
    prune_expired_context_snapshots(&mut registry, now_millis());
    registry
        .snapshots
        .insert(snapshot.snapshot_id.clone(), snapshot);
    Ok(public)
}

fn take_context_snapshot(
    snapshot_id: &str,
    window_label: &str,
    provider_id: &str,
) -> Result<StoredAiContextSnapshot, AppError> {
    let snapshot_id = normalize_context_snapshot_id(snapshot_id)?;
    let mut registry = context_registry_lock()?;
    let now = now_millis();
    prune_expired_context_snapshots(&mut registry, now);
    let Some(snapshot) = registry.snapshots.get(&snapshot_id).cloned() else {
        if registry.consumed_snapshot_ids.contains_key(&snapshot_id) {
            return Err(ai_error(
                "AI_CONTEXT_ALREADY_USED",
                "上下文预览已发送过，请重新预览",
            ));
        }
        if registry.expired_snapshot_ids.contains_key(&snapshot_id) {
            return Err(ai_error(
                "AI_CONTEXT_EXPIRED",
                "上下文预览已过期，请重新预览",
            ));
        }
        return Err(ai_error(
            "AI_CONTEXT_NOT_FOUND",
            "上下文预览已失效，请重新预览",
        ));
    };
    if snapshot.expires_at_millis <= now {
        registry.snapshots.remove(&snapshot_id);
        return Err(ai_error(
            "AI_CONTEXT_EXPIRED",
            "上下文预览已过期，请重新预览",
        ));
    }
    if snapshot.window_label != window_label {
        return Err(ai_error(
            "AI_CONTEXT_FORBIDDEN",
            "上下文预览仅可由原窗口发送",
        ));
    }
    if snapshot.provider_id != provider_id {
        return Err(ai_error(
            "AI_CONTEXT_TARGET_CHANGED",
            "AI Provider 已变化，请重新预览上下文",
        ));
    }
    registry.snapshots.remove(&snapshot_id);
    registry.consumed_snapshot_ids.insert(
        snapshot_id,
        now.saturating_add(CONTEXT_SNAPSHOT_TTL.as_millis()),
    );
    Ok(snapshot)
}

async fn consume_context_snapshot(
    app: &AppHandle,
    window_label: &str,
    provider_id: &str,
    snapshot_id: &str,
) -> Result<(AiContextAttachment, AiPromptContext), AppError> {
    let snapshot = take_context_snapshot(snapshot_id, window_label, provider_id)?;
    let (current_target, _) = resolve_context_target(
        app,
        &snapshot.target.tab_id,
        Some(&snapshot.target.root_tab_id),
        false,
    )
    .await?;
    if current_target != snapshot.target {
        return Err(ai_error(
            "AI_CONTEXT_TARGET_CHANGED",
            "终端目标已变化，请重新预览并确认上下文",
        ));
    }
    let attachment = AiContextAttachment {
        mode: snapshot.mode,
        target: snapshot.target,
        redactions: snapshot.redactions,
        truncated: snapshot.truncated,
    };
    let prompt_context = AiPromptContext {
        mode: attachment.mode,
        preview: snapshot.preview,
        network_device: attachment.target.network_device,
    };
    Ok((attachment, prompt_context))
}

async fn refresh_copilot_prompt_context(
    app: &AppHandle,
    prepared: &PreparedChatRequest,
) -> Result<Option<AiPromptContext>, AppError> {
    let Some(attachment) = prepared.context_attachment.as_ref() else {
        return Ok(None);
    };
    let include_terminal_transcript = context_mode_reads_terminal_transcript(attachment.mode);
    let (current_target, transcript) = resolve_context_target(
        app,
        &attachment.target.tab_id,
        Some(&attachment.target.root_tab_id),
        include_terminal_transcript,
    )
    .await?;
    if current_target != attachment.target {
        return Err(ai_error(
            "AI_CONTEXT_TARGET_CHANGED",
            "终端目标已变化，请重新预览并确认上下文",
        ));
    }

    let preview = if include_terminal_transcript {
        sanitize_recent_terminal_output(transcript.as_deref().unwrap_or_default()).0
    } else {
        String::new()
    };
    Ok(Some(AiPromptContext {
        mode: attachment.mode,
        preview,
        network_device: attachment.target.network_device,
    }))
}

