fn file_operation_timeout(profile: &Value) -> Duration {
    seconds_from_profile(
        profile,
        "operationTimeoutSeconds",
        FILE_OPERATION_TIMEOUT,
        Duration::from_secs(5),
        Duration::from_secs(3600),
    )
}

async fn open_sftp_session(
    handle: &Handle<ClientHandler>,
    request_timeout: Duration,
) -> Result<SftpSession, String> {
    let sftp_channel = timeout(SFTP_INIT_STEP_TIMEOUT, handle.channel_open_session())
        .await
        .map_err(|_| "SFTP init failed: 打开 channel 超时".to_string())?
        .map_err(|error| format!("无法打开 SFTP channel: {error}"))?;
    timeout(
        SFTP_INIT_STEP_TIMEOUT,
        sftp_channel.request_subsystem(true, "sftp"),
    )
    .await
    .map_err(|_| "SFTP init failed: 请求 subsystem 超时".to_string())?
    .map_err(|error| format!("SFTP subsystem request failed: {error}"))?;
    // russh-sftp defaults each request to 10 seconds. Keep that library-level
    // deadline aligned with FileTerm's operation timeout so a slow SFTP
    // server is not failed early with a misleading bare `Timeout` error.
    let sftp_config = SftpConfig {
        request_timeout_secs: request_timeout.as_secs().max(1),
        ..SftpConfig::default()
    };
    timeout(
        SFTP_INIT_STEP_TIMEOUT,
        SftpSession::new_with_config(sftp_channel.into_stream(), sftp_config),
    )
    .await
    .map_err(|_| "SFTP init failed: 协议握手超时".to_string())?
    .map_err(|error| format!("SFTP init failed: {error}"))
}

type SharedSftpSession = Arc<RwLock<SftpSession>>;
type TransferSftpSlot = Arc<Mutex<Option<SharedSftpSession>>>;

/// `/` was the historical SSH form default, but it is not a portable initial
/// directory: a normal OpenSSH server exposes the whole filesystem while a
/// chrooted or hosted SFTP server may expose the user's home as `/`. Treat the
/// old `/` value and the new `.` value as an implicit Home request, then let
/// the SFTP server resolve the actual namespace.
fn is_implicit_ssh_home_path(path: &str) -> bool {
    matches!(path.trim(), "" | "/" | ".")
}

async fn resolve_initial_sftp_home_path(
    sftp: &SharedSftpSession,
    operation_timeout: Duration,
) -> Result<String, String> {
    let resolution_timeout = operation_timeout.min(INITIAL_SFTP_HOME_RESOLUTION_TIMEOUT);
    let canonical_path = timeout(resolution_timeout, async {
        let sftp = sftp.write().await;
        sftp.canonicalize(".").await
    })
    .await
    .map_err(|_| "SFTP canonical home path resolution timed out".to_string())?
    .map_err(|error| error.to_string())?;

    let canonical_path = canonical_path.trim();
    if canonical_path.is_empty() || canonical_path == "." {
        return Err("SFTP server returned an empty canonical home path".to_string());
    }
    Ok(canonical_path.to_string())
}

fn is_sftp_not_found(error: &SftpError) -> bool {
    matches!(
        error,
        SftpError::Status(status) if status.status_code == StatusCode::NoSuchFile
    ) || is_sftp_path_not_found_message(&error.to_string())
}

async fn acquire_transfer_sftp(
    handle: &Handle<ClientHandler>,
    primary: &SharedSftpSession,
    slot: &TransferSftpSlot,
    app: &AppHandle,
    tab_id: &str,
    request_timeout: Duration,
) -> SharedSftpSession {
    let mut slot_guard = slot.lock().await;
    if let Some(session) = slot_guard.as_ref() {
        return Arc::clone(session);
    }
    match open_sftp_session(handle, request_timeout).await {
        Ok(session) => {
            let session = Arc::new(RwLock::new(session));
            *slot_guard = Some(Arc::clone(&session));
            crate::services::logging::session(
                app,
                "INFO",
                "sftp",
                tab_id,
                "dedicated transfer channel opened",
            );
            session
        }
        Err(error) => {
            crate::services::logging::session(
                app,
                "WARN",
                "sftp",
                tab_id,
                format!("dedicated transfer channel unavailable; using browse channel: {error}"),
            );
            Arc::clone(primary)
        }
    }
}

async fn invalidate_transfer_sftp(
    session: &SharedSftpSession,
    primary: &SharedSftpSession,
    slot: &TransferSftpSlot,
) {
    if Arc::ptr_eq(session, primary) {
        return;
    }
    let mut slot_guard = slot.lock().await;
    if slot_guard
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, session))
    {
        *slot_guard = None;
    }
}

/// Convert a russh SFTP handshake error into an actionable, non-ambiguous
/// renderer message. A timeout here happens after the interactive shell is
/// established, so it must not be presented as a failed SSH login.
fn format_sftp_unavailable_reason(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        format!(
            "SFTP 子系统在初始化期间没有响应。SSH 终端已连接，服务器可能禁用或拒绝了 sftp subsystem；请在服务器启用/修复 SFTP 后重连。原始错误: {error}"
        )
    } else {
        format!(
            "SFTP 文件通道不可用（{error}）。SSH 终端和隧道仍可使用；请在服务器启用/修复 SFTP 后重连。"
        )
    }
}

fn sftp_unavailable_result<T>(reason: &str) -> Result<T, String> {
    Err(reason.to_string())
}
