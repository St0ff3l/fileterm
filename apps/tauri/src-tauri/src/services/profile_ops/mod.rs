//! Profile / folder / command CRUD operations with group/parentId self-healing.
//!
//! Mirrors the semantics of `FileProfileRepository` from the Electron backend:
//! - `profile.group` (folder name) and `profile.parentId` (folder id) are kept
//!   in sync on every read; if healing modifies anything the profiles are
//!   persisted back.
//! - profile update / delete, folder update / delete, and entity order updates
//!   follow the same cascade rules as the Electron side.

use crate::storage::{new_id, read_json_array, workspace_file, write_json_array};
use crate::AppError;
use serde_json::{Map, Value};
use tauri::AppHandle;

const DEFAULT_GROUP: &str = "默认";

include!("healing.rs");
include!("profiles.rs");
include!("secrets.rs");
include!("folders.rs");
include!("tests.rs");
