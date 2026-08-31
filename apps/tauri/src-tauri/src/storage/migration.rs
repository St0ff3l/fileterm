/// Import Electron's established `FileTerm` user-data store exactly once.
///
/// Tauri owns an independent app-data directory. The completed marker is the
/// boundary that prevents a later delete or clear in Tauri from being undone
/// by another read of Electron's still-live store.
pub fn migrate_legacy_data_once(app: &AppHandle) -> Result<(), AppError> {
    let current_dir = storage_root(app)?;
    crate::services::logging::info(
        app,
        "storage",
        format!(
            "data migration started root={} portable={} compiled_portable={}",
            current_dir.display(),
            portable_config_directory().is_some(),
            is_compiled_portable_build()
        ),
    );
    #[cfg(target_os = "windows")]
    if portable_config_directory().is_some() {
        let portable_marker_exists = std::env::current_exe()
            .ok()
            .and_then(|executable| portable_marker_path_for_executable(&executable))
            .map(|path| path.is_file())
            .unwrap_or(false);
        if portable_marker_exists {
            // The parent marker is intentionally independent from config/.
            // If a user clears portable data, neither the old Tauri store nor
            // the historical Electron store should be copied back on the
            // next launch.
            crate::services::logging::info(
                app,
                "storage",
                format!(
                    "portable migration skipped because the executable marker exists root={}",
                    current_dir.display()
                ),
            );
            return Ok(());
        }
        let result = migrate_portable_data_once(app, &current_dir);
        match &result {
            Ok(()) => crate::services::logging::info(
                app,
                "storage",
                format!(
                    "portable data migration completed root={}",
                    current_dir.display()
                ),
            ),
            Err(error) => crate::services::logging::error(
                app,
                "storage",
                format!(
                    "portable data migration failed root={} error={error}",
                    current_dir.display()
                ),
            ),
        }
        result?;
    }
    let config_dir = app.path().app_config_dir().ok();
    let data_dir = app.path().app_data_dir().ok();
    let legacy_dir =
        select_legacy_directory(&current_dir, config_dir.as_deref(), data_dir.as_deref())?;
    crate::services::logging::info(
        app,
        "storage",
        format!(
            "legacy migration source selected root={} source={}",
            current_dir.display(),
            legacy_dir.display()
        ),
    );
    match migrate_legacy_store(&current_dir, &legacy_dir) {
        Ok(report) => {
            crate::services::logging::info(
                app,
                "storage",
                format!(
                    "legacy migration completed root={} source_files={} migrated_files={} kept_current_files={}",
                    current_dir.display(),
                    report.source_files.len(),
                    report.migrated_files.len(),
                    report.kept_current_files.len()
                ),
            );
            Ok(())
        }
        Err(error) => {
            crate::services::logging::error(
                app,
                "storage",
                format!(
                    "legacy migration failed root={} source={} error={error}",
                    current_dir.display(),
                    legacy_dir.display()
                ),
            );
            Err(error)
        }
    }
}

#[cfg(any(test, target_os = "windows"))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortableMigrationReport {
    version: u32,
    status: String,
    completed_at: u64,
    source_directory: Option<String>,
    copied_files: Vec<String>,
}

#[cfg(target_os = "windows")]
fn migrate_portable_data_once(app: &AppHandle, current_dir: &Path) -> Result<(), AppError> {
    let source_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Storage(error.to_string()))?;
    crate::services::logging::debug(
        app,
        "storage",
        format!(
            "portable migration source={} target={}",
            source_dir.display(),
            current_dir.display()
        ),
    );
    migrate_portable_data_from_source(current_dir, &source_dir)
}

