use base64::Engine;
use serde_json::Value;
use std::time::Instant;
use tauri::AppHandle;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration, MissedTickBehavior};
use tokio_socks::tcp::Socks5Stream;
use tokio_util::sync::CancellationToken;

use super::reconnect::{port_from_profile, seconds_from_profile, KeepalivePolicy, ReconnectPolicy};
use super::telnet_direct::connect_direct_telnet;
use super::terminal::{decode_terminal, emit_terminal_data, encode_terminal, set_terminal_state};
use super::WorkerCmd;
use crate::services::WorkspaceTabStatus;

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const AYT: u8 = 246;
const BINARY: u8 = 0;
const ECHO: u8 = 1;
const SUPPRESS_GO_AHEAD: u8 = 3;
const TERMINAL_TYPE: u8 = 24;
const NAWS: u8 = 31;
/// Telnet 传输层连接（直连或经代理）整体超时。Telnet 服务器或代理无响应时，
/// `TcpStream::connect` 和 SOCKS5/HTTP CONNECT 握手都会永久 await，导致
/// 标签页卡在 connecting 状态无法重试。30s 与 SSH 侧 SSH_TRANSPORT_TIMEOUT 对齐。
const TELNET_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(30);
/// HTTP/SOCKS5 代理单步 IO 超时。代理服务器或中间网络卡住时，TCP 连接、
/// CONNECT 请求写入、响应逐字节读取都不能让外层 30s 超时全部消耗在
/// 单次 read 上——慢速代理可以每 29s 发一个字节拖满整个阶段。8s 覆盖
/// 正常代理 RTT，超时后立即给出明确错误。
const PROXY_IO_TIMEOUT: Duration = Duration::from_secs(8);
/// A Telnet peer that stopped consuming its socket must not pin the worker
/// while `write_all` waits for the TCP send buffer. Returning the error lets
/// the shared reconnect policy reset the session instead of leaving the tab
/// looking connected but unable to accept input.
const TELNET_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

trait TelnetTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> TelnetTransport for T {}

include!("parser.rs");
include!("worker.rs");
include!("transport.rs");
include!("tests.rs");
