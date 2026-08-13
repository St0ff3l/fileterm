//! Profile / folder / command CRUD operations with group/parentId self-healing.
//!
//! Mirrors the semantics of `FileProfileRepository` from the Electron backend:
//! - `profile.group` (folder name) and `profile.parentId` (folder id) are kept
//!   in sync on every read; if healing modifies anything the profiles are
//!   persisted back.
//! - profile update / delete, folder update / delete, and entity order updates
//!   follow the same cascade rules as the Electron side.

use crate::storage::{new_id, read_json_array, workspace_file, write_json_array};
use crate::AppError;
use serde_json::{Map, Value};
use tauri::AppHandle;

const DEFAULT_GROUP: &str = "默认";

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
        if profile
            .as_object_mut()
            .is_some_and(|profile| normalize_profile_secret_input(profile, None))
        {
            secret_shape_dirty = true;
        }
    }
    let folders = read_and_heal_connection_folders(app)?;
    let dirty = secret_shape_dirty || heal_profiles(&mut profiles, &folders);
    if dirty {
        // Strip secrets before writing back. Secrets live in
        // profile-secrets.json; profiles.json should never contain them.
        let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
        write_json_array(app, "profiles.json", &stripped)?;
    }
    reconcile_profile_secrets(app, &profiles)?;
    Ok((profiles, folders))
}

