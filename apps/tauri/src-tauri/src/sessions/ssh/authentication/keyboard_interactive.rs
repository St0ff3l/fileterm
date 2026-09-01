#[derive(Clone, Debug)]
struct KeyboardInteractivePrompt {
    prompt: String,
    echo: bool,
}

#[derive(Clone, Debug)]
struct KeyboardInteractiveRequest {
    round: usize,
    name: String,
    instructions: String,
    prompts: Vec<KeyboardInteractivePrompt>,
}

// Keyboard-interactive and multi-factor continuation on the same Handle.

async fn try_keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    password: Option<&str>,
    app: &AppHandle,
    interaction: &SshInteractionContext,
    mode: KeyboardInteractiveMode,
    interaction_timeout: Duration,
) -> Result<bool, String> {
    let app = app.clone();
    let interaction = interaction.clone();
    try_keyboard_interactive_with_responder(handle, username, password, mode, move |request| {
        let app = app.clone();
        let interaction = interaction.clone();
        let interaction_timeout = interaction_timeout;
        async move {
            let request_id = uuid::Uuid::new_v4().to_string();
            let sequence = interaction.next_sequence();
            let expires_at = ssh_interaction_expires_at(interaction_timeout);
            let prompt_count = request.prompts.len();
            let echo_prompt_count = request.prompts.iter().filter(|prompt| prompt.echo).count();
            let (tx, rx) = oneshot::channel::<Value>();
            let pending_count = {
                let state = app.state::<crate::services::workspace::WorkspaceState>();
                let mut pending = state.pending_interactions.write().await;
                pending.insert(request_id.clone(), tx);
                pending.len()
            };
            interaction.log_interaction(
                &app,
                "DEBUG",
                &request_id,
                "keyboard-interactive",
                "keyboard-interactive",
                sequence,
                format!(
                    "queued round={} prompt_count={} echo_prompts={} pending={} timeout_secs={}",
                    request.round,
                    prompt_count,
                    echo_prompt_count,
                    pending_count,
                    interaction_timeout.as_secs(),
                ),
            );
            let payload = serde_json::json!({
                "requestId": request_id.clone(),
                "kind": "keyboard-interactive",
                "flowId": interaction.flow.flow_id,
                "tabId": interaction.tab_id,
                "profileId": interaction.profile_id,
                "connectionName": interaction.connection_name,
                "authenticationTarget": interaction.authentication_target.as_str(),
                "hopIndex": interaction.hop_index,
                "stage": "keyboard-interactive",
                "sequence": sequence,
                "expiresAt": expires_at,
                "host": interaction.host,
                "port": interaction.port,
                "round": request.round,
                "name": request.name,
                "instructions": request.instructions,
                "prompts": request.prompts.into_iter().map(|prompt| serde_json::json!({
                    "prompt": prompt.prompt,
                    "echo": prompt.echo,
                })).collect::<Vec<_>>(),
            });
            if emit_ssh_interaction(
                &app,
                interaction.interaction_window_label.as_deref(),
                &payload,
            )
            .is_err()
            {
                let (_, pending_after) =
                    remove_pending_ssh_interaction(&app, &request_id).await;
                interaction.log_interaction(
                    &app,
                    "WARN",
                    &request_id,
                    "keyboard-interactive",
                    "keyboard-interactive",
                    sequence,
                    format!("event emission failed pending={pending_after}"),
                );
                return None;
            }
            match timeout(interaction_timeout, rx).await {
                Ok(Ok(response)) => {
                    let canceled = response
                        .get("canceled")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    let answers = response.get("answers").and_then(|value| value.as_array());
                    let answers_count = answers.map_or(0, Vec::len);
                    let (_, pending_after) =
                        remove_pending_ssh_interaction(&app, &request_id).await;
                    if canceled {
                        interaction.log_interaction(
                            &app,
                            "INFO",
                            &request_id,
                            "keyboard-interactive",
                            "keyboard-interactive",
                            sequence,
                            format!("resolved canceled=true answers={} pending={pending_after}", answers_count),
                        );
                        return None;
                    }
                    let Some(answers) = answers else {
                        interaction.log_interaction(
                            &app,
                            "WARN",
                            &request_id,
                            "keyboard-interactive",
                            "keyboard-interactive",
                            sequence,
                            format!("resolved without answers pending={pending_after}"),
                        );
                        return None;
                    };
                    interaction.log_interaction(
                        &app,
                        "INFO",
                        &request_id,
                        "keyboard-interactive",
                        "keyboard-interactive",
                        sequence,
                        format!("resolved canceled=false answers={} pending={pending_after}", answers.len()),
                    );
                    Some(
                        answers
                            .iter()
                            .map(|answer| answer.as_str().unwrap_or("").to_string())
                            .collect(),
                    )
                }
                Ok(Err(_)) => {
                    let (_, pending_after) =
                        remove_pending_ssh_interaction(&app, &request_id).await;
                    interaction.log_interaction(
                        &app,
                        "WARN",
                        &request_id,
                        "keyboard-interactive",
                        "keyboard-interactive",
                        sequence,
                        format!("renderer receiver closed pending={pending_after}"),
                    );
                    None
                }
                Err(_) => {
                    let (_, pending_after) =
                        remove_pending_ssh_interaction(&app, &request_id).await;
                    interaction.log_interaction(
                        &app,
                        "WARN",
                        &request_id,
                        "keyboard-interactive",
                        "keyboard-interactive",
                        sequence,
                        format!(
                            "expired reason=interaction-timeout timeout_secs={} pending={pending_after}",
                            interaction_timeout.as_secs()
                        ),
                    );
                    None
                }
            }
        }
    })
    .await
}

