//! Manual S3-compatible backup for the complete FileTerm connection bundle.
//!
//! The renderer only receives non-secret connection settings. Access keys and
//! the bundle containing profile credentials stay in this Rust service and are
//! persisted in a user-only file. Cloudflare R2 is an S3-compatible preset
//! using its required `auto` region and path-style addressing.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, ETAG};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use url::{Position, Url};

use crate::services::{backup_crypto, backup_prompt, webdav};
use crate::storage::workspace_file;
use crate::AppError;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_REMOTE_PATH: &str = "fileterm/connections.json";
const MAX_BUNDLE_BYTES: usize = 5 * 1024 * 1024;
const PROVIDER_CUSTOM: &str = "custom";
const PROVIDER_CLOUDFLARE_R2: &str = "cloudflare-r2";
const PROVIDER_BITIFUL_S4: &str = "bitiful-s4";
const BITIFUL_S4_ENDPOINT: &str = "https://s3.bitiful.net";
const BITIFUL_S4_REGION: &str = "cn-east-1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConfig {
    enabled: bool,
    provider: String,
    endpoint: String,
    region: String,
    bucket: String,
    remote_path: String,
    path_style_access_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret_access_key: Option<String>,
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
            provider: PROVIDER_CUSTOM.to_string(),
            endpoint: String::new(),
            region: "us-east-1".to_string(),
            bucket: String::new(),
            remote_path: DEFAULT_REMOTE_PATH.to_string(),
            path_style_access_enabled: true,
            access_key_id: None,
            secret_access_key: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        }
    }
}

struct ObjectTarget {
    url: Url,
    canonical_uri: String,
    host: String,
}

include!("config.rs");
include!("signing.rs");
include!("sync.rs");
include!("tests.rs");
