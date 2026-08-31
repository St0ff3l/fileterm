/// Dispatch the SSH worker commands owned by the file group.
#[allow(clippy::too_many_arguments)]
async fn dispatch_file_cmd(
    cmd: WorkerCmd,
    handle: &Arc<Handle<ClientHandler>>,
    _shell_writer: &SshShellWriteHalf,
    sftp: &SharedSftpSession,
    _transfer_sftp_slot: &TransferSftpSlot,
    operation_timeout: Duration,
    file_access_mode: &mut String,
    root_file_access_method: &mut RootFileAccessMethod,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    saved_sudo_password: &mut Option<String>,
    saved_su_password: &mut Option<String>,
    tab_id: &str,
    _app: &AppHandle,
    state: &tauri::State<'_, crate::services::workspace::WorkspaceState>,
    _tunnel_commands: &mpsc::UnboundedSender<TunnelCommand>,
    exec_channel_enabled: bool,
) -> Result<bool, String> {
    match cmd {
        WorkerCmd::ListRemoteFiles {
            path,
            cancellation,
            respond_to,
        } => {
            // spawn 避免阻塞主循环；timeout 防止 SFTP 卡住时任务永久挂起。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_list_dir_via_shell(&handle, &path, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("打开远程文件夹{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, list_dir(&sftp_guard, &path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("打开远程目录{}超时", path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::ReadRemoteFile {
            path,
            encoding,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_read_file_via_shell(&handle, &path, &encoding, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, read_file(&sftp_guard, &path, &encoding)).await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::WriteRemoteFile {
            path,
            content,
            encoding,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_write_file_via_shell(
                            &handle, &path, &content, &encoding, method, &su, &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        operation_timeout,
                        write_file(&sftp_guard, &path, &content, &encoding),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteDirectory {
            parent_path,
            name,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!("mkdir -p {}", shell_quote(&full_path)),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, create_dir(&sftp_guard, &full_path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteFile {
            parent_path,
            name,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_write_file_via_shell(
                            &handle, &full_path, "", "utf-8", method, &su, &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        operation_timeout,
                        write_file(&sftp_guard, &full_path, "", "utf-8"),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::CopyRemotePath {
            target_path,
            destination_path,
            target_type,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let dest_dir = std::path::Path::new(&destination_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let cp_cmd = if target_type == "folder" {
                    "cp -R"
                } else {
                    "cp"
                };
                let cmd_str = format!(
                    "mkdir -p {} && {} {} {}",
                    shell_quote(&dest_dir),
                    cp_cmd,
                    shell_quote(&target_path),
                    shell_quote(&destination_path)
                );
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(&handle, &cmd_str, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                } else {
                    match timeout(
                        operation_timeout,
                        crate::sessions::system_metrics::exec_command(&handle, &cmd_str),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::MoveRemotePath {
            target_path,
            destination_path,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!(
                                "mv {} {}",
                                shell_quote(&target_path),
                                shell_quote(&destination_path)
                            ),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        operation_timeout,
                        sftp_guard.rename(&target_path, &destination_path),
                    )
                    .await
                    {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::RenameRemotePath {
            target_path,
            new_name,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let parent = std::path::Path::new(&target_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let dest = format!("{}/{}", parent.trim_end_matches('/'), new_name);
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!("mv {} {}", shell_quote(&target_path), shell_quote(&dest)),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, sftp_guard.rename(&target_path, &dest)).await {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::DeleteRemotePath {
            target_path,
            target_type,
            target_is_symlink: _,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let cmd_str = if target_type == "folder" {
                    format!("rm -rf {}", shell_quote(&target_path))
                } else {
                    format!("rm -f {}", shell_quote(&target_path))
                };
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(&handle, &cmd_str, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                } else {
                    match timeout(
                        operation_timeout,
                        crate::sessions::system_metrics::exec_command(&handle, &cmd_str),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::ChangeRemotePermissions {
            target_path,
            permissions,
            recursive,
            apply_to,
            cancellation,
            respond_to,
        } => {
            // Mirrors Electron's `changeRemotePermissions`:
            // - `apply_to='all'` → `chmod -R` for recursive, plain `chmod` otherwise
            // - `apply_to='files'` → `chmod <mode> <path>` + `find <path> -type f -exec chmod <mode> {} +`
            // - `apply_to='directories'` → `chmod <mode> <path>` + `find <path> -type d -exec chmod <mode> {} +`
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let mode_str = format!("{:o}", permissions);
                let cmd_str = if !recursive {
                    format!("chmod {} {}", mode_str, shell_quote(&target_path))
                } else {
                    match apply_to.as_str() {
                        "files" => format!(
                            "chmod {} {} && find {} -type f -exec chmod {} {} +",
                            mode_str,
                            shell_quote(&target_path),
                            shell_quote(&target_path),
                            mode_str,
                            "{}"
                        ),
                        "directories" => format!(
                            "chmod {} {} && find {} -type d -exec chmod {} {} +",
                            mode_str,
                            shell_quote(&target_path),
                            shell_quote(&target_path),
                            mode_str,
                            "{}"
                        ),
                        _ => format!("chmod -R {} {}", mode_str, shell_quote(&target_path)),
                    }
                };
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(&handle, &cmd_str, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                } else {
                    let wrapped = format!("sh -lc {}", shell_quote(&cmd_str));
                    match timeout(
                        operation_timeout,
                        crate::sessions::system_metrics::exec_command(&handle, &wrapped),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::SetRemoteFileAccessMode {
            mode,
            root_access_method: new_root_access_method,
            sudo_user: new_sudo_user,
            sudo_password: new_sudo_password,
            use_saved_password,
            respond_to,
        } => {
            if mode == "root" && !exec_channel_enabled {
                let _ = respond_to.send(Err(
                    "SSH Exec 通道已关闭，无法启用 root 文件视图。".to_string()
                ));
                return Ok(false);
            }
            let requested_access_method =
                match parse_root_file_access_method(new_root_access_method.as_deref()) {
                    Ok(method) => method,
                    Err(error) => {
                        let _ = respond_to.send(Err(error));
                        return Ok(false);
                    }
                };
            // 对照 Electron verifyRootFileAccess：切到 root 前先验证 sudo 凭据
            // 可用，失败则回滚状态并返回错误，让用户在弹窗里立即看到反馈，
            // 而不是等到第一次文件操作才失败（用户会以为"root 切换没接入"）。
            let prev_sudo_user = sudo_user.clone();
            let prev_sudo_password = sudo_password.clone();
            let prev_saved_sudo_password = saved_sudo_password.clone();
            let prev_saved_su_password = saved_su_password.clone();
            let prev_mode = file_access_mode.clone();
            let prev_access_method = *root_file_access_method;

            if let Some(next_user) = new_sudo_user.filter(|user| !user.trim().is_empty()) {
                *sudo_user = Some(next_user);
            }
            if mode == "root"
                && (use_saved_password
                    || requested_access_method != prev_access_method
                    || sudo_password.is_none())
            {
                *sudo_password = root_password_for_method(
                    requested_access_method,
                    saved_sudo_password,
                    saved_su_password,
                );
            }
            if let Some(pwd) = new_sudo_password {
                if !pwd.is_empty() {
                    *sudo_password = Some(pwd.clone());
                    match requested_access_method {
                        RootFileAccessMethod::Sudo => *saved_sudo_password = Some(pwd),
                        RootFileAccessMethod::Su => *saved_su_password = Some(pwd),
                    }
                }
                // empty password ⇒ keep existing (cache reuse)
            }

            if mode == "root" {
                // 手动弹窗流程验证选择的 sudo/su 凭据，失败则回滚。`exec_shell_file_command`
                // 内部最长会等 SUDO_VERIFY_TIMEOUT（10s）才放弃，对 worker
                // 主循环来说太长——一旦 sudo 提示卡住或网络抖动，整个
                // 终端 select! 都停在这里，连 Ctrl+C 都进不去。这里用
                // ROOT_ACCESS_VERIFY_TIMEOUT（1.5s）做外层硬截断：超时
                // 同样视为验证失败并回滚，让用户拿到明确错误，而不是
                // 让 worker loop 沉默地阻塞数秒。
                let verify = match timeout(
                    ROOT_ACCESS_VERIFY_TIMEOUT,
                    exec_shell_file_command(
                        handle,
                        "true",
                        requested_access_method,
                        sudo_user,
                        sudo_password,
                    ),
                )
                .await
                {
                    Ok(inner) => inner,
                    Err(_) => Err(format!(
                        "{} 验证超时：服务器未在 1.5 秒内响应",
                        root_file_access_method_label(requested_access_method)
                    )),
                };
                if let Err(err) = verify {
                    // 回滚到切换前的状态
                    *file_access_mode = prev_mode;
                    *root_file_access_method = prev_access_method;
                    *sudo_user = prev_sudo_user;
                    *sudo_password = prev_sudo_password;
                    *saved_sudo_password = prev_saved_sudo_password;
                    *saved_su_password = prev_saved_su_password;
                    let _ = respond_to.send(Err(err));
                    return Ok(false);
                }
                *root_file_access_method = requested_access_method;
            }

            *file_access_mode = mode.clone();
            let has_reusable =
                *root_file_access_method == RootFileAccessMethod::Sudo && sudo_password.is_some();
            let su_user = sudo_user.clone();
            let mut sessions = state.sessions.write().await;
            if let Some(s) = sessions.get_mut(tab_id) {
                s.file_access_mode = mode;
                s.sudo_user = su_user;
                s.has_reusable_sudo_auth = has_reusable;
            }
            let _ = respond_to.send(Ok(()));
            Ok(false)
        }
        WorkerCmd::Disconnect => Ok(true),
            _ => unreachable!("all SSH worker commands are covered by dispatch groups"),
    }
}
