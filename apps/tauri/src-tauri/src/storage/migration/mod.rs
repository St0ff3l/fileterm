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

include!("portable.rs");
include!("legacy.rs");
include!("staging.rs");
