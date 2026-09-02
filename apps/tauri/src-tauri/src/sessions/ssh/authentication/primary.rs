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
    interaction: &SshInteractionContext,
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
                    &interaction.tab_id,
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
        &interaction.tab_id,
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
    interaction: &SshInteractionContext,
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
                &interaction.tab_id,
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
                        &interaction.tab_id,
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
                                    return Ok(AuthenticationResult::KeyboardInteractiveAvailable {
                                        mode,
                                    })
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
                        &interaction.tab_id,
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
                    &interaction.tab_id,
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
            interaction,
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

// Configured password, private-key, and agent authentication.

async fn try_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    auth_type: &str,
    profile: &Value,
    app: &AppHandle,
    interaction: &SshInteractionContext,
    interaction_timeout: Duration,
) -> Result<AuthenticationResult, String> {
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
                &interaction.tab_id,
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
                    &interaction.tab_id,
                    "SSH server accepted none authentication",
                );
                return Ok(AuthenticationResult::Authenticated);
            }
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                &interaction.tab_id,
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
                &interaction.tab_id,
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
                resolve_managed_private_key(app, interaction, interaction_timeout, key_id).await?
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
                interaction,
            )
            .await?;
            Ok(authentication_result_from_auth_result(&result))
        }
        "keyboard-interactive" => Ok(AuthenticationResult::KeyboardInteractiveAvailable {
            mode: KeyboardInteractiveMode::PasswordFallback,
        }),
        _ => {
            try_system_authenticate(handle, username, profile, app, interaction).await
        }
    }
}

async fn resolve_managed_private_key(
    app: &AppHandle,
    interaction: &SshInteractionContext,
    interaction_timeout: Duration,
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
        interaction,
        interaction_timeout,
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
    interaction: &SshInteractionContext,
    interaction_timeout: Duration,
    key_id: &str,
    key_name: &str,
    reason: &str,
) -> Result<Option<(String, bool)>, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let sequence = interaction.next_sequence();
    let expires_at = ssh_interaction_expires_at(interaction_timeout);
    let (tx, rx) = oneshot::channel::<Value>();
    let pending_count = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let mut pending = state.pending_interactions.write().await;
        pending.insert(request_id.clone(), tx);
        pending.len()
    };
    let _pending_cleanup = PendingSshInteractionGuard::new(
        app,
        &interaction.tab_id,
        &request_id,
    );
    interaction.log_interaction(
        app,
        "DEBUG",
        &request_id,
        "key-passphrase",
        "key-passphrase",
        sequence,
        format!(
            "queued key_id_present={} key_name_present={} reason={} pending={} timeout_secs={}",
            !key_id.trim().is_empty(),
            !key_name.trim().is_empty(),
            reason,
            pending_count,
            interaction_timeout.as_secs(),
        ),
    );
    let payload = serde_json::json!({
        "requestId": request_id.clone(),
        "kind": "key-passphrase",
        "flowId": interaction.flow.flow_id,
        "tabId": interaction.tab_id,
        "profileId": interaction.profile_id,
        "connectionName": interaction.connection_name,
        "authenticationTarget": interaction.authentication_target.as_str(),
        "hopIndex": interaction.hop_index,
        "stage": "key-passphrase",
        "sequence": sequence,
        "expiresAt": expires_at,
        "keyId": key_id,
        "keyName": key_name,
        "reason": reason,
    });
    if let Err(error) = emit_ssh_interaction(
        app,
        interaction.interaction_window_label.as_deref(),
        &payload,
    ) {
        let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
        interaction.log_interaction(
            app,
            "WARN",
            &request_id,
            "key-passphrase",
            "key-passphrase",
            sequence,
            format!("event emission failed pending={pending_after} error={error}"),
        );
        return Err(error.to_string());
    }
    match wait_for_ssh_interaction(interaction, rx, interaction_timeout).await {
        SshInteractionWaitResult::Response(response) => {
            let canceled = response
                .get("canceled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let passphrase = response
                .get("passphrase")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let save_passphrase = response
                .get("savePassphrase")
                .and_then(|item| item.as_bool())
                .unwrap_or(false);
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            if canceled {
                interaction.log_interaction(
                    app,
                    "INFO",
                    &request_id,
                    "key-passphrase",
                    "key-passphrase",
                    sequence,
                    format!("resolved canceled=true pending={pending_after}"),
                );
                return Ok(None);
            }
            let passphrase_present = passphrase.is_some();
            interaction.log_interaction(
                app,
                if passphrase_present { "INFO" } else { "WARN" },
                &request_id,
                "key-passphrase",
                "key-passphrase",
                sequence,
                format!(
                    "resolved canceled=false passphrase_present={passphrase_present} save_passphrase={save_passphrase} pending={pending_after}"
                ),
            );
            Ok(passphrase.map(|value| (value, save_passphrase)))
        }
        SshInteractionWaitResult::ReceiverClosed => {
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            interaction.log_interaction(
                app,
                "WARN",
                &request_id,
                "key-passphrase",
                "key-passphrase",
                sequence,
                format!("renderer receiver closed pending={pending_after}"),
            );
            Ok(None)
        }
        SshInteractionWaitResult::Timeout => {
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            interaction.log_interaction(
                app,
                "WARN",
                &request_id,
                "key-passphrase",
                "key-passphrase",
                sequence,
                format!(
                    "expired reason=interaction-timeout timeout_secs={} pending={pending_after}",
                    interaction_timeout.as_secs()
                ),
            );
            Ok(None)
        }
        SshInteractionWaitResult::Cancelled => {
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            interaction.log_interaction(
                app,
                "INFO",
                &request_id,
                "key-passphrase",
                "key-passphrase",
                sequence,
                format!(
                    "canceled reason=connection-cancelled pending={pending_after}"
                ),
            );
            Ok(None)
        }
    }
}
