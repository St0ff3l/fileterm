// Command contract and behavior tests.
#[cfg(test)]
mod command_template_tests {
    use super::render_command_template;

    #[test]
    fn renders_positional_command_template_arguments() {
        assert_eq!(
            render_command_template(
                "deploy [p#1] --region [p#2] --empty=[p#3]",
                &["api".to_string(), "cn-north".to_string(),]
            ),
            "deploy api --region cn-north --empty="
        );
    }
}

#[cfg(test)]
mod mcp_agent_setup_tests {
    use super::{
        app_get_mcp_agent_setup, append_home_cli_search_paths, opencode_extra_search_paths,
        resolve_local_cli_from_paths,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn resolves_cli_from_ordered_search_paths_without_running_it() {
        let root =
            std::env::temp_dir().join(format!("fileterm-cli-discovery-{}", uuid::Uuid::new_v4()));
        let first_dir = root.join("first");
        let second_dir = root.join("second");
        std::fs::create_dir_all(&first_dir).expect("first search directory should be created");
        std::fs::create_dir_all(&second_dir).expect("second search directory should be created");
        let first_cli = first_dir.join("claude");
        let second_cli = second_dir.join("claude");
        std::fs::write(&first_cli, b"placeholder")
            .expect("first CLI placeholder should be written");
        std::fs::write(&second_cli, b"placeholder")
            .expect("second CLI placeholder should be written");

        let resolved = resolve_local_cli_from_paths("claude", vec![first_dir, second_dir]);

        assert_eq!(resolved, Some(first_cli));
        std::fs::remove_dir_all(root).expect("temporary CLI discovery directory should be removed");
    }

    #[test]
    fn includes_nvm_node_bins_for_desktop_launcher_fallback() {
        let root = std::env::temp_dir().join(format!("fileterm-cli-home-{}", uuid::Uuid::new_v4()));
        let nvm_bin = root.join(".nvm/versions/node/v24.15.0/bin");
        std::fs::create_dir_all(&nvm_bin).expect("nvm bin directory should be created");
        let claude = nvm_bin.join("claude");
        std::fs::write(&claude, b"placeholder").expect("Claude placeholder should be written");

        let mut search_paths = Vec::new();
        append_home_cli_search_paths(&mut search_paths, &root);
        let resolved = resolve_local_cli_from_paths("claude", search_paths);

        assert_eq!(resolved, Some(claude));
        std::fs::remove_dir_all(root).expect("temporary CLI home should be removed");
    }

    #[test]
    fn ignores_claude_npm_stub_when_native_binary_is_missing() {
        let root =
            std::env::temp_dir().join(format!("fileterm-claude-stub-{}", uuid::Uuid::new_v4()));
        let npm_bin = root.join("npm");
        let native_bin = npm_bin.join("node_modules/@anthropic-ai/claude-code/bin");
        std::fs::create_dir_all(&native_bin)
            .expect("Claude native bin directory should be created");
        std::fs::write(
            npm_bin.join("claude"),
            b"#!/bin/sh\nexec node_modules/@anthropic-ai/claude-code/bin/claude.exe\n",
        )
        .expect("Claude npm shim should be written");
        std::fs::write(
            native_bin.join("claude.exe"),
            b"echo \"Error: claude native binary not installed.\" >&2\nexit 1\n",
        )
        .expect("Claude fallback stub should be written");

        let resolved = resolve_local_cli_from_paths("claude", vec![npm_bin]);

        assert_eq!(resolved, None);
        std::fs::remove_dir_all(root).expect("temporary Claude stub directory should be removed");
    }

    #[test]
    fn ignores_codex_bundled_inside_a_macos_desktop_app() {
        let root =
            std::env::temp_dir().join(format!("fileterm-codex-app-{}", uuid::Uuid::new_v4()));
        let app_resources = root.join("ChatGPT.app/Contents/Resources");
        std::fs::create_dir_all(&app_resources)
            .expect("desktop app Resources directory should be created");
        let bundled_codex = app_resources.join("codex");
        std::fs::write(&bundled_codex, b"desktop helper")
            .expect("bundled Codex placeholder should be written");

        let resolved = resolve_local_cli_from_paths("codex", vec![app_resources]);

        assert_eq!(resolved, None);
        std::fs::remove_dir_all(root).expect("temporary desktop app directory should be removed");
    }

    #[test]
    fn still_resolves_user_codex_cli_outside_a_desktop_app() {
        let root =
            std::env::temp_dir().join(format!("fileterm-codex-cli-{}", uuid::Uuid::new_v4()));
        let cli_dir = root.join(".local/bin");
        std::fs::create_dir_all(&cli_dir).expect("user CLI directory should be created");
        let codex = cli_dir.join("codex");
        std::fs::write(&codex, b"user CLI").expect("user Codex placeholder should be written");

        let resolved = resolve_local_cli_from_paths("codex", vec![cli_dir]);

        assert_eq!(resolved, Some(codex));
        std::fs::remove_dir_all(root).expect("temporary user CLI directory should be removed");
    }

    #[test]
    fn includes_opencode_official_and_manager_install_paths_in_priority_order() {
        let home = Path::new("/home/tester");
        let gopath = std::env::join_paths([PathBuf::from("/go/path1"), PathBuf::from("/go/path2")])
            .expect("test GOPATH should be representable");

        let paths = opencode_extra_search_paths(
            home,
            Some(std::ffi::OsString::from("/custom/opencode/bin")),
            Some(std::ffi::OsString::from("/xdg/bin")),
            Some(gopath),
        );

        assert_eq!(paths[0], PathBuf::from("/custom/opencode/bin"));
        assert_eq!(paths[1], PathBuf::from("/xdg/bin"));
        assert_eq!(paths[2], PathBuf::from("/home/tester/bin"));
        assert_eq!(paths[3], PathBuf::from("/home/tester/.opencode/bin"));
        assert!(paths.contains(&PathBuf::from("/home/tester/.bun/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/go/bin")));
        assert!(paths.contains(&PathBuf::from("/go/path1/bin")));
        assert!(paths.contains(&PathBuf::from("/go/path2/bin")));
    }

    #[test]
    fn resolves_opencode_from_the_official_user_install_directory() {
        let root =
            std::env::temp_dir().join(format!("fileterm-opencode-cli-{}", uuid::Uuid::new_v4()));
        let cli_dir = root.join(".opencode/bin");
        std::fs::create_dir_all(&cli_dir).expect("OpenCode bin directory should be created");
        let opencode = cli_dir.join("opencode");
        std::fs::write(&opencode, b"OpenCode CLI").expect("OpenCode placeholder should be written");

        let paths = opencode_extra_search_paths(&root, None, None, None);
        let resolved = resolve_local_cli_from_paths("opencode", paths);

        assert_eq!(resolved, Some(opencode));
        std::fs::remove_dir_all(root).expect("temporary OpenCode directory should be removed");
    }

    #[test]
    fn deduplicates_opencode_install_paths() {
        let home = Path::new("/home/tester");
        let same_dir = std::ffi::OsString::from("/same/opencode/bin");

        let paths = opencode_extra_search_paths(home, Some(same_dir.clone()), Some(same_dir), None);

        assert_eq!(
            paths
                .iter()
                .filter(|path| path.as_path() == Path::new("/same/opencode/bin"))
                .count(),
            1
        );
    }

    #[test]
    fn generates_stdio_registration_commands_for_supported_clients() {
        let setup = app_get_mcp_agent_setup().expect("MCP Agent setup should be readable");
        assert!(!setup.fileterm_command.is_empty());
        assert!(
            setup.fileterm_command.starts_with('\'') || setup.fileterm_command.starts_with('"')
        );

        let claude = setup
            .clients
            .iter()
            .find(|client| client.id == "claude-code")
            .expect("Claude Code client should be exposed");
        assert!(claude
            .registration_command
            .starts_with("claude mcp add --scope user fileterm -- "));
        assert!(claude.registration_command.ends_with(" mcp"));

        let codex = setup
            .clients
            .iter()
            .find(|client| client.id == "codex-cli")
            .expect("Codex CLI client should be exposed");
        assert!(codex
            .registration_command
            .starts_with("codex mcp add fileterm -- "));
        assert!(codex.registration_command.ends_with(" mcp"));

        let opencode = setup
            .clients
            .iter()
            .find(|client| client.id == "opencode")
            .expect("OpenCode client should be exposed");
        assert_eq!(opencode.command, "opencode");
        assert!(opencode
            .registration_command
            .starts_with("opencode mcp add fileterm -- "));
        assert!(opencode.registration_command.ends_with(" mcp"));
    }
}

#[cfg(test)]
mod split_pane_close_tests {
    use super::{attach_split_pane_to_tabs, remove_split_pane_from_tabs, supports_split_panes};
    use crate::services::{PaneNode, SplitDirection, WorkspaceTab, WorkspaceTabStatus};

