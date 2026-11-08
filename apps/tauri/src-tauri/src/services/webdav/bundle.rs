pub fn export_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    // Howard Hinnant's civil date conversion, with 1970-01-01 as day 0.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_parameter = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_parameter + 2) / 5 + 1;
    let month = month_parameter + if month_parameter < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

/// Serializes the complete connection bundle for an explicit user-initiated
/// backup. S3 backup shares this payload format with WebDAV so both targets
/// preserve the same secret-handling and integrity semantics.
async fn local_backup_snapshot(
    app: &AppHandle,
) -> Result<(Vec<Value>, Vec<Value>, backup_crypto::BackupDependencies), AppError> {
    use tauri::Manager;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _guard = state.library_mutation.lock().await;
    let (profiles, _) = profile_ops::read_and_heal_profiles(app)?;
    let keys = ssh_keys::export_for_backup(app)?;
    let root = workspace_file(app, "profiles.json")?;
    let dependencies = dependencies::export_at(
        root.parent()
            .ok_or_else(|| command_error("无法解析备份目录"))?,
        &profiles,
    )?;
    Ok((profiles, keys, dependencies))
}

pub(crate) async fn export_bundle(
    app: &AppHandle,
    password: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let (profiles, keys, dependencies) = local_backup_snapshot(app).await?;
    build_export_bundle(profiles, keys, dependencies, password)
}

pub(crate) fn build_export_bundle(
    mut profiles: Vec<Value>,
    mut ssh_keys: Vec<Value>,
    dependencies: backup_crypto::BackupDependencies,
    password: &str,
) -> Result<(Vec<u8>, String), AppError> {
    if let Err(error) = validate_profile_key_references(&profiles, &ssh_keys) {
        backup_crypto::zeroize_values(&mut profiles);
        backup_crypto::zeroize_values(&mut ssh_keys);
        return Err(error);
    }
    let (bytes, hash) = backup_crypto::encrypt_bundle(
        profiles,
        ssh_keys,
        dependencies,
        password,
        &export_timestamp(),
    )
    .map_err(|error| command_error(error.to_string()))?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(command_error("加密备份包超过 16 MB 限制，已取消上传。"));
    }
    Ok((bytes, hash))
}

fn validate_profile_key_references(profiles: &[Value], ssh_keys: &[Value]) -> Result<(), AppError> {
    let key_ids = ssh_keys
        .iter()
        .filter_map(|key| key.get("id").and_then(Value::as_str))
        .collect::<std::collections::HashSet<_>>();
    for profile in profiles {
        let Some(key_id) = profile
            .get("privateKeyId")
            .and_then(Value::as_str)
            .filter(|key_id| !key_id.is_empty())
        else {
            continue;
        };
        if !key_ids.contains(key_id) {
            return Err(command_error(format!(
                "连接“{}”引用的 SSH 私钥未包含在本机密钥库中，已取消备份。",
                profile
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("未命名")
            )));
        }
    }
    Ok(())
}

fn profile_fingerprint(profile: &Value) -> Option<(String, String, String, u64, String)> {
    let kind = profile.get("type")?.as_str()?.to_ascii_lowercase();
    let endpoint_field = if kind == "serial" {
        "devicePath"
    } else {
        "host"
    };
    Some((
        kind,
        profile.get("name")?.as_str()?.trim().to_string(),
        profile
            .get(endpoint_field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        profile.get("port").and_then(Value::as_u64).unwrap_or(0),
        profile
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
    ))
}

fn sanitize_import_profile(value: &Value) -> Result<Value, String> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| "配置项不是对象".to_string())?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("ssh")
        .to_ascii_lowercase();
    if !matches!(kind.as_str(), "ssh" | "ftp" | "telnet" | "serial") {
        return Err("不支持的连接类型".to_string());
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("连接名称为空".to_string());
    }
    if kind != "serial" {
        let host = object
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let port = object.get("port").and_then(Value::as_u64).unwrap_or(0);
        if host.is_empty() || !(1..=65535).contains(&port) {
            return Err("主机或端口无效".to_string());
        }
    } else if object
        .get("devicePath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err("串口设备路径为空".to_string());
    }
    object.insert("type".to_string(), Value::String(kind));
    object
        .entry("group".to_string())
        .or_insert_with(|| Value::String("默认".to_string()));
    object
        .entry("remotePath".to_string())
        .or_insert_with(|| Value::String("/".to_string()));
    object
        .entry("username".to_string())
        .or_insert_with(|| Value::String(String::new()));
    for key in ["parentId", "order", "lastUsedAt"] {
        object.remove(key);
    }
    Ok(Value::Object(object))
}

