use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

use crate::AppError;

const LEGACY_MIGRATION_VERSION: u32 = 1;
const LEGACY_MIGRATION_MARKER: &str = "legacy-fileterm-migration.json";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_CONFIG_DIRECTORY: &str = "config";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_MARKER_FILE: &str = "portable";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_MIGRATION_VERSION: u32 = 1;
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_MIGRATION_MARKER: &str = "portable-migration.json";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_DATA_ENTRIES: &[(&str, bool)] = &[
    ("profiles.json", false),
    ("folders.json", false),
    ("profile-secrets.json", true),
    ("command-folders.json", false),
    ("commands.json", false),
    ("command-history.json", false),
    ("command-send-preferences.json", false),
    ("ui-state.json", false),
    ("ui-preferences.json", false),
    ("transfer-journal.json", false),
    ("webdav-sync.json", true),
    ("s3-backup.json", true),
    ("ai-providers.json", false),
    ("ai-provider-secrets.json", true),
    ("ai-conversations.json", true),
    ("ai-conversations", true),
    ("fonts.json", false),
    ("fonts", false),
    ("ssh-keys.json", false),
    ("ssh-key-secrets.json", true),
    ("ssh-keys", true),
    ("secret-store-v1.key", true),
    (LEGACY_MIGRATION_MARKER, false),
];

#[derive(Clone, Copy)]
enum JsonMergeMode {
    ArrayById,
    ObjectCurrentWins,
    NestedObjectCurrentWins(&'static str),
    CurrentFileWins,
}

#[derive(Clone, Copy)]
struct LegacyJsonStore {
    name: &'static str,
    mode: JsonMergeMode,
    confidential: bool,
}

const LEGACY_JSON_STORES: &[LegacyJsonStore] = &[
    LegacyJsonStore {
        name: "profiles.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "folders.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "profile-secrets.json",
        mode: JsonMergeMode::NestedObjectCurrentWins("profiles"),
        confidential: true,
    },
    LegacyJsonStore {
        name: "command-folders.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "commands.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "command-history.json",
        mode: JsonMergeMode::ObjectCurrentWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "command-send-preferences.json",
        mode: JsonMergeMode::ObjectCurrentWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "ui-state.json",
        mode: JsonMergeMode::CurrentFileWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "ui-preferences.json",
        mode: JsonMergeMode::ObjectCurrentWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "transfer-journal.json",
        mode: JsonMergeMode::CurrentFileWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "webdav-sync.json",
        mode: JsonMergeMode::CurrentFileWins,
        confidential: true,
    },
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacySourceSnapshot {
    name: String,
    bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyMigrationReport {
    version: u32,
    status: String,
    completed_at: u64,
    source_directory: String,
    conflict_policy: String,
    source_files: Vec<LegacySourceSnapshot>,
    migrated_files: Vec<String>,
    kept_current_files: Vec<String>,
    rollback_performed: bool,
}

struct PendingFile {
    target: PathBuf,
    staged: PathBuf,
    backup: PathBuf,
    confidential: bool,
}

#[cfg(any(test, target_os = "windows"))]
fn portable_config_directory_for_executable(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    let executable_name = executable
        .file_stem()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let has_portable_name = executable_name == "portable"
        || executable_name.ends_with("-portable")
        || executable_name.ends_with("_portable");
    let has_marker = parent.join(PORTABLE_MARKER_FILE).is_file();

    has_portable_name
        .then(|| parent.join(PORTABLE_CONFIG_DIRECTORY))
        .or_else(|| has_marker.then(|| parent.join(PORTABLE_CONFIG_DIRECTORY)))
}

pub fn portable_config_directory() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::current_exe()
            .ok()
            .and_then(|executable| portable_config_directory_for_executable(&executable))
    }

    #[cfg(not(target_os = "windows"))]
    None
}

pub fn storage_root(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = if let Some(portable_directory) = portable_config_directory() {
        // Portable mode must not silently fall back to a user directory. A
        // read-only USB or network location should report the real problem so
        // the user can move it to a writable directory.
        portable_directory
    } else {
        app.path()
            .app_data_dir()
            .map_err(|error| AppError::Storage(error.to_string()))?
    };
    fs::create_dir_all(&dir).map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(dir)
}

pub fn state_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = storage_root(app)?;
    Ok(dir.join("ui-preferences.json"))
}

