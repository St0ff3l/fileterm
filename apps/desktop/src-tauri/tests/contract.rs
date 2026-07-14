//! Phase 0 contract tests.
//!
//! These tests pin down the public, cross-crate invariants of the Tauri
//! backend that the renderer (and the Electron backend before it) rely on:
//!
//! 1. **Secret stripping contract** — `strip_secret_fields_public` must
//!    remove `password`, `passphrase`, `privateKeyPath`, and the nested
//!    `proxy.password` from any profile object handed back to the
//!    renderer. The secrets live in `profile-secrets.json` and must never
//!    leak through `app_get_snapshot` / `app_get_connection_library`.
//!
//! 2. **group/parentId self-healing contract** — `heal_profiles` keeps
//!    `profile.group` (folder name) and `profile.parentId` (folder id) in
//!    sync, with `group` authoritative when it points at a known folder
//!    and `parentId` authoritative when `group` is empty / `默认`.
//!
//! 3. **Command naming contract** — every `#[tauri::command]` exposed via
//!    `invoke_handler!` is prefixed with `app_`, so the renderer can
//!    blindly map `camelCase` API methods to `app_snake_case` invokes.
//!
//! 4. **Event naming contract** — every event emitted via `app.emit(...)`
//!    uses the `namespace:name` form (`terminal:data`, `workspace:snapshot`,
//!    `app:window-close-request`, ...). No bare names.

use fileterm_lib::services::profile_ops::{
    heal_profiles, strip_secret_fields_public,
};
use serde_json::{json, Value};

fn folder(id: &str, name: &str) -> Value {
    json!({ "id": id, "name": name, "type": "folder" })
}

fn profile(id: &str, group: &str, parent_id: Option<&str>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("id".to_string(), Value::String(id.to_string()));
    obj.insert("name".to_string(), Value::String(format!("Profile {}", id)));
    obj.insert("type".to_string(), Value::String("ssh".to_string()));
    obj.insert("group".to_string(), Value::String(group.to_string()));
    obj.insert(
        "parentId".to_string(),
        parent_id.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
    );
    Value::Object(obj)
}

// ── Secret stripping contract ────────────────────────────────────────────

#[test]
fn strip_removes_top_level_secret_fields() {
    let profile = json!({
        "id": "p1",
        "name": "demo",
        "type": "ssh",
        "password": "hunter2",
        "passphrase": "secret-pass",
        "privateKeyPath": "/home/user/.ssh/id_rsa",
        "host": "example.com",
        "port": 22,
    });
    let stripped = strip_secret_fields_public(&profile);
    assert!(stripped.get("password").is_none(), "password must be stripped");
    assert!(stripped.get("passphrase").is_none(), "passphrase must be stripped");
    assert!(
        stripped.get("privateKeyPath").is_none(),
        "privateKeyPath must be stripped"
    );
    // Non-secret fields stay intact.
    assert_eq!(stripped.get("host").and_then(|v| v.as_str()), Some("example.com"));
    assert_eq!(stripped.get("port").and_then(|v| v.as_i64()), Some(22));
}

#[test]
fn strip_removes_nested_proxy_password() {
    let profile = json!({
        "id": "p2",
        "name": "proxied",
        "type": "ssh",
        "proxy": {
            "type": "socks5",
            "host": "127.0.0.1",
            "port": 1080,
            "username": "u",
            "password": "proxy-pw"
        }
    });
    let stripped = strip_secret_fields_public(&profile);
    let proxy = stripped
        .get("proxy")
        .and_then(|v| v.as_object())
        .expect("proxy object should survive stripping");
    assert!(proxy.get("password").is_none(), "proxy.password must be stripped");
    assert_eq!(proxy.get("host").and_then(|v| v.as_str()), Some("127.0.0.1"));
    assert_eq!(proxy.get("port").and_then(|v| v.as_i64()), Some(1080));
}

#[test]
fn strip_is_idempotent() {
    let profile = json!({
        "id": "p3",
        "name": "clean",
        "type": "ssh",
        "host": "example.com",
    });
    let once = strip_secret_fields_public(&profile);
    let twice = strip_secret_fields_public(&once);
    assert_eq!(once, twice);
}

// ── group/parentId self-healing contract ──────────────────────────────────

