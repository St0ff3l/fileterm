// Names are single path components. Remote paths use POSIX separators even
// when the client runs on Windows; local downloads must also obey host rules.
fn validate_transfer_name(name: &str, windows: bool) -> Result<(), AppError> {
    let invalid = name.is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\0'])
        || (windows
            && (name.chars().any(|c| c < ' ' || "\\:<>\"|?*".contains(c))
                || name.ends_with(['.', ' '])
                || is_windows_device_name(name)));
    if invalid {
        return Err(transfer_error(format!(
            "无效的传输目标名称：{name:?}，请使用当前文件系统支持的单个文件或目录名"
        )));
    }
    Ok(())
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}

fn download_target_name(
    remote_path: &str,
    target_name: Option<String>,
    windows: bool,
) -> Result<String, AppError> {
    let name = target_name.unwrap_or_else(|| {
        remote_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string()
    });
    validate_transfer_name(&name, windows)?;
    Ok(name)
}

#[cfg(test)]
#[path = "tests/path_validation_tests.rs"]
mod path_validation_tests;
