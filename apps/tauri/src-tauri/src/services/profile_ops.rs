//! Profile / folder / command CRUD operations with group/parentId self-healing.
//!
//! Mirrors the semantics of `FileProfileRepository` from the Electron backend:
//! - `profile.group` (folder name) and `profile.parentId` (folder id) are kept
//!   in sync on every read; if healing modifies anything the profiles are
//!   persisted back.
//! - profile update / delete, folder update / delete, and entity order updates
//!   follow the same cascade rules as the Electron side.

use serde_json::{Map, Value};
use tauri::AppHandle;
use crate::AppError;
use crate::storage::{read_json_array, write_json_array, new_id};

const DEFAULT_GROUP: &str = "默认";

/// Heal `group` / `parentId` consistency on every profile.
///
/// Returns `(healed_profiles, dirty)`. Callers should persist the result when
/// `dirty` is true.
pub fn heal_profiles(
    profiles: &mut Vec<Value>,
    folders: &[Value],
) -> bool {
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
                    obj.insert(
                        "parentId".to_string(),
                        Value::String(matching_id.clone()),
                    );
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
                        obj.insert(
                            "group".to_string(),
                            Value::String(matching_name.clone()),
                        );
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
    let mut profiles = read_json_array(app, "profiles.json")?;
    let folders = read_json_array(app, "folders.json")?;
    let dirty = heal_profiles(&mut profiles, &folders);
    if dirty {
        // Strip secrets before writing back. Secrets live in
        // profile-secrets.json; profiles.json should never contain them.
        let stripped: Vec<Value> = profiles
            .iter()
            .map(strip_secret_fields)
            .collect();
        write_json_array(app, "profiles.json", &stripped)?;
    }
    Ok((profiles, folders))
}

fn strip_secret_fields(profile: &Value) -> Value {
    let mut clone = profile.clone();
    if let Some(obj) = clone.as_object_mut() {
        for key in ["password", "passphrase", "privateKeyPath"] {
            obj.remove(key);
        }
        if let Some(proxy) = obj.get_mut("proxy").and_then(|v| v.as_object_mut()) {
            proxy.remove("password");
        }
    }
    clone
}

/// Public wrapper so callers outside this module can strip secrets before
/// returning a profile to the renderer (e.g. workspace snapshot).
pub fn strip_secret_fields_public(profile: &Value) -> Value {
    strip_secret_fields(profile)
}

fn ensure_object(value: &Value) -> Map<String, Value> {
    value
        .as_object()
        .cloned()
        .unwrap_or_else(|| Map::new())
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
        .find(|f| {
            f.get("name").and_then(|v| v.as_str()) == Some(group.as_str())
        })
        .and_then(|f| f.get("id").and_then(|v| v.as_str()))
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);

    let id = new_id("profile");
    let mut profile = ensure_object(&input);
    profile.insert("id".to_string(), Value::String(id.clone()));
    profile.insert("group".to_string(), Value::String(group));
    profile.insert("parentId".to_string(), parent_id);
    if !profile.contains_key("order") {
        let now = chrono_now_ms();
        profile.insert("order".to_string(), Value::Number(now.into()));
    }
    let profile_value = Value::Object(profile);
    profiles.insert(0, profile_value.clone());

    let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    write_json_array(app, "profiles.json", &stripped)?;
    // Also persist secrets to profile-secrets.json (best-effort).
    let _ = persist_profile_secrets(app, &profiles);
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
        .find(|f| {
            f.get("name").and_then(|v| v.as_str()) == Some(group.as_str())
        })
        .and_then(|f| f.get("id").and_then(|v| v.as_str()))
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);

    let mut profile = ensure_object(&input);
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

    let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    write_json_array(app, "profiles.json", &stripped)?;
    let _ = persist_profile_secrets(app, &profiles);
    Ok(profile_value)
}

/// Delete a profile by id.
pub fn delete_profile(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    profiles.retain(|p| p.get("id").and_then(|v| v.as_str()) != Some(profile_id));
    let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    write_json_array(app, "profiles.json", &stripped)?;
    Ok(())
}