/// Run SSH keyboard-interactive authentication and ask a caller to supply
/// only prompts that cannot safely use the profile password. Keeping this
/// protocol loop separate from Tauri events makes its MFA behaviour directly
/// testable against a real SSH server implementation.
async fn try_keyboard_interactive_with_responder<H, F, Fut>(
    handle: &mut Handle<H>,
    username: &str,
    password: Option<&str>,
    mode: KeyboardInteractiveMode,
    mut request_answers: F,
) -> Result<bool, String>
where
    H: Handler,
    F: FnMut(KeyboardInteractiveRequest) -> Fut,
    Fut: Future<Output = Option<Vec<String>>>,
{
    // SSH 协议层交互加 timeout：authenticate_keyboard_interactive_start /
    // respond 在服务器不响应时可能永久 await，而本函数在 open_session
    // 阶段调用，卡住会让 worker 永远起不来。request_answers 等待用户
    // 输入 MFA，不加 timeout。
    let mut password_used = false;
    let mut allow_password_autofill = mode == KeyboardInteractiveMode::PasswordFallback;
    let mut challenge_round = 0;
    let mut restart_count = 0;
    let mut response = wait_for_ssh_stage(
        "SSH keyboard-interactive start",
        SSH_PASSWORD_AUTH_TIMEOUT,
        async {
            handle
                .authenticate_keyboard_interactive_start(username, None)
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await?;

    loop {
        response = match response {
            russh::client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            russh::client::KeyboardInteractiveAuthResponse::Failure {
                remaining_methods,
                partial_success,
            } => {
                if !should_restart_keyboard_interactive(
                    partial_success,
                    &remaining_methods,
                    restart_count,
                ) {
                    return Ok(false);
                }
                // Once KBI itself has completed a factor, a repeated KBI
                // challenge is another factor. A prompt named "Password" is
                // not enough evidence that it accepts the original login
                // password (PAM/EDR integrations commonly reuse that label).
                allow_password_autofill = false;
                restart_count += 1;
                wait_for_ssh_stage(
                    "SSH keyboard-interactive next factor",
                    SSH_PASSWORD_AUTH_TIMEOUT,
                    async {
                        handle
                            .authenticate_keyboard_interactive_start(username, None)
                            .await
                            .map_err(|e| e.to_string())
                    },
                )
                .await?
            }
            russh::client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                if challenge_round >= MAX_KEYBOARD_INTERACTIVE_ROUNDS {
                    return Ok(false);
                }
                challenge_round += 1;
                let current = KeyboardInteractiveRequest {
                    round: challenge_round,
                    name,
                    instructions,
                    prompts: prompts
                        .into_iter()
                        .map(|prompt| KeyboardInteractivePrompt {
                            prompt: prompt.prompt,
                            echo: prompt.echo,
                        })
                        .collect(),
                };
                let mut answers = vec![String::new(); current.prompts.len()];
                let mut pending_indexes = Vec::new();
                let mut pending_prompts = Vec::new();
                for (index, prompt) in current.prompts.iter().enumerate() {
                    if allow_password_autofill
                        && !password_used
                        && password.is_some()
                        && is_password_prompt(&prompt.prompt)
                    {
                        answers[index] = password.unwrap_or_default().to_string();
                        password_used = true;
                    } else {
                        pending_indexes.push(index);
                        pending_prompts.push(prompt.clone());
                    }
                }

                if !pending_prompts.is_empty() {
                    let Some(supplied_answers) = request_answers(KeyboardInteractiveRequest {
                        round: current.round,
                        name: current.name.clone(),
                        instructions: current.instructions.clone(),
                        prompts: pending_prompts,
                    })
                    .await
                    else {
                        return Ok(false);
                    };
                    if supplied_answers.len() != pending_indexes.len() {
                        return Ok(false);
                    }
                    for (index, answer) in pending_indexes.into_iter().zip(supplied_answers) {
                        answers[index] = answer;
                    }
                }

                wait_for_ssh_stage(
                    "SSH keyboard-interactive respond",
                    SSH_PASSWORD_AUTH_TIMEOUT,
                    async {
                        handle
                            .authenticate_keyboard_interactive_respond(answers)
                            .await
                            .map_err(|e| e.to_string())
                    },
                )
                .await?
            }
        };
    }
}

fn is_password_prompt(prompt: &str) -> bool {
    let normalized = prompt.to_lowercase();
    !looks_like_mfa_prompt(&normalized)
        && (normalized.contains("password") || normalized.contains("密码"))
}

fn looks_like_mfa_prompt(prompt: &str) -> bool {
    [
        "code",
        "otp",
        "mfa",
        "2fa",
        "factor",
        "duo",
        "verification",
        "verify",
        "token",
        "authenticator",
        "passcode",
        "one-time",
        "one time",
        "验证码",
        "动态",
        "令牌",
    ]
    .iter()
    .any(|needle| prompt.contains(needle))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=haystack.len() - needle.len() {
        if haystack[i..i + needle.len()] == *needle {
            return Some(i);
        }
    }
    None
}
