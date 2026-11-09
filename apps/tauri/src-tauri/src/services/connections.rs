//! Connection import/export codecs shared by the Tauri command layer.
//! Parsing happens entirely in the main process so the confirmation UI only
//! receives non-secret preview metadata; the selected plan stays local until
//! the user commits it.

use serde_json::Value;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager};

use crate::services::profile_ops;
use crate::services::workspace::ConnectionImportPlanEntry;
use crate::AppError;

fn command_error(message: impl Into<String>) -> AppError {
    AppError::Command(message.into())
}

fn profile_type(value: Option<&str>) -> String {
    let value = value.unwrap_or("ssh").to_ascii_lowercase();
    if value.contains("sftp") {
        "sftp".to_string()
    } else if value.contains("ftp") {
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

fn normalize_host(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed[1..trimmed.len() - 1].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn valid_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let host = normalize_host(raw);
    if host.is_empty()
        || trimmed
            .contains(|c: char| c.is_whitespace() || matches!(c, '/' | '@' | '?' | '#' | '\\'))
        || trimmed.contains("://")
        || (trimmed.starts_with('[') != trimmed.ends_with(']'))
        || host.contains(['(', ')', '[', ']', '{', '}'])
    {
        return None;
    }
    if host.contains(':') {
        let (address, zone) = host.split_once('%').unwrap_or((&host, ""));
        if Ipv6Addr::from_str(address).is_err()
            || (!zone.is_empty()
                && (host.matches('%').count() != 1
                    || !zone
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))))
        {
            return None;
        }
    } else if host.chars().all(|c| c.is_ascii_digit() || c == '.')
        && Ipv4Addr::from_str(&host).is_err()
    {
        return None;
    }
    Some(host)
}

fn expand_home(value: Option<&Value>) -> Option<Value> {
    let path = value?.as_str()?;
    let relative = path.strip_prefix("~/")?;
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    Some(Value::String(
        Path::new(&home)
            .join(relative)
            .to_string_lossy()
            .into_owned(),
    ))
}

fn map_auth(value: Option<&Value>) -> &'static str {
    let value = value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if value.contains("kubernetes") || value.contains("k8s") {
        "kubernetes"
    } else if (value.contains("jumpserver") || value.contains("koko")) && value.contains("mfa") {
        "jumpserver-koko-mfa"
    } else if value.contains("interactive") {
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
    let host = normalize_host(&safe_string(source.get("host")));
    let port = source
        .get("port")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| default_port(&kind));
    if kind != "serial" && (valid_host(&host).is_none() || !(1..=65535).contains(&port)) {
        return Err("缺少有效 Host 或 Port".to_string());
    }
    if kind == "serial" && safe_string(source.get("devicePath")).is_empty() {
        return Err("缺少串口设备路径".to_string());
    }

    let mut profile = Value::Object(source.clone());
    let object = profile
        .as_object_mut()
        .ok_or_else(|| "配置项不是对象".to_string())?;
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
                .unwrap_or(if kind == "ssh" || kind == "sftp" {
                    "."
                } else {
                    "/"
                })
                .to_string(),
        ),
    );
    if kind == "ssh" || kind == "sftp" {
        let auth = source
            .get("authType")
            .or_else(|| source.get("authentication_type"));
        // External exporters may put either the canonical value or the
        // localized/display label in `authType`. Always map it at the import
        // boundary so the runtime only receives the internal enum value.
        object.insert(
            "authType".to_string(),
            Value::String(map_auth(auth).to_string()),
        );
    }
    if let Some(value) = source.get("terminal_encoding") {
        object
            .entry("encoding".to_string())
            .or_insert_with(|| value.clone());
    }
    if let Some(private_key_path) = expand_home(
        source
            .get("private_key_path")
            .or_else(|| source.get("privateKeyPath")),
    ) {
        object.insert("privateKeyPath".to_string(), private_key_path);
    }
    Ok(profile)
}

