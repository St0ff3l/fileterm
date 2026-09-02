//! Platform-specific application updates.
//!
//! Windows installers use Tauri's signed updater and keep a verified package
//! in memory until the user confirms the restart. Windows portable builds and
//! macOS intentionally use the GitHub Release-page path instead.

use semver::Version;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppError;

const RELEASES_API: &str = "https://api.github.com/repos/St0ff3l/fileterm/releases?per_page=100";
const LATEST_RELEASE_PAGE: &str = "https://github.com/St0ff3l/fileterm/releases/latest";
const ALL_RELEASES_PAGE: &str = "https://github.com/St0ff3l/fileterm/releases";
#[cfg(target_os = "windows")]
const RELEASE_DOWNLOAD_BASE: &str = "https://github.com/St0ff3l/fileterm/releases/download";
const DEFAULT_UPDATE_CHANNEL: &str = "stable";
const RELEASE_PAGE_UPDATE_MODE: &str = "release-page";
#[cfg(any(test, target_os = "windows"))]
const IN_APP_UPDATE_MODE: &str = "in-app";

#[derive(Clone, Debug, Deserialize)]
struct GithubRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

#[derive(Clone, Debug)]
struct SelectedRelease {
    tag_name: String,
    version: Version,
    html_url: String,
}

#[derive(Clone, Debug)]
struct UpdateSelection {
    channel: String,
    release: SelectedRelease,
}

#[cfg(target_os = "windows")]
pub struct WindowsDownloadedUpdate {
    update: tauri_plugin_updater::Update,
    bytes: Vec<u8>,
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn normalize_update_channel(value: &str) -> &'static str {
    match value {
        "beta" => "beta",
        _ => DEFAULT_UPDATE_CHANNEL,
    }
}

fn update_channel(app: &AppHandle) -> String {
    crate::commands::app_get_ui_preferences(app.clone())
        .map(|preferences| normalize_update_channel(&preferences.update_channel).to_string())
        .unwrap_or_else(|_| DEFAULT_UPDATE_CHANNEL.to_string())
}

#[cfg(any(test, target_os = "windows"))]
const fn windows_update_mode_for_portable(is_portable: bool) -> &'static str {
    if is_portable {
        RELEASE_PAGE_UPDATE_MODE
    } else {
        IN_APP_UPDATE_MODE
    }
}

#[cfg(target_os = "windows")]
fn is_portable_build() -> bool {
    crate::storage::portable_config_directory().is_some()
}

#[cfg(not(target_os = "windows"))]
const fn is_portable_build() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn primary_update_mode() -> &'static str {
    windows_update_mode_for_portable(is_portable_build())
}

#[cfg(not(target_os = "windows"))]
const fn primary_update_mode() -> &'static str {
    RELEASE_PAGE_UPDATE_MODE
}

#[cfg(target_os = "windows")]
fn initial_update_message(update_mode: &str) -> &'static str {
    if update_mode == RELEASE_PAGE_UPDATE_MODE {
        "Windows 便携版将打开 GitHub Release 下载页面。"
    } else {
        "Windows 将下载并验证签名，重启后安装更新。"
    }
}

#[cfg(not(target_os = "windows"))]
const fn initial_update_message(_: &str) -> &'static str {
    "检查 GitHub Release；安装将通过发布页完成。"
}

fn initial_status(app: &AppHandle) -> serde_json::Value {
    let channel = update_channel(app);
    let update_mode = primary_update_mode();

    serde_json::json!({
        "state": "idle",
        "currentVersion": current_version(app),
        "updateMode": update_mode,
        "isPortable": is_portable_build(),
        "updateChannel": channel,
        "message": initial_update_message(update_mode),
    })
}

async fn set_status(app: &AppHandle, status: serde_json::Value) {
    *app.state::<crate::services::workspace::WorkspaceState>()
        .update_status
        .write()
        .await = Some(status.clone());
    let _ = app.emit("app:update-status", status);
}

pub async fn get_status(app: &AppHandle) -> serde_json::Value {
    app.state::<crate::services::workspace::WorkspaceState>()
        .update_status
        .read()
        .await
        .clone()
        .unwrap_or_else(|| initial_status(app))
}

