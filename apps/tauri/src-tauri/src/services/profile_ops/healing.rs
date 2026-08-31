/// Heal `group` / `parentId` consistency on every profile.
///
/// Returns `(healed_profiles, dirty)`. Callers should persist the result when
/// `dirty` is true.
pub fn heal_profiles(profiles: &mut [Value], folders: &[Value]) -> bool {
    let folder_by_name: std::collections::HashMap<String, String> = folders
        .iter()
        .filter_map(|f| {
            let name = f.get("name").and_then(|v| v.as_str())?.to_string();
            let id = f.get("id").and_then(|v| v.as_str())?.to_string();
            Some((name, id))
        })
        .collect();
    let folder_name_by_id: std::collections::HashMap<String, String> = folders
        .iter()
        .filter_map(|f| {
            let name = f.get("name").and_then(|v| v.as_str())?.to_string();
            let id = f.get("id").and_then(|v| v.as_str())?.to_string();
            Some((id, name))
        })
        .collect();

    let mut dirty = false;
    for profile in profiles.iter_mut() {
        let obj = match profile.as_object_mut() {
            Some(o) => o,
            None => continue,
        };
        let group = obj
            .get("group")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let parent_id = obj
            .get("parentId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let group_is_default = group.is_empty() || group == DEFAULT_GROUP;

        if !group_is_default {
            // group is authoritative
            if let Some(matching_id) = folder_by_name.get(&group) {
                if parent_id.as_deref() != Some(matching_id.as_str()) {
                    obj.insert("parentId".to_string(), Value::String(matching_id.clone()));
                    dirty = true;
                }
            } else {
                // group points to a non-existent folder → fall back
                if parent_id.is_some() || group != DEFAULT_GROUP {
                    obj.insert("parentId".to_string(), Value::Null);
                    obj.insert(
                        "group".to_string(),
                        Value::String(DEFAULT_GROUP.to_string()),
                    );
                    dirty = true;
                }
            }
        } else {
            // group is empty / 默认 → parentId authoritative
            if let Some(pid) = &parent_id {
                if let Some(matching_name) = folder_name_by_id.get(pid) {
                    if group != *matching_name {
                        obj.insert("group".to_string(), Value::String(matching_name.clone()));
                        dirty = true;
                    }
                } else {
                    // parentId points to a non-existent folder
                    obj.insert("parentId".to_string(), Value::Null);
                    obj.insert(
                        "group".to_string(),
                        Value::String(DEFAULT_GROUP.to_string()),
                    );
                    dirty = true;
                }
            }
        }
    }
    dirty
}

/// Read profiles + folders, run healing, persist if dirty.
pub fn read_and_heal_profiles(app: &AppHandle) -> Result<(Vec<Value>, Vec<Value>), AppError> {
    let secrets_path = workspace_file(app, "profile-secrets.json")?;
    if secrets_path.exists() {
        lock_down_secret_file(&secrets_path)?;
    }

    let mut profiles = read_json_array(app, "profiles.json")?;
    let mut secret_shape_dirty = false;
    for profile in &mut profiles {
        let had_public_secret_fields = profile_contains_secret_fields(profile);
        if profile
            .as_object_mut()
            .is_some_and(|profile| normalize_profile_secret_input(profile, None))
        {
            secret_shape_dirty = true;
        }
        secret_shape_dirty |= had_public_secret_fields;
    }
    hydrate_profile_secrets(&secrets_path, &mut profiles)?;
    let folders = read_and_heal_connection_folders(app)?;
    let dirty = secret_shape_dirty || heal_profiles(&mut profiles, &folders);
    reconcile_profile_secrets(app, &profiles)?;
    if dirty {
        // Strip secrets before writing back. Secrets live in
        // profile-secrets.json; profiles.json should never contain them.
        let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
        write_json_array(app, "profiles.json", &stripped)?;
    }
    Ok((profiles, folders))
}

fn profile_contains_secret_fields(profile: &Value) -> bool {
    [
        "password",
        "passphrase",
        "privateKeyPath",
        "proxyPassword",
        "sudoPassword",
        "suPassword",
    ]
    .iter()
    .any(|field| profile.get(*field).is_some_and(|value| !value.is_null()))
        || profile
            .get("proxy")
            .and_then(Value::as_object)
            .is_some_and(|proxy| proxy.get("password").is_some_and(|value| !value.is_null()))
}

fn strip_secret_fields(profile: &Value) -> Value {
    let mut clone = profile.clone();
    if let Some(obj) = clone.as_object_mut() {
        for key in [
            "password",
            "passphrase",
            "privateKeyPath",
            "proxyPassword",
            "sudoPassword",
            "suPassword",
        ] {
            obj.remove(key);
        }
        // The old UI exposed a login-password reuse switch for sudo. Keep
        // legacy profiles loadable, but stop surfacing the retired setting.
        obj.remove("sudoSameAsLogin");
        if let Some(proxy) = obj.get_mut("proxy").and_then(|v| v.as_object_mut()) {
            proxy.remove("password");
        }
    }
    clone
}

/// Public wrapper so callers outside this module can strip secrets before
/// returning a profile to the renderer (e.g. workspace snapshot). The
/// non-secret presence bit lets an editor explain why its password input is
/// intentionally empty without disclosing the credential itself.
pub fn strip_secret_fields_public(profile: &Value) -> Value {
    let has_saved_password = !profile
        .get("useEmptyPassword")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && profile
            .get("password")
            .and_then(Value::as_str)
            .is_some_and(|password| !password.is_empty());
    let has_saved_sudo_password = profile
        .get("sudoPassword")
        .and_then(Value::as_str)
        .is_some_and(|password| !password.is_empty());
    let has_saved_su_password = profile
        .get("suPassword")
        .and_then(Value::as_str)
        .is_some_and(|password| !password.is_empty());
    let mut public = strip_secret_fields(profile);
    if let Some(object) = public.as_object_mut() {
        object.insert(
            "hasSavedPassword".to_string(),
            Value::Bool(has_saved_password),
        );
        object.insert(
            "hasSavedSudoPassword".to_string(),
            Value::Bool(has_saved_sudo_password),
        );
        object.insert(
            "hasSavedSuPassword".to_string(),
            Value::Bool(has_saved_su_password),
        );
    }
    public
}

fn ensure_object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_else(Map::new)
}