pub fn workspace_file(app: &AppHandle, name: &str) -> Result<PathBuf, AppError> {
    Ok(state_path(app)?.with_file_name(name))
}

/// Import Electron's established `FileTerm` user-data store exactly once.
///
/// Tauri owns an independent app-data directory. The completed marker is the
/// boundary that prevents a later delete or clear in Tauri from being undone
/// by another read of Electron's still-live store.
pub fn migrate_legacy_data_once(app: &AppHandle) -> Result<(), AppError> {
    let current_dir = storage_root(app)?;
    #[cfg(target_os = "windows")]
    if portable_config_directory().is_some() {
        migrate_portable_data_once(app, &current_dir)?;
    }
    let config_dir = app.path().app_config_dir().ok();
    let data_dir = app.path().app_data_dir().ok();
    let legacy_dir =
        select_legacy_directory(&current_dir, config_dir.as_deref(), data_dir.as_deref())?;
    migrate_legacy_store(&current_dir, &legacy_dir).map(|_| ())
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

fn commit_pending_files(pending: &[PendingFile]) -> Result<(), AppError> {
    let mut committed: Vec<(&Path, &Path)> = Vec::new();
    for file in pending {
        if let Some(parent) = file.target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                rollback_committed(&committed);
                AppError::Storage(error.to_string())
            })?;
            if file.confidential {
                if let Err(error) = lock_down_directory(parent) {
                    rollback_committed(&committed);
                    return Err(error);
                }
            }
        }
        let had_current = file.target.exists();
        if had_current {
            if let Some(parent) = file.backup.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    rollback_committed(&committed);
                    AppError::Storage(error.to_string())
                })?;
                if let Err(error) = lock_down_directory(parent) {
                    rollback_committed(&committed);
                    return Err(error);
                }
            }
            if let Err(error) = fs::rename(&file.target, &file.backup) {
                rollback_committed(&committed);
                return Err(AppError::Storage(error.to_string()));
            }
        }
        if let Err(error) = fs::rename(&file.staged, &file.target) {
            if had_current {
                let _ = fs::rename(&file.backup, &file.target);
            }
            rollback_committed(&committed);
            return Err(AppError::Storage(error.to_string()));
        }
        if file.confidential {
            if let Err(error) = lock_down_file(&file.target) {
                let _ = fs::remove_file(&file.target);
                if had_current {
                    let _ = fs::rename(&file.backup, &file.target);
                }
                rollback_committed(&committed);
                return Err(error);
            }
        }
        committed.push((&file.target, &file.backup));
    }
    Ok(())
}

fn rollback_committed(committed: &[(&Path, &Path)]) {
    for (target, backup) in committed.iter().rev() {
        let _ = fs::remove_file(target);
        if backup.exists() {
            let _ = fs::rename(backup, target);
        }
    }
}

fn read_optional_json_file(path: &Path) -> Result<Option<Value>, AppError> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| AppError::Serialization(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let content = fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
    serde_json::from_str(&content).map_err(|error| AppError::Serialization(error.to_string()))
}

#[cfg(unix)]
pub(crate) fn write_restricted_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    result.map_err(|error| restricted_write_error(path, error))
}

#[cfg(not(unix))]
pub(crate) fn write_restricted_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    result.map_err(|error| restricted_write_error(path, error))
}

fn restricted_write_error(path: &Path, error: std::io::Error) -> AppError {
    match fs::remove_file(path) {
        Ok(()) => AppError::Storage(error.to_string()),
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
            AppError::Storage(error.to_string())
        }
        Err(cleanup_error) => {
            AppError::Storage(format!("{error}; 清理受限临时文件失败: {cleanup_error}"))
        }
    }
}