fn parse_version(value: &str) -> Option<Version> {
    Version::parse(value.trim().trim_start_matches(['v', 'V'])).ok()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn fallback_release_page(channel: &str) -> &'static str {
    if channel == "beta" {
        ALL_RELEASES_PAGE
    } else {
        LATEST_RELEASE_PAGE
    }
}

fn status_error(
    app: &AppHandle,
    update_mode: &str,
    channel: &str,
    message: impl Into<String>,
) -> serde_json::Value {
    serde_json::json!({
        "state": "error",
        "currentVersion": current_version(app),
        "updateMode": update_mode,
        "updateChannel": channel,
        "message": message.into(),
    })
}

fn status_not_available(app: &AppHandle, update_mode: &str, channel: &str) -> serde_json::Value {
    serde_json::json!({
        "state": "not-available",
        "currentVersion": current_version(app),
        "updateMode": update_mode,
        "updateChannel": channel,
    })
}

fn select_release(releases: Vec<GithubRelease>, channel: &str) -> Option<SelectedRelease> {
    releases
        .into_iter()
        .filter_map(|release| {
            if release.draft {
                return None;
            }
            let version = parse_version(&release.tag_name)?;
            if channel != "beta" && (release.prerelease || !version.pre.is_empty()) {
                return None;
            }
            let html_url = if release.html_url.trim().is_empty() {
                format!("{ALL_RELEASES_PAGE}/tag/{}", release.tag_name)
            } else {
                release.html_url
            };
            Some(SelectedRelease {
                tag_name: release.tag_name,
                version,
                html_url,
            })
        })
        .max_by(|left, right| {
            left.version
                .cmp(&right.version)
                .then_with(|| left.tag_name.cmp(&right.tag_name))
        })
}

async fn fetch_release_selection(app: &AppHandle) -> Result<UpdateSelection, String> {
    let channel = update_channel(app);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("FileTerm-Tauri")
        .build()
        .map_err(|error| format!("更新检查初始化失败: {error}"))?;
    let response = client
        .get(RELEASES_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("更新检查失败: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("更新检查失败 ({})", response.status()));
    }
    let releases: Vec<GithubRelease> = response
        .json()
        .await
        .map_err(|error| format!("更新元数据无效: {error}"))?;
    let release = select_release(releases, &channel)
        .ok_or_else(|| format!("没有找到符合“{channel}”更新通道的 Release"))?;
    Ok(UpdateSelection { channel, release })
}

async fn check_release_page_update(app: &AppHandle) -> serde_json::Value {
    let selection = match fetch_release_selection(app).await {
        Ok(selection) => selection,
        Err(error) => {
            let channel = update_channel(app);
            return status_error(app, "release-page", &channel, error);
        }
    };
    let current = current_version(app);
    let version = selection.release.version.to_string();
    if !is_newer(&version, &current) {
        return status_not_available(app, "release-page", &selection.channel);
    }
    serde_json::json!({
        "state": "available",
        "currentVersion": current,
        "updateMode": RELEASE_PAGE_UPDATE_MODE,
        "isPortable": is_portable_build(),
        "updateChannel": selection.channel,
        "availableVersion": version,
        "releaseTag": selection.release.tag_name,
        "releaseUrl": selection.release.html_url,
    })
}

#[cfg(target_os = "windows")]
fn updater_manifest_url(tag: &str) -> Result<url::Url, String> {
    let mut endpoint =
        url::Url::parse(RELEASE_DOWNLOAD_BASE).map_err(|error| format!("更新地址无效: {error}"))?;
    {
        let mut segments = endpoint
            .path_segments_mut()
            .map_err(|_| "更新地址不支持路径追加".to_string())?;
        segments.push(tag).push("latest.json");
    }
    Ok(endpoint)
}

#[cfg(target_os = "windows")]
fn updater_for_release(
    app: &AppHandle,
    tag: &str,
) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;

    let endpoint = updater_manifest_url(tag)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| format!("更新地址配置失败: {error}"))?
        .build()
        .map_err(|error| format!("自动更新初始化失败: {error}"))?;
    Ok(updater)
}

