const ROOT_ACCESS_DIRECTORY_MARKER: &str = "__FILETERM_ACCESS_DIRECTORY__";

/// A file pane may hold an SFTP alias, while privileged exec uses shell paths.
/// Derive mappings from an observed CWD rather than assuming a volume or home.
fn root_access_directory_candidates(path: &str, shell_cwd: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if !path.is_empty() && path != "." {
        push_candidate(&mut candidates, path);
    }
    if let Some(cwd) = shell_cwd.filter(|cwd| cwd.starts_with('/')) {
        for alias in shell_cwd_sftp_path_candidates(cwd) {
            if alias == "/" || alias == cwd {
                continue;
            }
            if path == alias {
                push_candidate(&mut candidates, cwd);
            } else if let Some(suffix) = path.strip_prefix(&format!("{alias}/")) {
                push_candidate(&mut candidates, &format!("{cwd}/{suffix}"));
            }
        }
        if let Some(prefix) = synology_volume_prefix(cwd) {
            push_candidate(
                &mut candidates,
                &format!("{prefix}/{trimmed}", trimmed = path.trim_start_matches('/')),
            );
        }
        if cwd.starts_with("/var/services/") {
            push_candidate(
                &mut candidates,
                &format!(
                    "/var/services/{trimmed}",
                    trimmed = path.trim_start_matches('/')
                ),
            );
        }
        push_candidate(&mut candidates, cwd);
    }
    // The filesystem root is distinct from root's home (/root), which can be
    // absent on appliances, containers, or accounts with disabled home service.
    push_candidate(&mut candidates, "/");
    candidates
}

fn root_access_directory_command(path: &str, shell_cwd: Option<&str>) -> String {
    let paths = root_access_directory_candidates(path, shell_cwd)
        .iter()
        .map(|path| shell_quote(path))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "for candidate in {paths}; do\n  if [ -d \"$candidate\" ]; then\n    if [ ! -r \"$candidate\" ] || [ ! -x \"$candidate\" ]; then\n      printf '没有读取目录权限: %s\\n' \"$candidate\" >&2\n      exit 1\n    fi\n    cd \"$candidate\" || exit 1\n    printf '%s' {}\n    pwd -P\n    exit 0\n  fi\n  if [ -e \"$candidate\" ]; then\n    printf '远程路径不是目录: %s\\n' \"$candidate\" >&2\n    exit 1\n  fi\ndone\nexit 1",
        shell_quote(ROOT_ACCESS_DIRECTORY_MARKER),
    )
}

fn parse_root_access_directory(output: &str) -> Result<String, String> {
    let path = output
        .split_once(ROOT_ACCESS_DIRECTORY_MARKER)
        .map(|(_, path)| path.trim_end_matches(['\r', '\n']))
        .filter(|path| path.starts_with('/') && !path.contains(['\r', '\n', '\0']))
        .ok_or_else(|| "root 文件通道未返回有效的系统目录".to_string())?;
    Ok(path.to_string())
}

#[cfg(test)]
include!("access_directory_tests.rs");
