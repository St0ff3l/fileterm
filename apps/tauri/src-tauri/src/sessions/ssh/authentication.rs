#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyboardInteractiveMode {
    /// A previous password/key/agent factor succeeded. Do not reuse its
    /// secret for any later KBI challenge, even if the prompt says Password.
    AdditionalFactor,
    /// KBI is the normal password-like fallback (for example, a server that
    /// does not advertise the SSH `password` method).
    PasswordFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthenticationResult {
    Authenticated,
    /// The server kept this SSH transport open and advertised
    /// keyboard-interactive as the next authentication method.
    KeyboardInteractiveAvailable { mode: KeyboardInteractiveMode },
    Rejected,
}

fn authentication_result_from_auth_result(result: &AuthResult) -> AuthenticationResult {
    match result {
        AuthResult::Success => AuthenticationResult::Authenticated,
        AuthResult::Failure {
            remaining_methods,
            partial_success,
        } if remaining_methods.contains(&MethodKind::KeyboardInteractive) => {
            // `partial_success` distinguishes an additional MFA factor from
            // a normal method fallback, but both cases keep the SSH handle
            // alive and advertise keyboard-interactive. The latter is common
            // on devices that implement password login only as KBI (there is
            // no `password` method and partial_success is false).
            AuthenticationResult::KeyboardInteractiveAvailable {
                mode: if *partial_success {
                    KeyboardInteractiveMode::AdditionalFactor
                } else {
                    KeyboardInteractiveMode::PasswordFallback
                },
            }
        }
        AuthResult::Failure { .. } => AuthenticationResult::Rejected,
    }
}

fn should_restart_keyboard_interactive(
    partial_success: bool,
    remaining_methods: &MethodSet,
    restart_count: usize,
) -> bool {
    partial_success
        && remaining_methods.contains(&MethodKind::KeyboardInteractive)
        && restart_count < MAX_KEYBOARD_INTERACTIVE_RESTARTS
}

/// Complete the configured primary authentication and, when the server
/// advertises it, continue keyboard-interactive on the same authenticated
/// transport. Reconnecting here loses partial-success state and can make a
/// jump-host target ask for its host key a second time.
async fn authenticate_session(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    auth_type: &str,
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
    authentication_target: SshAuthenticationTarget,
) -> Result<bool, String> {
    match try_authenticate(handle, username, auth_type, profile, app, tab_id).await? {
        AuthenticationResult::Authenticated => Ok(true),
        AuthenticationResult::KeyboardInteractiveAvailable { mode } => {
            let profile_id = profile.get("id").and_then(Value::as_str).unwrap_or("");
            let host = profile.get("host").and_then(Value::as_str).unwrap_or("");
            let port = port_from_profile(profile, 22, "SSH")?;
            let connection_name = profile
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(host);
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                format!(
                    "continuing authentication with keyboard-interactive target={} host={host}:{port}",
                    authentication_target.as_str()
                ),
            );
            try_keyboard_interactive(
                handle,
                username,
                password_for_authentication(profile),
                app,
                tab_id,
                profile_id,
                host,
                port,
                connection_name,
                authentication_target,
                mode,
            )
            .await
        }
        AuthenticationResult::Rejected => Ok(false),
    }
}

fn default_ssh_key_paths(home_directory: &Path) -> Vec<PathBuf> {
    DEFAULT_SSH_KEY_FILES
        .iter()
        .map(|file_name| home_directory.join(".ssh").join(file_name))
        .collect()
}

async fn authenticate_private_key_content(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    key_content: &str,
    passphrase: Option<&str>,
    app: &AppHandle,
    tab_id: &str,
) -> Result<AuthResult, String> {
    let key_pair = russh::keys::decode_secret_key(key_content, passphrase)
        .map_err(|error| error.to_string())?;
    // Best-effort: pick the strongest RSA hash the server advertises. For
    // non-RSA keys, hash_alg is ignored by PrivateKeyWithHashAlg::new.
    // 加 timeout：best_supported_rsa_hash 在服务器不响应时可能永久 await，
    // 而 authenticate_private_key_content 在 open_session 阶段调用，卡住
    // 会让 worker 永远起不来。使用 SSH_PASSWORD_AUTH_TIMEOUT 与密码认证
    // 对齐，保持一致的认证阶段超时语义。
    let hash_alg: Option<russh::keys::HashAlg> = if key_pair.algorithm().is_rsa() {
        match wait_for_ssh_stage(
            "SSH RSA hash negotiation",
            SSH_PASSWORD_AUTH_TIMEOUT,
            async {
                handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|e| e.to_string())
            },
        )
        .await
        {
            Ok(Some(Some(hash))) => Some(hash),
            Ok(_) => Some(russh::keys::HashAlg::Sha512),
            Err(error) => {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "ssh",
                    tab_id,
                    format!("RSA hash negotiation failed, falling back to Sha512: {error}"),
                );
                Some(russh::keys::HashAlg::Sha512)
            }
        }
    } else {
        None
    };
    let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash_alg);
    let result = wait_for_ssh_stage(
        "SSH public key authentication",
        SSH_PASSWORD_AUTH_TIMEOUT,
        async {
            handle
                .authenticate_publickey(username, key_with_hash)
                .await
                .map_err(|error| error.to_string())
        },
    )
    .await?;
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "public key authentication completed success={}",
            result.success()
        ),
    );
    Ok(result)
}