fn strip_secret_fields(profile: &Value) -> Value {
    let mut clone = profile.clone();
    if let Some(obj) = clone.as_object_mut() {
        for key in ["password", "passphrase", "privateKeyPath", "proxyPassword"] {
            obj.remove(key);
        }
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
    let mut public = strip_secret_fields(profile);
    if let Some(object) = public.as_object_mut() {
        object.insert(
            "hasSavedPassword".to_string(),
            Value::Bool(has_saved_password),
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
    for key in ["password", "passphrase", "privateKeyPath"] {
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

/// Delete a profile by id.
pub fn delete_profile(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    profiles.retain(|p| p.get("id").and_then(|v| v.as_str()) != Some(profile_id));
    persist_profiles(app, &profiles)
}

/// Record a successful user-initiated open so renderer "recent connections"
/// can use the same persisted `lastUsedAt` ordering as Electron.
pub fn touch_profile(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    let mut found = false;
    for profile in &mut profiles {
        if profile.get("id").and_then(Value::as_str) == Some(profile_id) {
            if let Some(object) = profile.as_object_mut() {
                object.insert(
                    "lastUsedAt".to_string(),
                    Value::Number(chrono_now_ms().into()),
                );
                found = true;
            }
            break;
        }
    }
    if !found {
        return Err(AppError::Storage("Profile not found".to_string()));
    }
    let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    write_json_array(app, "profiles.json", &stripped)
}

/// Update only the `trustedHostFingerprint` field on a profile. Called from
/// the SSH worker's `check_server_key` when the user picks "accept-and-save".
/// This avoids clobbering other profile fields (which a full `update_profile`
/// would require) and is safe to call from the worker context.
pub async fn update_trusted_host_fingerprint(
    app: &AppHandle,
    profile_id: &str,
    fingerprint: &str,
) -> Result<(), AppError> {
    crate::services::logging::info(app, "profile", "saving trusted host fingerprint");
    let app = app.clone();
    let profile_id = profile_id.to_string();
    let fingerprint = fingerprint.to_string();
    tokio::task::spawn_blocking(move || {
        let (mut profiles, _) = read_and_heal_profiles(&app)?;
        let mut found = false;
        if let Some(profile) = profiles
            .iter_mut()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(profile_id.as_str()))
        {
            if let Some(obj) = profile.as_object_mut() {
                obj.insert(
                    "trustedHostFingerprint".to_string(),
                    Value::String(fingerprint.clone()),
                );
                found = true;
            }
        }
        crate::services::logging::debug(
            &app,
            "profile",
            format!("trusted host fingerprint profile_found={found}"),
        );
        persist_profiles(&app, &profiles)
    })
    .await
    .map_err(|e| AppError::Storage(format!("join error: {}", e)))?
}

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

/// Build the complete secret store from the current profile set. Rebuilding
/// instead of incrementally merging guarantees that deleted profiles cannot
/// leave orphan credentials behind.
fn profile_secret_storage_root(path: &std::path::Path) -> Result<&std::path::Path, AppError> {
    path.parent()
        .ok_or_else(|| AppError::Storage("无法解析连接凭据存储目录".to_string()))
}

fn current_profile_secret<'a>(
    current: Option<&'a Value>,
    profile_id: &str,
    field: &str,
) -> Option<&'a Value> {
    current?
        .get("profiles")?
        .get(profile_id)?
        .get(field)?
        .get("value")
}

fn encrypted_profile_secret(
    storage_root: &std::path::Path,
    current: Option<&Value>,
    profile_id: &str,
    field: &str,
    plaintext: &str,
) -> Result<Value, AppError> {
    let scope = format!("profile/{profile_id}/{field}");
    if let Some(existing) = current_profile_secret(current, profile_id, field)
        .and_then(Value::as_str)
        .filter(|existing| crate::services::secret_crypto::is_encrypted(existing))
    {
        if crate::services::secret_crypto::decrypt(storage_root, &scope, existing)? == plaintext {
            return Ok(serde_json::json!({
                "storage": crate::services::secret_crypto::ENCRYPTED_STORAGE,
                "value": existing,
            }));
        }
    }
    Ok(serde_json::json!({
        "storage": crate::services::secret_crypto::ENCRYPTED_STORAGE,
        "value": crate::services::secret_crypto::encrypt(storage_root, &scope, plaintext)?,
    }))
}

/// Build the complete secret store from the current profile set. Rebuilding
/// instead of incrementally merging guarantees that deleted profiles cannot
/// leave orphan credentials behind. Existing matching ciphertext is reused so
/// normal reads do not rotate values just because AES-GCM has a fresh nonce.
fn build_profile_secrets(
    path: &std::path::Path,
    profiles: &[Value],
    current: Option<&Value>,
) -> Result<Value, AppError> {
    let storage_root = profile_secret_storage_root(path)?;
    let mut secrets_profiles = Map::new();
    for profile in profiles {
        let id = match profile.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let mut entry = Map::new();
        for key in ["password", "passphrase", "privateKeyPath"] {
            if let Some(value) = profile.get(key).and_then(Value::as_str) {
                entry.insert(
                    key.to_string(),
                    encrypted_profile_secret(storage_root, current, &id, key, value)?,
                );
            }
        }
        if let Some(proxy) = profile.get("proxy").and_then(|v| v.as_object()) {
            if let Some(password) = proxy.get("password").and_then(Value::as_str) {
                entry.insert(
                    "proxyPassword".to_string(),
                    encrypted_profile_secret(
                        storage_root,
                        current,
                        &id,
                        "proxyPassword",
                        password,
                    )?,
                );
            }
        }
        if !entry.is_empty() {
            secrets_profiles.insert(id, Value::Object(entry));
        }
    }

    Ok(serde_json::json!({
        "version": 1,
        "profiles": secrets_profiles,
    }))
}

#[cfg(unix)]
fn lock_down_secret_file(path: &std::path::Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_secret_file(_path: &std::path::Path) -> Result<(), AppError> {
    // Windows ACL semantics are inherited from the per-user app-data
    // directory. Keep this best-effort behavior aligned with Electron.
    Ok(())
}

fn remove_file_if_present(path: &std::path::Path) -> Result<(), AppError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn write_secure_secret_file(path: &std::path::Path, content: &[u8]) -> Result<(), AppError> {
    use std::io::Write;

    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let nonce = uuid::Uuid::new_v4();
    let temp_path = path.with_file_name(format!(".{file_name}.{nonce}.tmp"));
    let backup_path = path.with_file_name(format!(".{file_name}.{nonce}.bak"));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut temp_file = options
        .open(&temp_path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    if let Err(error) = temp_file
        .write_all(content)
        .and_then(|_| temp_file.sync_all())
    {
        drop(temp_file);
        let cleanup_error = remove_file_if_present(&temp_path).err();
        return Err(AppError::Storage(match cleanup_error {
            Some(cleanup_error) => format!("{error}; 清理凭据临时文件失败: {cleanup_error}"),
            None => error.to_string(),
        }));
    }
    drop(temp_file);
    if let Err(error) = lock_down_secret_file(&temp_path) {
        let cleanup_error = remove_file_if_present(&temp_path).err();
        return Err(AppError::Storage(match cleanup_error {
            Some(cleanup_error) => format!("{error}; 清理凭据临时文件失败: {cleanup_error}"),
            None => error.to_string(),
        }));
    }

    let had_previous = path.exists();
    if had_previous {
        if let Err(error) = std::fs::rename(path, &backup_path) {
            let cleanup_error = remove_file_if_present(&temp_path).err();
            return Err(AppError::Storage(match cleanup_error {
                Some(cleanup_error) => {
                    format!("{error}; 清理凭据临时文件失败: {cleanup_error}")
                }
                None => error.to_string(),
            }));
        }
    }

    if let Err(error) = std::fs::rename(&temp_path, path) {
        let restore_error = if had_previous {
            std::fs::rename(&backup_path, path).err()
        } else {
            None
        };
        let _ = remove_file_if_present(&temp_path);
        return Err(AppError::Storage(match restore_error {
            Some(restore_error) => format!("{error}; 恢复原凭据文件失败: {restore_error}"),
            None => error.to_string(),
        }));
    }

    if let Err(error) = lock_down_secret_file(path) {
        let remove_error = remove_file_if_present(path).err();
        let restore_error = if had_previous {
            std::fs::rename(&backup_path, path).err()
        } else {
            None
        };
        return match (remove_error, restore_error) {
            (None, None) => Err(error),
            (remove_error, restore_error) => Err(AppError::Storage(format!(
                "{error}; 清理失败: {}; 恢复失败: {}",
                remove_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "无".to_string()),
                restore_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "无".to_string())
            ))),
        };
    }

    remove_file_if_present(&backup_path)?;
    Ok(())
}

fn read_profile_secret_store(path: &std::path::Path) -> Result<Option<Value>, AppError> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| AppError::Serialization(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn persist_profile_secrets_at(path: &std::path::Path, profiles: &[Value]) -> Result<(), AppError> {
    let current = read_profile_secret_store(path)?;
    let content =
        serde_json::to_vec_pretty(&build_profile_secrets(path, profiles, current.as_ref())?)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
    write_secure_secret_file(path, &content)
}

fn persist_profile_secrets(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let path = workspace_file(app, "profile-secrets.json")?;
    persist_profile_secrets_at(&path, profiles)
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<Vec<u8>>, AppError> {
    match std::fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn restore_secret_file(path: &std::path::Path, content: Option<&[u8]>) -> Result<(), AppError> {
    match content {
        Some(content) => write_secure_secret_file(path, content),
        None => remove_file_if_present(path),
    }
}

/// Persist both halves of the profile store. Secrets are written first; if
/// the public profile write fails, the previous secret file is restored so a
/// failed operation cannot silently strand a profile without its credentials.
fn persist_profiles(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let secrets_path = workspace_file(app, "profile-secrets.json")?;
    let previous_secrets = read_optional_file(&secrets_path)?;
    persist_profile_secrets(app, profiles)?;

    let public_profiles: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    if let Err(public_error) = write_json_array(app, "profiles.json", &public_profiles) {
        return match restore_secret_file(&secrets_path, previous_secrets.as_deref()) {
            Ok(()) => Err(public_error),
            Err(rollback_error) => Err(AppError::Storage(format!(
                "{public_error}; 恢复凭据文件失败: {rollback_error}"
            ))),
        };
    }
    Ok(())
}

/// Heal legacy modes and prune stale secret IDs on normal profile reads.
fn reconcile_profile_secrets(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let path = workspace_file(app, "profile-secrets.json")?;
    let current = read_profile_secret_store(&path)?;
    let expected = build_profile_secrets(&path, profiles, current.as_ref())?;

    let expected_has_secrets = expected["profiles"]
        .as_object()
        .is_some_and(|profiles| !profiles.is_empty());
    if current.as_ref() != Some(&expected) && (path.exists() || expected_has_secrets) {
        let content = serde_json::to_vec_pretty(&expected)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        write_secure_secret_file(&path, &content)?;
    }
    if path.exists() {
        lock_down_secret_file(&path)?;
    }
    Ok(())
}

fn chrono_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn folder(id: &str, name: &str) -> Value {
        json!({ "id": id, "name": name, "type": "folder" })
    }

    fn profile(id: &str, group: &str, parent_id: Option<&str>) -> Value {
        let mut obj = Map::new();
        obj.insert("id".to_string(), Value::String(id.to_string()));
        obj.insert("name".to_string(), Value::String(format!("Profile {}", id)));
        obj.insert("type".to_string(), Value::String("ssh".to_string()));
        obj.insert("group".to_string(), Value::String(group.to_string()));
        obj.insert(
            "parentId".to_string(),
            parent_id
                .map(|s| Value::String(s.to_string()))
                .unwrap_or(Value::Null),
        );
        Value::Object(obj)
    }

    #[test]
    fn heals_when_group_points_to_valid_folder_but_parent_id_wrong() {
        let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
        let mut profiles = vec![profile("p1", "Alpha", Some("f2"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(
            profiles[0].get("parentId").and_then(|v| v.as_str()),
            Some("f1")
        );
    }

    #[test]
    fn heals_when_parent_id_points_to_valid_folder_but_group_wrong() {
        let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
        let mut profiles = vec![profile("p1", "默认", Some("f2"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(
            profiles[0].get("group").and_then(|v| v.as_str()),
            Some("Beta")
        );
    }

    #[test]
    fn heals_when_group_points_to_missing_folder() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "Ghost", Some("ghost-id"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(
            profiles[0].get("group").and_then(|v| v.as_str()),
            Some("默认")
        );
        assert!(profiles[0].get("parentId").unwrap().is_null());
    }

    #[test]
    fn no_change_when_consistent() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "Alpha", Some("f1"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(!dirty);
    }

    #[test]
    fn default_group_with_null_parent_id_untouched() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "默认", None)];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(!dirty);
    }

    #[test]
    fn heals_legacy_folder_and_command_entity_shapes() {
        let mut connection_folders = vec![json!({ "id": "f1", "name": "Legacy" })];
        let mut command_folders = vec![json!({ "id": "cf1", "name": "Legacy commands" })];
        let mut commands = vec![json!({ "id": "c1", "name": "Legacy command" })];

        assert!(heal_connection_folders(&mut connection_folders));
        assert!(heal_command_folders(&mut command_folders));
        assert!(heal_command_templates(&mut commands));

        assert_eq!(connection_folders[0]["type"], "folder");
        assert!(connection_folders[0]["order"].is_number());
        assert_eq!(command_folders[0]["type"], "command-folder");
        assert!(command_folders[0]["order"].is_number());
        assert_eq!(commands[0]["type"], "command-template");
        assert!(commands[0]["order"].is_number());
        assert_eq!(commands[0]["command"], "");
        assert_eq!(commands[0]["appendCarriageReturn"], true);

        assert!(!heal_connection_folders(&mut connection_folders));
        assert!(!heal_command_folders(&mut command_folders));
        assert!(!heal_command_templates(&mut commands));
    }

    #[test]
    fn rebuilding_secrets_prunes_deleted_profile_ids_and_encrypts_values() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-profile-secrets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("profile-secrets.json");
        let profiles = vec![serde_json::json!({
            "id": "profile-current",
            "password": "plain-text-password",
            "proxy": { "password": "plain-text-proxy-password" }
        })];
        let secrets = build_profile_secrets(&path, &profiles, None).unwrap();
        let stored = secrets["profiles"].as_object().unwrap();

        assert_eq!(stored.len(), 1);
        assert!(stored.contains_key("profile-current"));
        assert!(!stored.contains_key("profile-deleted"));
        assert_eq!(
            stored["profile-current"]["password"]["storage"].as_str(),
            Some(crate::services::secret_crypto::ENCRYPTED_STORAGE)
        );
        assert_ne!(
            stored["profile-current"]["password"]["value"].as_str(),
            Some("plain-text-password")
        );

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn redacted_edit_placeholders_preserve_stored_profile_secrets() {
        let previous = json!({
            "id": "profile-1",
            "password": "stored-password",
            "passphrase": "stored-passphrase",
            "privateKeyPath": "/keys/id_ed25519",
            "proxy": {
                "type": "http",
                "host": "proxy.example.com",
                "port": 8080,
                "password": "stored-proxy-password"
            }
        });
        let mut edit = json!({
            "id": "profile-1",
            "password": "",
            "passphrase": "",
            "privateKeyPath": "",
            "proxyPassword": "",
            "proxy": {
                "type": "http",
                "host": "proxy.example.com",
                "port": 8080
            }
        })
        .as_object()
        .unwrap()
        .clone();

        assert!(normalize_profile_secret_input(&mut edit, Some(&previous)));
        assert_eq!(edit["password"], "stored-password");
        assert_eq!(edit["passphrase"], "stored-passphrase");
        assert_eq!(edit["privateKeyPath"], "/keys/id_ed25519");
        assert_eq!(edit["proxy"]["password"], "stored-proxy-password");
        assert!(!edit.contains_key("proxyPassword"));

        let public = strip_secret_fields_public(&Value::Object(edit));
        assert!(public.get("password").is_none());
        assert!(public.get("passphrase").is_none());
        assert!(public.get("privateKeyPath").is_none());
        assert!(public.get("proxyPassword").is_none());
        assert!(public["proxy"].get("password").is_none());
        assert_eq!(public["hasSavedPassword"], true);
    }

    #[test]
    fn empty_password_mode_discards_any_saved_password() {
        let previous = json!({
            "id": "profile-1",
            "password": "stored-password"
        });
        let mut edit = json!({
            "id": "profile-1",
            "password": "",
            "useEmptyPassword": true
        })
        .as_object()
        .unwrap()
        .clone();

        assert!(normalize_profile_secret_input(&mut edit, Some(&previous)));
        assert!(!edit.contains_key("password"));
        assert_eq!(edit["useEmptyPassword"], true);
        assert_eq!(
            strip_secret_fields_public(&Value::Object(edit))["hasSavedPassword"],
            false
        );
    }

    #[test]
    fn proxy_form_password_is_normalized_and_can_be_explicitly_cleared() {
        let mut create = json!({
            "proxyPassword": "new-proxy-password",
            "proxy": { "type": "socks5", "host": "proxy.example.com", "port": 1080 }
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(normalize_profile_secret_input(&mut create, None));
        assert_eq!(create["proxy"]["password"], "new-proxy-password");
        assert!(!create.contains_key("proxyPassword"));

        let previous = Value::Object(create.clone());
        let mut clear = json!({
            "proxyPassword": null,
            "proxy": { "type": "socks5", "host": "proxy.example.com", "port": 1080 }
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(normalize_profile_secret_input(&mut clear, Some(&previous)));
        assert!(clear["proxy"].get("password").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn plaintext_secret_file_is_written_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            std::env::temp_dir().join(format!("fileterm-profile-secrets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("profile-secrets.json");
        let profiles = vec![serde_json::json!({
            "id": "profile-1",
            "password": "plain-text-password"
        })];

        persist_profile_secrets_at(&path, &profiles).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        lock_down_secret_file(&path).unwrap();
        let healed_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(healed_mode, 0o600);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
