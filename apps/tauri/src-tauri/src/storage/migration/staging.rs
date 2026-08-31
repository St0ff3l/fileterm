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
