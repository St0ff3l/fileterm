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
//!
//! 5. **Renderer style CSP contract** — xterm and Monaco generate runtime
//!    styles, so Tauri must not add a nonce to `style-src` that disables the
//!    configured `unsafe-inline`; script CSP modification remains enabled.

use fileterm_lib::commands::OpenWindowInput;
use fileterm_lib::services::profile_ops::{heal_profiles, strip_secret_fields_public};
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
        parent_id
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
    );
    Value::Object(obj)
}

fn command_body<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("pub async fn {name}");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("command `{name}` must exist"));
    let remainder = &source[start..];
    let end = remainder[1..]
        .find("#[tauri::command]")
        .map(|index| index + 1)
        .unwrap_or(remainder.len());
    &remainder[..end]
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
        "proxyPassword": "legacy-form-proxy-secret",
        "host": "example.com",
        "port": 22,
    });
    let stripped = strip_secret_fields_public(&profile);
    assert!(
        stripped.get("password").is_none(),
        "password must be stripped"
    );
    assert!(
        stripped.get("passphrase").is_none(),
        "passphrase must be stripped"
    );
    assert!(
        stripped.get("privateKeyPath").is_none(),
        "privateKeyPath must be stripped"
    );
    assert!(
        stripped.get("proxyPassword").is_none(),
        "legacy top-level proxyPassword must be stripped"
    );
    // Non-secret fields stay intact.
    assert_eq!(
        stripped.get("host").and_then(|v| v.as_str()),
        Some("example.com")
    );
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
    assert!(
        proxy.get("password").is_none(),
        "proxy.password must be stripped"
    );
    assert_eq!(
        proxy.get("host").and_then(|v| v.as_str()),
        Some("127.0.0.1")
    );
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
        "app_cancel_file_editor_close",
        "app_show_window_menu",
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

#[test]
fn renderer_csp_allows_runtime_styles_without_relaxing_scripts() {
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json"))
        .expect("tauri.conf.json must remain valid JSON");
    let security = config
        .pointer("/app/security")
        .and_then(Value::as_object)
        .expect("Tauri security configuration must exist");
    let csp = security
        .get("csp")
        .and_then(Value::as_str)
        .expect("production CSP must be configured");

    assert!(
        csp.contains("style-src 'self' 'unsafe-inline'"),
        "xterm and Monaco runtime style elements must be allowed"
    );
    assert!(
        !csp.contains("script-src 'self' 'unsafe-inline'"),
        "runtime styles must not weaken the script policy"
    );

    let disabled_directives = security
        .get("dangerousDisableAssetCspModification")
        .and_then(Value::as_array)
        .expect("Tauri CSP modification exceptions must be directive-scoped");
    assert_eq!(
        disabled_directives,
        &[Value::String("style-src".to_string())],
        "only style-src nonce injection may be disabled"
    );
}

#[test]
fn ui_preference_setter_returns_the_shared_contract_shape() {
    let source = include_str!("../src/commands/mod.rs");
    assert!(
        source.contains("Result<UiPreferences, AppError>"),
        "app_set_ui_preferences must return the updated preferences like Electron and FileTermDesktopApi"
    );
    assert!(
        source.contains("Ok(preferences)"),
        "app_set_ui_preferences must not resolve with an empty IPC payload"
    );
    assert!(
        source.contains("install_localized_application_menu")
            && source.contains("install_localized_tray_menu"),
        "locale changes must refresh both the application and tray menus"
    );
}

#[test]
fn connection_library_never_returns_profile_secrets() {
    let source = include_str!("../src/commands/mod.rs");
    let start = source
        .find("pub async fn app_get_connection_library")
        .expect("connection library command must exist");
    let remainder = &source[start..];
    let end = remainder[1..]
        .find("#[tauri::command]")
        .map(|index| index + 1)
        .expect("another command should follow the connection library");
    let command = &remainder[..end];
    assert!(
        command.contains("strip_secret_fields_public"),
        "standalone connection windows must receive the same scrubbed profiles as workspace snapshots"
    );
}

