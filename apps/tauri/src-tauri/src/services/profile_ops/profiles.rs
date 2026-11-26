pub fn create_folder(
    app: &AppHandle,
    name: &str,
    parent_id: Option<&str>,
) -> Result<Value, AppError> {
    let mut folders = read_and_heal_connection_folders(app)?;
    let folder = serde_json::json!({
        "id": new_id("folder"),
        "type": "folder",
        "name": name,
        "parentId": parent_id,
        "order": chrono_now_ms(),
    });
    folders.insert(0, folder.clone());
    write_json_array(app, "folders.json", &folders)?;
    Ok(folder)
}

pub fn create_command_folder(
    app: &AppHandle,
    name: &str,
    parent_id: Option<&str>,
) -> Result<Value, AppError> {
    let (mut folders, _) = read_and_heal_command_library(app)?;
    let folder = serde_json::json!({
        "id": new_id("cmd-folder"),
        "type": "command-folder",
        "name": name,
        "parentId": parent_id,
        "order": chrono_now_ms(),
    });
    folders.insert(0, folder.clone());
    write_json_array(app, "command-folders.json", &folders)?;
    Ok(folder)
}

pub fn create_command_template(app: &AppHandle, input: Value) -> Result<Value, AppError> {
    let (_, mut commands) = read_and_heal_command_library(app)?;
    let mut command = ensure_object(&input);
    command.insert("id".to_string(), Value::String(new_id("cmd")));
    command.insert(
        "type".to_string(),
        Value::String("command-template".to_string()),
    );
    if command.get("order").and_then(Value::as_f64).is_none() {
        command.insert("order".to_string(), Value::Number(chrono_now_ms().into()));
    }
    if command.get("command").and_then(Value::as_str).is_none() {
        command.insert("command".to_string(), Value::String(String::new()));
    }
    if command
        .get("appendCarriageReturn")
        .and_then(Value::as_bool)
        .is_none()
    {
        command.insert("appendCarriageReturn".to_string(), Value::Bool(true));
    }
    let command = Value::Object(command);
    commands.insert(0, command.clone());
    write_json_array(app, "commands.json", &commands)?;
    Ok(command)
}

/// Create a new profile. `input` is the raw profile payload from the renderer.
pub fn create_profile(app: &AppHandle, input: Value) -> Result<Value, AppError> {
    let (mut profiles, folders) = read_and_heal_profiles(app)?;
    let group = input
        .get("group")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_GROUP)
        .to_string();
    let parent_id = folders
        .iter()
        .find(|f| f.get("name").and_then(|v| v.as_str()) == Some(group.as_str()))
        .and_then(|f| f.get("id").and_then(|v| v.as_str()))
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);

    let id = new_id("profile");
    let mut profile = ensure_object(&input);
    profile.remove(BACKUP_IDENTITY_FIELD);
    let ssh_route_draft = profile.remove("sshRouteRule");
    if profile.get("sshNetworkInterface") == Some(&Value::Null) {
        profile.remove("sshNetworkInterface");
    }
    crate::services::totp::validate_profile(&profile)?;
    normalize_profile_secret_input(&mut profile, None);
    normalize_profile_tags(&mut profile);
    profile.insert("id".to_string(), Value::String(id.clone()));
    profile.insert("group".to_string(), Value::String(group));
    profile.insert("parentId".to_string(), parent_id);
    if !profile.contains_key("order") {
        let now = chrono_now_ms();
        profile.insert("order".to_string(), Value::Number(now.into()));
    }
    let profile_value = Value::Object(profile);
    crate::services::ssh_network_interface::validate_save(&profile_value, &profiles)
        .map_err(AppError::Command)?;
    profiles.insert(0, profile_value.clone());

    crate::sessions::ssh::persist_new_profile_with_ssh_route(
        app,
        &profile_value,
        ssh_route_draft,
        || persist_profiles(app, &profiles),
    )?;
    Ok(profile_value)
}

/// Replace the complete local connection list with already-validated backup
/// profiles. The remote bundle does not carry local ids or ordering metadata,
/// so those fields are rebuilt here while the local encrypted secret store is
/// persisted atomically with the public profile list.
pub fn replace_profiles(app: &AppHandle, inputs: Vec<Value>) -> Result<(), AppError> {
    let (_, folders) = read_and_heal_profiles(app)?;
    let mut profiles = Vec::with_capacity(inputs.len());
    let now = chrono_now_ms();

    for (index, input) in inputs.into_iter().enumerate() {
        let group = input
            .get("group")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_GROUP)
            .to_string();
        let matching_folder = folders
            .iter()
            .find(|folder| folder.get("name").and_then(Value::as_str) == Some(group.as_str()));
        let parent_id = matching_folder
            .and_then(|folder| folder.get("id").and_then(Value::as_str))
            .map(|id| Value::String(id.to_string()))
            .unwrap_or(Value::Null);
        let mut profile = ensure_object(&input);
        crate::services::totp::validate_profile(&profile)?;
        normalize_profile_secret_input(&mut profile, None);
        normalize_profile_tags(&mut profile);
        let profile_id = profile
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| new_id("profile"));
        profile.insert("id".to_string(), Value::String(profile_id));
        profile.insert("group".to_string(), Value::String(group));
        profile.insert("parentId".to_string(), parent_id);
        if profile.get("order").and_then(Value::as_i64).is_none() {
            profile.insert(
                "order".to_string(),
                Value::Number(now.saturating_add(index as i64).into()),
            );
        }
        profiles.push(Value::Object(profile));
    }

    persist_profiles(app, &profiles)
}

