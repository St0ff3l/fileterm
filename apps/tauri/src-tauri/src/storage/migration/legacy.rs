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