    fn tab(id: &str, pane_root: Option<PaneNode>, pane_root_tab_id: Option<&str>) -> WorkspaceTab {
        WorkspaceTab {
            id: id.to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status: WorkspaceTabStatus::Connected,
            is_background: false,
            source: None,
            pane_root,
            pane_root_tab_id: pane_root_tab_id.map(str::to_string),
        }
    }

    fn local_tab(
        id: &str,
        pane_root: Option<PaneNode>,
        pane_root_tab_id: Option<&str>,
    ) -> WorkspaceTab {
        WorkspaceTab {
            id: id.to_string(),
            profile_id: "__local_terminal__".to_string(),
            session_type: "local".to_string(),
            title: "Local Terminal".to_string(),
            layout: "terminal-only".to_string(),
            status: WorkspaceTabStatus::Connected,
            is_background: false,
            source: None,
            pane_root,
            pane_root_tab_id: pane_root_tab_id.map(str::to_string),
        }
    }

    #[test]
    fn closing_a_child_pane_keeps_the_existing_root_tab() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "child".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("child", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "child").unwrap();

        assert_eq!(outcome.root_tab_id, "root");
        assert!(!outcome.keeps_split);
        assert_eq!(outcome.remaining_pane_tab_ids, vec!["root"]);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "root");
        assert!(tabs[0].pane_root.is_none());
    }

    #[test]
    fn only_ssh_and_local_sessions_can_be_split() {
        assert!(supports_split_panes("ssh"));
        assert!(supports_split_panes("local"));
        assert!(!supports_split_panes("ftp"));
        assert!(!supports_split_panes("telnet"));
        assert!(!supports_split_panes("serial"));
    }

    #[test]
    fn closing_a_local_child_pane_preserves_the_local_root() {
        let mut tabs = vec![
            local_tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Column,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "child".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            local_tab("child", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "child").unwrap();

        assert_eq!(outcome.root_tab_id, "root");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].session_type, "local");
        assert!(tabs[0].pane_root.is_none());
    }

    #[test]
    fn closing_the_original_root_leaf_promotes_a_surviving_pane() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Split {
                            direction: SplitDirection::Column,
                            children: vec![
                                PaneNode::Leaf {
                                    tab_id: "second".to_string(),
                                },
                                PaneNode::Leaf {
                                    tab_id: "third".to_string(),
                                },
                            ],
                            weights: vec![0.5, 0.5],
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("second", None, Some("root")),
            tab("third", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "root").unwrap();

        assert_eq!(outcome.root_tab_id, "second");
        assert!(outcome.keeps_split);
        assert_eq!(outcome.remaining_pane_tab_ids, vec!["second", "third"]);
        assert_eq!(tabs.len(), 2);

        let promoted_root = tabs.iter().find(|tab| tab.id == "second").unwrap();
        assert!(promoted_root.pane_root.is_some());
        assert!(promoted_root.pane_root_tab_id.is_none());
        assert_eq!(
            tabs.iter()
                .find(|tab| tab.id == "third")
                .and_then(|tab| tab.pane_root_tab_id.as_deref()),
            Some("second")
        );
    }

    #[test]
    fn attaching_a_pane_to_an_independent_tab_creates_a_root_tree() {
        let mut tabs = vec![tab("root", None, None), tab("child", None, Some("root"))];

        let root_id =
            attach_split_pane_to_tabs(&mut tabs, "root", "child", SplitDirection::Row).unwrap();

        assert_eq!(root_id, "root");
        let root = tabs.iter().find(|tab| tab.id == "root").unwrap();
        assert_eq!(
            root.pane_root.as_ref().unwrap().leaf_tab_ids(),
            vec!["root", "child"]
        );
    }

    #[test]
    fn attaching_a_pane_to_an_existing_leaf_preserves_the_root_id() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "other".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("other", None, Some("root")),
            tab("child", None, Some("root")),
        ];

        let root_id =
            attach_split_pane_to_tabs(&mut tabs, "other", "child", SplitDirection::Column).unwrap();

        assert_eq!(root_id, "root");
        assert_eq!(
            tabs[0].pane_root.as_ref().unwrap().leaf_tab_ids(),
            vec!["root", "other", "child"]
        );
    }

    #[test]
    fn attaching_a_pane_fails_without_mutating_tabs_when_source_vanished() {
        let mut tabs = vec![tab("root", None, None)];
        let before = tabs.clone();

        let result = attach_split_pane_to_tabs(&mut tabs, "missing", "child", SplitDirection::Row);

        assert!(result.is_err());
        assert_eq!(
            serde_json::to_value(&tabs).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::claim_reconnect_tab;
    use crate::services::{WorkspaceTab, WorkspaceTabStatus};

    fn tab(status: WorkspaceTabStatus) -> WorkspaceTab {
        WorkspaceTab {
            id: "tab-1".to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status,
            is_background: false,
            source: None,
            pane_root: None,
            pane_root_tab_id: None,
        }
    }

    #[test]
    fn reconnect_can_only_be_claimed_once_while_connecting() {
        let mut tabs = vec![tab(WorkspaceTabStatus::Closed)];

        assert!(claim_reconnect_tab(&mut tabs, "tab-1"));
        assert_eq!(tabs[0].status, WorkspaceTabStatus::Connecting);
        assert!(!claim_reconnect_tab(&mut tabs, "tab-1"));
    }

    #[test]
    fn reconnect_does_not_claim_an_unknown_tab() {
        let mut tabs = vec![tab(WorkspaceTabStatus::Closed)];

        assert!(!claim_reconnect_tab(&mut tabs, "missing"));
        assert_eq!(tabs[0].status, WorkspaceTabStatus::Closed);
    }
}

#[cfg(test)]
mod architecture_tests {
    use super::resolve_native_arch;

    #[test]
    fn reports_apple_silicon_when_x64_process_runs_under_rosetta() {
        assert_eq!(resolve_native_arch("macos", "x86_64", true), "arm64");
    }

    #[test]
    fn canonicalizes_native_rust_architecture_names() {
        assert_eq!(resolve_native_arch("macos", "aarch64", true), "arm64");
        assert_eq!(resolve_native_arch("macos", "x86_64", false), "x64");
        assert_eq!(resolve_native_arch("linux", "x86_64", false), "x64");
    }
}

#[cfg(test)]
mod window_lifecycle_tests {
    use super::renderer_approved_close_should_destroy;

    #[test]
    fn main_window_close_keeps_the_lifecycle_guard() {
        assert!(!renderer_approved_close_should_destroy("main"));
        assert!(renderer_approved_close_should_destroy(
            "file-editor-local-1"
        ));
        assert!(renderer_approved_close_should_destroy("connection-manager"));
    }
}

#[cfg(test)]
mod ui_state_tests {
    use super::normalize_ui_state;

    #[test]
    fn reads_current_object_ui_state() {
        let states = normalize_ui_state(serde_json::json!({ "main.tab-ui": "tabs" })).unwrap();
        assert_eq!(
            states.get("main.tab-ui").and_then(|value| value.as_str()),
            Some("tabs")
        );
    }

    #[test]
    fn migrates_electron_and_legacy_array_ui_state() {
        let electron = normalize_ui_state(serde_json::json!({
            "version": 1,
            "values": { "ssh-key-manager-ui": "folders" }
        }))
        .unwrap();
        assert_eq!(
            electron
                .get("ssh-key-manager-ui")
                .and_then(|value| value.as_str()),
            Some("folders")
        );

        let legacy = normalize_ui_state(serde_json::json!([
            { "key": "ssh-key-manager-ui", "value": "legacy-folders" }
        ]))
        .unwrap();
        assert_eq!(
            legacy
                .get("ssh-key-manager-ui")
                .and_then(|value| value.as_str()),
            Some("legacy-folders")
        );
    }
}

#[cfg(test)]
mod ui_preferences_tests {
    use std::collections::BTreeMap;

    use super::{
        default_local_terminal_shells, default_overview_section_order,
        default_resource_monitoring_metric_order, default_resource_monitoring_metrics,
        default_theme_config, default_update_channel, normalize_local_terminal_shells,
        normalize_mcp_operation_policy, normalize_resource_monitoring_metric_order,
        normalize_theme_config, normalize_ui_preferences, reset_active_theme_for_app_version,
        resolve_profile_with_connection_defaults,
        LocalTerminalShellPreferences, McpAgentPreferences, SavedTheme, SshConnectionDefaults,
        UiPreferences, UiPreferencesInput,
    };

    #[test]
    fn normalizes_theme_config_colors_fonts_and_variant() {
        let mut config = default_theme_config();
        config.variant = "light".to_string();
        config.theme.contrast = 255;
        config.theme.accent = "not-a-color".to_string();
        config.theme.surface_secondary = "not-a-color".to_string();
        config.theme.semantic_colors.text_secondary = "not-a-color".to_string();
        config.theme.terminal.ansi.red = "#abc".to_string();
        config
            .theme
            .overrides
            .insert("--bg-main".to_string(), "#abc".to_string());
        config.theme.fonts.ui = Some("Inter".to_string());
        config.theme.fonts.code = Some("font-family: unsafe".to_string());

        let normalized = normalize_theme_config(config, "light");

        assert_eq!(normalized.variant, "light");
        assert_eq!(normalized.theme.contrast, 100);
        assert_eq!(normalized.theme.accent, "#3B82F6");
        assert_eq!(normalized.theme.surface_secondary, "#FFFFFF");
        assert_eq!(normalized.theme.semantic_colors.text_secondary, "#5E5E61");
        assert_eq!(normalized.theme.terminal.ansi.red, "#ABC");
        assert_eq!(
            normalized.theme.overrides.get("--bg-main"),
            Some(&"#ABC".to_string())
        );
        assert_eq!(normalized.theme.fonts.ui.as_deref(), Some("Inter"));
        assert_eq!(normalized.theme.fonts.code, None);
    }

    #[test]
    fn default_theme_config_keeps_the_compact_contract_and_terminal_selection_alpha() {
        let dark = default_theme_config();
        let light = super::default_theme_config_for_variant("light");
        let serialized = serde_json::to_value(&dark).expect("default theme should serialize");

        assert!(serialized["theme"].get("overrides").is_none());
        assert_eq!(dark.code_theme_id, "fileterm");
        assert_eq!(dark.base_theme_id.as_deref(), Some("fileterm"));
        assert_eq!(dark.theme.semantic_colors.diff_added, "#34d399");
        assert_eq!(dark.theme.semantic_colors.keyword, "#fbbf24");
        assert_eq!(dark.theme.semantic_colors.info, "#38bdf8");
        assert_eq!(dark.theme.semantic_colors.success, "#34d399");
        assert_eq!(dark.theme.terminal.selection_background, "#388BFD85");
        assert_eq!(light.theme.terminal.selection_background, "#0969DA42");
    }

    #[test]
    fn legacy_component_color_table_does_not_override_the_default_css_theme() {
        let mut legacy =
            serde_json::to_value(default_theme_config()).expect("default theme should serialize");
        for key in ["surfaceSecondary", "surfaceElevated"] {
            legacy["theme"]
                .as_object_mut()
                .expect("theme should be an object")
                .remove(key);
        }
        for key in [
            "secondary",
            "textSecondary",
            "info",
            "warning",
            "error",
            "success",
        ] {
            legacy["theme"]["semanticColors"]
                .as_object_mut()
                .expect("semantic colors should be an object")
                .remove(key);
        }
        legacy["theme"]["ui"] = serde_json::json!({
            "surfaces": { "app": "#FF00FF" },
            "dialog": { "surface": "#00FF00" }
        });

        let config: super::ThemeConfig =
            serde_json::from_value(legacy).expect("legacy theme should still deserialize");
        let normalized = normalize_theme_config(config, "dark");

        assert!(normalized.theme.overrides.is_empty());
        assert_eq!(normalized.code_theme_id, "fileterm");
        assert_eq!(normalized.theme.surface_secondary, "#1E1E1E");
        assert_eq!(normalized.theme.surface_elevated, "#2A2A2A");
        assert_eq!(normalized.theme.semantic_colors.secondary, "#8BBFFF");
        assert_eq!(normalized.theme.semantic_colors.success, "#34D399");
    }

    #[test]
    fn canonicalizes_legacy_fileterm_variant_id() {
        let mut config = default_theme_config();
        config.code_theme_id = "fileterm-dark".to_string();

        let normalized = normalize_theme_config(config, "dark");

        assert_eq!(normalized.code_theme_id, "fileterm");
    }

    #[test]
    fn normalizes_saved_theme_identity_and_inherited_base() {
        let mut custom = default_theme_config();
        custom.code_theme_id = "custom".to_string();
        custom.base_theme_id = Some("codex".to_string());
        custom.theme.accent = "not-a-color".to_string();

        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: vec![
                SavedTheme {
                    id: "  custom-one  ".to_string(),
                    name: "  My Codex Tweak  ".to_string(),
                    config: custom,
                    variants: BTreeMap::new(),
                },
                SavedTheme {
                    id: "custom-one".to_string(),
                    name: "Duplicate".to_string(),
                    config: default_theme_config(),
                    variants: BTreeMap::new(),
                },
            ],
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(preferences.custom_themes.len(), 1);
        assert_eq!(preferences.custom_themes[0].id, "custom-one");
        assert_eq!(preferences.custom_themes[0].name, "My Codex Tweak");
        assert_eq!(
            preferences.custom_themes[0].config.base_theme_id.as_deref(),
            Some("codex")
        );
        assert_eq!(preferences.custom_themes[0].config.theme.accent, "#0169CC");
    }

    #[test]
    fn resets_any_existing_theme_after_each_app_update_without_changing_the_variant() {
        let mut custom_theme = default_theme_config();
        custom_theme.code_theme_id = "custom".to_string();
        custom_theme.base_theme_id = Some("codex".to_string());
        custom_theme.variant = "light".to_string();
        custom_theme.theme.accent = "#123456".to_string();
        let mut preferences = normalize_ui_preferences(UiPreferences {
            theme: "fileterm-light".to_string(),
            locale: "zhCN".to_string(),
            theme_config: custom_theme,
            fileterm_theme_reset_app_version: None,
            custom_themes: Vec::new(),
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert!(reset_active_theme_for_app_version(&mut preferences, "2.2.8"));
        assert_eq!(preferences.theme, "fileterm-light");
        assert_eq!(preferences.theme_config.code_theme_id, "fileterm");
        assert_eq!(preferences.theme_config.base_theme_id.as_deref(), Some("fileterm"));
        assert_eq!(preferences.theme_config.variant, "light");
        assert_eq!(
            preferences.fileterm_theme_reset_app_version.as_deref(),
            Some("2.2.8")
        );

        preferences.theme_config.code_theme_id = "codex".to_string();
        preferences.theme_config.base_theme_id = Some("codex".to_string());
        assert!(!reset_active_theme_for_app_version(&mut preferences, "2.2.8"));
        assert_eq!(preferences.theme_config.code_theme_id, "codex");
        assert!(reset_active_theme_for_app_version(&mut preferences, "2.2.9"));
        assert_eq!(preferences.theme_config.code_theme_id, "fileterm");
        assert_eq!(preferences.theme_config.variant, "light");

        preferences.theme = "codex-light".to_string();
        preferences.fileterm_theme_reset_app_version = None;
        preferences.theme_config = default_theme_config();
        let preferences = normalize_ui_preferences(preferences);
        assert_eq!(preferences.theme_config.variant, "light");
        let mut preferences = preferences;
        assert!(reset_active_theme_for_app_version(&mut preferences, "2.2.10"));
        assert_eq!(preferences.theme_config.code_theme_id, "fileterm");
        assert_eq!(preferences.theme_config.variant, "light");
    }

    #[test]
    fn falls_back_to_safe_values_for_unknown_preferences() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "unknown-theme".to_string(),
            locale: "unknown-locale".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: false,
            update_channel: "nightly".to_string(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: vec![
                "unknown".to_string(),
                "stats".to_string(),
                "stats".to_string(),
            ],
        });

        assert_eq!(preferences.theme, "fileterm-dark");
        assert_eq!(preferences.locale, "zhCN");
        assert_eq!(preferences.update_channel, "stable");
        assert!(preferences.overview_show_recent);
        assert!(preferences.overview_show_all_connections);
        assert_eq!(
            preferences.overview_section_order,
            default_overview_section_order()
        );
    }

    #[test]
    fn resource_monitoring_defaults_keep_gpu_metrics_opt_in_and_order_complete() {
        let enabled = default_resource_monitoring_metrics();
        let order = default_resource_monitoring_metric_order();

        assert!(!enabled.iter().any(|metric| metric == "gpu"));
        assert!(!enabled.iter().any(|metric| metric == "gpuMemory"));
        assert!(!enabled.iter().any(|metric| metric == "gpuTemperature"));
        assert!(!enabled.iter().any(|metric| metric == "gpuPower"));
        assert_eq!(order.len(), 11);
        let expected_order: Vec<String> = [
            "load",
            "cpu",
            "memory",
            "swap",
            "disk",
            "gpu",
            "gpuMemory",
            "gpuTemperature",
            "gpuPower",
            "processes",
            "network",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(order, expected_order);
    }

    #[test]
    fn resource_monitoring_metric_order_deduplicates_and_appends_missing_items() {
        let normalized = normalize_resource_monitoring_metric_order(vec![
            "network".to_string(),
            "network".to_string(),
            "not-a-metric".to_string(),
            "cpu".to_string(),
        ]);

        assert_eq!(normalized[0], "network");
        assert_eq!(normalized[1], "cpu");
        assert_eq!(normalized.len(), 11);
        assert_eq!(
            normalized
                .iter()
                .filter(|metric| metric.as_str() == "network")
                .count(),
            1
        );
    }

    #[test]
    fn keeps_supported_preferences_unchanged() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-light".to_string(),
            locale: "enUS".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: false,
            update_channel: "beta".to_string(),
            terminal_zoom_locked: true,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: false,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: false,
            overview_show_recent: false,
            overview_show_all_connections: true,
            overview_show_quick_actions: false,
            overview_section_order: vec![
                "allConnections".to_string(),
                "stats".to_string(),
                "recent".to_string(),
                "quickActions".to_string(),
            ],
        });

        assert_eq!(preferences.theme, "default-light");
        assert_eq!(preferences.locale, "enUS");
        assert_eq!(preferences.update_channel, "beta");
        assert!(!preferences.auto_check_updates);
        assert!(preferences.terminal_zoom_locked);
        assert!(!preferences.overview_show_stats);
        assert!(!preferences.overview_show_recent);
        assert!(preferences.overview_show_all_connections);
        assert!(!preferences.overview_show_quick_actions);
        assert_eq!(
            preferences.overview_section_order,
            vec![
                "allConnections".to_string(),
                "stats".to_string(),
                "recent".to_string(),
                "quickActions".to_string()
            ]
        );
    }

    #[test]
    fn normalizes_invalid_mcp_agent_preferences_fail_closed() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences {
                connection_scope: "not-a-scope".to_string(),
                operation_policy: "not-a-policy".to_string(),
                allowed_profile_ids: vec![" profile-1 ".to_string(), "profile-1".to_string()],
                legacy_default_profile_id: Some("  ".to_string()),
            },
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(
            preferences.mcp_agent.connection_scope,
            "selected-connections"
        );
        assert_eq!(
            preferences.mcp_agent.operation_policy,
            "basic-safe-operations"
        );
        assert_eq!(
            preferences.mcp_agent.allowed_profile_ids,
            vec!["profile-1".to_string()]
        );
        assert_eq!(preferences.mcp_agent.legacy_default_profile_id, None);
        let serialized = serde_json::to_value(&preferences.mcp_agent)
            .expect("normalized MCP preferences should serialize");
        assert!(serialized.get("defaultProfileId").is_none());
        assert_eq!(
            normalize_mcp_operation_policy("approved-operations"),
            "basic-safe-operations"
        );
    }

    #[test]
    fn legacy_default_connection_scope_migrates_to_selected_allowlist() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences {
                connection_scope: "default-connection".to_string(),
                operation_policy: "read-only".to_string(),
                allowed_profile_ids: Vec::new(),
                legacy_default_profile_id: Some("profile-1".to_string()),
            },
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(
            preferences.mcp_agent.connection_scope,
            "selected-connections"
        );
        assert_eq!(preferences.mcp_agent.operation_policy, "read-only");
        assert_eq!(
            preferences.mcp_agent.allowed_profile_ids,
            vec!["profile-1".to_string()]
        );
        assert_eq!(preferences.mcp_agent.legacy_default_profile_id, None);
    }

    #[test]
    fn legacy_active_session_scope_fails_closed_to_empty_allowlist() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences {
                connection_scope: "active-session".to_string(),
                operation_policy: "read-only".to_string(),
                allowed_profile_ids: vec!["stale-profile".to_string()],
                legacy_default_profile_id: None,
            },
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(
            preferences.mcp_agent.connection_scope,
            "selected-connections"
        );
        assert!(preferences.mcp_agent.allowed_profile_ids.is_empty());
    }

    #[test]
    fn preserves_saved_connection_values_and_explicit_overrides() {
        let defaults = SshConnectionDefaults {
            use_empty_password: true,
            enable_exec_channel: false,
            enable_resource_monitoring: false,
            resource_monitoring_interval_seconds: 15,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            reconnect_mode: "enter".to_string(),
            legacy_algorithms: false,
        };
        let profile = serde_json::json!({
            "type": "ssh",
            "enableExecChannel": true,
            "enableResourceMonitoring": true,
            "resourceMonitoringIntervalSeconds": 5,
            "reconnectMode": "none",
            "legacyAlgorithms": false,
            "connectionOverrides": {
                "reconnectMode": "auto",
                "legacyAlgorithms": true
            }
        });

        let resolved = resolve_profile_with_connection_defaults(&profile, &defaults);

        assert_eq!(resolved["useEmptyPassword"], true);
        assert_eq!(resolved["enableExecChannel"], true);
        assert_eq!(resolved["enableResourceMonitoring"], true);
        assert_eq!(resolved["resourceMonitoringIntervalSeconds"], 5);
        assert_eq!(resolved["reconnectMode"], "auto");
        assert_eq!(resolved["legacyAlgorithms"], true);
        assert_eq!(profile["enableExecChannel"], true);
    }

    #[test]
    fn preserves_legacy_profile_values_without_override_metadata() {
        let defaults = SshConnectionDefaults::default();
        let profile = serde_json::json!({
            "type": "ssh",
            "enableExecChannel": false,
            "reconnectMode": "auto"
        });

        let resolved = resolve_profile_with_connection_defaults(&profile, &defaults);

        assert_eq!(resolved["enableExecChannel"], false);
        assert_eq!(resolved["reconnectMode"], "auto");
        assert_eq!(resolved["enableResourceMonitoring"], true);
        assert_eq!(resolved["resourceMonitoringIntervalSeconds"], 1);
    }

    #[test]
    fn defaults_auto_update_checks_for_existing_preferences() {
        let preferences: UiPreferences = serde_json::from_value(serde_json::json!({
            "theme": "default-dark",
            "locale": "zhCN"
        }))
        .expect("legacy UI preferences should still deserialize");

        assert!(preferences.auto_check_updates);
        assert_eq!(preferences.update_channel, "stable");
        assert!(preferences.overview_show_stats);
        assert!(preferences.overview_show_recent);
        assert!(preferences.overview_show_all_connections);
        assert!(preferences.overview_show_quick_actions);
        assert_eq!(preferences.theme_config.schema_version, "codex-theme-v1");
        assert_eq!(
            preferences.overview_section_order,
            default_overview_section_order()
        );
        let local_terminal_shell_defaults = default_local_terminal_shells();
        assert_eq!(
            preferences.local_terminal_shells.win32,
            local_terminal_shell_defaults.win32
        );
        assert_eq!(
            preferences.local_terminal_shells.darwin,
            local_terminal_shell_defaults.darwin
        );
        assert_eq!(
            preferences.local_terminal_shells.linux,
            local_terminal_shell_defaults.linux
        );
    }

    #[test]
    fn restores_blank_local_terminal_shells_to_platform_defaults() {
        let defaults = default_local_terminal_shells();
        let normalized = normalize_local_terminal_shells(LocalTerminalShellPreferences {
            win32: "  ".to_string(),
            darwin: " /custom/zsh ".to_string(),
            linux: String::new(),
        });

        assert_eq!(normalized.win32, defaults.win32);
        assert_eq!(normalized.darwin, "/custom/zsh");
        assert_eq!(normalized.linux, defaults.linux);
    }

    #[test]
    fn uses_camel_case_for_the_update_check_preference_contract() {
        let input: UiPreferencesInput = serde_json::from_value(serde_json::json!({
            "autoCheckUpdates": false,
            "updateChannel": "beta",
            "overviewShowStats": false,
            "overviewShowRecent": false,
            "overviewShowAllConnections": true,
            "overviewShowQuickActions": false,
            "overviewSectionOrder": ["recent", "allConnections", "stats", "quickActions"],
            "localTerminalShells": { "win32": "pwsh.exe" }
        }))
        .expect("renderer preference input should deserialize");
        assert_eq!(input.auto_check_updates, Some(false));
        assert_eq!(input.update_channel.as_deref(), Some("beta"));
        assert_eq!(input.overview_show_stats, Some(false));
        assert_eq!(input.overview_show_recent, Some(false));
        assert_eq!(input.overview_show_all_connections, Some(true));
        assert_eq!(input.overview_show_quick_actions, Some(false));
        assert_eq!(
            input
                .local_terminal_shells
                .as_ref()
                .and_then(|shells| shells.win32.as_deref()),
            Some("pwsh.exe")
        );
        assert_eq!(
            input.overview_section_order,
            Some(vec![
                "recent".to_string(),
                "allConnections".to_string(),
                "stats".to_string(),
                "quickActions".to_string()
            ])
        );

        let preferences = serde_json::to_value(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some("2.2.8".to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: false,
            update_channel: "beta".to_string(),
            terminal_zoom_locked: true,
            local_terminal_shells: default_local_terminal_shells(),
            file_panel_remember_ratio: false,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: false,
            overview_show_recent: false,
            overview_show_all_connections: true,
            overview_show_quick_actions: false,
            overview_section_order: vec![
                "recent".to_string(),
                "allConnections".to_string(),
                "stats".to_string(),
                "quickActions".to_string(),
            ],
        })
        .expect("preferences should serialize");
        assert_eq!(preferences["autoCheckUpdates"], false);
        assert_eq!(preferences["updateChannel"], "beta");
        assert_eq!(preferences["overviewShowStats"], false);
        assert_eq!(preferences["overviewShowRecent"], false);
        assert_eq!(preferences["overviewShowAllConnections"], true);
        assert_eq!(preferences["overviewShowQuickActions"], false);
        assert_eq!(
            preferences["themeConfig"]["schemaVersion"],
            "codex-theme-v1"
        );
        assert!(preferences["themeConfig"]["theme"]["terminal"]["ansi"]["brightBlack"].is_string());
        assert_eq!(
            preferences["overviewSectionOrder"],
            serde_json::json!(["recent", "allConnections", "stats", "quickActions"])
        );
        assert_eq!(preferences["localTerminalShells"]["win32"], "pwsh.exe");
    }
}