async fn try_system_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
) -> Result<AuthenticationResult, String> {
    let mut candidate_found = false;
    let mut authentication_attempted = false;
    let mut candidate_errors = Vec::new();

    // Agent support is Unix-only in russh, but a missing/broken agent must not
    // prevent the default-key fallback (including on Windows).
    // 加 timeout：AgentClient::connect_env / request_identities /
    // authenticate_publickey_with 在 SSH agent 卡住（unix socket 阻塞、
    // agent 进程 hang）时会永久 await，而本函数在 open_session 阶段
    // 调用，卡住会让 worker 永远起不来。
    #[cfg(unix)]
    match wait_for_ssh_stage("SSH agent connect", SSH_PASSWORD_AUTH_TIMEOUT, async {
        russh::keys::agent::client::AgentClient::connect_env()
            .await
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(mut agent) => {
            candidate_found = true;
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                "SSH agent connected, listing identities",
            );
            match wait_for_ssh_stage(
                "SSH agent list identities",
                SSH_PASSWORD_AUTH_TIMEOUT,
                async { agent.request_identities().await.map_err(|e| e.to_string()) },
            )
            .await
            {
                Ok(identities) => {
                    crate::services::logging::session(
                        app,
                        "INFO",
                        "ssh",
                        tab_id,
                        format!("SSH agent returned {} identities", identities.len()),
                    );
                    for identity in identities {
                        authentication_attempted = true;
                        let public_key = identity.public_key().into_owned();
                        match wait_for_ssh_stage(
                            "SSH agent public key authentication",
                            SSH_PASSWORD_AUTH_TIMEOUT,
                            async {
                                handle
                                    .authenticate_publickey_with(
                                        username, public_key, None, &mut agent,
                                    )
                                    .await
                                    .map_err(|error| error.to_string())
                            },
                        )
                        .await
                        {
                            Ok(result) => match authentication_result_from_auth_result(&result) {
                                AuthenticationResult::Authenticated => {
                                    return Ok(AuthenticationResult::Authenticated)
                                }
                                AuthenticationResult::KeyboardInteractiveAvailable { mode } => {
                                    return Ok(AuthenticationResult::KeyboardInteractiveAvailable { mode })
                                }
                                AuthenticationResult::Rejected => authentication_attempted = true,
                            },
                            Err(error) => candidate_errors.push(error),
                        }
                    }
                }
                Err(error) => {
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        format!("SSH agent list identities failed: {error}"),
                    );
                    candidate_errors.push(error);
                }
            }
        }
        Err(error) => {
            // agent 不可用很常见（Windows、无 agent 的 Linux），只在 DEBUG
            // 级别记录，避免日志噪音。但超时（30s）需要 WARN 提醒用户。
            if error.contains("timed out") {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "ssh",
                    tab_id,
                    format!("SSH agent connect timed out: {error}"),
                );
            }
            candidate_errors.push(error);
        }
    }

    let home_directory = app.path().home_dir().map_err(|error| error.to_string())?;
    let passphrase = profile.get("passphrase").and_then(Value::as_str);
    for path in default_ssh_key_paths(&home_directory) {
        let key_content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                candidate_found = true;
                candidate_errors.push(error.to_string());
                continue;
            }
        };
        candidate_found = true;
        match authenticate_private_key_content(
            handle,
            username,
            &key_content,
            passphrase,
            app,
            tab_id,
        )
        .await
        {
            Ok(result) => match authentication_result_from_auth_result(&result) {
                AuthenticationResult::Authenticated => {
                    return Ok(AuthenticationResult::Authenticated)
                }
                AuthenticationResult::KeyboardInteractiveAvailable { mode } => {
                    return Ok(AuthenticationResult::KeyboardInteractiveAvailable { mode })
                }
                AuthenticationResult::Rejected => authentication_attempted = true,
            },
            Err(error) => candidate_errors.push(error),
        }
    }

    if !candidate_found {
        return Err("No SSH agent or default private key found on this computer".to_string());
    }
    if !authentication_attempted && !candidate_errors.is_empty() {
        return Err(format!(
            "Unable to load SSH agent/default private key: {}",
            candidate_errors.remove(0)
        ));
    }
    Ok(AuthenticationResult::Rejected)
}

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

