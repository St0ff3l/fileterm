fn root_stat_shell_command(path: &str) -> String {
    format!(
        "if [ -e {} ] && [ ! -d {} ]; then stat -c '%s|%Y' -- {}; fi",
        shell_quote(path),
        shell_quote(path),
        shell_quote(path),
    )
}

fn root_editor_write_shell_command(staging_path: &str, expected_size: u64) -> String {
    format!(
        "set -e\nbase64 -d > {}\ntest \"$(wc -c < {})\" -eq {}",
        shell_quote(staging_path),
        shell_quote(staging_path),
        expected_size,
    )
}

fn root_editor_verify_shell_command(path: &str, expected_size: u64) -> String {
    format!(
        "set -e\ntest \"$(wc -c < {})\" -eq {}",
        shell_quote(path),
        expected_size,
    )
}

async fn stat_root_remote_file(
    handle: &Handle<ClientHandler>,
    path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Option<TransferFileStat>, String> {
    // A missing .fileterm-part means a fresh upload, not a failed stat.
    // Keep the shell command successful in that case so exec status handling
    // can distinguish it from an actual root/su failure.
    let command = root_stat_shell_command(path);
    let output =
        exec_shell_file_command(handle, &command, access_method, sudo_user, sudo_password).await?;
    let Some((size, modified_at)) = output
        .trim()
        .lines()
        .next()
        .and_then(|line| line.split_once('|'))
    else {
        return Ok(None);
    };
    let size = size
        .trim()
        .parse::<u64>()
        .map_err(|_| "无法解析 root 文件大小".to_string())?;
    let modified_at = modified_at
        .trim()
        .parse::<u64>()
        .ok()
        .map(|value| value * 1000);
    Ok(Some(TransferFileStat { size, modified_at }))
}

async fn replace_root_remote_file(
    handle: &Handle<ClientHandler>,
    partial_path: &str,
    destination_path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let parent = std::path::Path::new(destination_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let command = root_replace_remote_file_command(&parent, partial_path, destination_path);
    exec_shell_file_command(handle, &command, access_method, sudo_user, sudo_password)
        .await
        .map(|_| ())
}

fn root_replace_remote_file_command(
    parent: &str,
    partial_path: &str,
    destination_path: &str,
) -> String {
    format!(
        "set -e\nmkdir -p {parent}\nif [ -L {destination} ]; then\n  target=\"$(readlink -f -- {destination} 2>/dev/null || true)\"\n  if [ -n \"$target\" ] && [ -f \"$target\" ]; then\n    chown --reference=\"$target\" -- {partial} 2>/dev/null || true\n    chmod --reference=\"$target\" -- {partial} 2>/dev/null || true\n    mv -f -- {partial} \"$target\"\n  else\n    cat -- {partial} > {destination}\n    rm -f -- {partial}\n  fi\nelse\n  if [ -e {destination} ]; then\n    chown --reference={destination} -- {partial} 2>/dev/null || true\n    chmod --reference={destination} -- {partial} 2>/dev/null || true\n  fi\n  mv -f -- {partial} {destination}\nfi",
        parent = shell_quote(parent),
        partial = shell_quote(partial_path),
        destination = shell_quote(destination_path),
    )
}

// Shell-backed privileged file commands and binary-safe exec helpers.

/// POSIX shell quoting: wrap in single quotes, escape embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn root_file_command(
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
    command: &str,
) -> (String, Option<String>) {
    let user = sudo_user.as_deref().unwrap_or("root");
    match access_method {
        RootFileAccessMethod::Sudo => {
            let full_command = if sudo_password.is_some() {
                format!(
                    "sudo -S -p '' -u {} sh -lc {}",
                    shell_quote(user),
                    shell_quote(command)
                )
            } else {
                format!(
                    "sudo -n -u {} sh -lc {}",
                    shell_quote(user),
                    shell_quote(command)
                )
            };
            (full_command, sudo_password.clone())
        }
        RootFileAccessMethod::Su => (
            format!(
                "su -s /bin/sh -c {} {}",
                shell_quote(command),
                shell_quote(user)
            ),
            sudo_password.clone(),
        ),
    }
}

/// Add a post-authentication frame to commands executed through `su`.
///
/// A PTY combines the password prompt and command output into one stream. The
/// marker is printed only after `su` has accepted the password, so consumers
/// can discard the prompt without guessing at localized text or corrupting a
/// `stat`/`base64` payload.
fn su_exec_command(command: &str) -> String {
    format!(
        "printf '%s\\n' {}; {}",
        shell_quote(SU_EXEC_OUTPUT_MARKER),
        command
    )
}

fn strip_su_exec_output(output: &str) -> Result<String, String> {
    let Some(marker_start) = output.find(SU_EXEC_OUTPUT_MARKER) else {
        return Err("su root 文件命令未返回认证后的输出标记".to_string());
    };
    let body = &output[marker_start + SU_EXEC_OUTPUT_MARKER.len()..];
    // PTY line discipline may translate LF to CRLF. Normalize it before
    // parsing `find -printf` rows or other line-oriented root command output.
    Ok(body
        .trim_start_matches(['\r', '\n'])
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

async fn request_root_exec_pty(channel: &Channel<russh::client::Msg>) -> Result<(), String> {
    timeout(
        SUDO_VERIFY_TIMEOUT,
        channel.request_pty(
            true,
            "xterm-256color",
            80,
            24,
            0,
            0,
            &[
                (russh::Pty::ECHO, 0),
                (russh::Pty::ECHOE, 0),
                (russh::Pty::ECHOK, 0),
                (russh::Pty::ECHONL, 0),
                (russh::Pty::TTY_OP_ISPEED, 115200),
                (russh::Pty::TTY_OP_OSPEED, 115200),
            ],
        ),
    )
    .await
    .map_err(|_| "su 认证超时：服务器未响应 PTY 请求".to_string())?
    .map_err(|error| format!("su 文件通道无法申请 PTY: {error}"))
}

/// Authenticate a `su` exec channel and return any bytes that followed the
/// post-authentication marker in the same SSH packet.  Streaming upload and
/// download use this handshake before sending/decoding their payload.
async fn wait_for_su_output_marker(
    channel: &mut Channel<russh::client::Msg>,
    password: Option<&str>,
) -> Result<Vec<u8>, String> {
    let marker = SU_EXEC_OUTPUT_MARKER.as_bytes();
    let mut output = Vec::new();
    let mut password_sent = password.is_none();
    loop {
        let message = timeout(SUDO_VERIFY_TIMEOUT, channel.wait())
            .await
            .map_err(|_| "su 认证超时：服务器未在 10 秒内响应".to_string())?;
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if output.len() < 64 * 1024 {
                    output.extend_from_slice(data.as_ref());
                }
                let visible = visible_shell_text(&String::from_utf8_lossy(&output));
                let lower = visible.to_ascii_lowercase();
                let marker_seen = output.windows(marker.len()).any(|window| window == marker);
                if !password_sent
                    && !marker_seen
                    && (lower.contains("password") || visible.contains("密码"))
                {
                    if let Some(password) = password {
                        channel
                            .data_bytes(format!("{password}\n").into_bytes())
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    password_sent = true;
                }
                if let Some(start) = output
                    .windows(marker.len())
                    .position(|window| window == marker)
                {
                    return Ok(output[start + marker.len()..].to_vec());
                }
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                let detail = String::from_utf8_lossy(&output).trim().to_string();
                if root_access_auth_failed(&detail.to_lowercase())
                    || detail.to_lowercase().contains("password")
                    || detail.contains("密码")
                {
                    return Err("su 认证失败：密码错误或未授予 su 权限".to_string());
                }
                let detail = if detail.is_empty() {
                    String::new()
                } else {
                    format!("：{}", detail.chars().take(512).collect::<String>())
                };
                return Err(format!("su 文件命令失败（exit={status}）{detail}"));
            }
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    let detail = String::from_utf8_lossy(&output).trim().to_string();
    if root_access_auth_failed(&detail.to_lowercase())
        || detail.to_lowercase().contains("password")
        || detail.contains("密码")
    {
        Err("su 认证失败：密码错误或未授予 su 权限".to_string())
    } else {
        Err("su root 文件命令未返回认证后的输出标记".to_string())
    }
}

async fn wait_for_root_stream_exit(
    channel: &mut Channel<russh::client::Msg>,
) -> Result<u32, String> {
    let mut exit_status = None;
    let mut detail = String::new();
    loop {
        let message = timeout(SUDO_VERIFY_TIMEOUT, channel.wait())
            .await
            .map_err(|_| "root 文件传输完成后远端命令未退出，已超时".to_string())?;
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if detail.len() < 4096 {
                    detail.push_str(&String::from_utf8_lossy(data.as_ref()));
                }
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                exit_status = Some(status);
            }
            ChannelMsg::Eof | ChannelMsg::Close if exit_status.is_some() => break,
            ChannelMsg::Eof | ChannelMsg::Close => {}
            _ => {}
        }
    }
    let status = exit_status.ok_or_else(|| "root 文件传输未返回退出状态".to_string())?;
    if status != 0 {
        let detail = detail.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!("：{}", detail.chars().take(512).collect::<String>())
        };
        return Err(format!("root 文件传输命令失败（exit={status}）{detail}"));
    }
    Ok(status)
}

/// Execute a `su -c` command through a PTY and complete the password
/// handshake before sending any command input. Some PAM/su combinations drop
/// bytes that arrive before the password prompt, even though a normal shell
/// accepts them from the PTY input queue. The marker printed by
/// `su_exec_command` also gives passwordless/root callers a safe point at
/// which to send payload data.
#[allow(clippy::too_many_arguments)]
async fn exec_su_command_with_pty_input(
    handle: &Handle<ClientHandler>,
    command: &str,
    password: Option<&str>,
    input: Option<&[u8]>,
    send_eof: bool,
) -> Result<(String, Option<u32>), String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    request_root_exec_pty(&channel).await?;
    channel
        .exec(true, command)
        .await
        .map_err(|error| error.to_string())?;

    let marker = SU_EXEC_OUTPUT_MARKER.as_bytes();
    let mut output = Vec::new();
    let mut password_sent = password.is_none();
    let mut input_sent = input.is_none();
    let mut exit_status = None;
    let mut marker_seen = false;
    let mut password_prompt_seen = false;

    loop {
        let message = timeout(SUDO_VERIFY_TIMEOUT, channel.wait())
            .await
            .map_err(|_| "su 认证超时：服务器未在 10 秒内响应".to_string())?;
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                let bytes = data.as_ref();
                if output.len() < 64 * 1024 {
                    let remaining = 64 * 1024 - output.len();
                    output.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
                }
                if output.windows(marker.len()).any(|window| window == marker) {
                    marker_seen = true;
                }
                let visible = visible_shell_text(&String::from_utf8_lossy(&output));
                let lower = visible.to_ascii_lowercase();
                if !marker_seen && (lower.contains("password") || visible.contains("密码")) {
                    password_prompt_seen = true;
                }

                if !password_sent && (password_prompt_seen || marker_seen) {
                    if let Some(password) = password {
                        if password_prompt_seen {
                            channel
                                .data_bytes(format!("{password}\n").into_bytes())
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    password_sent = true;
                }
                if marker_seen && !input_sent {
                    if let Some(input) = input {
                        channel
                            .data_bytes(input.to_vec())
                            .await
                            .map_err(|error| error.to_string())?;
                        if send_eof {
                            channel
                                .data_bytes(vec![0x04])
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    input_sent = true;
                }
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                exit_status = Some(status);
            }
            ChannelMsg::Eof | ChannelMsg::Close if exit_status.is_some() => break,
            ChannelMsg::Eof | ChannelMsg::Close => {}
            _ => {}
        }
    }

    Ok((String::from_utf8_lossy(&output).into_owned(), exit_status))
}

/// Run a shell command through the independent exec channel with the same
/// root strategy observed in the interactive terminal.
async fn exec_shell_file_command(
    handle: &Handle<ClientHandler>,
    command: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<String, String> {
    let command = if access_method == RootFileAccessMethod::Su {
        su_exec_command(command)
    } else {
        command.to_string()
    };
    let (full_cmd, password) = root_file_command(access_method, sudo_user, sudo_password, &command);

    // 整个 exec 包超时：PTY 模式下 root 错误密码可能 retry 多次，channel
    // 不会自然退出。超时后返回错误，前端 loading 能在 10 秒内解除。
    let (output, exit_status) = if access_method == RootFileAccessMethod::Su {
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            exec_su_command_with_pty_input(handle, &full_cmd, password.as_deref(), None, false),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(
                    "root 认证超时：服务器未在 10 秒内响应，可能密码错误或网络中断".to_string(),
                )
            }
        }
    } else if let Some(pwd) = password {
        let stdin = format!("{pwd}\n");
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            crate::sessions::system_metrics::exec_command_with_stdin_status(
                handle, &full_cmd, &stdin,
            ),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(
                    "root 认证超时：服务器未在 10 秒内响应，可能密码错误或网络中断".to_string(),
                )
            }
        }
    } else {
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            crate::sessions::system_metrics::exec_command_with_status(handle, &full_cmd),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => return Err("root 认证超时：服务器未在 10 秒内响应".to_string()),
        }
    };

    let lower = output.to_lowercase();
    if root_access_auth_failed(&lower)
        || lower.contains("a password is required")
        || lower.contains("no password was provided")
        || lower.contains("sudo: permission denied")
        || (access_method == RootFileAccessMethod::Su
            && (lower.contains("password") || output.contains("密码"))
            && !output.contains(SU_EXEC_OUTPUT_MARKER))
    {
        return Err(match access_method {
            RootFileAccessMethod::Sudo => "sudo 认证失败：密码错误或未授予 sudo 权限".to_string(),
            RootFileAccessMethod::Su => "su 认证失败：密码错误或未授予 su 权限".to_string(),
        });
    }

    let command_output = if access_method == RootFileAccessMethod::Su {
        strip_su_exec_output(&output).unwrap_or_else(|_| output.clone())
    } else {
        output.clone()
    };
    let status = exit_status.ok_or_else(|| "root 文件命令未返回退出状态".to_string())?;
    if status != 0 {
        let detail = command_output.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!("：{}", detail.chars().take(512).collect::<String>())
        };
        return Err(format!("root 文件命令失败（exit={status}）{detail}"));
    }
    if access_method == RootFileAccessMethod::Su {
        strip_su_exec_output(&output)
    } else {
        Ok(output)
    }
}