#[cfg(test)]
mod permission_contract_tests {
    use super::{
        parse_remote_permission_mode, PermissionApplyTarget, RemotePermissionChangeOptions,
    };

    #[test]
    fn reads_shared_camel_case_permission_contract() {
        let options: RemotePermissionChangeOptions = serde_json::from_value(serde_json::json!({
            "mode": "0640",
            "recursive": true,
            "applyTo": "files"
        }))
        .expect("shared permission options should deserialize");

        assert_eq!(parse_remote_permission_mode(&options.mode).unwrap(), 0o640);
        assert!(options.recursive);
        assert!(matches!(
            options.apply_to,
            Some(PermissionApplyTarget::Files)
        ));
    }

    #[test]
    fn rejects_legacy_permissions_field_instead_of_defaulting_to_0755() {
        let options = serde_json::from_value::<RemotePermissionChangeOptions>(serde_json::json!({
            "permissions": 384,
            "recursive": false
        }));
        assert!(options.is_err());
    }

    #[test]
    fn validates_octal_permission_modes() {
        assert_eq!(parse_remote_permission_mode("600").unwrap(), 0o600);
        assert_eq!(parse_remote_permission_mode("755").unwrap(), 0o755);
        assert!(parse_remote_permission_mode("888").is_err());
        assert!(parse_remote_permission_mode("75").is_err());
    }
}

