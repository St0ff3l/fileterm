//! Only update the installed package format, never a repackaged or extracted binary.

use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, PermissionsExt},
    },
    path::Path,
};

use tauri::utils::config::BundleType;

fn target_for_installation(
    bundle: Option<BundleType>,
    architecture: &str,
    has_appimage: bool,
    package_owned: bool,
) -> Option<&'static str> {
    match (bundle, architecture) {
        (Some(BundleType::AppImage), "x86_64") if has_appimage => Some("linux-x86_64-appimage"),
        (Some(BundleType::AppImage), "aarch64") if has_appimage => Some("linux-aarch64-appimage"),
        (Some(BundleType::Deb), "x86_64") if package_owned => Some("linux-x86_64-deb"),
        (Some(BundleType::Deb), "aarch64") if package_owned => Some("linux-aarch64-deb"),
        (Some(BundleType::Rpm), "x86_64") if package_owned => Some("linux-x86_64-rpm"),
        (Some(BundleType::Rpm), "aarch64") if package_owned => Some("linux-aarch64-rpm"),
        _ => None,
    }
}

fn owner_matches(bundle: &BundleType, output: &str, executable: &Path) -> bool {
    match bundle {
        BundleType::Deb => output.lines().any(|line| {
            line.split_once(": ").is_some_and(|(owner, path)| {
                owner.split(':').next() == Some("file-term") && Path::new(path) == executable
            })
        }),
        BundleType::Rpm => output.trim() == "file-term",
        _ => false,
    }
}

fn command_output(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(error) => error,
                    _ => std::io::Error::new(std::io::ErrorKind::TimedOut, "查询系统包归属超时"),
                });
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn update_target(app: &tauri::AppHandle) -> Option<&'static str> {
    use tauri::{utils::platform, Manager};

    static TARGET: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    *TARGET.get_or_init(|| {
        let bundle = platform::bundle_type();
        // Arch packages reuse the DEB binary. Confirm ownership before invoking dpkg/rpm.
        let package_owned = platform::current_exe().ok().is_some_and(|executable| {
            let mut command = match bundle {
                Some(BundleType::Deb) => {
                    let mut command = std::process::Command::new("dpkg-query");
                    command.arg("--search");
                    command
                }
                Some(BundleType::Rpm) => {
                    let mut command = std::process::Command::new("rpm");
                    command.args(["-qf", "--queryformat", "%{NAME}"]);
                    command
                }
                _ => return false,
            };
            command_output(command.arg(&executable), std::time::Duration::from_secs(2)).is_ok_and(
                |output| {
                    output.status.success()
                        && owner_matches(
                            bundle.as_ref().unwrap(),
                            &String::from_utf8_lossy(&output.stdout),
                            &executable,
                        )
                },
            )
        });
        let environment = app.env();
        let has_appimage = environment.appimage.is_some()
            && environment.appdir.is_some_and(|directory| {
                platform::current_exe()
                    .is_ok_and(|executable| executable.starts_with(Path::new(&directory)))
            });
        target_for_installation(bundle, std::env::consts::ARCH, has_appimage, package_owned)
    })
}

