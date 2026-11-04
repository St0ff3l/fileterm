use crate::AppError;
use tauri::AppHandle;

pub fn validate_url(value: &str) -> Result<url::Url, AppError> {
    let parsed = url::Url::parse(value)
        .map_err(|error| AppError::Command(format!("外部链接无效: {error}")))?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(parsed)
    } else {
        Err(AppError::Command(
            "仅允许打开 http 或 https 外部链接".into(),
        ))
    }
}

pub async fn open_url(app: &AppHandle, value: &str) -> Result<(), AppError> {
    let url = validate_url(value)?;
    // Desktop launchers can wait for the browser. Never block Tauri's UI thread.
    let result = tokio::task::spawn_blocking(move || launch_url(url.as_str()))
        .await
        .map_err(|error| AppError::Command(format!("Browser launcher task failed: {error}")))?;
    if let Err(error) = &result {
        super::logging::error(app, "external-link", error.to_string());
    }
    result
}

#[cfg(not(target_os = "linux"))]
fn launch_url(url: &str) -> Result<(), AppError> {
    open::that(url).map_err(|error| AppError::Command(error.to_string()))
}

#[cfg(target_os = "linux")]
fn launch_url(url: &str) -> Result<(), AppError> {
    // open::that stops when an installed launcher exits with a nonzero status.
    // On LXQt, xdg-open can exist but fail because its desktop helper is absent.
    // Continue with GIO and the remaining system launchers in that case.
    try_launchers(open::commands(url), run_launcher)
}

#[cfg(any(target_os = "linux", test))]
fn run_launcher(command: &mut std::process::Command) -> Result<(), String> {
    use std::process::Stdio;

    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("exited with {status}"))
    }
}

#[cfg(any(target_os = "linux", test))]
fn try_launchers(
    commands: Vec<std::process::Command>,
    mut run: impl FnMut(&mut std::process::Command) -> Result<(), String>,
) -> Result<(), AppError> {
    let mut failures = Vec::new();
    for mut command in commands {
        match run(&mut command) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!(
                "{}: {error}",
                command.get_program().to_string_lossy()
            )),
        }
    }
    Err(AppError::Command(format!(
        "Could not open the default browser. Check xdg-utils and the default HTTP/HTTPS browser association. {}",
        failures.join("; ")
    )))
}

#[cfg(test)]
mod tests;