#[cfg(test)]
mod serial_port_contract_tests {
    use crate::services::serial_ports::map_serial_port_info;

    #[test]
    fn maps_usb_metadata_without_accessing_hardware() {
        let item = map_serial_port_info(tokio_serial::SerialPortInfo {
            port_name: "/dev/cu.test".to_string(),
            port_type: tokio_serial::SerialPortType::UsbPort(tokio_serial::UsbPortInfo {
                vid: 0x1234,
                pid: 0xabcd,
                serial_number: Some("SN-1".to_string()),
                manufacturer: Some("Test Vendor".to_string()),
                product: Some("Test Adapter".to_string()),
            }),
        });

        assert_eq!(item.port_name, "/dev/cu.test");
        assert_eq!(item.port_type, "usb");
        assert_eq!(item.vendor_id, Some(0x1234));
        assert_eq!(item.product_id, Some(0xabcd));
        assert_eq!(item.manufacturer.as_deref(), Some("Test Vendor"));
        assert_eq!(item.product.as_deref(), Some("Test Adapter"));
        assert_eq!(item.serial_number.as_deref(), Some("SN-1"));

        let serialized = serde_json::to_value(item).expect("serial port item should serialize");
        assert_eq!(serialized["portName"], "/dev/cu.test");
        assert_eq!(serialized["vendorId"], 0x1234);
        assert_eq!(serialized["productId"], 0xabcd);
    }
}

