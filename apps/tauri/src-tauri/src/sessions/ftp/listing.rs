async fn rename_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    source: &str,
    destination: &str,
) -> Result<(), String> {
    ftp.rename(source, destination)
        .await
        .map_err(|error| error.to_string())
}

async fn chmod_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    mode: &str,
) -> Result<(), String> {
    ftp.site(format!("CHMOD {mode} {path}"))
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn remove_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<(), String> {
    match ftp.rm(path).await {
        Ok(()) => Ok(()),
        Err(error) if is_ftp_file_not_found(&error) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn is_ftp_file_not_found(error: &FtpError) -> bool {
    let FtpError::UnexpectedResponse(response) = error else {
        return false;
    };
    if response.status != Status::FileUnavailable {
        return false;
    }
    let message = String::from_utf8_lossy(&response.body).to_lowercase();
    [
        "not found",
        "no such",
        "does not exist",
        "cannot find",
        "can't find",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn is_ftp_existing_path(error: &FtpError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "file exists",
        "already exists",
        "directory exists",
        "path exists",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

async fn quit<T: TokioTlsStream + Send>(ftp: &mut ImplAsyncFtpStream<T>) -> Result<(), String> {
    ftp.quit().await.map_err(|error| error.to_string())
}

async fn list_files<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<Vec<Value>, String> {
    let mut state = FtpListingState::default();
    list_files_with_state(ftp, path, &mut state).await
}

async fn list_files_with_state<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    state: &mut FtpListingState,
) -> Result<Vec<Value>, String> {
    let lines = if state.mlsd_disabled {
        ftp.list(Some(path))
            .await
            .map_err(|error| error.to_string())?
    } else {
        match ftp.mlsd(Some(path)).await {
            Ok(lines) if lines.iter().all(|line| looks_like_mlsd_line(line)) => lines,
            Ok(lines) => {
                // A few embedded servers accept MLSD but return classic LIST
                // rows. Keep those rows, but do not pay the failed capability
                // probe again on every directory navigation.
                state.mlsd_disabled = true;
                lines
            }
            Err(_) => {
                state.mlsd_disabled = true;
                ftp.list(Some(path))
                    .await
                    .map_err(|error| error.to_string())?
            }
        }
    };
    let mut files = Vec::new();
    if path != "/" {
        files.push(serde_json::json!({
            "name": "..", "path": parent_remote_path(path), "type": "folder", "size": "-",
            "modified": "", "permission": "", "ownerGroup": ""
        }));
    }
    for line in lines {
        // `File::from_str` deliberately tries POSIX and DOS LIST formats
        // before MLSD. Some embedded FTP servers accept MLSD but still send
        // classic Unix LIST rows; parsing those as MLSD first succeeds with
        // the entire row as the name and zeroed metadata.
        let Some(parsed) = parse_ftp_listing_line(&line) else {
            continue;
        };
        let entry = parsed.entry;
        let name = entry.name();
        if matches!(name, "." | "..") {
            continue;
        }
        let full_path = join_remote_path(path, name);
        let is_symlink = entry.is_symlink();
        let mut is_directory = entry.is_directory();
        let mut size = entry.size();
        if !parsed.type_is_trusted || is_symlink {
            let resolved = resolve_untrusted_ftp_entry(ftp, &full_path, state).await;
            is_directory = resolved.0;
            if let Some(resolved_size) = resolved.1 {
                size = resolved_size;
            }
        } else {
            state.resolved_types.insert(full_path.clone(), is_directory);
        }
        let modified = entry
            .modified()
            .duration_since(UNIX_EPOCH)
            .map(|value| super::ssh::format_unix_ts(value.as_secs() as i64))
            .unwrap_or_default();
        let permission = ftp_listing_permission(&line);
        files.push(serde_json::json!({
            "name": name,
            "path": full_path,
            "type": super::ssh::effective_remote_file_type(
                is_directory,
                is_symlink,
                is_directory,
            ),
            "isSymlink": is_symlink,
            "size": if is_directory { "-".to_string() } else { format_bytes(size as u64) },
            "modified": modified,
            "permission": permission,
            "ownerGroup": match (entry.uid(), entry.gid()) { (Some(uid), Some(gid)) => format!("{uid}/{gid}"), _ => String::new() },
        }));
    }
    files.sort_by(|left, right| {
        let left_folder = left.get("type").and_then(Value::as_str) == Some("folder");
        let right_folder = right.get("type").and_then(Value::as_str) == Some("folder");
        right_folder
            .cmp(&left_folder)
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    Ok(files)
}

async fn resolve_untrusted_ftp_entry<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    state: &mut FtpListingState,
) -> (bool, Option<usize>) {
    if let Some(is_directory) = state.resolved_types.get(path).copied() {
        return (is_directory, state.resolved_sizes.get(path).copied());
    }

    if !state.mlst_disabled {
        match ftp.mlst(Some(path)).await {
            Ok(line) if looks_like_mlsd_line(&line) => {
                if let Ok(entry) = ListParser::parse_mlst(&line) {
                    let is_directory = entry.is_directory();
                    state.resolved_types.insert(path.to_string(), is_directory);
                    if !is_directory {
                        state.resolved_sizes.insert(path.to_string(), entry.size());
                    }
                    return (is_directory, Some(entry.size()));
                }
                state.mlst_disabled = true;
            }
            Ok(_) => state.mlst_disabled = true,
            Err(_) => state.mlst_disabled = true,
        }
    }

    if !state.size_disabled {
        match ftp.size(path).await {
            Ok(size) => {
                state.resolved_types.insert(path.to_string(), false);
                state.resolved_sizes.insert(path.to_string(), size);
                return (false, Some(size));
            }
            Err(error) => {
                if is_unsupported_ftp_command(&error.to_string()) {
                    state.size_disabled = true;
                }
            }
        }
    }

    let previous_path = ftp.pwd().await.ok();
    let is_directory = ftp.cwd(path).await.is_ok();
    if is_directory {
        if let Some(previous_path) = previous_path {
            let _ = ftp.cwd(previous_path).await;
        }
    }
    state.resolved_types.insert(path.to_string(), is_directory);
    (is_directory, None)
}

fn parse_ftp_listing_line(line: &str) -> Option<ParsedFtpListing> {
    if let Ok(entry) = ListParser::parse_posix(line) {
        return Some(ParsedFtpListing {
            entry,
            type_is_trusted: true,
        });
    }
    if let Ok(entry) = ListParser::parse_dos(line) {
        return Some(ParsedFtpListing {
            entry,
            type_is_trusted: true,
        });
    }
    if looks_like_mlsd_line(line) {
        if let Ok(entry) = ListParser::parse_mlsd(line) {
            return Some(ParsedFtpListing {
                entry,
                type_is_trusted: true,
            });
        }
    }
    line.parse::<ListedFile>()
        .ok()
        .map(|entry| ParsedFtpListing {
            entry,
            type_is_trusted: false,
        })
}

fn looks_like_mlsd_line(line: &str) -> bool {
    let facts = line.trim_start().split_once(' ').map(|value| value.0);
    facts.is_some_and(|facts| {
        facts.contains(';')
            && facts.split(';').any(|fact| {
                fact.split_once('=')
                    .is_some_and(|(key, value)| !key.is_empty() && !value.is_empty())
            })
    })
}

fn is_unsupported_ftp_command(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "500",
        "501",
        "502",
        "504",
        "unknown command",
        "not implemented",
        "unsupported",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn ftp_listing_permission(line: &str) -> String {
    let token = line.split_whitespace().next().unwrap_or_default();
    if token.len() == 10 && matches!(token.as_bytes().first(), Some(b'-' | b'd' | b'l')) {
        return token.to_string();
    }

    let lower = line.to_ascii_lowercase();
    let Some(mode_start) = lower.find("unix.mode=") else {
        return String::new();
    };
    let mode = line[mode_start + "unix.mode=".len()..]
        .split(';')
        .next()
        .unwrap_or_default();
    let mode = mode.strip_prefix('0').unwrap_or(mode);
    if mode.len() != 3 || !mode.bytes().all(|value| matches!(value, b'0'..=b'7')) {
        return String::new();
    }
    let kind = if lower.contains("type=dir;") {
        'd'
    } else {
        '-'
    };
    let mut permission = String::with_capacity(10);
    permission.push(kind);
    for value in mode.bytes().map(|value| value - b'0') {
        permission.push(if value & 4 != 0 { 'r' } else { '-' });
        permission.push(if value & 2 != 0 { 'w' } else { '-' });
        permission.push(if value & 1 != 0 { 'x' } else { '-' });
    }
    permission
}