#[cfg(unix)]
fn lock_down_file(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_file(_path: &Path) -> Result<(), AppError> {
    // Windows relies on the per-user application-data directory ACL. A
    // platform-specific restricted ACL remains a release acceptance item.
    Ok(())
}

#[cfg(unix)]
fn lock_down_directory(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn read_json_object(app: &AppHandle, name: &str) -> Result<Value, AppError> {
    let path = workspace_file(app, name)?;
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    read_json_file(&path)
}

pub fn read_json_array(app: &AppHandle, name: &str) -> Result<Vec<Value>, AppError> {
    let path = workspace_file(app, name)?;
    let mut values: Vec<Value> = if path.exists() {
        read_json_file(&path)?
    } else {
        Vec::new()
    };

    if name == "profiles.json" {
        let secrets_path = workspace_file(app, "profile-secrets.json")?;
        let storage_root = secrets_path
            .parent()
            .ok_or_else(|| AppError::Storage("无法解析连接凭据存储目录".to_string()))?;
        let mut secrets = if secrets_path.exists() {
            read_json_file(&secrets_path)?
        } else {
            serde_json::json!({})
        };
        let mut migrated = false;
        if let Some(secrets_profiles) = secrets.get_mut("profiles").and_then(Value::as_object_mut) {
            for profile in &mut values {
                if let Some(profile_obj) = profile.as_object_mut() {
                    if let Some(profile_id) = profile_obj
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                    {
                        if let Some(profile_secrets) = secrets_profiles
                            .get_mut(&profile_id)
                            .and_then(Value::as_object_mut)
                        {
                            let (password, password_migrated) = profile_secret_value(
                                storage_root,
                                &profile_id,
                                "password",
                                profile_secrets,
                            )?;
                            migrated |= password_migrated;
                            if let Some(password) = password {
                                profile_obj.insert("password".to_string(), Value::String(password));
                            }
                            let (passphrase, passphrase_migrated) = profile_secret_value(
                                storage_root,
                                &profile_id,
                                "passphrase",
                                profile_secrets,
                            )?;
                            migrated |= passphrase_migrated;
                            if let Some(passphrase) = passphrase {
                                profile_obj
                                    .insert("passphrase".to_string(), Value::String(passphrase));
                            }
                            let (private_key_path, private_key_path_migrated) =
                                profile_secret_value(
                                    storage_root,
                                    &profile_id,
                                    "privateKeyPath",
                                    profile_secrets,
                                )?;
                            migrated |= private_key_path_migrated;
                            if let Some(private_key_path) = private_key_path {
                                profile_obj.insert(
                                    "privateKeyPath".to_string(),
                                    Value::String(private_key_path),
                                );
                            }
                            let (proxy_password, proxy_password_migrated) = profile_secret_value(
                                storage_root,
                                &profile_id,
                                "proxyPassword",
                                profile_secrets,
                            )?;
                            migrated |= proxy_password_migrated;
                            if let Some(proxy_password) = proxy_password {
                                if let Some(proxy_obj) =
                                    profile_obj.get_mut("proxy").and_then(Value::as_object_mut)
                                {
                                    proxy_obj.insert(
                                        "password".to_string(),
                                        Value::String(proxy_password),
                                    );
                                }
                            }
                            for field in ["sudoPassword", "suPassword"] {
                                let (password, password_migrated) = profile_secret_value(
                                    storage_root,
                                    &profile_id,
                                    field,
                                    profile_secrets,
                                )?;
                                migrated |= password_migrated;
                                if let Some(password) = password {
                                    profile_obj.insert(field.to_string(), Value::String(password));
                                }
                            }
                        }
                    }
                }
            }
        }
        if migrated {
            write_restricted_json(&secrets_path, &secrets)?;
        }
    }

    Ok(values)
}

fn profile_secret_value(
    storage_root: &Path,
    profile_id: &str,
    field: &str,
    profile_secrets: &mut Map<String, Value>,
) -> Result<(Option<String>, bool), AppError> {
    let Some(value) = profile_secrets
        .get(field)
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    else {
        return Ok((None, false));
    };
    let scope = format!("profile/{profile_id}/{field}");
    let (plaintext, should_migrate) =
        crate::services::secret_crypto::decrypt_or_migrate(storage_root, &scope, &value)?;
    if should_migrate {
        let encrypted = crate::services::secret_crypto::encrypt(storage_root, &scope, &plaintext)?;
        if let Some(entry) = profile_secrets
            .get_mut(field)
            .and_then(Value::as_object_mut)
        {
            entry.insert(
                "storage".to_string(),
                Value::String(crate::services::secret_crypto::ENCRYPTED_STORAGE.to_string()),
            );
            entry.insert("value".to_string(), Value::String(encrypted));
        }
    }
    Ok((Some(plaintext), should_migrate))
}

fn write_restricted_json(path: &Path, value: &Value) -> Result<(), AppError> {
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&temporary, &content)?;
    if let Err(error) = replace_file_atomically(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

pub fn write_json_array(app: &AppHandle, name: &str, values: &[Value]) -> Result<(), AppError> {
    let path = workspace_file(app, name)?;
    let temp_path = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let content = serde_json::to_string_pretty(values)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    fs::write(&temp_path, content).map_err(|error| AppError::Storage(error.to_string()))?;
    replace_file_atomically(&temp_path, &path)
}

/// Replace a file using same-directory staging and rollback. Moving the
/// current target aside first keeps this path compatible with Windows, where
/// `rename(staged, existing_target)` does not replace the target.
pub fn replace_file_atomically(staged: &Path, target: &Path) -> Result<(), AppError> {
    let backup = target.with_file_name(format!(
        ".{}.{}.bak",
        target.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let had_current = target.exists();
    if had_current {
        if let Err(error) = fs::rename(target, &backup) {
            let _ = fs::remove_file(staged);
            return Err(AppError::Storage(error.to_string()));
        }
    }

    if let Err(error) = fs::rename(staged, target) {
        let restore_error = if had_current {
            fs::rename(&backup, target).err()
        } else {
            None
        };
        let _ = fs::remove_file(staged);
        return Err(AppError::Storage(match restore_error {
            Some(restore_error) => {
                format!("{error}; 恢复原文件失败: {restore_error}")
            }
            None => error.to_string(),
        }));
    }

    if had_current {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::{
        commit_pending_files, migrate_legacy_store, migrate_portable_data_from_source,
        portable_config_directory_for_executable, replace_file_atomically, select_legacy_directory,
        PendingFile, LEGACY_MIGRATION_MARKER,
    };
    use serde_json::{json, Value};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_dirs(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("fileterm-storage-{name}-{}", uuid::Uuid::new_v4()));
        let current = root.join("com.fileterm.desktop");
        let legacy = root.join("FileTerm");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        (root, current, legacy)
    }

    #[test]
    fn portable_executable_uses_a_config_directory_next_to_the_binary() {
        let root =
            std::env::temp_dir().join(format!("fileterm-portable-path-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let portable_executable = root.join("FileTerm-2.2.4-windows-x64-portable.exe");
        assert_eq!(
            portable_config_directory_for_executable(&portable_executable),
            Some(root.join("config"))
        );

        let regular_executable = root.join("FileTerm.exe");
        assert_eq!(
            portable_config_directory_for_executable(&regular_executable),
            None
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_marker_enables_local_config_for_a_renamed_binary() {
        let root =
            std::env::temp_dir().join(format!("fileterm-portable-marker-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("portable"), b"").unwrap();

        assert_eq!(
            portable_config_directory_for_executable(&root.join("FileTerm.exe")),
            Some(root.join("config"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_data_migration_copies_owned_files_once() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-portable-data-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("app-data");
        let current = root.join("portable").join("config");
        fs::create_dir_all(source.join("ai-conversations")).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(source.join("profiles.json"), b"profiles").unwrap();
        fs::write(source.join("secret-store-v1.key"), b"seed").unwrap();
        fs::write(
            source.join("ai-conversations").join("conversation.json"),
            b"conversation",
        )
        .unwrap();
        fs::write(source.join("mcp-runtime.json"), b"stale runtime").unwrap();
        fs::create_dir_all(source.join("logs")).unwrap();
        fs::write(source.join("logs").join("app.log"), b"diagnostic log").unwrap();

        migrate_portable_data_from_source(&current, &source).unwrap();

        assert_eq!(
            fs::read(current.join("profiles.json")).unwrap(),
            b"profiles"
        );
        assert_eq!(
            fs::read(current.join("secret-store-v1.key")).unwrap(),
            b"seed"
        );
        assert_eq!(
            fs::read(current.join("ai-conversations").join("conversation.json")).unwrap(),
            b"conversation"
        );
        assert!(!current.join("mcp-runtime.json").exists());
        assert!(!current.join("logs").exists());
        assert!(current.join("portable-migration.json").exists());

        fs::write(source.join("profiles.json"), b"changed source").unwrap();
        migrate_portable_data_from_source(&current, &source).unwrap();
        assert_eq!(
            fs::read(current.join("profiles.json")).unwrap(),
            b"profiles"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_directory_uses_config_root_when_data_and_config_roots_differ() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-legacy-root-selection-{}",
            uuid::Uuid::new_v4()
        ));
        let current_data = root.join("data").join("com.fileterm.desktop");
        let current_config = root.join("config").join("com.fileterm.desktop");
        let empty_data_candidate = root.join("data").join("FileTerm");
        let electron = root.join("config").join("FileTerm");
        fs::create_dir_all(&current_data).unwrap();
        fs::create_dir_all(&current_config).unwrap();
        fs::create_dir_all(&empty_data_candidate).unwrap();
        fs::create_dir_all(&electron).unwrap();
        fs::write(electron.join("profiles.json"), b"[]").unwrap();

        assert_eq!(
            select_legacy_directory(&current_data, Some(&current_config), None).unwrap(),
            electron
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_directory_uses_app_data_parent_for_portable_storage() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-portable-legacy-root-selection-{}",
            uuid::Uuid::new_v4()
        ));
        let portable_config = root.join("portable").join("config");
        let app_data = root.join("data").join("com.fileterm.desktop");
        let electron = root.join("data").join("FileTerm");
        fs::create_dir_all(&portable_config).unwrap();
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&electron).unwrap();
        fs::write(electron.join("profiles.json"), b"[]").unwrap();

        assert_eq!(
            select_legacy_directory(&portable_config, None, Some(&app_data)).unwrap(),
            electron
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_replace_updates_an_existing_file_without_losing_the_old_on_staging() {
        let root =
            std::env::temp_dir().join(format!("fileterm-atomic-replace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("state.json");
        let staged = root.join("state.json.tmp");
        fs::write(&target, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();

        replace_file_atomically(&staged, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn write_json(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn migration_is_one_time_and_deleted_records_do_not_return() {
        let (root, current, legacy) = test_dirs("one-time");
        write_json(
            &legacy.join("profiles.json"),
            &json!([{ "id": "legacy-profile", "name": "Legacy" }]),
        );

        let first = migrate_legacy_store(&current, &legacy).unwrap();
        assert_eq!(first.status, "completed");
        assert!(current.join(LEGACY_MIGRATION_MARKER).is_file());
        write_json(&current.join("profiles.json"), &json!([]));

        let second = migrate_legacy_store(&current, &legacy).unwrap();
        assert_eq!(first.completed_at, second.completed_at);
        let profiles: Value =
            serde_json::from_slice(&fs::read(current.join("profiles.json")).unwrap()).unwrap();
        assert_eq!(profiles, json!([]));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_keeps_current_records_and_adds_only_missing_ids() {
        let (root, current, legacy) = test_dirs("conflicts");
        write_json(
            &current.join("profiles.json"),
            &json!([{ "id": "same", "name": "Current" }]),
        );
        write_json(
            &legacy.join("profiles.json"),
            &json!([
                { "id": "same", "name": "Legacy" },
                { "id": "missing", "name": "Imported" }
            ]),
        );

        migrate_legacy_store(&current, &legacy).unwrap();
        let profiles: Vec<Value> =
            serde_json::from_slice(&fs::read(current.join("profiles.json")).unwrap()).unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0]["name"], "Current");
        assert_eq!(profiles[1]["id"], "missing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_legacy_json_rolls_back_without_writing_a_marker() {
        let (root, current, legacy) = test_dirs("rollback");
        write_json(
            &current.join("profiles.json"),
            &json!([{ "id": "current", "name": "Keep" }]),
        );
        fs::write(legacy.join("profiles.json"), b"not-json").unwrap();

        assert!(migrate_legacy_store(&current, &legacy).is_err());
        assert!(!current.join(LEGACY_MIGRATION_MARKER).exists());
        let profiles: Value =
            serde_json::from_slice(&fs::read(current.join("profiles.json")).unwrap()).unwrap();
        assert_eq!(profiles[0]["id"], "current");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commit_failure_restores_files_already_replaced_in_the_transaction() {
        let (root, current, _) = test_dirs("commit-rollback");
        let transaction = current.join("transaction");
        let staged = transaction.join("staged");
        let backup = transaction.join("backup");
        fs::create_dir_all(&staged).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(current.join("first.json"), b"first-old").unwrap();
        fs::write(current.join("second.json"), b"second-old").unwrap();
        fs::write(staged.join("first.json"), b"first-new").unwrap();

        let pending = vec![
            PendingFile {
                target: current.join("first.json"),
                staged: staged.join("first.json"),
                backup: backup.join("first.json"),
                confidential: false,
            },
            PendingFile {
                target: current.join("second.json"),
                staged: staged.join("missing-second.json"),
                backup: backup.join("second.json"),
                confidential: false,
            },
        ];

        assert!(commit_pending_files(&pending).is_err());
        assert_eq!(fs::read(current.join("first.json")).unwrap(), b"first-old");
        assert_eq!(
            fs::read(current.join("second.json")).unwrap(),
            b"second-old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_copies_only_indexed_ssh_keys_once() {
        let (root, current, legacy) = test_dirs("ssh-keys");
        let key_id = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(legacy.join("ssh-keys")).unwrap();
        write_json(
            &legacy.join("ssh-keys.json"),
            &json!({
                "version": 1,
                "keys": [{
                    "id": key_id,
                    "name": "id_ed25519",
                    "algorithm": "ssh-ed25519",
                    "fingerprint": "SHA256:test",
                    "encrypted": false,
                    "importedAt": 1
                }]
            }),
        );
        fs::write(
            legacy.join("ssh-keys").join(format!("{key_id}.key")),
            b"PRIVATE KEY",
        )
        .unwrap();

        migrate_legacy_store(&current, &legacy).unwrap();
        let target_key = current.join("ssh-keys").join(format!("{key_id}.key"));
        assert_eq!(fs::read(&target_key).unwrap(), b"PRIVATE KEY");

        write_json(
            &current.join("ssh-keys.json"),
            &json!({ "version": 1, "keys": [] }),
        );
        fs::remove_file(&target_key).unwrap();
        migrate_legacy_store(&current, &legacy).unwrap();
        assert!(!target_key.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_prunes_profile_secrets_without_a_matching_profile() {
        let (root, current, legacy) = test_dirs("orphan-profile-secret");
        write_json(
            &legacy.join("profile-secrets.json"),
            &json!({
                "version": 1,
                "profiles": {
                    "deleted-profile": {
                        "password": { "storage": "plain-text-fallback", "value": "secret" }
                    }
                }
            }),
        );

        migrate_legacy_store(&current, &legacy).unwrap();
        let secrets: Value =
            serde_json::from_slice(&fs::read(current.join("profile-secrets.json")).unwrap())
                .unwrap();
        assert_eq!(secrets["profiles"], json!({}));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn migrated_plaintext_secrets_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (root, current, legacy) = test_dirs("secret-mode");
        write_json(
            &legacy.join("profiles.json"),
            &json!([{ "id": "profile-1", "name": "Secret" }]),
        );
        write_json(
            &legacy.join("profile-secrets.json"),
            &json!({
                "version": 1,
                "profiles": {
                    "profile-1": {
                        "password": { "storage": "plain-text-fallback", "value": "secret" }
                    }
                }
            }),
        );

        migrate_legacy_store(&current, &legacy).unwrap();
        let mode = fs::metadata(current.join("profile-secrets.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(root).unwrap();
    }
}