#[cfg(any(test, target_os = "windows"))]
fn migrate_portable_data_from_source(
    current_dir: &Path,
    source_dir: &Path,
) -> Result<(), AppError> {
    fs::create_dir_all(current_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    lock_down_directory(current_dir)?;

    let marker_path = current_dir.join(PORTABLE_MIGRATION_MARKER);
    if marker_path.exists() {
        let report: PortableMigrationReport = read_json_file(&marker_path)?;
        if report.version >= PORTABLE_MIGRATION_VERSION
            && matches!(
                report.status.as_str(),
                "completed" | "existing" | "no-source"
            )
        {
            return Ok(());
        }
        return Err(AppError::Storage(
            "便携版数据迁移标记无效，拒绝重复复制用户数据".to_string(),
        ));
    }

    if directory_has_entries(current_dir)? {
        return write_portable_migration_marker(current_dir, "existing", None, Vec::new());
    }

    if source_dir == current_dir || !source_dir.is_dir() {
        return write_portable_migration_marker(
            current_dir,
            "no-source",
            Some(source_dir),
            Vec::new(),
        );
    }

    let transaction_dir = current_dir.join(format!(".portable-migration-{}", uuid::Uuid::new_v4()));
    let staged_dir = transaction_dir.join("staged");
    let backup_dir = transaction_dir.join("backup");
    fs::create_dir_all(&staged_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    fs::create_dir_all(&backup_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    lock_down_directory(&transaction_dir)?;

    let result = (|| {
        let mut pending = Vec::new();
        let mut copied_files = Vec::new();
        for (relative, confidential) in PORTABLE_DATA_ENTRIES {
            stage_portable_entry(
                current_dir,
                &staged_dir,
                &backup_dir,
                Path::new(relative),
                &source_dir.join(relative),
                *confidential,
                &mut pending,
                &mut copied_files,
            )?;
        }

        if copied_files.is_empty() {
            return Ok(None);
        }

        copied_files.sort();
        let report = PortableMigrationReport {
            version: PORTABLE_MIGRATION_VERSION,
            status: "completed".to_string(),
            completed_at: now_millis(),
            source_directory: Some(source_dir.to_string_lossy().into_owned()),
            copied_files,
        };
        let report_value = serde_json::to_value(&report)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        stage_json_file(
            current_dir,
            &staged_dir,
            &backup_dir,
            PORTABLE_MIGRATION_MARKER,
            &report_value,
            false,
            &mut pending,
        )?;
        commit_pending_files(&pending)?;
        Ok(Some(()))
    })();

    let cleanup_result = fs::remove_dir_all(&transaction_dir);
    match (result, cleanup_result) {
        (Ok(Some(())), Ok(())) => Ok(()),
        (Ok(Some(())), Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(Some(())), Err(error)) => Err(AppError::Storage(format!(
            "便携版数据迁移成功，但无法删除事务目录: {error}"
        ))),
        (Ok(None), _) => {
            write_portable_migration_marker(current_dir, "no-source", Some(source_dir), Vec::new())
        }
        (Err(error), _) => Err(error),
    }
}

#[cfg(any(test, target_os = "windows"))]
fn directory_has_entries(directory: &Path) -> Result<bool, AppError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };

    for entry in entries {
        let entry = entry.map_err(|error| AppError::Storage(error.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == PORTABLE_MIGRATION_MARKER
            || name == "mcp-runtime.json"
            || name.starts_with(".portable-migration-")
            || name.starts_with(".portable-migration.json.")
        {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

#[cfg(any(test, target_os = "windows"))]
#[allow(clippy::too_many_arguments)]
fn stage_portable_entry(
    current_dir: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    relative: &Path,
    source: &Path,
    confidential: bool,
    pending: &mut Vec<PendingFile>,
    copied_files: &mut Vec<String>,
) -> Result<(), AppError> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };
    if metadata.file_type().is_symlink() {
        return Err(AppError::Storage(format!(
            "便携版数据迁移拒绝复制符号链接: {}",
            source.display()
        )));
    }
    if metadata.is_file() {
        stage_file_copy(
            current_dir,
            staged_dir,
            backup_dir,
            relative,
            source,
            confidential,
            pending,
        )?;
        copied_files.push(relative.to_string_lossy().into_owned());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(source).map_err(|error| AppError::Storage(error.to_string()))? {
        let entry = entry.map_err(|error| AppError::Storage(error.to_string()))?;
        let child_relative = relative.join(entry.file_name());
        stage_portable_entry(
            current_dir,
            staged_dir,
            backup_dir,
            &child_relative,
            &entry.path(),
            confidential,
            pending,
            copied_files,
        )?;
    }
    Ok(())
}

#[cfg(any(test, target_os = "windows"))]
fn write_portable_migration_marker(
    current_dir: &Path,
    status: &str,
    source_directory: Option<&Path>,
    copied_files: Vec<String>,
) -> Result<(), AppError> {
    let marker = PortableMigrationReport {
        version: PORTABLE_MIGRATION_VERSION,
        status: status.to_string(),
        completed_at: now_millis(),
        source_directory: source_directory.map(|path| path.to_string_lossy().into_owned()),
        copied_files,
    };
    let bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let target = current_dir.join(PORTABLE_MIGRATION_MARKER);
    let temporary = target.with_file_name(format!(
        ".{PORTABLE_MIGRATION_MARKER}.{}.tmp",
        uuid::Uuid::new_v4()
    ));
    write_restricted_file(&temporary, &bytes)?;
    if let Err(error) = replace_file_atomically(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn select_legacy_directory(
    current_dir: &Path,
    config_dir: Option<&Path>,
    data_dir: Option<&Path>,
) -> Result<PathBuf, AppError> {
    let mut candidates = Vec::new();
    if let Some(parent) = current_dir.parent() {
        candidates.push(parent.join("FileTerm"));
    }
    if let Some(parent) = config_dir.and_then(Path::parent) {
        let candidate = parent.join("FileTerm");
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    if let Some(parent) = data_dir.and_then(Path::parent) {
        let candidate = parent.join("FileTerm");
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
        .iter()
        .filter(|candidate| candidate.is_dir())
        .max_by_key(|candidate| legacy_directory_score(candidate))
        .cloned()
        .or_else(|| candidates.into_iter().next())
        .ok_or_else(|| AppError::Storage("无法解析 Electron 用户数据目录".to_string()))
}

fn legacy_directory_score(directory: &Path) -> usize {
    LEGACY_JSON_STORES
        .iter()
        .filter(|store| directory.join(store.name).is_file())
        .count()
        + ["ssh-keys.json", "ssh-key-secrets.json"]
            .iter()
            .filter(|name| directory.join(name).is_file())
            .count()
}

fn migrate_legacy_store(
    current_dir: &Path,
    legacy_dir: &Path,
) -> Result<LegacyMigrationReport, AppError> {
    fs::create_dir_all(current_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    lock_down_directory(current_dir)?;

    let marker_path = current_dir.join(LEGACY_MIGRATION_MARKER);
    if marker_path.exists() {
        let report: LegacyMigrationReport = read_json_file(&marker_path)?;
        if report.version >= LEGACY_MIGRATION_VERSION && report.status == "completed" {
            return Ok(report);
        }
        return Err(AppError::Storage(
            "旧数据迁移标记无效，拒绝重复合并 Electron 数据".to_string(),
        ));
    }

    let transaction_dir = current_dir.join(format!(
        ".legacy-fileterm-migration-{}",
        uuid::Uuid::new_v4()
    ));
    let staged_dir = transaction_dir.join("staged");
    let backup_dir = transaction_dir.join("backup");
    fs::create_dir_all(&staged_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    fs::create_dir_all(&backup_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    lock_down_directory(&transaction_dir)?;

    let result = (|| {
        let mut pending = Vec::new();
        let mut source_files = Vec::new();
        let mut migrated_files = Vec::new();
        let mut kept_current_files = Vec::new();
        let mut profile_ids = read_optional_json_file(&current_dir.join("profiles.json"))?
            .map(|profiles| value_ids(&profiles))
            .unwrap_or_default();

        if legacy_dir.is_dir() && legacy_dir != current_dir {
            for store in LEGACY_JSON_STORES {
                let source = legacy_dir.join(store.name);
                if !source.is_file() {
                    continue;
                }
                source_files.push(source_snapshot(store.name, &source)?);
                let target = current_dir.join(store.name);
                let current = read_optional_json_file(&target)?;

                if matches!(store.mode, JsonMergeMode::CurrentFileWins) && current.is_some() {
                    kept_current_files.push(store.name.to_string());
                    continue;
                }

                let legacy: Value = read_json_file(&source)?;
                let mut merged = merge_json_values(store.mode, current.clone(), legacy)?;
                if store.name == "profiles.json" {
                    profile_ids = value_ids(&merged);
                } else if store.name == "profile-secrets.json" {
                    retain_nested_keys(&mut merged, "profiles", &profile_ids)?;
                }

                if current.as_ref() == Some(&merged) {
                    kept_current_files.push(store.name.to_string());
                    continue;
                }
                stage_json_file(
                    current_dir,
                    &staged_dir,
                    &backup_dir,
                    store.name,
                    &merged,
                    store.confidential,
                    &mut pending,
                )?;
                migrated_files.push(store.name.to_string());
            }

            let ssh_context = stage_legacy_ssh_keys(
                current_dir,
                legacy_dir,
                &staged_dir,
                &backup_dir,
                &mut pending,
                &mut source_files,
                &mut migrated_files,
                &mut kept_current_files,
            )?;
            stage_legacy_ssh_key_secrets(
                current_dir,
                legacy_dir,
                &staged_dir,
                &backup_dir,
                ssh_context,
                &mut pending,
                &mut source_files,
                &mut migrated_files,
                &mut kept_current_files,
            )?;
        }

        source_files.sort_by(|left, right| left.name.cmp(&right.name));
        migrated_files.sort();
        migrated_files.dedup();
        kept_current_files.sort();
        kept_current_files.dedup();
        let report = LegacyMigrationReport {
            version: LEGACY_MIGRATION_VERSION,
            status: "completed".to_string(),
            completed_at: now_millis(),
            source_directory: legacy_dir.to_string_lossy().into_owned(),
            conflict_policy: "Tauri/current values win matching keys and IDs; Electron contributes only missing records once".to_string(),
            source_files,
            migrated_files,
            kept_current_files,
            rollback_performed: false,
        };
        stage_json_file(
            current_dir,
            &staged_dir,
            &backup_dir,
            LEGACY_MIGRATION_MARKER,
            &serde_json::to_value(&report)
                .map_err(|error| AppError::Serialization(error.to_string()))?,
            false,
            &mut pending,
        )?;
        commit_pending_files(&pending)?;
        Ok(report)
    })();

    let cleanup_result = fs::remove_dir_all(&transaction_dir);
    match (result, cleanup_result) {
        (Ok(report), Ok(())) => Ok(report),
        (Ok(report), Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(report),
        (Ok(_), Err(error)) => Err(AppError::Storage(format!(
            "旧数据迁移成功，但无法删除受限事务目录: {error}"
        ))),
        (Err(error), _) => Err(error),
    }
}

fn source_snapshot(name: &str, path: &Path) -> Result<LegacySourceSnapshot, AppError> {
    let metadata = fs::metadata(path).map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(LegacySourceSnapshot {
        name: name.to_string(),
        bytes: metadata.len(),
    })
}

fn merge_json_values(
    mode: JsonMergeMode,
    current: Option<Value>,
    legacy: Value,
) -> Result<Value, AppError> {
    match mode {
        JsonMergeMode::ArrayById => {
            let mut values = match current {
                Some(Value::Array(values)) => values,
                Some(_) => return Err(invalid_store_shape("current", "array")),
                None => Vec::new(),
            };
            let Value::Array(legacy_values) = legacy else {
                return Err(invalid_store_shape("Electron", "array"));
            };
            let mut known_ids = value_ids(&Value::Array(values.clone()));
            for value in legacy_values {
                let id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                if id.as_ref().is_some_and(|id| !known_ids.insert(id.clone())) {
                    continue;
                }
                values.push(value);
            }
            Ok(Value::Array(values))
        }
        JsonMergeMode::ObjectCurrentWins => {
            let mut object = object_or_empty(current, "current")?;
            let Value::Object(legacy_object) = legacy else {
                return Err(invalid_store_shape("Electron", "object"));
            };
            for (key, value) in legacy_object {
                object.entry(key).or_insert(value);
            }
            Ok(Value::Object(object))
        }
        JsonMergeMode::NestedObjectCurrentWins(nested_key) => {
            let mut object = object_or_empty(current, "current")?;
            let Value::Object(mut legacy_object) = legacy else {
                return Err(invalid_store_shape("Electron", "object"));
            };
            let legacy_nested =
                object_value_or_empty(legacy_object.remove(nested_key), "Electron")?;
            let mut current_nested = object_value_or_empty(object.remove(nested_key), "current")?;
            for (key, value) in legacy_nested {
                current_nested.entry(key).or_insert(value);
            }
            for (key, value) in legacy_object {
                object.entry(key).or_insert(value);
            }
            object.insert(nested_key.to_string(), Value::Object(current_nested));
            Ok(Value::Object(object))
        }
        JsonMergeMode::CurrentFileWins => Ok(current.unwrap_or(legacy)),
    }
}

fn object_or_empty(value: Option<Value>, label: &str) -> Result<Map<String, Value>, AppError> {
    object_value_or_empty(value, label)
}

fn object_value_or_empty(
    value: Option<Value>,
    label: &str,
) -> Result<Map<String, Value>, AppError> {
    match value {
        Some(Value::Object(object)) => Ok(object),
        Some(_) => Err(invalid_store_shape(label, "object")),
        None => Ok(Map::new()),
    }
}

fn invalid_store_shape(label: &str, expected: &str) -> AppError {
    AppError::Serialization(format!("{label} 旧数据迁移源应为 JSON {expected}"))
}

fn value_ids(value: &Value) -> HashSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.get("id").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

fn retain_nested_keys(
    value: &mut Value,
    nested_key: &str,
    allowed: &HashSet<String>,
) -> Result<(), AppError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid_store_shape("merged", "object"))?;
    let nested = object
        .get_mut(nested_key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_store_shape("merged nested", "object"))?;
    nested.retain(|key, _| allowed.contains(key));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn stage_legacy_ssh_keys(
    current_dir: &Path,
    legacy_dir: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    pending: &mut Vec<PendingFile>,
    source_files: &mut Vec<LegacySourceSnapshot>,
    migrated_files: &mut Vec<String>,
    kept_current_files: &mut Vec<String>,
) -> Result<(HashSet<String>, bool), AppError> {
    let target_index = current_dir.join("ssh-keys.json");
    let source_index = legacy_dir.join("ssh-keys.json");
    let current = read_optional_json_file(&target_index)?;
    let has_index_context = current.is_some() || source_index.is_file();
    let mut object = object_or_empty(current.clone(), "current SSH key index")?;
    let mut keys = match object.remove("keys") {
        Some(Value::Array(keys)) => keys,
        Some(_) => return Err(invalid_store_shape("current SSH key index keys", "array")),
        None => Vec::new(),
    };
    let mut known_ids = value_ids(&Value::Array(keys.clone()));

    if source_index.is_file() {
        source_files.push(source_snapshot("ssh-keys.json", &source_index)?);
        let Value::Object(mut legacy_object) = read_json_file::<Value>(&source_index)? else {
            return Err(invalid_store_shape("Electron SSH key index", "object"));
        };
        let legacy_keys = match legacy_object.remove("keys") {
            Some(Value::Array(keys)) => keys,
            Some(_) => return Err(invalid_store_shape("Electron SSH key index keys", "array")),
            None => Vec::new(),
        };
        for (key, value) in legacy_object {
            object.entry(key).or_insert(value);
        }
        for key in legacy_keys {
            let Some(id) = key.get("id").and_then(Value::as_str) else {
                continue;
            };
            if uuid::Uuid::parse_str(id).is_err() || known_ids.contains(id) {
                continue;
            }
            let source_key = legacy_dir.join("ssh-keys").join(format!("{id}.key"));
            let target_key = current_dir.join("ssh-keys").join(format!("{id}.key"));
            if source_key.is_file() || target_key.is_file() {
                known_ids.insert(id.to_string());
                keys.push(key);
            }
        }
    }

    object.insert("keys".to_string(), Value::Array(keys));
    let merged = Value::Object(object);
    if source_index.is_file() {
        if current.as_ref() == Some(&merged) {
            kept_current_files.push("ssh-keys.json".to_string());
        } else {
            stage_json_file(
                current_dir,
                staged_dir,
                backup_dir,
                "ssh-keys.json",
                &merged,
                false,
                pending,
            )?;
            migrated_files.push("ssh-keys.json".to_string());
        }
    }

    for id in &known_ids {
        if uuid::Uuid::parse_str(id).is_err() {
            continue;
        }
        let relative = PathBuf::from("ssh-keys").join(format!("{id}.key"));
        let target = current_dir.join(&relative);
        if target.is_file() {
            continue;
        }
        let source = legacy_dir.join(&relative);
        if !source.is_file() {
            continue;
        }
        source_files.push(source_snapshot(&relative.to_string_lossy(), &source)?);
        stage_file_copy(
            current_dir,
            staged_dir,
            backup_dir,
            &relative,
            &source,
            true,
            pending,
        )?;
        migrated_files.push(relative.to_string_lossy().into_owned());
    }

    Ok((known_ids, has_index_context))
}

#[allow(clippy::too_many_arguments)]
fn stage_legacy_ssh_key_secrets(
    current_dir: &Path,
    legacy_dir: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    ssh_context: (HashSet<String>, bool),
    pending: &mut Vec<PendingFile>,
    source_files: &mut Vec<LegacySourceSnapshot>,
    migrated_files: &mut Vec<String>,
    kept_current_files: &mut Vec<String>,
) -> Result<(), AppError> {
    let source = legacy_dir.join("ssh-key-secrets.json");
    if !source.is_file() {
        return Ok(());
    }
    source_files.push(source_snapshot("ssh-key-secrets.json", &source)?);
    let target = current_dir.join("ssh-key-secrets.json");
    let current = read_optional_json_file(&target)?;
    let legacy = read_json_file(&source)?;
    let mut merged = merge_json_values(
        JsonMergeMode::NestedObjectCurrentWins("passphrases"),
        current.clone(),
        legacy,
    )?;
    let (known_ids, _has_index_context) = ssh_context;
    retain_nested_keys(&mut merged, "passphrases", &known_ids)?;
    if current.as_ref() == Some(&merged) {
        kept_current_files.push("ssh-key-secrets.json".to_string());
        return Ok(());
    }
    stage_json_file(
        current_dir,
        staged_dir,
        backup_dir,
        "ssh-key-secrets.json",
        &merged,
        true,
        pending,
    )?;
    migrated_files.push("ssh-key-secrets.json".to_string());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn stage_json_file(
    current_dir: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    name: &str,
    value: &Value,
    confidential: bool,
    pending: &mut Vec<PendingFile>,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    stage_bytes(
        current_dir,
        staged_dir,
        backup_dir,
        Path::new(name),
        &bytes,
        confidential,
        pending,
    )
}

#[allow(clippy::too_many_arguments)]
fn stage_file_copy(
    current_dir: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    relative: &Path,
    source: &Path,
    confidential: bool,
    pending: &mut Vec<PendingFile>,
) -> Result<(), AppError> {
    let bytes = fs::read(source).map_err(|error| AppError::Storage(error.to_string()))?;
    stage_bytes(
        current_dir,
        staged_dir,
        backup_dir,
        relative,
        &bytes,
        confidential,
        pending,
    )
}

#[allow(clippy::too_many_arguments)]
fn stage_bytes(
    current_dir: &Path,
    staged_dir: &Path,
    backup_dir: &Path,
    relative: &Path,
    bytes: &[u8],
    confidential: bool,
    pending: &mut Vec<PendingFile>,
) -> Result<(), AppError> {
    let staged = staged_dir.join(relative);
    if let Some(parent) = staged.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
        lock_down_directory(parent)?;
    }
    write_restricted_file(&staged, bytes)?;
    pending.push(PendingFile {
        target: current_dir.join(relative),
        staged,
        backup: backup_dir.join(relative),
        confidential,
    });
    Ok(())
}