#[cfg(test)]
mod external_url_tests {
    use super::validate_external_url;

    #[test]
    fn external_url_policy_accepts_only_web_links() {
        for allowed in [
            "https://github.com/St0ff3l/fileterm",
            "http://127.0.0.1/docs",
        ] {
            assert!(validate_external_url(allowed).is_ok());
        }
        for denied in [
            "file:///etc/passwd",
            "ssh://example.com",
            "javascript:alert(1)",
        ] {
            assert!(validate_external_url(denied).is_err());
        }
        assert!(validate_external_url("not a url").is_err());
    }
}

#[cfg(test)]
mod background_session_tests {
    use super::{
        attach_background_session_in_tabs, detach_session_to_background_in_tabs,
        next_visible_top_level_tab_id,
    };
    use crate::services::{WorkspaceSessionSource, WorkspaceTab, WorkspaceTabStatus};

    fn tab(id: &str, is_background: bool) -> WorkspaceTab {
        WorkspaceTab {
            id: id.to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status: WorkspaceTabStatus::Connected,
            is_background,
            source: None,
            pane_root: None,
            pane_root_tab_id: None,
        }
    }

    fn external_tab(id: &str, is_background: bool) -> WorkspaceTab {
        let mut tab = tab(id, is_background);
        tab.source = Some(WorkspaceSessionSource::Cli);
        tab
    }

