use crate::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

#[cfg(any(target_os = "macos", target_os = "windows"))]
const SMB_CREDENTIALS_REQUIRED: &str = "SMB_CREDENTIALS_REQUIRED";

/// Synthetic path for the Windows "This PC" drive list.
pub const WINDOWS_DRIVES_PATH: &str = "fileterm://windows-drives";

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalFileItem {
    pub path: String,
    pub name: String,
    pub r#type: String,
    pub modified: String,
    pub size: String,
    pub permission: String,
    pub owner_group: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySnapshot {
    pub path: String,
    pub items: Vec<LocalFileItem>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalNetworkShareConnectionResult {
    pub kind: String,
    pub path: String,
    pub shares: Vec<String>,
}

#[derive(Clone, Copy, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PermissionApplyTarget {
    All,
    Files,
    Directories,
}

impl PermissionApplyTarget {
    fn includes(self, is_directory: bool) -> bool {
        matches!(self, Self::All)
            || matches!(self, Self::Files) && !is_directory
            || matches!(self, Self::Directories) && is_directory
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionChangeOptions {
    mode: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    apply_to: Option<PermissionApplyTarget>,
}

include!("network.rs");
include!("directory.rs");
include!("file_operations.rs");
include!("dialogs.rs");
include!("tests.rs");