fn merge_synced_profile(existing: &Value, incoming: &Value) -> Result<Value, String> {
    let mut merged = existing
        .as_object()
        .cloned()
        .ok_or_else(|| "本地配置项不是对象".to_string())?;
    let incoming = incoming
        .as_object()
        .ok_or_else(|| "远端配置项不是对象".to_string())?;
    for (key, value) in incoming {
        if !matches!(key.as_str(), "id" | "parentId" | "order" | "lastUsedAt") {
            merged.insert(key.clone(), value.clone());
        }
    }
    profile_ops::normalize_profile_secret_input(&mut merged, Some(existing));
    Ok(Value::Object(merged))
}

/// Merge the local profiles into a decoded remote bundle before uploading it
/// again. Matching connections keep the remote identity while local fields
/// win; connections that exist on only one side are retained.
pub(crate) async fn merge_bundle_with_local(
    app: &AppHandle,
    remote_bytes: &[u8],
    password: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let mut decoded = parse_bundle(remote_bytes, Some(password))?;
    let (local_profiles, local_keys, local_dependencies) = local_backup_snapshot(app).await?;
    let (mut remote, skipped) =
        prepare_import_profiles(decoded.profiles.to_vec(), DownloadMode::OverwriteLocal)?;
    if skipped > 0 {
        return Err(command_error("云端备份包含无效连接，已停止合并"));
    }
    for profile in &mut remote {
        if profile
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            profile["id"] = Value::String(crate::storage::new_id("profile"));
        }
    }
    let (merged, _, _) = merge_profiles(&remote, local_profiles, DownloadMode::MergeLocal)?;
    let keys = ssh_keys::merge_for_upload(decoded.ssh_keys.as_deref(), local_keys)?;
    dependencies::merge(&mut decoded.dependencies, local_dependencies);
    build_export_bundle(
        merged,
        keys,
        std::mem::take(&mut decoded.dependencies),
        password,
    )
}

fn parse_bundle(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<backup_crypto::DecodedBundle, AppError> {
    backup_crypto::decode_bundle(bytes, password).map_err(|error| command_error(error.to_string()))
}

fn prepare_import_profiles(
    incoming: Vec<Value>,
    mode: DownloadMode,
) -> Result<(Vec<Value>, u64), AppError> {
    let mut profiles = Vec::with_capacity(incoming.len());
    let mut skipped = 0;
    for profile in incoming {
        match sanitize_import_profile(&profile) {
            Ok(profile) => profiles.push(profile),
            Err(_) => skipped += 1,
        }
    }
    // A partial restore must never silently replace a complete local library.
    if mode == DownloadMode::OverwriteLocal && skipped > 0 {
        return Err(command_error(format!(
            "备份包含 {skipped} 个无效连接，已取消覆盖本地配置；请修复备份或使用合并模式"
        )));
    }
    Ok((profiles, skipped))
}

pub(crate) struct ProfileImportSummary {
    pub imported: u64,
    pub updated: u64,
    pub replaced: u64,
    pub skipped: u64,
    pub ssh_keys_imported: u64,
    pub ssh_keys_updated: u64,
    pub legacy_plaintext: bool,
}

/// Imports a verified FileTerm profile bundle. Transport services call this
/// only after their own authenticated download has completed, which keeps the
/// merge and secret persistence rules identical across WebDAV and S3.
pub(crate) async fn import_bundle(
    app: &AppHandle,
    bytes: &[u8],
    password: Option<&str>,
    mode: DownloadMode,
) -> Result<ProfileImportSummary, AppError> {
    use tauri::Manager;
    let decoded = parse_bundle(bytes, password)?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _guard = state.library_mutation.lock().await;
    let (existing, _) = profile_ops::read_and_heal_profiles(app)?;
    let (mut profiles, skipped) = prepare_import_profiles(decoded.profiles.to_vec(), mode)?;
    if let Some(keys) = decoded.ssh_keys.as_deref() {
        validate_profile_key_references(&profiles, keys)?;
    }
    for profile in &profiles {
        if let Some(profile) = profile.as_object() {
            crate::services::totp::validate_profile(profile)?;
        }
    }
    let mut dependencies = dependencies::restore(app, &decoded.dependencies, &mut profiles)?;
    let (merged, imported, updated) = merge_profiles(&existing, profiles, mode)?;
    let persist = || profile_ops::replace_profiles(app, merged);
    let key_summary = if let Some(keys) = decoded.ssh_keys.as_deref() {
        ssh_keys::restore_for_backup(app, keys, persist)?
    } else {
        persist()?;
        ssh_keys::SshKeyRestoreSummary::default()
    };
    dependencies.commit();
    Ok(ProfileImportSummary {
        imported,
        updated,
        replaced: if mode == DownloadMode::OverwriteLocal {
            existing.len() as u64
        } else {
            0
        },
        skipped,
        ssh_keys_imported: key_summary.imported,
        ssh_keys_updated: key_summary.updated,
        legacy_plaintext: decoded.legacy_plaintext,
    })
}
