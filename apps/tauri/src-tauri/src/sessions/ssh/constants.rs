const SFTP_UNAVAILABLE_FALLBACK: &str =
    "SFTP 文件通道不可用；终端和 SSH 隧道仍可继续使用。请在服务器启用或修复 sftp subsystem 后重新连接。";

/// Open the SFTP subsystem on an already authenticated SSH handle.
///
/// `russh-sftp` deliberately does not send the subsystem request itself, so
/// this boundary is also where we can distinguish a file-channel failure from
/// a terminal-session failure.
/// SFTP 初始化每一步的最大等待时间。
///
/// 这非常关键：`open_sftp_session` 在 worker 主 select! 循环之前调用，
/// 任何一步阻塞都会让整个 worker 启动不了——cmd_rx 队列堆满后所有
/// `app_write_terminal` 调用全部永久阻塞，表现为终端无法输入、多窗口
/// 发送整体卡死、Cmd+Q 退出也退不掉。服务器拒绝 sftp subsystem 时
/// russh-sftp 内部超时往往很长（30s+），这里强制收口到 8 秒。
const SFTP_INIT_STEP_TIMEOUT: Duration = Duration::from_secs(8);

/// Shell channel 建立阶段的单步超时。`channel_open_session` /
/// `request_pty` / `request_shell` 任一卡住都会让 worker 永远起不来——
/// 表现为"连接主机"loading 永不结束，所有后续命令（包括 Ctrl+C）都
/// 进不了 cmd_rx。服务器在 PTY 协商阶段卡住（罕见但确实发生过，尤其
/// 是某些嵌入式 dropbear / 网络设备）时，russh 默认无超时，会一直
/// await。8 秒与 SFTP_INIT_STEP_TIMEOUT 对齐，足够覆盖正常 RTT 与
/// 一次重试，同时不让用户对着 loading 望穿秋水。
const SHELL_INIT_STEP_TIMEOUT: Duration = Duration::from_secs(8);

/// `probe_remote_platform` 总超时。该函数在 worker 主循环之前调用，
/// 内部最多尝试 6 组探针（3 个 POSIX + 3 个 Windows probe；发现
/// PTY-only server 时每组最多再做一次 PTY 重试，因此最多 12 个 SSH
/// exec channel），每次
/// 都用 `channel.wait()` 循环读取，没有内层 timeout。如果服务器在 exec
/// 模式下卡住（不返回 EOF/Close），整个 probe 会永久 await，worker
/// 永远起不来，所有后续命令（含 Ctrl+C）都进不了 cmd_rx。20 秒覆盖
/// 最坏情况下的 4 次串行尝试 + RTT，超时后回落到 "unknown" 平台，
/// shell CWD 注入会被 fail-closed 门控跳过，不影响终端基本可用性。
const PLATFORM_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// SSH 隧道控制操作（tcpip_forward / cancel_tcpip_forward）的单步超时。
/// 这两个调用在 `handle_worker_cmd` 的 inline await 路径上，服务器卡住
/// 时会直接阻塞 worker 主循环，导致终端 select! 无法响应 Ctrl+C。
/// 5 秒覆盖正常 RTT 与一次重试，超时后让用户拿到明确错误而不是沉默
/// 地 hang 住整个会话。
const SSH_TUNNEL_OP_TIMEOUT: Duration = Duration::from_secs(5);

/// sudo 凭据验证超时。`exec_shell_file_command` 用 PTY 模式 exec，sudo
/// 密码错误时会重新 prompt 等待输入且不会自然退出，channel.wait() 永久
/// 阻塞。这里强制 10 秒收口，让前端 RootAccessModal 的 loading 状态能
/// 在合理时间内解除。
const SUDO_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
/// `su` 在独立 exec 通道里需要一个可用的控制终端来完成 PAM 密码交互。
/// 这个标记由提权后的 shell 打印，位于密码提示之后，用来从 PTY 合并
/// 输出中剥离 `Password:` / `密码:` 等前缀，避免污染 stat/base64 结果。
const SU_EXEC_OUTPUT_MARKER: &str = "__FILETERM_SU_EXEC_OUTPUT__";
/// Inline `SetRemoteFileAccessMode` verification budget. The full
/// `SUDO_VERIFY_TIMEOUT` (10s) is appropriate for spawned file operations,
/// but `SetRemoteFileAccessMode` runs inline on the worker loop — waiting
/// the full 10 seconds would freeze `terminal_input_rx` polling and make
/// Ctrl+C unresponsive while the user waits for the root-mode toggle to
/// finish. 1.5s is enough for a healthy sudo round-trip; slower responses
/// surface as a user-visible error instead of a frozen terminal.
const ROOT_ACCESS_VERIFY_TIMEOUT: Duration = Duration::from_millis(1500);

/// SFTP / exec 文件操作超时。
///
/// 这非常关键：worker 主循环是单 task 顺序处理 cmd 的，一个 ListRemoteFiles
/// / ReadRemoteFile 卡住会阻塞整个 select! 循环，cmd_rx.recv() 不被 poll，
/// 新来的 WriteTerminal 命令堆积直到 channel 满（100），之后所有
/// app_write_terminal 超时丢弃——终端和悬浮窗都无法输入。
///
/// SFTP read_dir / open 在网络抖动或服务器 SFTP subsystem 失效时可能
/// 长时间不返回，必须强制收口。
const FILE_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);

/// Keep the detached first browse request responsive even when a server's
/// SFTP subsystem accepts a request but never completes `READDIR`. User
/// initiated operations retain the profile-configured timeout below.
const INITIAL_SFTP_LISTING_TIMEOUT: Duration = Duration::from_secs(15);

/// Resolving the SFTP current directory is a small compatibility probe. It
/// must not inherit a one-hour operation timeout from a profile.
const INITIAL_SFTP_HOME_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(8);

/// A directory listing already carries metadata for the link itself. Following
/// a symlink is optional UI enrichment, so one inaccessible or slow target
/// must not hold the whole file pane behind the SFTP request timeout.
const SFTP_SYMLINK_TARGET_TIMEOUT: Duration = Duration::from_secs(2);
