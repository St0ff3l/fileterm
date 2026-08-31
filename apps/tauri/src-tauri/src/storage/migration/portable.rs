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
