/// Update a folder. If `name` is changed, cascade to children profiles' `group`.
pub fn update_folder(app: &AppHandle, folder_id: &str, updates: Value) -> Result<Value, AppError> {
    let (profiles, mut folders) = read_and_heal_profiles(app)?;
    let idx = folders
        .iter()
        .position(|f| f.get("id").and_then(|v| v.as_str()) == Some(folder_id))
        .ok_or_else(|| AppError::Storage("Folder not found".to_string()))?;

    let mut updated = folders[idx].clone();
    if let Some(obj) = updated.as_object_mut() {
        if let Some(updates_obj) = updates.as_object() {
            for (k, v) in updates_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    folders[idx] = updated.clone();
    write_json_array(app, "folders.json", &folders)?;

    // Cascade rename: if name changed, update child profiles' group.
    if let Some(new_name) = updates.get("name").and_then(|v| v.as_str()) {
        let mut next_profiles = profiles;
        let mut changed = false;
        for p in next_profiles.iter_mut() {
            let pid = p
                .get("parentId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if pid.as_deref() == Some(folder_id) {
                if let Some(obj) = p.as_object_mut() {
                    obj.insert("group".to_string(), Value::String(new_name.to_string()));
                    changed = true;
                }
            }
        }
        if changed {
            let stripped: Vec<Value> = next_profiles.iter().map(strip_secret_fields).collect();
            write_json_array(app, "profiles.json", &stripped)?;
        }
    }

    Ok(updated)
}

/// Delete a folder. Children profiles/folders move up to the deleted folder's
/// parent. Child profiles get their `group` updated to the parent folder name
/// (or `默认` if the deleted folder was at root).
pub fn delete_folder(app: &AppHandle, folder_id: &str) -> Result<(), AppError> {
    let (mut profiles, mut folders) = read_and_heal_profiles(app)?;
    let folder = folders
        .iter()
        .find(|f| f.get("id").and_then(|v| v.as_str()) == Some(folder_id))
        .cloned();
    let folder = match folder {
        Some(f) => f,
        None => return Ok(()), // silently succeed
    };
    let next_parent_id = folder
        .get("parentId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    folders.retain(|f| f.get("id").and_then(|v| v.as_str()) != Some(folder_id));

    let next_parent_folder = next_parent_id
        .as_ref()
        .and_then(|pid| {
            folders
                .iter()
                .find(|f| f.get("id").and_then(|v| v.as_str()) == Some(pid))
        })
        .cloned();
    let group_name = next_parent_folder
        .as_ref()
        .and_then(|f| f.get("name").and_then(|v| v.as_str()))
        .unwrap_or(DEFAULT_GROUP)
        .to_string();

    // Cascade: child profiles
    let mut profiles_changed = false;
    for p in profiles.iter_mut() {
        let pid = p
            .get("parentId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if pid.as_deref() == Some(folder_id) {
            if let Some(obj) = p.as_object_mut() {
                obj.insert(
                    "parentId".to_string(),
                    next_parent_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                obj.insert("group".to_string(), Value::String(group_name.clone()));
                profiles_changed = true;
            }
        }
    }

    // Cascade: child folders
    let mut folders_changed = false;
    for f in folders.iter_mut() {
        let pid = f
            .get("parentId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if pid.as_deref() == Some(folder_id) {
            if let Some(obj) = f.as_object_mut() {
                obj.insert(
                    "parentId".to_string(),
                    next_parent_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                folders_changed = true;
            }
        }
    }

    write_json_array(app, "folders.json", &folders)?;
    if profiles_changed {
        let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
        write_json_array(app, "profiles.json", &stripped)?;
    }
    let _ = folders_changed; // already persisted above
    Ok(())
}

/// Update entity order. Works for both profiles and folders (profile first).
pub fn update_entity_order(
    app: &AppHandle,
    id: &str,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<(), AppError> {
    let (mut profiles, mut folders) = read_and_heal_profiles(app)?;

    // Try profile first.
    let profile_idx = profiles
        .iter()
        .position(|p| p.get("id").and_then(|v| v.as_str()) == Some(id));
    if let Some(idx) = profile_idx {
        let group = match &new_parent_id {
            Some(pid) => folders
                .iter()
                .find(|f| f.get("id").and_then(|v| v.as_str()) == Some(pid))
                .and_then(|f| f.get("name").and_then(|v| v.as_str()))
                .unwrap_or(DEFAULT_GROUP)
                .to_string(),
            None => DEFAULT_GROUP.to_string(),
        };
        if let Some(obj) = profiles[idx].as_object_mut() {
            obj.insert(
                "parentId".to_string(),
                new_parent_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            obj.insert("group".to_string(), Value::String(group));
            obj.insert(
                "order".to_string(),
                Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())),
            );
        }
        let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
        write_json_array(app, "profiles.json", &stripped)?;
        return Ok(());
    }

    // Else, try folder.
    let folder_idx = folders
        .iter()
        .position(|f| f.get("id").and_then(|v| v.as_str()) == Some(id));
    if let Some(idx) = folder_idx {
        if let Some(obj) = folders[idx].as_object_mut() {
            obj.insert(
                "parentId".to_string(),
                new_parent_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            obj.insert(
                "order".to_string(),
                Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())),
            );
        }
        write_json_array(app, "folders.json", &folders)?;
    }
    Ok(())
}

// ── Command folder / template operations ────────────────────────────────────

pub fn update_command_folder(
    app: &AppHandle,
    folder_id: &str,
    updates: Value,
) -> Result<Value, AppError> {
    let mut folders = read_json_array(app, "command-folders.json")?;
    let idx = folders
        .iter()
        .position(|f| f.get("id").and_then(|v| v.as_str()) == Some(folder_id))
        .ok_or_else(|| AppError::Storage("Folder not found".to_string()))?;
    let mut updated = folders[idx].clone();
    if let Some(obj) = updated.as_object_mut() {
        if let Some(updates_obj) = updates.as_object() {
            for (k, v) in updates_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    folders[idx] = updated.clone();
    write_json_array(app, "command-folders.json", &folders)?;
    Ok(updated)
}

pub fn delete_command_folder(app: &AppHandle, folder_id: &str) -> Result<(), AppError> {
    let mut folders = read_json_array(app, "command-folders.json")?;
    let mut commands = read_json_array(app, "commands.json")?;

    let folder = folders
        .iter()
        .find(|f| f.get("id").and_then(|v| v.as_str()) == Some(folder_id))
        .cloned();
    let folder = match folder {
        Some(f) => f,
        None => return Ok(()),
    };
    let next_parent_id = folder
        .get("parentId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    folders.retain(|f| f.get("id").and_then(|v| v.as_str()) != Some(folder_id));

    for f in folders.iter_mut() {
        let pid = f
            .get("parentId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if pid.as_deref() == Some(folder_id) {
            if let Some(obj) = f.as_object_mut() {
                obj.insert(
                    "parentId".to_string(),
                    next_parent_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
            }
        }
    }
    for c in commands.iter_mut() {
        let pid = c
            .get("parentId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if pid.as_deref() == Some(folder_id) {
            if let Some(obj) = c.as_object_mut() {
                obj.insert(
                    "parentId".to_string(),
                    next_parent_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
            }
        }
    }

    write_json_array(app, "command-folders.json", &folders)?;
    write_json_array(app, "commands.json", &commands)?;
    Ok(())
}

pub fn update_command_order(
    app: &AppHandle,
    id: &str,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<(), AppError> {
    let mut folders = read_json_array(app, "command-folders.json")?;
    let mut commands = read_json_array(app, "commands.json")?;

    let cmd_idx = commands
        .iter()
        .position(|c| c.get("id").and_then(|v| v.as_str()) == Some(id));
    if let Some(idx) = cmd_idx {
        if let Some(obj) = commands[idx].as_object_mut() {
            obj.insert(
                "parentId".to_string(),
                new_parent_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            obj.insert(
                "order".to_string(),
                Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())),
            );
        }
        write_json_array(app, "commands.json", &commands)?;
        return Ok(());
    }

    let folder_idx = folders
        .iter()
        .position(|f| f.get("id").and_then(|v| v.as_str()) == Some(id));
    if let Some(idx) = folder_idx {
        if let Some(obj) = folders[idx].as_object_mut() {
            obj.insert(
                "parentId".to_string(),
                new_parent_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            obj.insert(
                "order".to_string(),
                Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())),
            );
        }
        write_json_array(app, "command-folders.json", &folders)?;
    }
    Ok(())
}

pub fn update_command_template(
    app: &AppHandle,
    command_id: &str,
    input: Value,
) -> Result<Value, AppError> {
    let (_, mut commands) = read_and_heal_command_library(app)?;
    let idx = commands
        .iter()
        .position(|c| c.get("id").and_then(|v| v.as_str()) == Some(command_id))
        .ok_or_else(|| AppError::Storage("Command not found".to_string()))?;
    let previous = commands[idx].clone();
    let mut updated = ensure_object(&input);
    updated.insert("id".to_string(), Value::String(command_id.to_string()));
    updated.insert(
        "type".to_string(),
        Value::String("command-template".to_string()),
    );
    if updated.get("order").and_then(Value::as_f64).is_none() {
        updated.insert(
            "order".to_string(),
            previous
                .get("order")
                .cloned()
                .unwrap_or_else(|| Value::Number(chrono_now_ms().into())),
        );
    }
    if updated.get("command").and_then(Value::as_str).is_none() {
        updated.insert("command".to_string(), Value::String(String::new()));
    }
    if updated
        .get("appendCarriageReturn")
        .and_then(Value::as_bool)
        .is_none()
    {
        updated.insert("appendCarriageReturn".to_string(), Value::Bool(true));
    }
    let updated_value = Value::Object(updated);
    commands[idx] = updated_value.clone();
    write_json_array(app, "commands.json", &commands)?;
    Ok(updated_value)
}

pub fn delete_command_template(app: &AppHandle, command_id: &str) -> Result<(), AppError> {
    let mut commands = read_json_array(app, "commands.json")?;
    commands.retain(|c| c.get("id").and_then(|v| v.as_str()) != Some(command_id));
    write_json_array(app, "commands.json", &commands)?;
    Ok(())
}

// ── Encrypted secrets persistence ───────────────────────────────────────────