/// Update an existing profile.
pub fn update_profile(app: &AppHandle, profile_id: &str, input: Value) -> Result<Value, AppError> {
    let (mut profiles, folders) = read_and_heal_profiles(app)?;
    let previous_idx = profiles
        .iter()
        .position(|p| p.get("id").and_then(|v| v.as_str()) == Some(profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
    let previous = profiles[previous_idx].clone();

    let group = input
        .get("group")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_GROUP)
        .to_string();
    let parent_id = folders
        .iter()
        .find(|f| f.get("name").and_then(|v| v.as_str()) == Some(group.as_str()))
        .and_then(|f| f.get("id").and_then(|v| v.as_str()))
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);

    let mut profile = ensure_object(&input);
    profile.remove(BACKUP_IDENTITY_FIELD);
    if profile.get("sshNetworkInterface") == Some(&Value::Null) {
        profile.remove("sshNetworkInterface");
    }
    preserve_trusted_host_fingerprint(&mut profile, Some(&previous));
    profile.remove("sshRouteRule");
    crate::services::totp::validate_profile(&profile)?;
    normalize_profile_secret_input(&mut profile, Some(&previous));
    normalize_profile_tags(&mut profile);
    profile.insert("id".to_string(), Value::String(profile_id.to_string()));
    profile.insert("group".to_string(), Value::String(group));
    profile.insert("parentId".to_string(), parent_id);

    // Preserve local ordering and private restore identity across renderer edits.
    for key in ["order", "lastUsedAt", BACKUP_IDENTITY_FIELD] {
        if let Some(v) = previous.get(key) {
            profile.insert(key.to_string(), v.clone());
        }
    }

    let profile_value = Value::Object(profile);
    crate::services::ssh_network_interface::validate_save(&profile_value, &profiles)
        .map_err(AppError::Command)?;
    if [
        "tags",
        "type",
        "authType",
        "proxy",
        "proxyProfileId",
        "sshNetworkInterface",
        "jumpProfileIds",
        "jumpProfileId",
    ]
    .iter()
    .any(|field| profile_value[*field] != previous[*field])
    {
        crate::sessions::ssh::validate_ssh_route_profile(app, &profile_value)?;
    }
    profiles[previous_idx] = profile_value.clone();

    persist_profiles(app, &profiles)?;
    Ok(profile_value)
}

pub fn set_file_pane_mode(app: &AppHandle, profile_id: &str, mode: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    set_file_pane_mode_in_profiles(&mut profiles, profile_id, mode)?;
    persist_profiles(app, &profiles)
}

pub fn set_default_directory(
    app: &AppHandle,
    profile_id: &str,
    pane: &str,
    path: &str,
) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
    set_default_directory_in_profile(profile, pane, path)?;
    persist_profiles(app, &profiles)
}

fn set_default_directory_in_profile(
    profile: &mut Value,
    pane: &str,
    path: &str,
) -> Result<(), AppError> {
    if path.trim().is_empty() || path.contains('\0') || path.len() > 32768 {
        return Err(AppError::Command(
            "Invalid default directory path".to_string(),
        ));
    }
    let field = match pane {
        "local" => "localPath",
        "remote" => "remotePath",
        _ => return Err(AppError::Command("Invalid file pane".to_string())),
    };
    let is_ssh = profile.get("type").and_then(Value::as_str) == Some("ssh");
    let profile = profile
        .as_object_mut()
        .ok_or_else(|| AppError::Storage("Invalid profile".to_string()))?;
    profile.insert(field.to_string(), Value::String(path.to_string()));
    if pane == "remote" && is_ssh {
        profile.insert("followShellCwd".to_string(), Value::Bool(false));
    }
    Ok(())
}

fn set_file_pane_mode_in_profiles(
    profiles: &mut [Value],
    profile_id: &str,
    mode: &str,
) -> Result<(), AppError> {
    if !matches!(mode, "local" | "remote-tree") {
        return Err(AppError::Command("Invalid file pane mode".to_string()));
    }
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
    let profile = profile
        .as_object_mut()
        .ok_or_else(|| AppError::Storage("Invalid profile".to_string()))?;
    profile.insert("filePaneMode".to_string(), Value::String(mode.to_string()));
    Ok(())
}