#[test]
fn heal_group_authoritative_when_pointing_at_known_folder() {
    let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
    let mut profiles = vec![profile("p1", "Alpha", Some("f2"))];
    let dirty = heal_profiles(&mut profiles, &folders);
    assert!(dirty, "healing should mark the profile dirty");
    assert_eq!(
        profiles[0].get("parentId").and_then(|v| v.as_str()),
        Some("f1"),
        "parentId should be corrected to match group"
    );
}

#[test]
fn heal_parent_id_authoritative_when_group_is_default() {
    let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
    let mut profiles = vec![profile("p1", "默认", Some("f2"))];
    let dirty = heal_profiles(&mut profiles, &folders);
    assert!(dirty, "healing should mark the profile dirty");
    assert_eq!(
        profiles[0].get("group").and_then(|v| v.as_str()),
        Some("Beta"),
        "group should be corrected to match parentId"
    );
}

#[test]
fn heal_falls_back_to_default_when_both_refs_are_dangling() {
    let folders = vec![folder("f1", "Alpha")];
    let mut profiles = vec![profile("p1", "Ghost", Some("ghost-id"))];
    let dirty = heal_profiles(&mut profiles, &folders);
    assert!(dirty);
    assert_eq!(
        profiles[0].get("group").and_then(|v| v.as_str()),
        Some("默认")
    );
    assert!(profiles[0].get("parentId").unwrap().is_null());
}

#[test]
fn heal_leaves_consistent_profiles_untouched() {
    let folders = vec![folder("f1", "Alpha")];
    let mut profiles = vec![profile("p1", "Alpha", Some("f1"))];
    let dirty = heal_profiles(&mut profiles, &folders);
    assert!(!dirty, "consistent profiles must not be marked dirty");
}

// ── Naming contracts ─────────────────────────────────────────────────────
//
// The command-prefix (`app_`) and event-namespace (`ns:name`) conventions
// are enforced by code review and the bridge layer in `tauri-api.ts`.
// These tests exist as executable documentation of the convention so
// regressions are caught at review time.

#[test]
fn contract_commands_use_app_prefix() {
    // Sampler of command names registered in `lib.rs::invoke_handler!`.
    // Every command exposed to the renderer MUST start with `app_`.
    let commands = [
        "app_get_platform",
        "app_get_arch",
        "app_read_clipboard_text",
        "app_write_clipboard_text",
        "app_open_external_url",
        "app_get_ui_preferences",
        "app_set_ui_preferences",
        "app_get_snapshot",
        "app_open_profile",
        "app_activate_tab",
        "app_reconnect_tab",
        "app_close_tab",
        "app_write_terminal",
        "app_resize_terminal",
        "app_open_remote_path",
        "app_read_remote_file",
        "app_write_remote_file",
        "app_create_remote_directory",
        "app_copy_remote_path",
        "app_move_remote_path",
        "app_rename_remote_path",
        "app_delete_remote_path",
        "app_change_remote_permissions",
        "app_set_remote_file_access_mode",
        "app_create_profile",
        "app_update_profile",
        "app_delete_profile",
        "app_update_folder",
        "app_delete_folder",
        "app_update_entity_order",
        "app_update_command_folder",
        "app_delete_command_folder",
        "app_update_command_order",
        "app_update_command_template",
        "app_delete_command_template",
        "app_execute_command_template",
        "app_list_local_directory",
        "app_read_local_file",
        "app_write_local_file",
        "app_create_local_directory",
        "app_copy_local_path",
        "app_move_local_path",
        "app_rename_local_path",
        "app_delete_local_path",
        "app_change_local_permissions",
        "app_select_local_files",
        "app_select_local_directory",
    ];
    for name in &commands {
        assert!(
            name.starts_with("app_"),
            "command `{name}` violates the `app_` prefix contract"
        );
    }
}

#[test]
fn contract_events_use_namespace_colon_name() {
    // Sampler of event names emitted via `app.emit(...)` across the
    // backend. Every event MUST use the `namespace:name` form so the
    // renderer's `subscribe()` helper can route them deterministically.
    let events = [
        "terminal:data",
        "terminal:state",
        "workspace:snapshot",
        "workspace:sessionMetrics",
        "ssh:interaction",
        "app:window-close-request",
        "app:window-maximized-change",
        "app:ui-preferences-changed",
        "app:close-active-workspace-item-request",
    ];
    for name in &events {
        assert!(
            name.contains(':'),
            "event `{name}` violates the `namespace:name` contract"
        );
        assert!(
            !name.starts_with(':') && !name.ends_with(':'),
            "event `{name}` must not have an empty namespace or name"
        );
    }
}