#[cfg(target_os = "windows")]
async fn check_windows_update(app: &AppHandle) -> serde_json::Value {
    let selection = match fetch_release_selection(app).await {
        Ok(selection) => selection,
        Err(error) => {
            let channel = update_channel(app);
            return status_error(app, "in-app", &channel, error);
        }
    };
    let current = current_version(app);
    let version = selection.release.version.to_string();
    if !is_newer(&version, &current) {
        return status_not_available(app, "in-app", &selection.channel);
    }

    let updater = match updater_for_release(app, &selection.release.tag_name) {
        Ok(updater) => updater,
        Err(error) => {
            crate::services::logging::warn(
                app,
                "update",
                format!("in-app updater unavailable, using release page fallback: {error}"),
            );
            let mut fallback = check_release_page_update(app).await;
            if fallback.get("state").and_then(serde_json::Value::as_str) == Some("available") {
                fallback["message"] = serde_json::Value::String(
                    "Windows 自动更新暂不可用，已切换到 GitHub 下载。".to_string(),
                );
            }
            return fallback;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => serde_json::json!({
            "state": "available",
            "currentVersion": current,
            "updateMode": "in-app",
            "updateChannel": selection.channel,
            "availableVersion": update.version,
            "releaseTag": selection.release.tag_name,
            "releaseUrl": selection.release.html_url,
        }),
        Ok(None) => status_not_available(app, "in-app", &selection.channel),
        Err(error) => {
            crate::services::logging::warn(
                app,
                "update",
                format!("in-app update check failed, using release page fallback: {error}"),
            );
            let mut fallback = check_release_page_update(app).await;
            if fallback.get("state").and_then(serde_json::Value::as_str) == Some("available") {
                fallback["message"] = serde_json::Value::String(
                    "Windows 自动更新暂不可用，已切换到 GitHub 下载。".to_string(),
                );
            }
            fallback
        }
    }
}

pub async fn check(app: &AppHandle) -> Result<serde_json::Value, AppError> {
    let update_check = app
        .state::<crate::services::workspace::WorkspaceState>()
        .update_check
        .clone();
    let check_guard = match update_check.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            // Another window already started the network request. Wait for it
            // instead of issuing a duplicate release/updater request.
            let guard = update_check.lock().await;
            drop(guard);
            return Ok(get_status(app).await);
        }
    };

    #[cfg(target_os = "windows")]
    {
        app.state::<crate::services::workspace::WorkspaceState>()
            .windows_downloaded_update
            .lock()
            .await
            .take();
    }

    crate::services::logging::info(app, "update", "check started");
    let channel = update_channel(app);
    set_status(
        app,
        serde_json::json!({
            "state": "checking",
            "currentVersion": current_version(app),
            "updateMode": primary_update_mode(),
            "updateChannel": channel,
        }),
    )
    .await;

    #[cfg(target_os = "windows")]
    let status = if is_portable_build() {
        check_release_page_update(app).await
    } else {
        check_windows_update(app).await
    };
    #[cfg(not(target_os = "windows"))]
    let status = check_release_page_update(app).await;

    let state = status
        .get("state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if state == "error" {
        crate::services::logging::warn(app, "update", "check completed state=error");
    } else {
        crate::services::logging::info(app, "update", format!("check completed state={state}"));
    }
    set_status(app, status.clone()).await;
    drop(check_guard);
    Ok(status)
}