/// Record a successful user-initiated open so renderer "recent connections"
/// can use the same persisted `lastUsedAt` ordering as Electron.
pub fn touch_profile(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    let mut found = false;
    for profile in &mut profiles {
        if profile.get("id").and_then(Value::as_str) == Some(profile_id) {
            if let Some(object) = profile.as_object_mut() {
                object.insert("lastUsedAt".to_string(), Value::Number(chrono_now_ms().into()));
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
        let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
        write_json_array(&app, "profiles.json", &stripped)?;
        let _ = persist_profile_secrets(&app, &profiles);
        Ok(())
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
            let pid = p.get("parentId").and_then(|v| v.as_str()).map(|s| s.to_string());
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
        let pid = p.get("parentId").and_then(|v| v.as_str()).map(|s| s.to_string());
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
        let pid = f.get("parentId").and_then(|v| v.as_str()).map(|s| s.to_string());
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
            obj.insert("order".to_string(), Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())));
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
            obj.insert("order".to_string(), Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())));
        }
        write_json_array(app, "folders.json", &folders)?;
    }
    Ok(())
}

// ── Command folder / template operations ────────────────────────────────────

pub fn update_command_folder(app: &AppHandle, folder_id: &str, updates: Value) -> Result<Value, AppError> {
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
        let pid = f.get("parentId").and_then(|v| v.as_str()).map(|s| s.to_string());
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
        let pid = c.get("parentId").and_then(|v| v.as_str()).map(|s| s.to_string());
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
            obj.insert("order".to_string(), Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())));
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
            obj.insert("order".to_string(), Value::Number(serde_json::Number::from_f64(new_order).unwrap_or_else(|| 0.into())));
        }
        write_json_array(app, "command-folders.json", &folders)?;
    }
    Ok(())
}

pub fn update_command_template(app: &AppHandle, command_id: &str, input: Value) -> Result<Value, AppError> {
    let mut commands = read_json_array(app, "commands.json")?;
    let idx = commands
        .iter()
        .position(|c| c.get("id").and_then(|v| v.as_str()) == Some(command_id))
        .ok_or_else(|| AppError::Storage("Command not found".to_string()))?;
    let mut updated = ensure_object(&input);
    updated.insert("id".to_string(), Value::String(command_id.to_string()));
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

// ── Secrets persistence (best-effort) ───────────────────────────────────────

/// Persist sensitive profile fields to `profile-secrets.json`.
/// Best-effort: failures are swallowed (returned as Ok).
fn persist_profile_secrets(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let path = crate::storage::workspace_file(app, "profile-secrets.json")?;
    let mut existing: Value = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Storage(e.to_string()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| {
            serde_json::json!({ "version": 1, "profiles": {} })
        })
    } else {
        serde_json::json!({ "version": 1, "profiles": {} })
    };

    let secrets_profiles = existing
        .get_mut("profiles")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| AppError::Storage("Invalid secrets file".to_string()))?;

    for profile in profiles {
        let id = match profile.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let mut entry = Map::new();
        for key in ["password", "passphrase", "privateKeyPath"] {
            if let Some(v) = profile.get(key) {
                if !v.is_null() {
                    entry.insert(
                        key.to_string(),
                        serde_json::json!({ "storage": "plain-text-fallback", "value": v }),
                    );
                }
            }
        }
        if let Some(proxy) = profile.get("proxy").and_then(|v| v.as_object()) {
            if let Some(pw) = proxy.get("password") {
                if !pw.is_null() {
                    entry.insert(
                        "proxyPassword".to_string(),
                        serde_json::json!({ "storage": "plain-text-fallback", "value": pw }),
                    );
                }
            }
        }
        if !entry.is_empty() {
            secrets_profiles.insert(id, Value::Object(entry));
        } else {
            secrets_profiles.remove(&id);
        }
    }

    let content = serde_json::to_string_pretty(&existing)
        .map_err(|e| AppError::Serialization(e.to_string()))?;
    std::fs::write(&path, content).map_err(|e| AppError::Storage(e.to_string()))?;
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
            parent_id.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        );
        Value::Object(obj)
    }

    #[test]
    fn heals_when_group_points_to_valid_folder_but_parent_id_wrong() {
        let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
        let mut profiles = vec![profile("p1", "Alpha", Some("f2"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(profiles[0].get("parentId").and_then(|v| v.as_str()), Some("f1"));
    }

    #[test]
    fn heals_when_parent_id_points_to_valid_folder_but_group_wrong() {
        let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
        let mut profiles = vec![profile("p1", "默认", Some("f2"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(profiles[0].get("group").and_then(|v| v.as_str()), Some("Beta"));
    }

    #[test]
    fn heals_when_group_points_to_missing_folder() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "Ghost", Some("ghost-id"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(profiles[0].get("group").and_then(|v| v.as_str()), Some("默认"));
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
}