/// Convert renderer form secret fields into the persisted profile shape and,
/// for edits, retain stored credentials when the renderer only has the
/// redacted empty placeholders. `null` remains an explicit clear operation
/// for bridge/import callers that need one.
fn normalize_profile_secret_input(
    profile: &mut Map<String, Value>,
    previous: Option<&Value>,
) -> bool {
    let mut changed = false;
    let use_empty_password = profile
        .get("useEmptyPassword")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    for key in [
        "password",
        "passphrase",
        "privateKeyPath",
        "sudoPassword",
        "suPassword",
    ] {
        if key == "password" && use_empty_password {
            if profile.remove(key).is_some() {
                changed = true;
            }
            continue;
        }
        let should_preserve = match profile.get(key) {
            None => true,
            Some(Value::String(value)) => value.is_empty(),
            Some(Value::Null) => false,
            Some(_) => true,
        };
        if should_preserve {
            if let Some(previous_value) = previous
                .and_then(|value| value.get(key))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                if profile.get(key).and_then(Value::as_str) != Some(previous_value) {
                    profile.insert(key.to_string(), Value::String(previous_value.to_string()));
                    changed = true;
                }
            } else if profile.remove(key).is_some() {
                changed = true;
            }
        } else if profile.get(key).is_some_and(Value::is_null) {
            profile.remove(key);
            changed = true;
        }
    }

    let form_proxy_password = profile.remove("proxyPassword");
    if form_proxy_password.is_some() {
        changed = true;
    }
    let explicit_proxy_clear = form_proxy_password.as_ref().is_some_and(Value::is_null);
    let form_proxy_password = form_proxy_password
        .as_ref()
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let nested_proxy_password = profile
        .get("proxy")
        .and_then(Value::as_object)
        .and_then(|proxy| proxy.get("password"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let previous_proxy_password = previous
        .and_then(|value| value.get("proxy"))
        .and_then(Value::as_object)
        .and_then(|proxy| proxy.get("password"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let proxy_enabled = profile
        .get("proxy")
        .and_then(Value::as_object)
        .and_then(|proxy| proxy.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|proxy_type| proxy_type != "none");
    let next_proxy_password = if explicit_proxy_clear || !proxy_enabled {
        None
    } else {
        form_proxy_password
            .or(nested_proxy_password)
            .or(previous_proxy_password)
            .map(ToOwned::to_owned)
    };

    if let Some(proxy) = profile.get_mut("proxy").and_then(Value::as_object_mut) {
        match next_proxy_password {
            Some(password) => {
                if proxy.get("password").and_then(Value::as_str) != Some(password.as_str()) {
                    proxy.insert("password".to_string(), Value::String(password));
                    changed = true;
                }
            }
            None => {
                if proxy.remove("password").is_some() {
                    changed = true;
                }
            }
        }
    }

    changed
}

/// Keep a previously trusted SSH host key when a redacted connection form
/// sends its empty placeholder back to Rust. An explicit `null` is reserved
/// for callers that intentionally clear the value.
fn preserve_trusted_host_fingerprint(
    profile: &mut Map<String, Value>,
    previous: Option<&Value>,
) -> bool {
    let should_preserve = match profile.get("trustedHostFingerprint") {
        None => true,
        Some(Value::String(value)) => value.trim().is_empty(),
        Some(Value::Null) | Some(_) => false,
    };
    if !should_preserve {
        return false;
    }

    let Some(previous_fingerprint) = previous
        .and_then(|value| value.get("trustedHostFingerprint"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    profile.insert(
        "trustedHostFingerprint".to_string(),
        Value::String(previous_fingerprint.to_string()),
    );
    true
}

/// Build an in-memory profile for a connection test without persisting the
/// form values. Existing profiles are hydrated first so an empty/redacted
/// password field continues to use the saved credential during the test.
pub fn profile_for_connection_test(
    app: &AppHandle,
    profile_id: Option<&str>,
    input: Value,
) -> Result<Value, AppError> {
    let input_object = input
        .as_object()
        .ok_or_else(|| AppError::Command("Connection profile is invalid".to_string()))?;
    let profile_id = profile_id.map(str::trim).filter(|value| !value.is_empty());
    let (profiles, _) = read_and_heal_profiles(app)?;
    let previous = profile_id.and_then(|id| {
        profiles
            .iter()
            .find(|profile| profile.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
    });

    if profile_id.is_some() && previous.is_none() {
        return Err(AppError::Storage("Profile not found".to_string()));
    }

    let mut profile = previous
        .clone()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let object = profile
        .as_object_mut()
        .ok_or_else(|| AppError::Command("Connection profile is invalid".to_string()))?;
    for (key, value) in input_object {
        object.insert(key.clone(), value.clone());
    }
    if let Some(profile_id) = profile_id {
        object.insert("id".to_string(), Value::String(profile_id.to_string()));
    }
    preserve_trusted_host_fingerprint(object, previous.as_ref());
    normalize_profile_secret_input(object, previous.as_ref());
    Ok(profile)
}

fn heal_typed_entities(entities: &mut [Value], expected_type: &str) -> bool {
    let mut dirty = false;
    let mut next_order = chrono_now_ms();
    for entity in entities {
        let Some(object) = entity.as_object_mut() else {
            continue;
        };
        if object.get("type").and_then(Value::as_str) != Some(expected_type) {
            object.insert("type".to_string(), Value::String(expected_type.to_string()));
            dirty = true;
        }
        if object.get("order").and_then(Value::as_f64).is_none() {
            object.insert("order".to_string(), Value::Number(next_order.into()));
            next_order = next_order.saturating_add(1);
            dirty = true;
        }
    }
    dirty
}

/// Repair legacy connection-folder rows that predate the core entity
/// discriminant/order contract.
pub fn heal_connection_folders(folders: &mut [Value]) -> bool {
    heal_typed_entities(folders, "folder")
}

/// Repair legacy command-folder rows that were persisted without their
/// discriminant/order fields.
pub fn heal_command_folders(folders: &mut [Value]) -> bool {
    heal_typed_entities(folders, "command-folder")
}

/// Repair legacy command rows and the defaults required by CommandTemplate.
pub fn heal_command_templates(commands: &mut [Value]) -> bool {
    let mut dirty = heal_typed_entities(commands, "command-template");
    for command in commands {
        let Some(object) = command.as_object_mut() else {
            continue;
        };
        if object.get("command").and_then(Value::as_str).is_none() {
            object.insert("command".to_string(), Value::String(String::new()));
            dirty = true;
        }
        if object
            .get("appendCarriageReturn")
            .and_then(Value::as_bool)
            .is_none()
        {
            object.insert("appendCarriageReturn".to_string(), Value::Bool(true));
            dirty = true;
        }
    }
    dirty
}

fn read_and_heal_connection_folders(app: &AppHandle) -> Result<Vec<Value>, AppError> {
    let mut folders = read_json_array(app, "folders.json")?;
    if heal_connection_folders(&mut folders) {
        write_json_array(app, "folders.json", &folders)?;
    }
    Ok(folders)
}

/// Read command folders/templates and persist any legacy-shape repairs before
/// exposing them to a renderer snapshot.
pub fn read_and_heal_command_library(
    app: &AppHandle,
) -> Result<(Vec<Value>, Vec<Value>), AppError> {
    let mut folders = read_json_array(app, "command-folders.json")?;
    let mut commands = read_json_array(app, "commands.json")?;
    if heal_command_folders(&mut folders) {
        write_json_array(app, "command-folders.json", &folders)?;
    }
    if heal_command_templates(&mut commands) {
        write_json_array(app, "commands.json", &commands)?;
    }
    Ok((folders, commands))
}