pub async fn open_release_page(app: &AppHandle) -> Result<(), AppError> {
    let status = get_status(app).await;
    let url = status
        .get("releaseUrl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(LATEST_RELEASE_PAGE);
    let result = open::that(url).map_err(|error| AppError::Command(error.to_string()));
    match &result {
        Ok(()) => crate::services::logging::info(app, "update", "release page opened"),
        Err(error) => crate::services::logging::error(
            app,
            "update",
            format!("open release page failed: {error}"),
        ),
    }
    result
}

#[cfg(target_os = "windows")]
fn current_update_mode(status: &serde_json::Value) -> &str {
    status
        .get("updateMode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("in-app")
}

#[cfg(target_os = "windows")]
async fn download_windows_update(app: &AppHandle) -> Result<(), AppError> {
    let update_operation = app
        .state::<crate::services::workspace::WorkspaceState>()
        .update_operation
        .clone();
    let _operation_guard = update_operation.lock().await;
    let existing_status = get_status(app).await;
    if primary_update_mode() == RELEASE_PAGE_UPDATE_MODE
        || current_update_mode(&existing_status) == RELEASE_PAGE_UPDATE_MODE
    {
        return open_release_page(app).await;
    }

    let channel = update_channel(app);
    let status_channel = existing_status
        .get("updateChannel")
        .and_then(serde_json::Value::as_str);
    let status_tag = existing_status
        .get("releaseTag")
        .and_then(serde_json::Value::as_str)
        .filter(|_| status_channel == Some(channel.as_str()))
        .map(str::to_string);
    let status_release_url = existing_status
        .get("releaseUrl")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let (release_tag, release_url) = match status_tag {
        Some(tag) => (
            tag,
            status_release_url.unwrap_or_else(|| fallback_release_page(&channel).to_string()),
        ),
        None => match fetch_release_selection(app).await {
            Ok(selection) => (selection.release.tag_name, selection.release.html_url),
            Err(error) => {
                set_status(app, status_error(app, "in-app", &channel, error)).await;
                return Ok(());
            }
        },
    };

    let updater = match updater_for_release(app, &release_tag) {
        Ok(updater) => updater,
        Err(error) => {
            set_status(app, status_error(app, "in-app", &channel, error)).await;
            return Ok(());
        }
    };
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => {
            let status = status_not_available(app, "in-app", &channel);
            set_status(app, status).await;
            return Ok(());
        }
        Err(error) => {
            let status = status_error(
                app,
                "in-app",
                &channel,
                format!("重新检查更新失败: {error}"),
            );
            set_status(app, status).await;
            return Ok(());
        }
    };

    let version = update.version.clone();
    let current = current_version(app);
    set_status(
        app,
        serde_json::json!({
            "state": "downloading", "currentVersion": current, "updateMode": "in-app",
            "updateChannel": channel.clone(), "availableVersion": version, "releaseTag": release_tag.clone(),
            "releaseUrl": release_url.clone(), "progress": 0,
        }),
    )
    .await;

    let app_for_progress = app.clone();
    let status_store = app
        .state::<crate::services::workspace::WorkspaceState>()
        .update_status
        .clone();
    let progress_current = current_version(app);
    let progress_version = version.clone();
    let progress_channel = channel.clone();
    let progress_tag = release_tag.clone();
    let progress_url = release_url.clone();
    let mut received = 0_u64;
    let mut last_progress = 0_u64;
    let bytes = update
        .download(
            move |chunk_length, content_length| {
                received = received.saturating_add(chunk_length as u64);
                let progress = content_length
                    .filter(|total| *total > 0)
                    .map(|total| (received.saturating_mul(100) / total).min(100))
                    .unwrap_or(0);
                if progress == last_progress && progress != 100 {
                    return;
                }
                last_progress = progress;
                let status = serde_json::json!({
                    "state": "downloading", "currentVersion": progress_current.clone(),
                    "updateMode": "in-app", "updateChannel": progress_channel.clone(),
                    "availableVersion": progress_version.clone(), "releaseTag": progress_tag.clone(),
                    "releaseUrl": progress_url.clone(),
                    "progress": progress,
                });
                if let Ok(mut current_status) = status_store.try_write() {
                    *current_status = Some(status.clone());
                }
                let _ = app_for_progress.emit("app:update-status", status);
            },
            || {},
        )
        .await;

    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(error) => {
            let status = serde_json::json!({
                "state": "error", "currentVersion": current_version(app), "updateMode": "in-app",
                "updateChannel": channel, "availableVersion": version, "releaseTag": release_tag,
                "releaseUrl": release_url,
                "message": format!("更新包下载或签名验证失败: {error}"),
            });
            set_status(app, status).await;
            crate::services::logging::warn(
                app,
                "update",
                "download or signature verification failed",
            );
            return Ok(());
        }
    };

    *app.state::<crate::services::workspace::WorkspaceState>()
        .windows_downloaded_update
        .lock()
        .await = Some(WindowsDownloadedUpdate { update, bytes });
    let status = serde_json::json!({
        "state": "downloaded", "currentVersion": current_version(app), "updateMode": "in-app",
        "updateChannel": channel, "availableVersion": version, "releaseTag": release_tag,
        "releaseUrl": release_url,
    });
    set_status(app, status).await;
    crate::services::logging::info(app, "update", "signed update downloaded and verified");
    Ok(())
}

