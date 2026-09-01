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
pub(crate) fn export_bundle(
    app: &AppHandle,
    password: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let (profiles, _) = profile_ops::read_and_heal_profiles(app)?;
    build_export_bundle(&profiles, password)
}

pub(crate) fn build_export_bundle(
    profiles: &[Value],
    password: &str,
) -> Result<(Vec<u8>, String), AppError> {
    backup_crypto::encrypt_profiles(profiles, password, &export_timestamp())
        .map_err(|error| command_error(error.to_string()))
}

fn profile_fingerprint(profile: &Value) -> Option<(String, String, String, u64, String)> {
    Some((
        profile.get("type")?.as_str()?.to_ascii_lowercase(),
        profile.get("name")?.as_str()?.trim().to_string(),
        profile
            .get("host")
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
    for key in ["id", "parentId", "order", "lastUsedAt"] {
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
    Ok(Value::Object(merged))
}

/// Merge the local profiles into a decoded remote bundle before uploading it
/// again. Matching connections keep the remote identity while local fields
/// win; connections that exist on only one side are retained.
pub(crate) fn merge_bundle_with_local(
    app: &AppHandle,
    remote_bytes: &[u8],
    password: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let decoded = parse_bundle(remote_bytes, Some(password))?;
    let (local_profiles, _) = profile_ops::read_and_heal_profiles(app)?;
    let mut merged = Vec::new();
    let mut positions = std::collections::HashMap::new();

    for profile in decoded.profiles {
        let Ok(sanitized) = sanitize_import_profile(&profile) else {
            continue;
        };
        let Some(fingerprint) = profile_fingerprint(&sanitized) else {
            continue;
        };
        if positions.contains_key(&fingerprint) {
            continue;
        }
        positions.insert(fingerprint, merged.len());
        merged.push(sanitized);
    }

    for profile in local_profiles {
        let sanitized = sanitize_import_profile(&profile)
            .map_err(|error| command_error(format!("本地连接配置无效: {error}")))?;
        let fingerprint = profile_fingerprint(&sanitized)
            .ok_or_else(|| command_error("本地连接配置缺少有效的连接标识"))?;
        if let Some(index) = positions.get(&fingerprint).copied() {
            merged[index] = merge_synced_profile(&merged[index], &sanitized)
                .map_err(|error| command_error(format!("合并连接配置失败: {error}")))?;
        } else {
            positions.insert(fingerprint, merged.len());
            merged.push(sanitized);
        }
    }

    build_export_bundle(&merged, password)
}

fn parse_bundle(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<backup_crypto::DecodedBundle, AppError> {
    backup_crypto::decode_bundle(bytes, password).map_err(|error| command_error(error.to_string()))
}

pub(crate) struct ProfileImportSummary {
    pub imported: u64,
    pub updated: u64,
    pub replaced: u64,
    pub skipped: u64,
    pub legacy_plaintext: bool,
}

/// Imports a verified FileTerm profile bundle. Transport services call this
/// only after their own authenticated download has completed, which keeps the
/// merge and secret persistence rules identical across WebDAV and S3.
pub(crate) fn import_bundle(
    app: &AppHandle,
    bytes: &[u8],
    password: Option<&str>,
    mode: DownloadMode,
) -> Result<ProfileImportSummary, AppError> {
    let decoded = parse_bundle(bytes, password)?;
    let (existing, _) = profile_ops::read_and_heal_profiles(app)?;
    let mut profiles = Vec::with_capacity(decoded.profiles.len());
    let mut skipped = 0_u64;
    for profile in decoded.profiles {
        match sanitize_import_profile(&profile) {
            Ok(profile) => profiles.push(profile),
            Err(_) => skipped += 1,
        }
    }

    if mode == DownloadMode::OverwriteLocal {
        let replaced = existing.len() as u64;
        let imported = profiles.len() as u64;
        let replacement_profiles = profiles
            .into_iter()
            .map(|mut profile| {
                if let Some(fingerprint) = profile_fingerprint(&profile) {
                    if let Some(existing_profile) = existing.iter().find(|existing_profile| {
                        profile_fingerprint(existing_profile).as_ref() == Some(&fingerprint)
                    }) {
                        if let Some(object) = profile.as_object_mut() {
                            for key in ["id", "order", "lastUsedAt"] {
                                if let Some(value) = existing_profile.get(key) {
                                    object.insert(key.to_string(), value.clone());
                                }
                            }
                        }
                    }
                }
                profile
            })
            .collect();
        profile_ops::replace_profiles(app, replacement_profiles)?;
        return Ok(ProfileImportSummary {
            imported,
            updated: 0,
            replaced,
            skipped,
            legacy_plaintext: decoded.legacy_plaintext,
        });
    }

    let mut known = existing
        .iter()
        .filter_map(|profile| {
            profile_fingerprint(profile).map(|fingerprint| (fingerprint, profile.clone()))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut imported = 0_u64;
    let mut updated = 0_u64;
    for profile in profiles {
        let Some(fingerprint) = profile_fingerprint(&profile) else {
            skipped += 1;
            continue;
        };
        if let Some(existing_profile) = known.get(&fingerprint).cloned() {
            let Some(profile_id) = existing_profile
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
            else {
                skipped += 1;
                continue;
            };
            let merged = match merge_synced_profile(&existing_profile, &profile) {
                Ok(profile) => profile,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let saved = profile_ops::update_profile(app, &profile_id, merged)?;
            known.insert(fingerprint, saved);
            updated += 1;
            continue;
        }
        let created = profile_ops::create_profile(app, profile)?;
        known.insert(fingerprint, created);
        imported += 1;
    }
    Ok(ProfileImportSummary {
        imported,
        updated,
        replaced: 0,
        skipped,
        legacy_plaintext: decoded.legacy_plaintext,
    })
}
