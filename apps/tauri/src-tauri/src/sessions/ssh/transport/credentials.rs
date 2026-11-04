fn missing_password_credential(profile: &Value) -> Option<&'static str> {
    let auth_type = profile
        .get("authType")
        .and_then(Value::as_str)
        .unwrap_or("password");
    if !matches!(auth_type, "password" | "kubernetes") {
        return None;
    }
    if profile
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Some("missing-username");
    }
    if profile
        .get("useEmptyPassword")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    if profile
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("")
        .is_empty()
    {
        return Some("missing-password");
    }
    None
}

fn password_for_authentication(profile: &Value) -> Option<&str> {
    if profile
        .get("useEmptyPassword")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Some("");
    }
    profile
        .get("password")
        .and_then(Value::as_str)
        .filter(|password| !password.is_empty())
}

/// Renderer-side connection forms keep an empty string as the stable default
/// for `trustedHostFingerprint`. Treat that exactly like an absent field: an
/// empty value is not a previously trusted key and must not be surfaced as a
/// misleading "mismatch" in the host-verification prompt.
fn trusted_host_fingerprint(profile: &Value) -> Option<String> {
    profile
        .get("trustedHostFingerprint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|fingerprint| !fingerprint.is_empty())
        .map(str::to_string)
}

async fn ensure_ssh_credentials(
    profile: &mut Value,
    app: &AppHandle,
    interaction: &SshInteractionContext,
    interaction_timeout: Duration,
) -> Result<(), String> {
    let Some(reason) = missing_password_credential(profile) else {
        return Ok(());
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let sequence = interaction.next_sequence();
    let expires_at = ssh_interaction_expires_at(interaction_timeout);
    let username_present = profile
        .get("username")
        .and_then(Value::as_str)
        .is_some_and(|username| !username.trim().is_empty());
    let (tx, rx) = oneshot::channel::<Value>();
    let pending_count = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let mut pending = state.pending_interactions.write().await;
        pending.insert(request_id.clone(), tx);
        pending.len()
    };
    let _pending_cleanup = PendingSshInteractionGuard::new(app, &interaction.tab_id, &request_id);
    interaction.log_interaction(
        app,
        "DEBUG",
        &request_id,
        "credentials",
        "credentials",
        sequence,
        format!(
            "queued reason={reason} username_present={username_present} pending={pending_count} timeout_secs={}",
            interaction_timeout.as_secs()
        ),
    );
    let payload = serde_json::json!({
        "requestId": request_id.clone(),
        "kind": "credentials",
        "flowId": interaction.flow.flow_id,
        "tabId": interaction.tab_id,
        "profileId": interaction.profile_id,
        "connectionName": interaction.connection_name,
        "authenticationTarget": interaction.authentication_target.as_str(),
        "hopIndex": interaction.hop_index,
        "stage": "credentials",
        "sequence": sequence,
        "expiresAt": expires_at,
        "host": interaction.host,
        "port": interaction.port,
        "username": profile.get("username").and_then(Value::as_str),
        "authType": profile.get("authType").and_then(Value::as_str).unwrap_or("password"),
        "passwordRequired": true,
        "reason": reason,
    });
    if let Err(error) = emit_ssh_interaction(
        app,
        interaction.interaction_window_label.as_deref(),
        &payload,
    ).await {
        let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
        interaction.log_interaction(
            app,
            "WARN",
            &request_id,
            "credentials",
            "credentials",
            sequence,
            format!("event emission failed pending={pending_after} error={error}"),
        );
        return Err(error.to_string());
    }

    let response = match wait_for_ssh_interaction(interaction, rx, interaction_timeout).await {
        SshInteractionWaitResult::Response(response) => response,
        SshInteractionWaitResult::ReceiverClosed => {
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            interaction.log_interaction(
                app,
                "WARN",
                &request_id,
                "credentials",
                "credentials",
                sequence,
                format!("renderer receiver closed pending={pending_after}"),
            );
            return Err("SSH credentials request canceled".to_string());
        }
        SshInteractionWaitResult::Timeout => {
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            interaction.log_interaction(
                app,
                "WARN",
                &request_id,
                "credentials",
                "credentials",
                sequence,
                format!(
                    "expired reason=interaction-timeout timeout_secs={} pending={pending_after}",
                    interaction_timeout.as_secs()
                ),
            );
            return Err("SSH credentials request timed out".to_string());
        }
        SshInteractionWaitResult::Cancelled => {
            let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
            interaction.log_interaction(
                app,
                "INFO",
                &request_id,
                "credentials",
                "credentials",
                sequence,
                format!("canceled reason=connection-cancelled pending={pending_after}"),
            );
            return Err("SSH credentials request canceled".to_string());
        }
    };
    let (_, pending_after) = remove_pending_ssh_interaction(app, &request_id).await;
    if response
        .get("canceled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        interaction.log_interaction(
            app,
            "INFO",
            &request_id,
            "credentials",
            "credentials",
            sequence,
            format!("resolved canceled=true pending={pending_after}"),
        );
        return Err("SSH credentials request canceled".to_string());
    }
    if let Err(error) = apply_ssh_credentials_response(profile, &response) {
        interaction.log_interaction(
            app,
            "WARN",
            &request_id,
            "credentials",
            "credentials",
            sequence,
            format!("rejected credential response pending={pending_after} error={error}"),
        );
        return Err(error);
    }
    interaction.log_interaction(
        app,
        "INFO",
        &request_id,
        "credentials",
        "credentials",
        sequence,
        format!(
            "resolved canceled=false auth_type={} pending={pending_after}",
            profile["authType"].as_str().unwrap_or("password")
        ),
    );
    Ok(())
}

fn apply_ssh_credentials_response(profile: &mut Value, response: &Value) -> Result<(), String> {
    let username = response
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if username.is_empty() {
        return Err("SSH username is required".to_string());
    }
    let auth_type = response
        .get("authType")
        .and_then(Value::as_str)
        .unwrap_or("password");
    let kubernetes = profile.get("authType").and_then(Value::as_str) == Some("kubernetes");
    let (field, value) = match auth_type {
        "password" => (
            "password",
            response
                .get("password")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty()),
        ),
        "privateKey" if !kubernetes => (
            "privateKeyId",
            response
                .get("privateKeyId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        ),
        _ => return Err("Unsupported SSH credential authentication method".to_string()),
    };
    let value = value.ok_or_else(|| format!("SSH {field} is required"))?;
    let object = profile
        .as_object_mut()
        .ok_or_else(|| "SSH profile is invalid".to_string())?;
    object.insert("username".to_string(), Value::String(username.to_string()));
    object.insert(
        "authType".to_string(),
        Value::String(if kubernetes { "kubernetes" } else { auth_type }.to_string()),
    );
    object.insert("useEmptyPassword".to_string(), Value::Bool(false));
    for key in ["password", "privateKeyId", "privateKeyPath", "passphrase"] {
        object.remove(key);
    }
    object.insert(field.to_string(), Value::String(value.to_string()));
    Ok(())
}

#[cfg(test)]
#[path = "tests/credentials_tests.rs"]
mod credentials_tests;
