// Copilot tool execution loop and chat request runtime.
async fn execute_copilot_tool_call(
    app: &AppHandle,
    prepared: &PreparedChatRequest,
    call: &ProviderToolCall,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<(ToolLoopResult, AiToolCallResult), AppError> {
    if cancellation.is_cancelled() {
        return Err(request_cancelled_error());
    }
    if call.name != COPILOT_EXECUTE_REMOTE_COMMAND_TOOL {
        return Ok(copilot_tool_result(
            &call.id,
            "invalid",
            Some("未知的 FileTerm 工具名称".to_string()),
        ));
    }
    let arguments = match copilot_tool_call_arguments(call) {
        Ok(arguments) => arguments,
        Err(error) => return Ok(copilot_tool_error_result(&call.id, &error)),
    };
    let command = arguments.command;
    let explanation = arguments.explanation;
    let risk = conservative_command_risk(&command, arguments.ai_risk);
    let Some(context_attachment) = prepared.context_attachment.as_ref() else {
        return Ok(copilot_tool_result(
            &call.id,
            "target-changed",
            Some("Copilot 工具调用缺少已确认的 L2 目标".to_string()),
        ));
    };
    let approval_request_id = if prepared.copilot_mode == AiCopilotMode::SemiAutomatic {
        Some(format!("action-approval-{}", uuid::Uuid::new_v4()))
    } else {
        None
    };
    let proposal = AiToolCallProposal {
        id: call.id.clone(),
        tool_name: call.name.clone(),
        command: command.clone(),
        risk,
        target: context_attachment.target.clone(),
        explanation,
        approval_request_id: approval_request_id.clone(),
    };
    emit_stream_event(
        channel,
        AiStreamEvent::ToolCall {
            proposal: proposal.clone(),
        },
    )?;

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let (current_target, _) = match resolve_context_target(
        app,
        &proposal.target.tab_id,
        Some(&proposal.target.root_tab_id),
        false,
    )
    .await
    {
        Ok(target) => target,
        Err(error) => {
            return Ok(copilot_tool_result(
                &proposal.id,
                "target-changed",
                Some(sanitize_review_error(&error.to_string())),
            ))
        }
    };
    if current_target != proposal.target {
        return Ok(copilot_tool_result(
            &proposal.id,
            "target-changed",
            Some("终端目标已变化，工具调用未执行".to_string()),
        ));
    }

    let source_window_label = prepared
        .source_window_label
        .as_deref()
        .ok_or_else(|| ai_error("AI_CONTEXT_FORBIDDEN", "Copilot 缺少来源窗口绑定"))?;
    let mode_state = mode_state_for_window(source_window_label)?;
    if !copilot_mode_state_is_current(
        &mode_state,
        prepared.copilot_mode,
        prepared.copilot_session_generation,
    ) {
        return Ok(copilot_tool_result(
            &proposal.id,
            "rejected",
            Some("Copilot 模式已变化，工具调用未执行".to_string()),
        ));
    }

    let terminal_transcript_len_before_approval =
        if prepared.copilot_mode == AiCopilotMode::SemiAutomatic {
            Some(terminal_transcript_len(app, &proposal.target.tab_id).await)
        } else {
            None
        };

    if prepared.copilot_mode == AiCopilotMode::SemiAutomatic {
        let risk_requires_acknowledgement = matches!(
            proposal.risk,
            AiCommandRisk::Destructive | AiCommandRisk::Privileged
        );
        let approval_request_id = approval_request_id
            .clone()
            .expect("semi-automatic Copilot calls always have an approval request ID");
        let approval = crate::services::action_review::request_action_approval_with_id_and_target(
            app,
            approval_request_id.clone(),
            crate::services::action_review::ActionApprovalSource::AiCopilot,
            "ai_copilot_execute_remote_command",
            crate::services::action_review::ActionApprovalDetails {
                title: "确认执行 Copilot 命令".to_string(),
                summary: if proposal.target.network_device {
                    "允许通过当前可见网络设备终端发送一条原生命令；不会打开独立 SSH exec 通道。"
                        .to_string()
                } else {
                    "允许执行会使用独立 SSH 通道；也可以改为交给当前可见终端执行。".to_string()
                },
                target: Some(review_target_label(&proposal.target)),
                details: Some(format!(
                    "工作目录：{}\n风险：{}\n超时：{} 秒\n命令：\n{}",
                    proposal.target.cwd.as_deref().unwrap_or("~"),
                    review_risk_label(&proposal.risk),
                    AI_REVIEW_TIMEOUT_MS / 1_000,
                    proposal.command
                )),
                destructive: matches!(
                    proposal.risk,
                    AiCommandRisk::Destructive | AiCommandRisk::Privileged
                ),
                requires_risk_acknowledgement: risk_requires_acknowledgement,
            },
            Some(
                crate::services::action_review::ActionApprovalTargetBinding {
                    tab_id: proposal.target.tab_id.clone(),
                    session_revision: proposal.target.session_revision.clone(),
                    command: proposal.command.clone(),
                },
            ),
        );
        let decision = tokio::select! {
            _ = cancellation.cancelled() => {
                let _ = crate::services::action_review::resolve_action_approval_decision(
                    app,
                    &approval_request_id,
                    crate::services::action_review::ActionApprovalDecision::Dismissed,
                ).await;
                return Err(request_cancelled_error());
            }
            result = approval => match result {
                Ok(decision) => decision,
                Err(error) => {
                    return Ok(copilot_tool_result(
                        &proposal.id,
                        "failed",
                        Some(sanitize_review_error(&error.to_string())),
                    ))
                }
            }
        };
        if cancellation.is_cancelled() {
            return Err(request_cancelled_error());
        }
        if matches!(
            decision,
            crate::services::action_review::ActionApprovalDecision::DelegatedToTerminal
        ) {
            wait_for_terminal_handoff_output(
                app,
                &proposal.target.tab_id,
                terminal_transcript_len_before_approval.unwrap_or_default(),
                cancellation,
            )
            .await;
            if cancellation.is_cancelled() {
                return Err(request_cancelled_error());
            }
            return Ok(copilot_tool_result(
                &proposal.id,
                "executed-in-terminal",
                Some(
                    "The command was handed to the visible terminal; do not execute it again through the background channel. Use the refreshed terminal context when summarizing the result.".to_string(),
                ),
            ));
        }
        if !matches!(
            decision,
            crate::services::action_review::ActionApprovalDecision::Approved
        ) {
            let terminal_command_observed = match terminal_transcript_len_before_approval {
                Some(previous_len) => {
                    terminal_command_was_observed(
                        app,
                        &proposal.target.tab_id,
                        previous_len,
                        &proposal.command,
                    )
                    .await
                }
                None => false,
            };
            if terminal_command_observed {
                wait_for_terminal_handoff_output(
                    app,
                    &proposal.target.tab_id,
                    terminal_transcript_len_before_approval.unwrap_or_default(),
                    cancellation,
                )
                .await;
                if cancellation.is_cancelled() {
                    return Err(request_cancelled_error());
                }
                return Ok(copilot_tool_result(
                    &proposal.id,
                    "executed-in-terminal",
                    Some(
                        "The same command was observed in the visible terminal after the background tool call was declined. Use the refreshed terminal context and do not execute it again through the background channel.".to_string(),
                    ),
                ));
            }
            return Ok(copilot_tool_result(
                &proposal.id,
                "rejected",
                Some(
                    decision
                        .rejection_message(
                            crate::services::action_review::ActionApprovalSource::AiCopilot,
                        )
                        .to_string(),
                ),
            ));
        }
    }

    // Approval and the guardrail check can both await local state or a
    // renderer decision. Re-read the process-local mode immediately before
    // opening the SSH exec channel so a mode switch cannot authorize a stale
    // tool call.
    let latest_mode_state = mode_state_for_window(source_window_label)?;
    if !copilot_mode_state_is_current(
        &latest_mode_state,
        prepared.copilot_mode,
        prepared.copilot_session_generation,
    ) {
        return Ok(copilot_tool_result(
            &proposal.id,
            "rejected",
            Some("Copilot 模式已变化，工具调用未执行".to_string()),
        ));
    }

    if cancellation.is_cancelled() {
        return Err(request_cancelled_error());
    }

    // Re-read the policy while holding the authoritative mode-state lock. The
    // earlier mode check is intentionally repeated here because a mode switch
    // may have happened while the collaboration approval dialog was open.
    if matches!(
        prepared.copilot_mode,
        AiCopilotMode::SemiAutomatic | AiCopilotMode::FullyAutomatic
    ) {
        let current_revision = state
            .ai_session_revision(&proposal.target.tab_id)
            .await
            .to_string();
        let registry = mode_registry_lock()?;
        let Some(latest_state) = registry.get(source_window_label) else {
            return Ok(copilot_tool_result(
                &proposal.id,
                "auto-blocked",
                Some("Copilot 护栏状态不可用，工具调用未执行".to_string()),
            ));
        };
        if !copilot_mode_state_is_current(
            latest_state,
            prepared.copilot_mode,
            prepared.copilot_session_generation,
        ) {
            return Ok(copilot_tool_result(
                &proposal.id,
                "rejected",
                Some("Copilot 模式已变化，工具调用未执行".to_string()),
            ));
        }
        if let Err(error) = crate::services::ai_guardrails::authorize_command(
            &proposal.command,
            proposal.risk,
            latest_state.dangerous_command_restrictions_enabled,
            Some(&proposal.target.session_revision),
            Some(&current_revision),
        ) {
            return Ok(copilot_tool_result(
                &proposal.id,
                "auto-blocked",
                Some(format!("{}: {}", error.code, error.reason)),
            ));
        }
    }

    let waiting_channel = channel.clone();
    let waiting_proposal_id = proposal.id.clone();
    let privileged_prompt_notice: crate::services::action_review::PrivilegedPromptNotice = Arc::new(
        move |needed_code: &str| {
            let notice = "\n\nFileTerm 已将主窗口置于前台，请在前台安全输入框中完成输入；当前工具调用会等待输入完成后继续执行。\n\n";
            let _ = emit_stream_event(
                &waiting_channel,
                AiStreamEvent::TextDelta {
                    text: notice.to_string(),
                },
            );
            let result = AiToolCallResult {
                proposal_id: waiting_proposal_id.clone(),
                status: "input-required".to_string(),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                reason: Some(format!(
                    "{needed_code}: FileTerm 已将主窗口置于前台，请等待用户在前台安全输入框中完成输入。"
                )),
                record_id: None,
                requested_at: None,
                approved_at: None,
                completed_at: None,
                timeout_ms: Some(AI_REVIEW_TIMEOUT_MS),
                output_truncated: None,
            };
            let _ = emit_stream_event(&waiting_channel, AiStreamEvent::ToolResult { result });
        },
    );
    let started_at = Instant::now();
    let execution = crate::services::action_review::execute_remote_command_cancellable(
        app,
        crate::services::action_review::RemoteExecRequest {
            tab_id: proposal.target.tab_id.clone(),
            command: proposal.command.clone(),
            cwd: proposal.target.cwd.clone(),
            timeout_ms: Some(AI_REVIEW_TIMEOUT_MS),
            expected_session_revision: Some(proposal.target.session_revision.clone()),
            // The in-app Copilot never receives or forwards a privileged
            // credential through a model tool call. action_review resolves an
            // encrypted profile secret or opens FileTerm's foreground secure
            // prompt instead.
            sudo_password: None,
            su_password: None,
            save_sudo_password: false,
            save_su_password: false,
            allow_local_privileged_prompt: true,
            privileged_prompt_notice: Some(privileged_prompt_notice),
        },
        cancellation,
    )
    .await;
    if cancellation.is_cancelled() {
        return Err(request_cancelled_error());
    }
    let duration = started_at.elapsed();
    let duration_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    let (loop_result, tool_result) = match execution {
        Ok(execution) => {
            let (mut output, _, _) = sanitize_recent_terminal_output(&execution.output);
            if output.chars().count() > MAX_COPILOT_TOOL_RESULT_CHARACTERS {
                output = truncate_characters(&output, MAX_COPILOT_TOOL_RESULT_CHARACTERS);
            }
            let status = if execution.timed_out {
                "timeout"
            } else if execution.input_required {
                "input-required"
            } else if execution.raw_terminal || execution.exit_code == Some(0) {
                "executed"
            } else {
                "failed"
            };
            let tool_result = AiToolCallResult {
                proposal_id: proposal.id.clone(),
                status: status.to_string(),
                exit_code: execution.exit_code,
                stdout: (!output.is_empty()).then_some(output),
                stderr: None,
                duration_ms: Some(duration_ms),
                reason: if execution.timed_out {
                    Some(
                        "网络设备命令等待终端输出静默超时，已返回当前可见终端中的部分输出。"
                            .to_string(),
                    )
                } else if execution.raw_terminal {
                    Some(
                        "命令已通过网络设备可见 raw PTY 发送；设备会话不提供可靠的进程 exit code。"
                            .to_string(),
                    )
                } else if execution.input_required {
                    Some(format!(
                        "{}: 该命令需要交互输入，请用户在可见 SSH 终端中完成操作后再重试。",
                        crate::services::action_review::REMOTE_INTERACTIVE_INPUT_REQUIRED
                    ))
                } else {
                    execution.timed_out.then(|| "远程命令超时".to_string())
                },
                record_id: None,
                requested_at: None,
                approved_at: None,
                completed_at: None,
                timeout_ms: Some(AI_REVIEW_TIMEOUT_MS),
                output_truncated: Some(execution.output_truncated),
            };
            (
                ToolLoopResult {
                    call_id: proposal.id.clone(),
                    content: copilot_tool_result_content(&tool_result),
                },
                tool_result,
            )
        }
        Err(error) => {
            let reason = sanitize_review_error(&error.to_string());
            let status = if reason.contains("TARGET_CHANGED") {
                "target-changed"
            } else if reason.contains("TIMEOUT") {
                "timeout"
            } else if reason.contains("PASSWORD_CANCELLED") {
                "cancelled"
            } else {
                "failed"
            };
            let tool_result = AiToolCallResult {
                proposal_id: proposal.id.clone(),
                status: status.to_string(),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(duration_ms),
                reason: Some(reason),
                record_id: None,
                requested_at: None,
                approved_at: None,
                completed_at: None,
                timeout_ms: Some(AI_REVIEW_TIMEOUT_MS),
                output_truncated: None,
            };
            (
                ToolLoopResult {
                    call_id: proposal.id.clone(),
                    content: copilot_tool_result_content(&tool_result),
                },
                tool_result,
            )
        }
    };
    Ok((loop_result, tool_result))
}

fn persisted_copilot_tool_activity(
    prepared: &PreparedChatRequest,
    call: &ProviderToolCall,
    result: &AiToolCallResult,
) -> Option<AiToolActivity> {
    if call.name != COPILOT_EXECUTE_REMOTE_COMMAND_TOOL {
        return None;
    }
    let context_attachment = prepared.context_attachment.as_ref()?;
    let arguments = copilot_tool_call_arguments(call).ok()?;
    let command = arguments.command;
    let explanation = arguments.explanation;
    let risk = conservative_command_risk(&command, arguments.ai_risk);
    Some(AiToolActivity {
        proposal: AiToolCallProposal {
            id: call.id.clone(),
            tool_name: call.name.clone(),
            command: command.clone(),
            risk,
            target: context_attachment.target.clone(),
            explanation,
            approval_request_id: None,
        },
        result: Some(result.clone()),
    })
}

fn request_cancelled_error() -> AppError {
    ai_error("AI_REQUEST_CANCELLED", "已停止 AI 回复")
}

async fn run_chat_request(
    app: &AppHandle,
    prepared: &PreparedChatRequest,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<(), AppError> {
    emit_stream_event(
        channel,
        AiStreamEvent::Started {
            request_id: prepared.request.request_id.clone(),
            message_id: prepared.request.assistant_message_id.clone(),
        },
    )?;

    let semaphore = CHAT_REQUEST_SEMAPHORE.clone();
    let _permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(request_cancelled_error()),
        permit = semaphore.acquire_owned() => permit.map_err(|_| {
            ai_error("AI_CONVERSATION_LIMIT", "AI 对话队列当前不可用，请稍后重试")
        })?,
    };

    let tools_enabled = prepared.copilot_mode.uses_tools();
    let mut tool_turns = Vec::new();
    let mut content = String::new();
    let mut assistant_messages = Vec::new();
    let mut finish_reason = None;
    let mut input_tokens = None;
    let mut output_tokens = None;
    let mut completed_without_tool_call = false;
    let mut tool_calls_allowed = tools_enabled;
    for iteration in 0..MAX_COPILOT_TOOL_ITERATIONS {
        let assistant_message_id = if iteration == 0 {
            prepared.request.assistant_message_id.clone()
        } else {
            let message_id = crate::storage::new_id("ai-message");
            emit_stream_event(
                channel,
                AiStreamEvent::AssistantMessageStarted {
                    message_id: message_id.clone(),
                },
            )?;
            message_id
        };
        // A user may copy a proposed command into the visible terminal and
        // run it there while the Copilot tool turn is waiting for a decision.
        // Re-read the approved target before every follow-up provider turn so
        // the final answer can use that new terminal evidence without ever
        // treating a changed target as the same session.
        let prompt_context = if iteration == 0 {
            prepared.prompt_context.clone()
        } else {
            refresh_copilot_prompt_context(app, prepared).await?
        };
        let stream = match prepared.provider.kind {
            AiProviderKind::OpenaiCompatibleChat => {
                stream_openai_compatible_chat_with_tools(
                    &prepared.provider,
                    prepared.api_key.as_deref(),
                    &prepared.conversation,
                    prompt_context.as_ref(),
                    prepared.response_mode,
                    &tool_turns,
                    tool_calls_allowed,
                    channel,
                    cancellation,
                )
                .await?
            }
            AiProviderKind::OpenaiResponses => {
                stream_openai_responses_with_tools(
                    &prepared.provider,
                    prepared.api_key.as_deref(),
                    &prepared.conversation,
                    prompt_context.as_ref(),
                    prepared.response_mode,
                    &tool_turns,
                    tool_calls_allowed,
                    channel,
                    cancellation,
                )
                .await?
            }
            AiProviderKind::AnthropicMessages => {
                stream_anthropic_messages_with_tools(
                    &prepared.provider,
                    prepared.api_key.as_deref(),
                    &prepared.conversation,
                    prompt_context.as_ref(),
                    prepared.response_mode,
                    &tool_turns,
                    tool_calls_allowed,
                    channel,
                    cancellation,
                )
                .await?
            }
        };
        if cancellation.is_cancelled() {
            return Err(request_cancelled_error());
        }
        if content
            .chars()
            .count()
            .saturating_add(stream.content.chars().count())
            > MAX_ASSISTANT_MESSAGE_LENGTH
        {
            return Err(ai_error(
                "AI_CONVERSATION_LIMIT",
                "Copilot 多轮回答超过本地对话长度限制",
            ));
        }
        if stream.tool_calls.len() > MAX_COPILOT_TOOL_CALLS_PER_TURN {
            return Err(ai_error(
                "AI_TOOL_LOOP_LIMIT",
                "Copilot 单轮工具调用数量超过上限",
            ));
        }
        let iteration_content = stream.content.clone();
        content.push_str(&iteration_content);
        input_tokens = match (input_tokens, stream.input_tokens) {
            (Some(total), Some(current)) => Some(total.saturating_add(current)),
            (Some(total), None) | (None, Some(total)) => Some(total),
            (None, None) => None,
        };
        output_tokens = match (output_tokens, stream.output_tokens) {
            (Some(total), Some(current)) => Some(total.saturating_add(current)),
            (Some(total), None) | (None, Some(total)) => Some(total),
            (None, None) => None,
        };
        finish_reason = stream.finish_reason.clone();
        if !tool_calls_allowed || stream.tool_calls.is_empty() {
            if !iteration_content.trim().is_empty() {
                assistant_messages.push(AssistantMessageDraft {
                    id: assistant_message_id,
                    content: iteration_content,
                    tool_activities: Vec::new(),
                });
            }
            completed_without_tool_call = true;
            break;
        }

        let mut results = Vec::with_capacity(stream.tool_calls.len());
        let mut iteration_tool_activities = Vec::new();
        for call in &stream.tool_calls {
            let (loop_result, public_result) = if tool_calls_allowed {
                execute_copilot_tool_call(app, prepared, call, channel, cancellation).await?
            } else {
                copilot_tool_blocked_after_failure(&call.id)
            };
            if cancellation.is_cancelled() {
                return Err(request_cancelled_error());
            }
            if let Some(activity) = persisted_copilot_tool_activity(prepared, call, &public_result)
            {
                iteration_tool_activities.push(activity);
            }
            emit_stream_event(
                channel,
                AiStreamEvent::ToolResult {
                    result: public_result.clone(),
                },
            )?;
            if !copilot_tool_result_allows_follow_up(&public_result) {
                tool_calls_allowed = false;
            }
            results.push(loop_result);
        }
        if !iteration_content.trim().is_empty() || !iteration_tool_activities.is_empty() {
            assistant_messages.push(AssistantMessageDraft {
                id: assistant_message_id,
                content: iteration_content.clone(),
                tool_activities: iteration_tool_activities,
            });
        }
        tool_turns.push(ToolLoopTurn {
            assistant_text: iteration_content,
            calls: stream
                .tool_calls
                .iter()
                .map(provider_safe_tool_call)
                .collect(),
            results,
        });
        if iteration + 1 == MAX_COPILOT_TOOL_ITERATIONS {
            return Err(ai_error(
                "AI_TOOL_LOOP_LIMIT",
                "Copilot 工具调用已达到单次回答的循环上限",
            ));
        }
    }
    if !completed_without_tool_call {
        return Err(ai_error(
            "AI_TOOL_LOOP_LIMIT",
            "Copilot 工具调用未能在限制内完成",
        ));
    }
    let conversation = append_assistant_messages(app, &prepared.request, assistant_messages)?;
    if input_tokens.is_some() || output_tokens.is_some() {
        emit_stream_event(
            channel,
            AiStreamEvent::Usage {
                input_tokens,
                output_tokens,
            },
        )?;
    }
    emit_stream_event(
        channel,
        AiStreamEvent::Completed {
            conversation,
            finish_reason,
        },
    )
}
