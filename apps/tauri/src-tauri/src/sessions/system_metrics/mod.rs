// System metrics collection — russh async port.
//
// `probe_remote_platform` and `exec_command[_with_stdin]` are async and
// operate on a `russh::client::Handle`. All parsing/formatting helpers
// below are pure functions and unchanged from the ssh2 era.
use std::io::Write;
use std::time::Duration;

use base64::Engine;
use flate2::write::GzEncoder;
use flate2::Compression;
use russh::client::{Handle, Handler};
use russh::ChannelMsg;
use tokio::time::timeout;

// A few SSH transports deliver a terminal channel marker before the final
// stdout packet is drained. Keep a very short grace window after EOF, CLOSE,
// or ExitStatus: it preserves output while still guaranteeing that servers
// which omit the remaining markers cannot hold a caller until its much longer
// command watchdog fires.
const EXEC_CHANNEL_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

/// `exec_command` / `exec_command_with_stdin` 收集的 output 字节上限。
/// probe 命令的正常输出只有几行（uname / 系统信息），超过 256KB 说明
/// 服务器配置异常或被入侵——这种情况下截断输出比让 Vec 无限增长更安全。
/// 参考 meatshell 的 MON_BUF_CAP 设计，防止恶意服务器内存 DoS。
const EXEC_COMMAND_OUTPUT_CAP: usize = 256 * 1024;

/// Bounded output collected from a dedicated SSH exec channel. The legacy
/// tuple helpers below intentionally keep their stable shape, while callers
/// that surface output to users can expose the truncation bit instead of
/// pretending the remote command produced a complete result.
#[derive(Clone, Debug)]
pub struct ExecCommandResult {
    pub output: String,
    pub exit_code: Option<u32>,
    pub output_truncated: bool,
    /// The command deadline elapsed after the exec channel was opened. Any
    /// safely collected bytes are still returned so callers can distinguish a
    /// command that never produced output from one that ran partially.
    pub timed_out: bool,
}

include!("exec.rs");
include!("parser.rs");
include!("posix.rs");
include!("freebsd.rs");
include!("windows.rs");
include!("tests.rs");