async fn try_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    auth_type: &str,
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
) -> Result<AuthenticationResult, String> {
    let profile_id = profile
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_string();
    match auth_type {
        "password" => {
            let Some(password) = password_for_authentication(profile) else {
                return Err("SSH password is missing".to_string());
            };
            // Some embedded SSH servers do not reply to a direct password
            // request until the client has first sent the RFC-standard
            // `none` probe. Electron's ssh2 client always performs this
            // negotiation before trying the saved password. Mirror that
            // sequence here for compatibility with those servers.
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                "password authentication method negotiation started",
            );
            let negotiation = wait_for_ssh_stage(
                "SSH authentication method negotiation",
                SSH_PASSWORD_AUTH_TIMEOUT,
                async {
                    handle
                        .authenticate_none(username)
                        .await
                        .map_err(|error| error.to_string())
                },
            )
            .await?;
            if negotiation.success() {
                crate::services::logging::session(
                    app,
                    "INFO",
                    "ssh",
                    tab_id,
                    "SSH server accepted none authentication",
                );
                return Ok(AuthenticationResult::Authenticated);
            }
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                "password authentication started",
            );
            let res = wait_for_ssh_stage(
                "SSH password authentication",
                SSH_PASSWORD_AUTH_TIMEOUT,
                async {
                    handle
                        .authenticate_password(username, password)
                        .await
                        .map_err(|error| error.to_string())
                },
            )
            .await?;
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                format!(
                    "password authentication response received success={}",
                    res.success()
                ),
            );
            Ok(authentication_result_from_auth_result(&res))
        }
        "privateKey" => {
            let (key_content, passphrase) = if let Some(key_id) =
                profile.get("privateKeyId").and_then(|value| value.as_str())
            {
                resolve_managed_private_key(app, tab_id, &profile_id, key_id).await?
            } else {
                let private_key_path = profile
                    .get("privateKeyPath")
                    .and_then(|p| p.as_str())
                    .unwrap_or("");
                let mut resolved = private_key_path.to_string();
                if resolved.starts_with("~/") || resolved == "~" {
                    if let Ok(home) = app.path().home_dir() {
                        let rest = if resolved == "~" { "" } else { &resolved[2..] };
                        resolved = home.join(rest).to_string_lossy().into_owned();
                    }
                }
                (
                    std::fs::read_to_string(&resolved).map_err(|error| error.to_string())?,
                    profile
                        .get("passphrase")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                )
            };

            let result = authenticate_private_key_content(
                handle,
                username,
                &key_content,
                passphrase.as_deref(),
                app,
                tab_id,
            )
            .await?;
            Ok(authentication_result_from_auth_result(&result))
        }
        "keyboard-interactive" => Ok(AuthenticationResult::KeyboardInteractiveAvailable {
            mode: KeyboardInteractiveMode::PasswordFallback,
        }),
        _ => try_system_authenticate(handle, username, profile, app, tab_id).await,
    }
}

async fn resolve_managed_private_key(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    key_id: &str,
) -> Result<(String, Option<String>), String> {
    let managed =
        crate::services::ssh_keys::resolve(app, key_id).map_err(|error| error.to_string())?;
    if !managed.key.encrypted {
        return Ok((managed.private_key, None));
    }

    let mut reason = "required";
    if let Some(saved) = managed.saved_passphrase {
        if russh::keys::decode_secret_key(&managed.private_key, Some(&saved)).is_ok() {
            return Ok((managed.private_key, Some(saved)));
        }
        crate::services::ssh_keys::set_passphrase(app, &managed.key.id, None)
            .map_err(|error| error.to_string())?;
        reason = "invalid-saved";
    }

    let response = request_key_passphrase(
        app,
        tab_id,
        profile_id,
        &managed.key.id,
        &managed.key.name,
        reason,
    )
    .await?
    .ok_or_else(|| "SSH key passphrase request canceled".to_string())?;
    if russh::keys::decode_secret_key(&managed.private_key, Some(&response.0)).is_err() {
        return Err("私钥口令不正确。".to_string());
    }
    if response.1 {
        crate::services::ssh_keys::set_passphrase(app, &managed.key.id, Some(response.0.clone()))
            .map_err(|error| error.to_string())?;
    }
    Ok((managed.private_key, Some(response.0)))
}