#[test]
fn profile_and_command_mutations_are_serialized_and_broadcast() {
    let source = include_str!("../src/commands/mod.rs");
    let mutations = [
        "app_workspace_mutation",
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
    ];
    for name in mutations {
        let body = command_body(source, name);
        assert!(
            body.contains("lock_library_after_transfer_hydration"),
            "`{name}` must share the multi-window library mutation lock"
        );
        assert!(
            body.contains("get_workspace_snapshot_and_emit"),
            "`{name}` must broadcast the persisted snapshot to every window"
        );
    }

    let open_profile = command_body(source, "app_open_profile");
    assert!(
        open_profile.contains("lock_library_after_transfer_hydration"),
        "opening a profile mutates recency and must use the library lock"
    );
    assert!(
        open_profile.contains("get_workspace_snapshot_and_emit"),
        "opening a profile must immediately publish its connecting tab"
    );
}

#[test]
fn protocol_workers_publish_connected_closed_and_error_states() {
    let protocols = [
        ("SSH", include_str!("../src/sessions/ssh.rs")),
        ("FTP", include_str!("../src/sessions/ftp.rs")),
        ("Telnet", include_str!("../src/sessions/telnet.rs")),
        ("Serial", include_str!("../src/sessions/serial.rs")),
    ];
    for (name, source) in protocols {
        for status in ["Connected", "Closed", "Error"] {
            assert!(
                source.contains(&format!("WorkspaceTabStatus::{status}")),
                "{name} worker must map its lifecycle to WorkspaceTabStatus::{status}"
            );
        }
    }
}

// ── Nested payload contracts ─────────────────────────────────────────────

#[test]
fn file_editor_window_payload_uses_camel_case_tab_id() {
    let input: OpenWindowInput = serde_json::from_value(json!({
        "kind": "file-editor",
        "source": "remote",
        "path": "/etc/hosts",
        "name": "hosts",
        "tabId": "tab-ssh-1",
        "encoding": "utf-8"
    }))
    .expect("camelCase file editor input should deserialize");
    assert_eq!(input.tab_id.as_deref(), Some("tab-ssh-1"));

    let snake_case = serde_json::from_value::<OpenWindowInput>(json!({
        "kind": "file-editor",
        "source": "remote",
        "path": "/etc/hosts",
        "name": "hosts",
        "tab_id": "tab-ssh-1"
    }));
    assert!(
        snake_case.is_err(),
        "snake_case payloads must fail instead of silently dropping tabId"
    );
}

#[test]
fn tauri_bridge_forwards_file_editor_tab_id_in_camel_case() {
    let bridge = include_str!("../../src/bridge/tauri-api.ts");
    assert!(
        bridge.contains("tabId: input.tabId"),
        "bridge must forward the remote editor session as tabId"
    );
    assert!(
        !bridge.contains("tab_id: input.tabId"),
        "bridge must not emit Rust field spellings"
    );
}

#[test]
fn tauri_bridge_is_structurally_checked_without_runtime_proxy_fallback() {
    let bridge = include_str!("../../src/bridge/tauri-api.ts");
    assert!(
        bridge.contains("satisfies FileTermDesktopApi"),
        "bridge object must be checked against the shared API at compile time"
    );
    assert!(
        !bridge.contains("as unknown as FileTermDesktopApi"),
        "bridge must not bypass the shared API type"
    );
    assert!(
        !bridge.contains("new Proxy("),
        "missing bridge methods must fail typecheck instead of becoming runtime stubs"
    );
}

#[test]
fn tauri_renderer_mounts_only_after_native_metadata_is_ready() {
    let bridge = include_str!("../../src/bridge/tauri-api.ts");
    assert!(bridge.contains("export async function createTauriApi"));
    assert!(!bridge.contains("appVersion: '0.0.0'"));
    assert!(!bridge.contains("arch = 'unknown'"));

    let bootstrap = include_str!("../../src/renderer/main.tsx");
    let assign = bootstrap
        .find("window.fileterm = api")
        .expect("bootstrap must expose the resolved API");
    let render = bootstrap
        .find("root.render(")
        .expect("bootstrap must mount React");
    assert!(
        assign < render,
        "native metadata must resolve before React mounts"
    );
}

#[test]
fn native_drop_fallback_is_cleared_only_after_renderer_consumption() {
    let bridge = include_str!("../../src/bridge/tauri-api.ts");
    assert!(
        bridge.contains("consume: clearNativeDropFallback"),
        "native drop events must expose an explicit acknowledgement"
    );
    let renderer = include_str!("../../src/renderer/hooks/useFileOperations.ts");
    assert!(
        renderer.contains("detail.consume()"),
        "the accepted remote-pane drop must acknowledge native path consumption"
    );
}
