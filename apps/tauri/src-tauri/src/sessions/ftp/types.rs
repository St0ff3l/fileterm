use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use sha2::{Digest, Sha256};
use suppaftp::list::{File as ListedFile, ListParser};
use suppaftp::tokio::{
    AsyncFtpStream, AsyncNativeTlsConnector, AsyncNativeTlsFtpStream, ImplAsyncFtpStream,
    TokioTlsStream,
};
use suppaftp::types::Mode;
use suppaftp::{FtpError, Status};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration, MissedTickBehavior};
use tokio_socks::tcp::Socks5Stream;
use tokio_util::sync::CancellationToken;

use super::terminal::{decode_terminal, encode_terminal};
use super::{
    reconnect::{port_from_profile, seconds_from_profile, KeepalivePolicy, ReconnectPolicy},
    TransferFileStat, WorkerCmd,
};
use crate::services::{workspace::RemoteFileCapabilities, WorkspaceTabStatus};

const TRANSFER_CANCELED: &str = "transfer canceled";
const DEFAULT_FTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_FTP_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);
const FTP_PROXY_IO_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_FTP_DELETE_DEPTH: usize = 64;
const MAX_FTP_DELETE_ENTRIES: usize = 100_000;

enum FtpClient {
    Plain(AsyncFtpStream),
    Secure(AsyncNativeTlsFtpStream),
}

#[derive(Default)]
struct FtpListingState {
    mlsd_disabled: bool,
    mlst_disabled: bool,
    size_disabled: bool,
    resolved_types: HashMap<String, bool>,
    resolved_sizes: HashMap<String, usize>,
}

struct ParsedFtpListing {
    entry: ListedFile,
    type_is_trusted: bool,
}

fn default_ftp_capabilities() -> RemoteFileCapabilities {
    RemoteFileCapabilities {
        protocol: "ftp".to_string(),
        protocol_version: None,
        extensions: Vec::new(),
        checksum_algorithms: Vec::new(),
        disk_space: None,
        server_copy: false,
        symlink: false,
        hardlink: false,
    }
}

fn ftp_error_requires_reconnect(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "timed out",
        "connection reset",
        "connection closed",
        "broken pipe",
        "unexpected eof",
        "failed to fill whole buffer",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

macro_rules! respond_ftp_result {
    ($respond_to:expr, $result:expr) => {{
        let result = $result;
        let reconnect_error = result
            .as_ref()
            .err()
            .filter(|error| ftp_error_requires_reconnect(error))
            .cloned();
        let _ = $respond_to.send(result);
        if let Some(error) = reconnect_error {
            return Err(format!(
                "FTP control connection is no longer usable: {error}"
            ));
        }
    }};
}
