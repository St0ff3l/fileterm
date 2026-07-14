//! Connection import/export codecs shared by the Tauri command layer.
//! Parsing happens entirely in the main process so the confirmation UI only
//! receives non-secret preview metadata; the selected plan stays local until
//! the user commits it.

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::services::profile_ops;
use crate::services::workspace::ConnectionImportPlanEntry;
use crate::AppError;

fn command_error(message: impl Into<String>) -> AppError {
    AppError::Command(message.into())
}

fn profile_type(value: Option<&str>) -> String {
    let value = value.unwrap_or("ssh").to_ascii_lowercase();
    if value.contains("ftp") {
        "ftp".to_string()
    } else if value.contains("telnet") {
        "telnet".to_string()
    } else if value.contains("serial") {
        "serial".to_string()
    } else {
        "ssh".to_string()
    }
}

fn default_port(kind: &str) -> u64 {
    match kind {
        "ftp" => 21,
        "telnet" => 23,
        _ => 22,
    }
}

fn safe_string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn map_auth(value: Option<&Value>) -> &'static str {
    let value = value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if value.contains("interactive") {
        "keyboard-interactive"
    } else if value.contains("key") {
        "privateKey"
    } else if value.contains("system") || value.contains("agent") {
        "system"
    } else {
        "password"
    }
}