fn fingerprint(profile: &Value) -> Option<(String, String, u64, String)> {
    Some((
        profile.get("type")?.as_str()?.to_ascii_lowercase(),
        profile
            .get("host")
            .and_then(Value::as_str)
            .map(normalize_host)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        profile
            .get("port")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        profile
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
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
            block.insert("hostalias".to_string(), value.to_string());
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
            let alias = block.get("hostalias").cloned().unwrap_or_else(|| "SSH".to_string());
            let host = normalize_host(&block.get("hostname").cloned().unwrap_or_default());
            let port = block.get("port").and_then(|value| value.parse::<u64>().ok()).unwrap_or(22);
            let unsupported_fields = block.keys().filter(|key| !matches!(key.as_str(), "hostalias" | "host" | "hostname" | "user" | "port" | "identityfile" | "proxyjump")).cloned().collect::<Vec<_>>();
            let item = if valid_host(&host).is_none() || !(1..=65535).contains(&port) {
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
                if !unsupported_fields.is_empty() {
                    preview["unsupportedFields"] = serde_json::json!(unsupported_fields);
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
    if text.len() > 2 * 1024 * 1024 {
        return Err(command_error("导入文件超过 2 MB 限制"));
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

fn supported_import_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("json" | "config" | "txt")
    ) || path.file_name().and_then(|name| name.to_str()) == Some("config")
}

const IMPORT_FILE_LIMIT: usize = 500;
/// Maximum directory depth for import scans. Acts as a belt-and-suspenders
/// guard against pathological layouts that aren't symlink loops (e.g. a
/// deeply nested `node_modules` tree) so the recursion can't overflow the
/// stack even when the file-count limit hasn't been hit yet.
const IMPORT_RECURSION_DEPTH: usize = 32;

fn collect_import_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    collect_import_files_with_depth(path, files, 0)
}

fn collect_import_files_with_depth(
    path: &Path,
    files: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if files.len() >= IMPORT_FILE_LIMIT || depth > IMPORT_RECURSION_DEPTH {
        return Ok(());
    }
    // Top-level path follows symlinks (the user explicitly selected it). Deeper
    // entries use `symlink_metadata` so symlinked directories are skipped
    // instead of followed: a loop would otherwise overflow the stack and a
    // symlinked escape could pull in files the user never selected.
    let metadata = if depth == 0 {
        std::fs::metadata(path).map_err(|error| error.to_string())?
    } else {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if metadata.is_symlink() {
            return Ok(());
        }
        metadata
    };
    if metadata.is_file() {
        if supported_import_file(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_name().to_string_lossy().starts_with('.') {
                collect_import_files_with_depth(&entry.path(), files, depth + 1)?;
            }

            if files.len() >= IMPORT_FILE_LIMIT {
                break;
            }
        }
    }
    Ok(())
}

pub async fn create_import_plan_from_paths(
    app: &AppHandle,
    paths: Vec<PathBuf>,
) -> Result<Value, AppError> {
    let files = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        for path in paths {
            collect_import_files(&path, &mut files)?;
        }
        Ok::<_, String>(files)
    })
    .await
    .map_err(|error| command_error(error.to_string()))?
    .map_err(command_error)?;
    let (existing, _) = profile_ops::read_and_heal_profiles(app)?;
    let mut entries = Vec::new();
    for path in files {
        let source_label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("连接文件")
            .to_string();
        match tokio::fs::metadata(&path).await {
            Ok(metadata) if metadata.len() > 2 * 1024 * 1024 => entries.push(ConnectionImportPlanEntry { preview: serde_json::json!({"id": format!("import-item-{}", uuid::Uuid::new_v4()), "sourceLabel": source_label, "name": path.file_name().and_then(|name| name.to_str()).unwrap_or("连接文件"), "type": "ssh", "status": "invalid", "reason": "导入文件超过 2 MB 限制"}), input: None }),
            Ok(_) => match tokio::fs::read_to_string(&path).await { Ok(text) => { let parsed = if path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| extension.eq_ignore_ascii_case("json")) { parse_json(&text, &source_label, &existing) } else { Ok(parse_ssh_config(&text, &source_label, &existing)) }; entries.extend(parsed?); }, Err(error) => entries.push(ConnectionImportPlanEntry { preview: serde_json::json!({"id": format!("import-item-{}", uuid::Uuid::new_v4()), "sourceLabel": source_label, "name": path.file_name().and_then(|name| name.to_str()).unwrap_or("连接文件"), "type": "ssh", "status": "invalid", "reason": error.to_string()}), input: None }) },
            Err(error) => entries.push(ConnectionImportPlanEntry { preview: serde_json::json!({"id": format!("import-item-{}", uuid::Uuid::new_v4()), "sourceLabel": source_label, "name": path.file_name().and_then(|name| name.to_str()).unwrap_or("连接文件"), "type": "ssh", "status": "invalid", "reason": error.to_string()}), input: None }),
        }
    }
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

    for mut entry in entries {
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
        let input = entry
            .input
            .take()
            .ok_or_else(|| command_error("导入条目缺少输入数据"))?;
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
                for (key, value) in input
                    .as_object()
                    .ok_or_else(|| command_error("导入条目格式无效"))?
                {
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
        if let Ok(snapshot) = crate::commands::get_workspace_snapshot_unlocked(app.clone()).await {
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
    let payload = build_export_payload(&profiles, format)?;
    serde_json::to_vec_pretty(&payload).map_err(|error| AppError::Serialization(error.to_string()))
}

pub fn build_compatible_profile_payload(profile: &Value) -> Value {
    serde_json::json!({
        "id": profile.get("id"), "name": profile.get("name"),
        "description": profile.get("note"), "conection_type": profile.get("type"),
        "host": profile.get("host"), "port": profile.get("port"),
        "user_name": profile.get("username"), "terminal_encoding": profile.get("encoding"),
        "authentication_type": profile.get("authType"), "password": profile.get("password"),
        "private_key_path": profile.get("privateKeyPath"), "passphrase": profile.get("passphrase"),
        "deviceMode": profile.get("deviceMode"), "terminalType": profile.get("terminalType"),
        "networkDeviceVendor": profile.get("networkDeviceVendor"),
        "exec_channel_enable": profile.get("enableExecChannel"),
        "port_forwarding_list": profile.get("forwards"),
    })
}

fn build_export_payload(profiles: &[Value], format: &str) -> Result<Value, AppError> {
    let payload = match format {
        "fileterm" => serde_json::json!({
            "schemaVersion": 2,
            "containsSecrets": true,
            "generatedAt": crate::services::webdav::export_timestamp(),
            "profiles": profiles,
        }),
        "compatible" => Value::Array(
            profiles
                .iter()
                .map(build_compatible_profile_payload)
                .collect(),
        ),
        _ => return Err(command_error("导出格式无效")),
    };
    Ok(payload)
}

pub fn export_filename(
    name: &str,
    id: &str,
    used_names: &mut std::collections::HashSet<String>,
) -> String {
    let mut normalized = name
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'..='\u{1f}'
            ) {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    normalized = normalized
        .trim()
        .trim_end_matches(['.', ' '])
        .chars()
        .take(100)
        .collect();
    if normalized.is_empty() {
        normalized = "connection".to_string();
    }
    let reserved = matches!(
        normalized.to_ascii_lowercase().as_str(),
        "con" | "prn" | "aux" | "nul"
    ) || (normalized.len() == 4
        && (normalized[..3].eq_ignore_ascii_case("com")
            || normalized[..3].eq_ignore_ascii_case("lpt"))
        && matches!(normalized.as_bytes()[3], b'1'..=b'9'));
    let stem = if reserved {
        format!("connection-{normalized}")
    } else {
        normalized
    };
    let mut candidate = stem.clone();
    let mut counter = 2;
    while used_names.contains(&candidate.to_ascii_lowercase()) {
        candidate = format!("{stem}-{}", counter);
        counter += 1;
    }
    used_names.insert(candidate.to_ascii_lowercase());
    format!("{}-{}", candidate, &id[..id.len().min(8)])
}

#[cfg(test)]
mod tests;