/// List a directory via `find -printf` under the active root strategy.
async fn exec_list_dir_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Vec<Value>, String> {
    let cmd = root_list_shell_command(path);
    let output =
        exec_shell_file_command(handle, &cmd, access_method, sudo_user, sudo_password).await?;
    Ok(parse_root_file_list(&output, path))
}

fn root_list_shell_command(path: &str) -> String {
    // `%y` is the entry type and `%Y` is the type after following a
    // symbolic link. Keep both so the renderer can retain link information
    // without mistaking a link to a regular file for a directory.
    format!(
        "find {} -maxdepth 1 -mindepth 1 -printf '%y|%Y|%s|%T@|%u:%g|%m|%f\\n' 2>/dev/null",
        shell_quote(path)
    )
}

fn parse_root_file_list(output: &str, path: &str) -> Vec<Value> {
    let path_norm = path.trim_end_matches('/');
    let mut items = Vec::new();
    if let Some(parent_item) = parent_remote_item(path) {
        items.push(parent_item);
    }
    for line in output.lines() {
        let line = line.trim_end_matches('\n');
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(7, '|').collect();
        if parts.len() < 7 {
            continue;
        }
        let type_char = parts[0].chars().next().unwrap_or('f');
        let is_dir = type_char == 'd';
        let is_link = type_char == 'l';
        let link_target_is_dir = is_link && parts[1].starts_with('d');
        let effective_is_dir = is_dir || link_target_is_dir;
        let size_value = parts[2].parse::<u64>().unwrap_or(0);
        let size_str = if effective_is_dir {
            "-".to_string()
        } else {
            format_bytes(size_value)
        };
        let mtime: i64 = parts[3]
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let owner_group = parts[4].to_string();
        let perm_octal = u32::from_str_radix(parts[5], 8).unwrap_or(0o644);
        let name = parts[6].to_string();
        if name == "." || name == ".." {
            continue;
        }

        let file_type = effective_remote_file_type(is_dir, is_link, link_target_is_dir);
        let permission = format_perm(perm_octal, is_dir, is_link);
        let full_path = if path_norm.is_empty() || path_norm == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", path_norm, name)
        };
        let modified = format_unix_ts(mtime);

        items.push(serde_json::json!({
            "name": name,
            "path": full_path,
            "type": file_type,
            "isSymlink": is_link,
            "size": size_str,
            "modified": modified,
            "permission": permission,
            "ownerGroup": owner_group,
        }));
    }
    items.sort_by(|a, b| {
        let af = a["type"].as_str() == Some("folder");
        let bf = b["type"].as_str() == Some("folder");
        bf.cmp(&af).then_with(|| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        })
    });
    items
}

