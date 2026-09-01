
fn spawn_cancellable_file_operation<T, F>(
    cancellation: CancellationToken,
    respond_to: oneshot::Sender<Result<T, String>>,
    operation: F,
) where
    T: Send + 'static,
    F: Future<Output = Result<T, String>> + Send + 'static,
{
    tokio::spawn(async move {
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err("远程文件操作已取消".to_string()),
            result = operation => result,
        };
        let _ = respond_to.send(result);
    });
}

/// Returns `Ok(true)` when the worker should exit (Disconnect requested),
/// `Ok(false)` otherwise.
///
/// 文件操作（List/Read/Write/Upload/Download/...）通过 `tokio::spawn` 分发到
/// 独立任务执行，主循环立即返回继续处理终端输入。这样单个慢速 SFTP 操作
/// 不会阻塞 `cmd_rx` 接收新的 `WriteTerminal` 命令——这是用户反馈"点上传
/// 后终端和文件都卡住"问题的根本修复。
#[allow(clippy::too_many_arguments)]
async fn handle_worker_cmd(
    cmd: WorkerCmd,
    handle: &Arc<Handle<ClientHandler>>,
    shell_writer: &SshShellWriteHalf,
    sftp: &SharedSftpSession,
    transfer_sftp_slot: &TransferSftpSlot,
    operation_timeout: Duration,
    file_access_mode: &mut String,
    root_file_access_method: &mut RootFileAccessMethod,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    saved_sudo_password: &mut Option<String>,
    saved_su_password: &mut Option<String>,
    tab_id: &str,
    app: &AppHandle,
    state: &tauri::State<'_, crate::services::workspace::WorkspaceState>,
    tunnel_commands: &mpsc::UnboundedSender<TunnelCommand>,
    exec_channel_enabled: bool,
) -> Result<bool, String> {
    dispatch_terminal_cmd(
        cmd,
        handle,
        shell_writer,
        sftp,
        transfer_sftp_slot,
        operation_timeout,
        file_access_mode,
        root_file_access_method,
        sudo_user,
        sudo_password,
        saved_sudo_password,
        saved_su_password,
        tab_id,
        app,
        state,
        tunnel_commands,
        exec_channel_enabled,
    )
    .await
}

// ─────────────────────────────────────────────────────────────────────────────
// SFTP helpers (russh-sftp 2.x)
// ─────────────────────────────────────────────────────────────────────────────
