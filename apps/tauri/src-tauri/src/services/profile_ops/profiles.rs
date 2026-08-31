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
    normalize_profile_secret_input(&mut profile, None);
    profile.insert("id".to_string(), Value::String(id.clone()));
    profile.insert("group".to_string(), Value::String(group));
    profile.insert("parentId".to_string(), parent_id);
    if !profile.contains_key("order") {
        let now = chrono_now_ms();
        profile.insert("order".to_string(), Value::Number(now.into()));
    }
    let profile_value = Value::Object(profile);
    profiles.insert(0, profile_value.clone());

    persist_profiles(app, &profiles)?;
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
        normalize_profile_secret_input(&mut profile, None);
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
    preserve_trusted_host_fingerprint(&mut profile, Some(&previous));
    normalize_profile_secret_input(&mut profile, Some(&previous));
    profile.insert("id".to_string(), Value::String(profile_id.to_string()));
    profile.insert("group".to_string(), Value::String(group));
    profile.insert("parentId".to_string(), parent_id);

    // Preserve order / lastUsedAt from previous.
    for key in ["order", "lastUsedAt"] {
        if let Some(v) = previous.get(key) {
            profile.insert(key.to_string(), v.clone());
        }
    }

    let profile_value = Value::Object(profile);
    profiles[previous_idx] = profile_value.clone();

    persist_profiles(app, &profiles)?;
    Ok(profile_value)
}