pub(crate) fn effective_remote_file_type(
    is_dir: bool,
    is_link: bool,
    link_target_is_dir: bool,
) -> &'static str {
    if is_dir || (is_link && link_target_is_dir) {
        "folder"
    } else {
        "file"
    }
}

/// Read a file via the active root strategy + base64 (binary-safe over exec).
/// Decodes the result using the given encoding (mirrors Electron's
/// `readRemoteFileViaShell` + `decodeBuffer`).
async fn exec_read_file_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    encoding: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<String, String> {
    let cmd = format!("base64 {}", shell_quote(path));
    let output =
        exec_shell_file_command(handle, &cmd, access_method, sudo_user, sudo_password).await?;
    let trimmed: String = output.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&trimmed)
        .map_err(|e| format!("base64 decode failed: {}", e))?;
    decode_bytes(&bytes, encoding)
}

/// Write a file via the active root strategy + base64 (binary-safe). Encodes the content
/// using the given encoding before base64-wrapping (mirrors Electron's
/// `writeRemoteFileViaShell` + `encodeText`).
async fn exec_write_file_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    content: &str,
    encoding: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let bytes = encode_text(content, encoding)?;
    let staging_path = editor_staging_path(path);
    // Never stream editor content directly into the destination. The old
    // `base64 -d | tee destination` pipeline truncated the original first,
    // and a failed/truncated base64 stage could still be reported successful
    // because the shell returned tee's status.
    let cmd = root_editor_write_shell_command(&staging_path, bytes.len() as u64);
    let command = if access_method == RootFileAccessMethod::Su {
        su_exec_command(&cmd)
    } else {
        cmd
    };
    let (full_cmd, password) = root_file_command(access_method, sudo_user, sudo_password, &command);
    let encoded_input = if access_method == RootFileAccessMethod::Su {
        // A PTY normally runs in canonical mode, whose input line limit is
        // commonly 4096 bytes. Keep every base64 line below that limit while
        // preserving 3-byte block boundaries so concatenated lines decode to
        // the original bytes exactly.
        let mut lines = String::new();
        for chunk in bytes.chunks(3000) {
            lines.push_str(&base64::engine::general_purpose::STANDARD.encode(chunk));
            lines.push('\n');
        }
        if lines.is_empty() {
            lines.push('\n');
        }
        lines
    } else {
        format!(
            "{}\n",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        )
    };
    let (output, exit_status) = if access_method == RootFileAccessMethod::Su {
        exec_su_command_with_pty_input(
            handle,
            &full_cmd,
            password.as_deref(),
            Some(encoded_input.as_bytes()),
            true,
        )
        .await?
    } else {
        let stdin = if let Some(pwd) = password {
            format!("{}\n{}", pwd, encoded_input)
        } else {
            encoded_input
        };
        crate::sessions::system_metrics::exec_command_with_stdin_status(handle, &full_cmd, &stdin)
            .await?
    };
    let lower = output.to_lowercase();
    if root_access_auth_failed(&lower)
        || (access_method == RootFileAccessMethod::Su
            && (lower.contains("password") || output.contains("密码"))
            && !output.contains(SU_EXEC_OUTPUT_MARKER))
    {
        return Err(match access_method {
            RootFileAccessMethod::Sudo => "sudo authentication failed".to_string(),
            RootFileAccessMethod::Su => "su authentication failed".to_string(),
        });
    }
    let status = exit_status.ok_or_else(|| "root 写入命令未返回退出状态".to_string())?;
    if status != 0 {
        return Err(format!("root 写入命令失败（exit={status}）"));
    }
    if access_method == RootFileAccessMethod::Su {
        strip_su_exec_output(&output)?;
    }

    replace_root_remote_file(
        handle,
        &staging_path,
        path,
        access_method,
        sudo_user,
        sudo_password,
    )
    .await
    .map_err(|error| format!("远端文件提交失败（临时文件保留：{staging_path}）：{error}"))?;

    exec_shell_file_command(
        handle,
        &root_editor_verify_shell_command(path, bytes.len() as u64),
        access_method,
        sudo_user,
        sudo_password,
    )
    .await
    .map(|_| ())
    .map_err(|error| format!("远端文件提交后校验失败：{error}"))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────────────────────────────────────
