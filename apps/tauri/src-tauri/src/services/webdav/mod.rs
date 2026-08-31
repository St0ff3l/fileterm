//! Manual, conflict-aware WebDAV synchronization for the complete profile
//! bundle. Connection credentials stay inside the Rust service boundary,
//! are encrypted with the user's one-time backup password, and can therefore
//! be restored by another FileTerm installation without being uploaded as
//! plaintext.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::{HeaderMap, ETAG, IF_MATCH, IF_NONE_MATCH};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use url::Url;

use crate::services::{backup_crypto, backup_prompt, profile_ops};
use crate::storage::workspace_file;
use crate::AppError;

const MAX_BUNDLE_BYTES: usize = 5 * 1024 * 1024;
const DEFAULT_REMOTE_PATH: &str = "fileterm-connections.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UploadMode {
    OverwriteCloud,
    MergeCloud,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DownloadMode {
    OverwriteLocal,
    MergeLocal,
}

pub(crate) fn parse_upload_mode(value: Option<&str>) -> Result<UploadMode, AppError> {
    match value.unwrap_or("overwrite-cloud") {
        "overwrite-cloud" => Ok(UploadMode::OverwriteCloud),
        "merge-cloud" => Ok(UploadMode::MergeCloud),
        _ => Err(command_error("无效的备份上传策略")),
    }
}

pub(crate) fn parse_download_mode(value: Option<&str>) -> Result<DownloadMode, AppError> {
    match value.unwrap_or("merge-local") {
        "overwrite-local" => Ok(DownloadMode::OverwriteLocal),
        "merge-local" => Ok(DownloadMode::MergeLocal),
        _ => Err(command_error("无效的备份下载策略")),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConfig {
    enabled: bool,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    remote_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allow_insecure_tls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            username: None,
            remote_path: DEFAULT_REMOTE_PATH.to_string(),
            allow_insecure_tls: None,
            password: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        }
    }
}

include!("config.rs");
include!("bundle.rs");
include!("sync.rs");
include!("tests.rs");