async fn request_key_passphrase(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    key_id: &str,
    key_name: &str,
    reason: &str,
) -> Result<Option<(String, bool)>, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<Value>();
    {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        state
            .pending_interactions
            .write()
            .await
            .insert(request_id.clone(), tx);
    }
    app.emit(
        "ssh:interaction",
        serde_json::json!({
            "requestId": request_id,
            "kind": "key-passphrase",
            "tabId": tab_id,
            "profileId": profile_id,
            "keyId": key_id,
            "keyName": key_name,
            "reason": reason,
        }),
    )
    .map_err(|error| error.to_string())?;
    match rx.await {
        Ok(response)
            if !response
                .get("canceled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false) =>
        {
            let passphrase = response
                .get("passphrase")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(passphrase.map(|value| {
                (
                    value,
                    response
                        .get("savePassphrase")
                        .and_then(|item| item.as_bool())
                        .unwrap_or(false),
                )
            }))
        }
        _ => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)] // Authentication prompts need the full connection identity for safe UI routing.
async fn try_keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    password: Option<&str>,
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    host: &str,
    port: u16,
    connection_name: &str,
    authentication_target: SshAuthenticationTarget,
    mode: KeyboardInteractiveMode,
) -> Result<bool, String> {
    let app = app.clone();
    let tab_id = tab_id.to_string();
    let profile_id = profile_id.to_string();
    let host = host.to_string();
    let connection_name = connection_name.to_string();
    let authentication_target = authentication_target.as_str().to_string();
    try_keyboard_interactive_with_responder(handle, username, password, mode, move |request| {
        let app = app.clone();
        let tab_id = tab_id.clone();
        let profile_id = profile_id.clone();
        let host = host.clone();
        let connection_name = connection_name.clone();
        let authentication_target = authentication_target.clone();
        async move {
            let request_id = uuid::Uuid::new_v4().to_string();
            let (tx, rx) = oneshot::channel::<Value>();
            {
                let state = app.state::<crate::services::workspace::WorkspaceState>();
                let mut pending = state.pending_interactions.write().await;
                pending.insert(request_id.clone(), tx);
            }
            let payload = serde_json::json!({
                "requestId": request_id.clone(),
                "kind": "keyboard-interactive",
                "flowId": tab_id.clone(),
                "tabId": tab_id.clone(),
                "profileId": profile_id.clone(),
                "connectionName": connection_name,
                "authenticationTarget": authentication_target,
                "host": host,
                "port": port,
                "round": request.round,
                "name": request.name,
                "instructions": request.instructions,
                "prompts": request.prompts.into_iter().map(|prompt| serde_json::json!({
                    "prompt": prompt.prompt,
                    "echo": prompt.echo,
                })).collect::<Vec<_>>(),
            });
            if app.emit("ssh:interaction", payload).is_err() {
                app.state::<crate::services::workspace::WorkspaceState>()
                    .pending_interactions
                    .write()
                    .await
                    .remove(&request_id);
                return None;
            }
            match rx.await {
                Ok(response)
                    if !response
                        .get("canceled")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false) =>
                {
                    response.get("answers").and_then(|answers| {
                        answers.as_array().map(|answers| {
                            answers
                                .iter()
                                .map(|answer| answer.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                    })
                }
                _ => None,
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

const SFTP_UNAVAILABLE_FALLBACK: &str =
    "SFTP 文件通道不可用；终端和 SSH 隧道仍可继续使用。请在服务器启用或修复 sftp subsystem 后重新连接。";

/// Open the SFTP subsystem on an already authenticated SSH handle.
///
/// `russh-sftp` deliberately does not send the subsystem request itself, so
/// this boundary is also where we can distinguish a file-channel failure from
/// a terminal-session failure.
/// SFTP 初始化每一步的最大等待时间。
///
/// 这非常关键：`open_sftp_session` 在 worker 主 select! 循环之前调用，
/// 任何一步阻塞都会让整个 worker 启动不了——cmd_rx 队列堆满后所有
/// `app_write_terminal` 调用全部永久阻塞，表现为终端无法输入、多窗口
/// 发送整体卡死、Cmd+Q 退出也退不掉。服务器拒绝 sftp subsystem 时
/// russh-sftp 内部超时往往很长（30s+），这里强制收口到 8 秒。
const SFTP_INIT_STEP_TIMEOUT: Duration = Duration::from_secs(8);

/// Shell channel 建立阶段的单步超时。`channel_open_session` /
/// `request_pty` / `request_shell` 任一卡住都会让 worker 永远起不来——
/// 表现为"连接主机"loading 永不结束，所有后续命令（包括 Ctrl+C）都
/// 进不了 cmd_rx。服务器在 PTY 协商阶段卡住（罕见但确实发生过，尤其
/// 是某些嵌入式 dropbear / 网络设备）时，russh 默认无超时，会一直
/// await。8 秒与 SFTP_INIT_STEP_TIMEOUT 对齐，足够覆盖正常 RTT 与
/// 一次重试，同时不让用户对着 loading 望穿秋水。
const SHELL_INIT_STEP_TIMEOUT: Duration = Duration::from_secs(8);

/// `probe_remote_platform` 总超时。该函数在 worker 主循环之前调用，
/// 内部最多尝试 4 次 exec_command（POSIX + 3 个 Windows probe），每次
/// 都用 `channel.wait()` 循环读取，没有内层 timeout。如果服务器在 exec
/// 模式下卡住（不返回 EOF/Close），整个 probe 会永久 await，worker
/// 永远起不来，所有后续命令（含 Ctrl+C）都进不了 cmd_rx。20 秒覆盖
/// 最坏情况下的 4 次串行尝试 + RTT，超时后回落到 "unknown" 平台，
/// shell CWD 注入会被 fail-closed 门控跳过，不影响终端基本可用性。
const PLATFORM_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// SSH 隧道控制操作（tcpip_forward / cancel_tcpip_forward）的单步超时。
/// 这两个调用在 `handle_worker_cmd` 的 inline await 路径上，服务器卡住
/// 时会直接阻塞 worker 主循环，导致终端 select! 无法响应 Ctrl+C。
/// 5 秒覆盖正常 RTT 与一次重试，超时后让用户拿到明确错误而不是沉默
/// 地 hang 住整个会话。
const SSH_TUNNEL_OP_TIMEOUT: Duration = Duration::from_secs(5);

/// sudo 凭据验证超时。`exec_shell_file_command` 用 PTY 模式 exec，sudo
/// 密码错误时会重新 prompt 等待输入且不会自然退出，channel.wait() 永久
/// 阻塞。这里强制 10 秒收口，让前端 RootAccessModal 的 loading 状态能
/// 在合理时间内解除。
const SUDO_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
/// `su` 在独立 exec 通道里需要一个可用的控制终端来完成 PAM 密码交互。
/// 这个标记由提权后的 shell 打印，位于密码提示之后，用来从 PTY 合并
/// 输出中剥离 `Password:` / `密码:` 等前缀，避免污染 stat/base64 结果。
const SU_EXEC_OUTPUT_MARKER: &str = "__FILETERM_SU_EXEC_OUTPUT__";
/// Inline `SetRemoteFileAccessMode` verification budget. The full
/// `SUDO_VERIFY_TIMEOUT` (10s) is appropriate for spawned file operations,
/// but `SetRemoteFileAccessMode` runs inline on the worker loop — waiting
/// the full 10 seconds would freeze `terminal_input_rx` polling and make
/// Ctrl+C unresponsive while the user waits for the root-mode toggle to
/// finish. 1.5s is enough for a healthy sudo round-trip; slower responses
/// surface as a user-visible error instead of a frozen terminal.
const ROOT_ACCESS_VERIFY_TIMEOUT: Duration = Duration::from_millis(1500);

/// SFTP / exec 文件操作超时。
///
/// 这非常关键：worker 主循环是单 task 顺序处理 cmd 的，一个 ListRemoteFiles
/// / ReadRemoteFile 卡住会阻塞整个 select! 循环，cmd_rx.recv() 不被 poll，
/// 新来的 WriteTerminal 命令堆积直到 channel 满（100），之后所有
/// app_write_terminal 超时丢弃——终端和悬浮窗都无法输入。
///
/// SFTP read_dir / open 在网络抖动或服务器 SFTP subsystem 失效时可能
/// 长时间不返回，必须强制收口。
const FILE_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);

/// Keep the detached first browse request responsive even when a server's
/// SFTP subsystem accepts a request but never completes `READDIR`. User
/// initiated operations retain the profile-configured timeout below.
const INITIAL_SFTP_LISTING_TIMEOUT: Duration = Duration::from_secs(15);

/// Resolving the SFTP current directory is a small compatibility probe. It
/// must not inherit a one-hour operation timeout from a profile.
const INITIAL_SFTP_HOME_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(8);

/// A directory listing already carries metadata for the link itself. Following
/// a symlink is optional UI enrichment, so one inaccessible or slow target
/// must not hold the whole file pane behind the SFTP request timeout.
const SFTP_SYMLINK_TARGET_TIMEOUT: Duration = Duration::from_secs(2);
