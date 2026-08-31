use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

use crate::AppError;

const LEGACY_MIGRATION_VERSION: u32 = 1;
const LEGACY_MIGRATION_MARKER: &str = "legacy-fileterm-migration.json";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_CONFIG_DIRECTORY: &str = "config";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_MARKER_FILE: &str = "portable";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_MIGRATION_VERSION: u32 = 1;
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_MIGRATION_MARKER: &str = "portable-migration.json";
#[cfg(any(test, target_os = "windows"))]
const PORTABLE_DATA_ENTRIES: &[(&str, bool)] = &[
    ("profiles.json", false),
    ("folders.json", false),
    ("profile-secrets.json", true),
    ("command-folders.json", false),
    ("commands.json", false),
    ("command-history.json", false),
    ("command-send-preferences.json", false),
    ("ui-state.json", false),
    ("ui-preferences.json", false),
    ("transfer-journal.json", false),
    ("webdav-sync.json", true),
    ("s3-backup.json", true),
    ("security.json", true),
    ("ai-providers.json", false),
    ("ai-provider-secrets.json", true),
    ("ai-conversations.json", true),
    ("ai-conversations", true),
    ("fonts.json", false),
    ("fonts", false),
    ("ssh-keys.json", false),
    ("ssh-key-secrets.json", true),
    ("ssh-keys", true),
    ("secret-store-v1.key", true),
    (LEGACY_MIGRATION_MARKER, false),
];

#[derive(Clone, Copy)]
enum JsonMergeMode {
    ArrayById,
    ObjectCurrentWins,
    NestedObjectCurrentWins(&'static str),
    CurrentFileWins,
}

#[derive(Clone, Copy)]
struct LegacyJsonStore {
    name: &'static str,
    mode: JsonMergeMode,
    confidential: bool,
}

const LEGACY_JSON_STORES: &[LegacyJsonStore] = &[
    LegacyJsonStore {
        name: "profiles.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "folders.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "profile-secrets.json",
        mode: JsonMergeMode::NestedObjectCurrentWins("profiles"),
        confidential: true,
    },
    LegacyJsonStore {
        name: "command-folders.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "commands.json",
        mode: JsonMergeMode::ArrayById,
        confidential: false,
    },
    LegacyJsonStore {
        name: "command-history.json",
        mode: JsonMergeMode::ObjectCurrentWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "command-send-preferences.json",
        mode: JsonMergeMode::ObjectCurrentWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "ui-state.json",
        mode: JsonMergeMode::CurrentFileWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "ui-preferences.json",
        mode: JsonMergeMode::ObjectCurrentWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "transfer-journal.json",
        mode: JsonMergeMode::CurrentFileWins,
        confidential: false,
    },
    LegacyJsonStore {
        name: "webdav-sync.json",
        mode: JsonMergeMode::CurrentFileWins,
        confidential: true,
    },
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacySourceSnapshot {
    name: String,
    bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyMigrationReport {
    version: u32,
    status: String,
    completed_at: u64,
    source_directory: String,
    conflict_policy: String,
    source_files: Vec<LegacySourceSnapshot>,
    migrated_files: Vec<String>,
    kept_current_files: Vec<String>,
    rollback_performed: bool,
}

struct PendingFile {
    target: PathBuf,
    staged: PathBuf,
    backup: PathBuf,
    confidential: bool,
}

include!("paths.rs");
include!("migration/mod.rs");
include!("json_io.rs");
include!("tests.rs");