#[cfg(target_os = "windows")]
async fn install_windows_update(app: &AppHandle) -> Result<(), AppError> {
    if primary_update_mode() == RELEASE_PAGE_UPDATE_MODE {
        return open_release_page(app).await;
    }

    let update_operation = app
        .state::<crate::services::workspace::WorkspaceState>()
        .update_operation
        .clone();
    let _operation_guard = update_operation.lock().await;
    let pending = app
        .state::<crate::services::workspace::WorkspaceState>()
        .windows_downloaded_update
        .lock()
        .await
        .take();
    let Some(pending) = pending else {
        let status = serde_json::json!({
            "state": "error", "currentVersion": current_version(app), "updateMode": "in-app",
            "message": "没有已验证的更新包，请重新检查更新。",
        });
        set_status(app, status).await;
        return Ok(());
    };

    crate::services::logging::info(app, "update", "launching verified Windows installer");
    if let Err(error) = pending.update.install(pending.bytes) {
        let status = serde_json::json!({
            "state": "error", "currentVersion": current_version(app), "updateMode": "in-app",
            "message": format!("启动更新安装器失败: {error}"),
        });
        set_status(app, status).await;
    }
    Ok(())
}

pub async fn download(app: &AppHandle) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        return download_windows_update(app).await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_release_page(app).await
    }
}

pub async fn install(app: &AppHandle) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        return install_windows_update(app).await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_release_page(app).await
    }
}

#[cfg(test)]
mod tests {
    use super::{is_newer, select_release, windows_update_mode_for_portable, GithubRelease};

    #[test]
    fn portable_windows_builds_use_the_release_page_instead_of_the_nsis_updater() {
        assert_eq!(windows_update_mode_for_portable(true), "release-page");
        assert_eq!(windows_update_mode_for_portable(false), "in-app");
    }

    #[test]
    fn compares_release_tags_numerically() {
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("v1.1.1", "1.1.1"));
        assert!(is_newer("v2.2.0-beta.3", "2.2.0-beta.2"));
        assert!(is_newer("v2.2.0", "2.2.0-beta.2"));
        assert!(!is_newer("v2.2.0-beta.2", "2.2.0"));
    }

    fn release(tag_name: &str, prerelease: bool) -> GithubRelease {
        GithubRelease {
            tag_name: tag_name.to_string(),
            html_url: format!("https://github.com/St0ff3l/fileterm/releases/tag/{tag_name}"),
            prerelease,
            draft: false,
        }
    }

    #[test]
    fn stable_channel_ignores_prereleases() {
        let selected = select_release(
            vec![
                release("v2.2.0-beta.3", true),
                release("v2.2.0-beta.2", false),
                release("v2.1.8", false),
            ],
            "stable",
        )
        .expect("stable release should be selected");

        assert_eq!(selected.tag_name, "v2.1.8");
    }

    #[test]
    fn beta_channel_includes_stable_releases() {
        let selected = select_release(
            vec![release("v2.2.0-beta.3", true), release("v2.2.0", false)],
            "beta",
        )
        .expect("beta release should be selected");

        assert_eq!(selected.tag_name, "v2.2.0");
    }

    #[test]
    fn draft_and_invalid_tags_are_ignored() {
        let mut draft = release("v9.0.0", false);
        draft.draft = true;
        let selected = select_release(
            vec![
                draft,
                release("not-a-version", true),
                release("v2.2.0-beta.2", true),
            ],
            "beta",
        )
        .expect("valid prerelease should be selected");

        assert_eq!(selected.tag_name, "v2.2.0-beta.2");
    }
}