    #[test]
    fn attaching_background_session_only_changes_visibility_and_returns_root_id() {
        let mut tabs = vec![tab("background", true), tab("visible", false)];

        assert_eq!(
            attach_background_session_in_tabs(&mut tabs, "background").unwrap(),
            "background"
        );
        assert!(!tabs[0].is_background);
        assert!(!tabs[1].is_background);
    }

    #[test]
    fn attaching_unknown_session_is_rejected() {
        let mut tabs = vec![tab("background", true)];

        assert!(attach_background_session_in_tabs(&mut tabs, "missing").is_err());
        assert!(tabs[0].is_background);
    }

    #[test]
    fn detaching_external_session_only_changes_visibility() {
        let mut tabs = vec![external_tab("external", false), tab("visible", false)];

        assert_eq!(
            detach_session_to_background_in_tabs(&mut tabs, "external").unwrap(),
            "external"
        );
        assert!(tabs[0].is_background);
        assert_eq!(
            next_visible_top_level_tab_id(&tabs, "external"),
            Some("visible".to_string())
        );
    }

    #[test]
    fn detaching_gui_session_is_rejected() {
        let mut tabs = vec![tab("gui", false)];

        assert!(detach_session_to_background_in_tabs(&mut tabs, "gui").is_err());
        assert!(!tabs[0].is_background);
    }
}
