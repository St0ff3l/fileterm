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
mod path_validation_tests {
    use super::{download_target_name, validate_transfer_name};

    #[test]
    fn target_names_cannot_replace_or_escape_the_selected_directory() {
        for windows in [false, true] {
            for name in [
                "",
                ".",
                "..",
                "/tmp/outside",
                "../outside",
                "nested/file",
                "bad\0name",
            ] {
                assert!(
                    download_target_name("/srv/file", Some(name.into()), windows).is_err(),
                    "{name:?}"
                );
                assert!(validate_transfer_name(name, windows).is_err());
            }
        }
    }

    #[test]
    fn windows_downloads_reject_path_prefixes_streams_and_name_aliases() {
        for name in [
            r"..\outside",
            r"C:\outside",
            "C:outside",
            r"\\server\share",
            "file:stream",
            "NUL.txt",
            "COM1",
            "lpt².log",
            "name.",
            "name ",
        ] {
            assert!(validate_transfer_name(name, true).is_err(), "{name:?}");
            assert!(
                download_target_name(&format!("/srv/{name}"), None, true).is_err(),
                "{name:?}"
            );
        }
    }

    #[test]
    fn downloads_preserve_valid_names_and_parse_remote_paths_independently_of_host() {
        for windows in [false, true] {
            for name in ["中文 空格.txt", ".hidden", "COM10.txt", "report (1).csv"] {
                assert_eq!(
                    download_target_name(&format!("/srv/{name}/"), None, windows).unwrap(),
                    name
                );
            }
            assert!(download_target_name("/", None, windows).is_err());
            assert_eq!(
                download_target_name("/", Some("root-copy".into()), windows).unwrap(),
                "root-copy"
            );
        }
        for name in [r"literal\name", "report:2026", "NUL.txt"] {
            assert_eq!(
                download_target_name(&format!("/srv/{name}"), None, false).unwrap(),
                name
            );
        }
    }
}