fn validate_appimage(bytes: &[u8], architecture: &str) -> Result<(), String> {
    let machine = match architecture {
        "x86_64" => 62_u16,
        "aarch64" => 183_u16,
        _ => return Err("当前架构不支持 AppImage 更新".to_string()),
    };
    if bytes.len() < 64
        || &bytes[..6] != b"\x7fELF\x02\x01"
        || &bytes[8..11] != b"AI\x02"
        || bytes[18..20] != machine.to_le_bytes()
    {
        return Err("更新包不是与当前架构匹配的 Type 2 AppImage，拒绝替换".to_string());
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<[u8; 32], String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

fn validate_restart_path(path: &Path) -> Result<(), String> {
    if !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(format!("重启程序不存在或不是普通文件: {}", path.display()));
    }
    let path_c =
        std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    // CString remains alive for access(2), which also checks directory traversal and mount permissions.
    if unsafe { libc::access(path_c.as_ptr(), libc::X_OK) } != 0 {
        return Err(format!(
            "重启程序不可执行: {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn restore_appimage(original: &Path, image: &Path) -> Result<(), String> {
    std::fs::rename(original, image).map_err(|error| {
        format!(
            "恢复失败: {error}。旧版 AppImage 保留在 {}，请手动恢复。",
            original.display()
        )
    })?;
    let parent = image.parent().ok_or("无法确定 AppImage 所在目录")?;
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("旧版已恢复至 {}，但目录刷盘失败: {error}", image.display()))
}

fn install_appimage_with_backup(
    image: &Path,
    bytes: &[u8],
    architecture: &str,
    prepare: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    validate_appimage(bytes, architecture)?;
    let metadata = std::fs::symlink_metadata(image)
        .map_err(|error| format!("无法读取当前 AppImage: {error}"))?;
    if !metadata.is_file() {
        return Err("请直接运行 AppImage 文件后再更新，不支持符号链接或解包目录。".to_string());
    }
    validate_restart_path(image)?;
    let original_hash = file_hash(image)?;
    let parent = image.parent().ok_or("无法确定 AppImage 所在目录")?;
    let backup = parent.join(format!(".fileterm-update-backup-{}", uuid::Uuid::new_v4()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&backup)
        .map_err(|error| format!("AppImage 所在目录不可写，请移至用户可写目录后再更新: {error}"))?;
    let original = backup.join("original.AppImage");
    let staged = backup.join("new.AppImage");
    let mut replaced = false;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::fs::copy(image, &original)
            .map_err(|error| format!("无法备份当前 AppImage: {error}"))?;
        std::fs::File::open(&original)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
        if file_hash(&original)? != original_hash {
            return Err("原程序在备份期间发生变化，拒绝覆盖".to_string());
        }
        std::fs::File::open(&backup)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        prepare(&staged)?;
        if !std::fs::symlink_metadata(&staged).is_ok_and(|metadata| metadata.is_file())
            || file_hash(&staged)? != <[u8; 32]>::from(Sha256::digest(bytes))
        {
            return Err("暂存更新包与已验证内容不一致，拒绝替换".to_string());
        }
        let file = std::fs::File::open(&staged).map_err(|error| error.to_string())?;
        file.set_permissions(std::fs::Permissions::from_mode(
            metadata.permissions().mode() & 0o777,
        ))
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())?;
        validate_restart_path(&staged)?;
        if std::fs::symlink_metadata(image).is_ok_and(|metadata| !metadata.is_file())
            || file_hash(image)? != original_hash
        {
            return Err("原程序在更新期间发生变化，拒绝覆盖".to_string());
        }
        // Both paths are on the same filesystem. The current image is never partially written.
        std::fs::rename(&staged, image)
            .map_err(|error| format!("无法原子替换 AppImage: {error}"))?;
        replaced = true;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        validate_restart_path(image)
    }))
    .unwrap_or_else(|_| Err("更新安装器异常退出".to_string()));
    if let Err(error) = result {
        if replaced {
            if let Err(restore_error) = restore_appimage(&original, image) {
                return Err(format!("更新失败: {error}；{restore_error}"));
            }
        } else if file_hash(image).ok() != Some(original_hash) && original.is_file() {
            return Err(format!(
                "更新失败: {error}；原程序已变化，未覆盖。旧版副本保留在 {}。",
                original.display()
            ));
        }
        let _ = std::fs::remove_dir_all(&backup);
        return Err(format!("更新失败，旧版 AppImage 已保留: {error}"));
    }
    let _ = std::fs::remove_dir_all(&backup);
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) async fn install(app: &tauri::AppHandle) -> Result<(), crate::AppError> {
    use tauri::Manager;

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _operation = state.update_operation.lock().await;
    if update_target(app).is_none()
        || super::current_update_mode(&super::get_status(app).await)
            == super::RELEASE_PAGE_UPDATE_MODE
    {
        return super::open_release_page(app).await;
    }
    if !crate::request_file_editors_for_quit(app).await? {
        return Ok(());
    }
    let Some(pending) = state.downloaded_update.lock().await.take() else {
        super::set_status(
            app,
            super::status_error(
                app,
                "in-app",
                &super::update_channel(app),
                "没有已验证的更新包，请重新检查更新。",
            ),
        )
        .await;
        return Ok(());
    };
    let image = if matches!(
        tauri::utils::platform::bundle_type(),
        Some(BundleType::AppImage)
    ) {
        app.env().appimage.map(std::path::PathBuf::from)
    } else {
        None
    };
    let restart_path =
        tauri::process::current_binary(&app.env()).map_err(|error| error.to_string());
    let result = tauri::async_runtime::spawn_blocking(move || {
        let restart_path = restart_path?;
        // Refuse an inherited AppImage environment that would restart another application.
        if image.is_none()
            && tauri::utils::platform::current_exe().map_err(|error| error.to_string())?
                != restart_path
        {
            return Err(
                "启动环境与系统安装路径不一致，请从系统菜单重新打开 FileTerm 后再更新".to_string(),
            );
        }
        validate_restart_path(&restart_path)?;
        match image {
            Some(image) => install_appimage_with_backup(
                &image,
                &pending.bytes,
                std::env::consts::ARCH,
                |staged| std::fs::write(staged, &pending.bytes).map_err(|error| error.to_string()),
            )?,
            None => pending
                .update
                .install(pending.bytes)
                .map_err(|error| error.to_string())?,
        }
        validate_restart_path(&restart_path).map_err(|error| {
            format!("新版已安装，但不能自动重启，请保存工作后从系统菜单手动打开 FileTerm: {error}")
        })
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    if let Err(error) = result {
        super::set_status(
            app,
            super::status_error(
                app,
                "in-app",
                &super::update_channel(app),
                format!("安装更新失败，请检查目录权限、系统授权或包管理器状态后重试: {error}"),
            ),
        )
        .await;
        return Ok(());
    }
    if let Err(error) = crate::services::transfers::shutdown(app).await {
        super::set_status(app, super::status_error(app, "in-app", &super::update_channel(app), format!("新版已安装，但保存传输断点失败，请保存工作后手动退出并重新打开 FileTerm: {error}"))).await;
        return Ok(());
    }
    crate::commands::shutdown_session_workers(app).await;
    app.request_restart();
    Ok(())
}

#[cfg(test)]
mod tests;
