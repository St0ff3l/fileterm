//! Release-page update checks. Tauri's signed in-app updater is intentionally
//! not enabled until the release signing key and platform notarization assets
//! are provisioned; this service still gives packaged users an authenticated
//! version check and a safe handoff to the GitHub release page.

use tauri::{AppHandle, Emitter, Manager};

use crate::AppError;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/St0ff3l/fileterm/releases/latest";

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn initial_status(app: &AppHandle) -> serde_json::Value {
    serde_json::json!({
        "state": "idle",
        "currentVersion": current_version(app),
        "updateMode": "release-page",
        "message": "检查 GitHub Release；安装将通过发布页完成。",
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

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim_start_matches('v')
        .split(|character: char| !(character.is_ascii_digit()))
        .filter(|part| !part.is_empty())
        .take(4)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let candidate = version_parts(candidate);
    let current = version_parts(current);
    let width = candidate.len().max(current.len());
    (0..width).find_map(|index| {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        (left != right).then_some(left > right)
    }) == Some(true)
}

pub async fn check(app: &AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::logging::info(app, "update", "check started");
    set_status(
        app,
        serde_json::json!({ "state": "checking", "currentVersion": current_version(app), "updateMode": "release-page" }),
    )
    .await;
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("FileTerm-Tauri")
        .build()
        .map_err(|error| AppError::Command(error.to_string()))?
        .get(LATEST_RELEASE_API)
        .send()
        .await;
    let status = match response {
        Ok(response) if response.status().is_success() => {
            let release: serde_json::Value = response
                .json()
                .await
                .map_err(|error| AppError::Command(format!("更新元数据无效: {error}")))?;
            let version = release
                .get("tag_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let release_url = release
                .get("html_url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("https://github.com/St0ff3l/fileterm/releases/latest");
            if !version.is_empty() && is_newer(version, &current_version(app)) {
                serde_json::json!({
                    "state": "available", "currentVersion": current_version(app), "updateMode": "release-page",
                    "availableVersion": version.trim_start_matches('v'), "releaseUrl": release_url,
                })
            } else {
                serde_json::json!({ "state": "not-available", "currentVersion": current_version(app), "updateMode": "release-page" })
            }
        }
        Ok(response) => serde_json::json!({
            "state": "error", "currentVersion": current_version(app), "updateMode": "release-page",
            "message": format!("更新检查失败 ({})", response.status()),
        }),
        Err(error) => serde_json::json!({
            "state": "error", "currentVersion": current_version(app), "updateMode": "release-page",
            "message": format!("更新检查失败: {error}"),
        }),
    };
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
    Ok(status)
}

pub async fn open_release_page(app: &AppHandle) -> Result<(), AppError> {
    let status = get_status(app).await;
    let url = status
        .get("releaseUrl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("https://github.com/St0ff3l/fileterm/releases/latest");
    let result = open::that(url).map_err(|error| AppError::Command(error.to_string()));
    match &result {
        Ok(()) => crate::services::logging::info(app, "update", "release page opened"),
        Err(error) => crate::services::logging::error(app, "update", format!("open release page failed: {error}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_release_tags_numerically() {
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("v1.1.1", "1.1.1"));
    }
}