fn normalize_external_profile(raw: &Value, fallback_name: &str) -> Result<Value, String> {
    let source = raw
        .as_object()
        .ok_or_else(|| "配置项不是对象".to_string())?;
    let kind = profile_type(
        source
            .get("type")
            .or_else(|| source.get("connection_type"))
            .or_else(|| source.get("conection_type"))
            .and_then(Value::as_str),
    );
    let name = safe_string(source.get("name"));
    let name = if name.is_empty() {
        fallback_name.to_string()
    } else {
        name
    };
    let host = safe_string(source.get("host"));
    let port = source
        .get("port")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| default_port(&kind));
    if kind != "serial" && (host.is_empty() || !(1..=65535).contains(&port)) {
        return Err("缺少有效 Host 或 Port".to_string());
    }
    if kind == "serial" && safe_string(source.get("devicePath")).is_empty() {
        return Err("缺少串口设备路径".to_string());
    }

    let mut profile = Value::Object(source.clone());
    let object = profile.as_object_mut().expect("source object cloned");
    object.remove("id");
    object.remove("parentId");
    object.remove("order");
    object.insert("type".to_string(), Value::String(kind.clone()));
    object.insert("name".to_string(), Value::String(name));
    object.insert("host".to_string(), Value::String(host));
    object.insert("port".to_string(), Value::Number(port.into()));
    object.insert(
        "username".to_string(),
        Value::String(
            source
                .get("username")
                .or_else(|| source.get("user_name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    );
    object.insert(
        "group".to_string(),
        Value::String(
            source
                .get("group")
                .and_then(Value::as_str)
                .unwrap_or("默认")
                .to_string(),
        ),
    );
    object.insert(
        "remotePath".to_string(),
        Value::String(
            source
                .get("remotePath")
                .or_else(|| source.get("remote_path"))
                .and_then(Value::as_str)
                .unwrap_or("/")
                .to_string(),
        ),
    );
    if kind == "ssh" {
        let auth = source
            .get("authType")
            .or_else(|| source.get("authentication_type"));
        object
            .entry("authType".to_string())
            .or_insert_with(|| Value::String(map_auth(auth).to_string()));
    }
    if let Some(value) = source.get("terminal_encoding") {
        object
            .entry("encoding".to_string())
            .or_insert_with(|| value.clone());
    }
    Ok(profile)
}

fn fingerprint(profile: &Value) -> Option<(String, String, String, u64, String)> {
    Some((
        profile.get("type")?.as_str()?.to_ascii_lowercase(),
        profile.get("name")?.as_str()?.to_string(),
        profile
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        profile
            .get("port")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        profile
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

fn preview(input: &Value, source_label: &str, existing: &[Value]) -> Value {
    let conflict = fingerprint(input).and_then(|needle| {
        existing
            .iter()
            .find(|profile| fingerprint(profile).as_ref() == Some(&needle))
            .and_then(|profile| {
                profile
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
    });
    serde_json::json!({
        "id": format!("import-item-{}", uuid::Uuid::new_v4()),
        "sourceLabel": source_label,
        "name": input.get("name").and_then(Value::as_str).unwrap_or("未命名连接"),
        "type": input.get("type").and_then(Value::as_str).unwrap_or("ssh"),
        "host": input.get("host").and_then(Value::as_str),
        "port": input.get("port").and_then(Value::as_u64),
        "username": input.get("username").and_then(Value::as_str),
        "status": "ready",
        "conflictProfileId": conflict,
    })
}

fn parse_ssh_config(
    text: &str,
    source_label: &str,
    existing: &[Value],
) -> Vec<ConnectionImportPlanEntry> {
    let mut blocks = Vec::<std::collections::HashMap<String, String>>::new();
    let mut current: Option<std::collections::HashMap<String, String>> = None;
    for raw_line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let key = key.to_ascii_lowercase();
        let value = value.trim();
        if key == "host" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            if value.split_whitespace().count() != 1 || value.contains(['*', '?', '!']) {
                continue;
            }
            let mut block = std::collections::HashMap::new();
            block.insert("hostAlias".to_string(), value.to_string());
            current = Some(block);
        } else if let Some(block) = current.as_mut() {
            block.insert(key, value.to_string());
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
        .into_iter()
        .map(|block| {
            let alias = block.get("hostAlias").cloned().unwrap_or_else(|| "SSH".to_string());
            let host = block.get("hostname").cloned().unwrap_or_default();
            let port = block.get("port").and_then(|value| value.parse::<u64>().ok()).unwrap_or(22);
            let item = if host.is_empty() || !(1..=65535).contains(&port) {
                serde_json::json!({
                    "id": format!("import-item-{}", uuid::Uuid::new_v4()),
                    "sourceLabel": source_label,
                    "name": alias,
                    "type": "ssh",
                    "status": "invalid",
                    "reason": "缺少有效 HostName 或 Port",
                })
            } else {
                let input = serde_json::json!({
                    "type": "ssh", "name": alias, "host": host, "port": port,
                    "username": block.get("user").cloned().unwrap_or_default(),
                    "group": "默认", "remotePath": "/",
                    "authType": if block.contains_key("identityfile") { "privateKey" } else { "system" },
                    "privateKeyPath": block.get("identityfile").cloned(),
                    "enableExecChannel": true, "enableResourceMonitoring": true,
                });
                let mut preview = preview(&input, source_label, existing);
                if block.contains_key("proxyjump") {
                    preview["unsupportedFields"] = serde_json::json!(["proxyjump"]);
                }
                return ConnectionImportPlanEntry { preview, input: Some(input) };
            };
            ConnectionImportPlanEntry { preview: item, input: None }
        })
        .collect()
}

fn parse_json(
    text: &str,
    source_label: &str,
    existing: &[Value],
) -> Result<Vec<ConnectionImportPlanEntry>, AppError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| command_error(format!("连接 JSON 无效: {error}")))?;
    let profiles = match value {
        Value::Array(values) => values,
        Value::Object(mut object) => match object.remove("profiles") {
            Some(Value::Array(values)) => values,
            _ => vec![Value::Object(object)],
        },
        _ => return Err(command_error("连接 JSON 必须是对象或数组")),
    };
    Ok(profiles
        .into_iter()
        .enumerate()
        .map(|(index, raw)| {
            match normalize_external_profile(&raw, &format!("导入连接 {}", index + 1)) {
                Ok(input) => ConnectionImportPlanEntry {
                    preview: preview(&input, source_label, existing),
                    input: Some(input),
                },
                Err(reason) => ConnectionImportPlanEntry {
                    preview: serde_json::json!({
                        "id": format!("import-item-{}", uuid::Uuid::new_v4()),
                        "sourceLabel": source_label,
                        "name": raw.get("name").and_then(Value::as_str).unwrap_or("未命名连接"),
                        "type": profile_type(raw.get("type").and_then(Value::as_str)),
                        "status": "invalid",
                        "reason": reason,
                    }),
                    input: None,
                },
            }
        })
        .collect())
}

pub async fn create_import_plan(
    app: &AppHandle,
    text: &str,
    source_label: &str,
) -> Result<Value, AppError> {
    if text.len() > 5 * 1024 * 1024 {
        return Err(command_error("导入文件超过 5 MB 限制"));
    }
    let (existing, _) = profile_ops::read_and_heal_profiles(app)?;
    let trimmed = text.trim_start();
    let entries = if trimmed.starts_with('{') || trimmed.starts_with('[') {
        parse_json(text, source_label, &existing)?
    } else {
        parse_ssh_config(text, source_label, &existing)
    };
    let plan_id = format!("connection-import-{}", uuid::Uuid::new_v4());
    let items = entries
        .iter()
        .map(|entry| entry.preview.clone())
        .collect::<Vec<_>>();
    app.state::<crate::services::workspace::WorkspaceState>()
        .connection_import_plans
        .write()
        .await
        .insert(plan_id.clone(), entries);
    Ok(serde_json::json!({ "id": plan_id, "items": items }))
}

pub async fn commit_import_plan(
    app: &AppHandle,
    plan_id: &str,
    selected_ids: &[String],
    strategy: &str,
) -> Result<Value, AppError> {
    if !matches!(strategy, "skip" | "overwrite" | "create") {
        return Err(command_error("导入冲突策略无效"));
    }
    let entries = app
        .state::<crate::services::workspace::WorkspaceState>()
        .connection_import_plans
        .write()
        .await
        .remove(plan_id)
        .ok_or_else(|| command_error("导入计划已过期，请重新选择文件"))?;
    let selected = selected_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let (mut profiles, _) = profile_ops::read_and_heal_profiles(app)?;
    let mut imported = 0_u64;
    let mut overwritten = 0_u64;
    let mut skipped = 0_u64;
    let mut failed = 0_u64;
    let mut results = Vec::new();

    for entry in entries {
        let id = entry
            .preview
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if entry.input.is_none() || !selected.contains(&id) {
            results.push(entry.preview);
            continue;
        }
        let input = entry.input.expect("checked above");
        let conflict = fingerprint(&input).and_then(|needle| {
            profiles
                .iter()
                .find(|profile| fingerprint(profile).as_ref() == Some(&needle))
                .cloned()
        });
        let result: Result<(), AppError> = match (conflict, strategy) {
            (Some(_), "skip") => {
                skipped += 1;
                Ok(())
            }
            (Some(existing), "overwrite") => {
                let profile_id = existing
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| command_error("重复连接缺少 ID"))?;
                let mut merged = existing;
                let target = merged
                    .as_object_mut()
                    .ok_or_else(|| command_error("重复连接格式无效"))?;
                for (key, value) in input.as_object().expect("normalized input object") {
                    target.insert(key.clone(), value.clone());
                }
                profile_ops::update_profile(app, &profile_id, merged)?;
                overwritten += 1;
                Ok(())
            }
            _ => {
                let created = profile_ops::create_profile(app, input)?;
                profiles.push(created);
                imported += 1;
                Ok(())
            }
        };
        if result.is_err() {
            failed += 1;
        }
        results.push(entry.preview);
    }
    if imported > 0 || overwritten > 0 {
        if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    Ok(serde_json::json!({
        "imported": imported, "overwritten": overwritten, "skipped": skipped,
        "failed": failed, "items": results,
    }))
}

pub fn export_bundle(app: &AppHandle, format: &str) -> Result<Vec<u8>, AppError> {
    let (profiles, _) = profile_ops::read_and_heal_profiles(app)?;
    let payload = match format {
        "fileterm" => serde_json::json!({
            "schemaVersion": 1,
            "generatedAt": crate::services::webdav::export_timestamp(),
            "profiles": profiles,
        }),
        "compatible" => Value::Array(
            profiles
                .iter()
                .map(|profile| {
                    serde_json::json!({
                        "id": profile.get("id"), "name": profile.get("name"),
                        "description": profile.get("note"), "conection_type": profile.get("type"),
                        "host": profile.get("host"), "port": profile.get("port"),
                        "user_name": profile.get("username"), "terminal_encoding": profile.get("encoding"),
                        "authentication_type": profile.get("authType"), "password": profile.get("password"),
                        "private_key_path": profile.get("privateKeyPath"), "passphrase": profile.get("passphrase"),
                        "exec_channel_enable": profile.get("enableExecChannel"),
                        "port_forwarding_list": profile.get("forwards"),
                    })
                })
                .collect(),
        ),
        _ => return Err(command_error("导出格式无效")),
    };
    serde_json::to_vec_pretty(&payload).map_err(|error| AppError::Serialization(error.to_string()))
}

pub fn export_filename(name: &str, fallback: &str) -> String {
    let clean = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if clean.trim_matches('_').is_empty() {
        fallback.to_string()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_external_profile, parse_ssh_config};
    use serde_json::json;

    #[test]
    fn parses_ssh_config_and_skips_wildcards() {
        let entries = parse_ssh_config(
            "Host dev\n HostName dev.example\n User ops\n\nHost *\n User default",
            "config",
            &[],
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].preview["name"], "dev");
    }

    #[test]
    fn normalizes_compatible_json() {
        let profile = normalize_external_profile(&json!({"name":"dev","conection_type":"ssh","host":"dev.example","port":22,"user_name":"ops","authentication_type":"key"}), "fallback").unwrap();
        assert_eq!(profile["type"], "ssh");
        assert_eq!(profile["username"], "ops");
        assert_eq!(profile["authType"], "privateKey");
    }
}
