#[cfg(any(test, target_os = "windows"))]
fn portable_config_directory_for_executable(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    let has_portable_name =
        is_compiled_portable_build() || has_portable_executable_name(executable);
    let has_marker = portable_marker_path_for_executable(executable)
        .map(|path| path.is_file())
        .unwrap_or(false);

    has_portable_name
        .then(|| parent.join(PORTABLE_CONFIG_DIRECTORY))
        .or_else(|| has_marker.then(|| parent.join(PORTABLE_CONFIG_DIRECTORY)))
}

#[cfg(any(test, target_os = "windows"))]
fn has_portable_executable_name(executable: &Path) -> bool {
    let Some(executable_name) = executable.file_stem() else {
        return false;
    };
    let executable_name = executable_name.to_string_lossy().to_ascii_lowercase();
    executable_name == "portable"
        || executable_name.ends_with("-portable")
        || executable_name.ends_with("_portable")
}

#[cfg(any(test, target_os = "windows"))]
fn portable_marker_path_for_executable(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|parent| parent.join(PORTABLE_MARKER_FILE))
}

#[cfg(any(test, target_os = "windows"))]
fn ensure_portable_marker_for_executable(executable: &Path) -> Result<Option<PathBuf>, AppError> {
    let Some(marker_path) = portable_marker_path_for_executable(executable) else {
        return Ok(None);
    };
    if portable_config_directory_for_executable(executable).is_none() {
        return Ok(None);
    }
    if marker_path.is_file() {
        return Ok(Some(marker_path));
    }

    // Persist the fact that this directory was launched as the portable
    // build. This survives a user renaming the executable or clearing the
    // config directory, so the next launch cannot silently fall back to
    // %APPDATA% and repopulate deleted data.
    write_restricted_file(&marker_path, b"")?;
    Ok(Some(marker_path))
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

pub fn is_compiled_portable_build() -> bool {
    option_env!("FILETERM_PORTABLE_BUILD")
        .map(|value| value == "1")
        .unwrap_or(false)
}

/// Persist a portable-mode marker next to the executable after the first
/// successful startup. The marker is deliberately outside `config/`: clearing
/// portable data must not make the next launch migrate the old app-data store
/// back into the freshly emptied directory.
pub fn ensure_portable_marker() -> Result<Option<PathBuf>, AppError> {
    #[cfg(target_os = "windows")]
    {
        let Some(executable) = std::env::current_exe().ok() else {
            return Ok(None);
        };
        return ensure_portable_marker_for_executable(&executable);
    }

    #[cfg(not(target_os = "windows"))]
    Ok(None)
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
