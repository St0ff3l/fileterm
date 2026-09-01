fn missing_password_credential(profile: &Value) -> Option<&'static str> {
    if profile
        .get("authType")
        .and_then(Value::as_str)
        .unwrap_or("password")
        != "password"
    {
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

async fn ensure_password_credentials(
    profile: &mut Value,
    app: &AppHandle,
    interaction: &SshInteractionContext,
    interaction_timeout: Duration,
) -> Result<(), String> {
    let Some(reason) = missing_password_credential(profile) else {
        return Ok(());
    };
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
    let payload = serde_json::json!({
        "requestId": request_id,
        "kind": "credentials",
        "flowId": interaction.flow.flow_id,
        "tabId": interaction.tab_id,
        "profileId": interaction.profile_id,
        "connectionName": interaction.connection_name,
        "authenticationTarget": interaction.authentication_target.as_str(),
        "hopIndex": interaction.hop_index,
        "stage": "credentials",
        "sequence": interaction.next_sequence(),
        "host": interaction.host,
        "port": interaction.port,
        "username": profile.get("username").and_then(Value::as_str),
        "passwordRequired": true,
        "reason": reason,
    });
    if let Err(error) = emit_ssh_interaction(
        app,
        interaction.interaction_window_label.as_deref(),
        &payload,
    ) {
        app.state::<crate::services::workspace::WorkspaceState>()
            .pending_interactions
            .write()
            .await
            .remove(&request_id);
        return Err(error.to_string());
    }

    let response = match timeout(interaction_timeout, rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => return Err("SSH credentials request canceled".to_string()),
        Err(_) => {
            app.state::<crate::services::workspace::WorkspaceState>()
                .pending_interactions
                .write()
                .await
                .remove(&request_id);
            return Err("SSH credentials request timed out".to_string());
        }
    };
    if response
        .get("canceled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("SSH credentials request canceled".to_string());
    }
    let username = response
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let password = response
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("");
    if username.is_empty() || password.is_empty() {
        return Err("SSH username and password are required".to_string());
    }
    let object = profile
        .as_object_mut()
        .ok_or_else(|| "SSH profile is invalid".to_string())?;
    object.insert("username".to_string(), Value::String(username.to_string()));
    object.insert("password".to_string(), Value::String(password.to_string()));
    Ok(())
}
