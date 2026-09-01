#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    #[cfg(unix)]
    use std::io::Read;
    #[cfg(unix)]
    use std::sync::mpsc as std_mpsc;
    #[cfg(unix)]
    use std::time::Duration;

    #[cfg(unix)]
    use crate::services::workspace::WorkspaceTabStatus;
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    use super::{
        append_local_output_chunk, available_shells, clamp_u16, cmd_args_have_explicit_command,
        configure_shell_command, default_launch, local_shell_exit_summary,
        powershell_args_have_explicit_command, resolve_launch, scan_alt_screen_transition,
        shell_name, validate_launch, AltScreenTransitionScanner, LocalOsc7CwdTracker,
        LocalOutputChunk, LocalProcessTree, LocalTerminalLaunch, LocalTerminalLaunchOptions,
        LocalTerminalQueryScanner, Utf8StreamDecoder,
    };
    #[cfg(unix)]
    use super::{run_pty_loop, LocalPtyCommand};

    #[cfg(unix)]
    type TestPtyMaster = Box<dyn portable_pty::MasterPty + Send>;
    #[cfg(unix)]
    type TestPtyChild = Box<dyn portable_pty::Child + Send + Sync>;

    #[cfg(unix)]
    fn spawn_posix_test_pty(
        script: &str,
    ) -> (
        TestPtyMaster,
        TestPtyChild,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        let child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        drop(slave);

        let mut reader = master
            .try_clone_reader()
            .expect("local PTY reader should clone");
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => output.extend_from_slice(&buffer[..size]),
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(output);
        });

        (master, child, output_rx)
    }

    #[test]
    fn pty_size_clamps_to_platform_u16_values() {
        assert_eq!(clamp_u16(0, 80), 80);
        assert_eq!(clamp_u16(120, 80), 120);
        assert_eq!(clamp_u16(u32::MAX, 80), u16::MAX);
    }

    #[test]
    fn utf8_stream_decoder_preserves_code_points_split_across_reads() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.decode("中".as_bytes().split_at(1).0), "");
        assert_eq!(decoder.decode(&"中".as_bytes()[1..]), "中");
        assert_eq!(decoder.decode(" + ".as_bytes()), " + ");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn utf8_stream_decoder_flushes_an_incomplete_tail_at_eof() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.decode(&[0xf0, 0x9f]), "");
        assert_eq!(decoder.finish(), "�");
    }

    #[test]
    fn shell_name_handles_paths_and_login_shell_markers() {
        assert_eq!(shell_name("/bin/zsh"), "zsh");
        assert_eq!(shell_name("-bash"), "bash");
        assert_eq!(shell_name("C:\\Windows\\System32\\cmd.exe"), "cmd.exe");
    }

    #[test]
    fn available_shells_do_not_return_duplicate_commands_or_paths() {
        let options = available_shells();

        assert!(options
            .windows(2)
            .all(|pair| { pair[0].shell != pair[1].shell && pair[0].path != pair[1].path }));
    }

    #[test]
    fn launch_validation_rejects_empty_or_missing_explicit_shell_paths() {
        assert!(validate_launch(&LocalTerminalLaunch {
            shell: "  ".to_string(),
            title: None,
            cwd: "/tmp".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
        })
        .is_err());
        assert!(validate_launch(&LocalTerminalLaunch {
            shell: "/definitely/missing/fileterm-shell".to_string(),
            title: None,
            cwd: "/tmp".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
        })
        .is_err());
        assert!(validate_launch(&LocalTerminalLaunch {
            shell: "zsh".to_string(),
            title: None,
            cwd: "/tmp".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
        })
        .is_ok());
    }

    #[test]
    fn launch_options_merge_platform_defaults_with_one_shot_overrides() {
        // validate_launch rejects path-like shells that do not exist, so the
        // override must name a real file on every platform the test runs on.
        let shell = if cfg!(windows) {
            std::env::var_os("SystemRoot")
                .map(|root| {
                    std::path::PathBuf::from(root)
                        .join("System32")
                        .join("cmd.exe")
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_else(|| "cmd.exe".to_string())
        } else {
            "/bin/sh".to_string()
        };
        let mut environment = BTreeMap::new();
        environment.insert("FILETERM_TEST".to_string(), "present".to_string());
        let launch = resolve_launch(Some(LocalTerminalLaunchOptions {
            shell: Some(shell.clone()),
            title: Some("Agent terminal".to_string()),
            cwd: Some("/tmp".to_string()),
            args: Some(vec!["-i".to_string()]),
            env: Some(environment.clone()),
        }))
        .expect("valid local launch options should resolve");

        assert_eq!(launch.shell, shell);
        assert_eq!(launch.title.as_deref(), Some("Agent terminal"));
        assert_eq!(launch.cwd, "/tmp");
        assert_eq!(launch.args, vec!["-i"]);
        assert_eq!(launch.env, environment);
    }

    #[test]
    fn launch_options_trim_optional_tab_title_and_reject_invalid_title() {
        let launch = resolve_launch(Some(LocalTerminalLaunchOptions {
            title: Some("  Claude Code  ".to_string()),
            ..Default::default()
        }))
        .expect("a trimmed local terminal title should resolve");
        assert_eq!(launch.title.as_deref(), Some("Claude Code"));

        let invalid = LocalTerminalLaunch {
            title: Some("x".repeat(121)),
            ..default_launch()
        };
        assert!(validate_launch(&invalid).is_err());
    }

    #[test]
    fn launch_validation_rejects_nul_and_oversized_overrides() {
        let mut launch = default_launch();
        launch.args = vec!["bad\0arg".to_string()];
        assert!(validate_launch(&launch).is_err());

        launch.args = vec!["x".repeat(32 * 1024 + 1)];
        assert!(validate_launch(&launch).is_err());
    }

    #[test]
    fn local_shell_exit_summary_keeps_exit_code_or_signal() {
        assert_eq!(
            local_shell_exit_summary(&portable_pty::ExitStatus::with_exit_code(0)),
            "Local shell exited with code 0"
        );
        assert_eq!(
            local_shell_exit_summary(&portable_pty::ExitStatus::with_exit_code(127)),
            "Local shell exited: Exited with code 127"
        );
        assert_eq!(
            local_shell_exit_summary(&portable_pty::ExitStatus::with_signal("SIGHUP")),
            "Local shell exited: Terminated by SIGHUP"
        );
    }

    #[test]
    fn local_output_drop_notice_is_inserted_before_resumed_output() {
        let mut batch = String::from("before");
        append_local_output_chunk(
            &mut batch,
            &LocalOutputChunk {
                data: "after".to_string(),
                dropped_bytes_before: 42,
                dropped_alt_screen_change: false,
            },
        );

        assert!(batch.starts_with("before\r\n[FileTerm: local terminal output dropped 42 bytes"));
        assert!(batch.ends_with("]\r\nafter"));
    }

    #[test]
    fn local_output_drop_notice_flags_alt_screen_transitions() {
        let mut batch = String::new();
        append_local_output_chunk(
            &mut batch,
            &LocalOutputChunk {
                data: "resumed".to_string(),
                dropped_bytes_before: 100,
                dropped_alt_screen_change: true,
            },
        );

        assert!(batch.contains("dropped 100 bytes"));
        assert!(batch.contains("alternate screen transitions"));
        assert!(batch.contains("reset"));
    }

    #[test]
    fn scan_alt_screen_transition_detects_common_modes() {
        // 1049 是 vim/less/nano 最常用的 alt screen 切换
        assert!(scan_alt_screen_transition("\x1b[?1049h"));
        assert!(scan_alt_screen_transition("\x1b[?1049l"));
        // 47 / 1047 是较早的 alt screen 实现
        assert!(scan_alt_screen_transition("\x1b[?47h"));
        assert!(scan_alt_screen_transition("\x1b[?1047l"));
        // 组合模式（同时设置多个私有模式）
        assert!(scan_alt_screen_transition("\x1b[?1;1049h"));
        assert!(scan_alt_screen_transition("\x1b[?47;1049h"));
    }

    #[test]
    fn scan_alt_screen_transition_ignores_unrelated_sequences() {
        // 普通光标移动、颜色等不应触发
        assert!(!scan_alt_screen_transition("\x1b[2J"));
        assert!(!scan_alt_screen_transition("\x1b[H"));
        assert!(!scan_alt_screen_transition("\x1b[31m"));
        assert!(!scan_alt_screen_transition("\x1b[?25h")); // 光标可见，不是 alt screen
        assert!(!scan_alt_screen_transition("\x1b[?2004h")); // bracketed paste
        assert!(!scan_alt_screen_transition("plain text"));
        assert!(!scan_alt_screen_transition(""));
    }

    #[test]
    fn scan_alt_screen_transition_handles_split_sequences() {
        let mut scanner = AltScreenTransitionScanner::default();
        assert!(!scanner.observe("output\x1b"));
        assert!(!scanner.observe("[?1049"));
        assert!(scanner.observe("h"));
    }

    #[test]
    fn scan_alt_screen_transition_handles_a_split_sequence_after_a_successful_chunk() {
        let mut scanner = AltScreenTransitionScanner::default();
        assert!(!scanner.observe("\x1b[?1049"));
        assert!(scanner.has_pending_sequence());
        assert!(scanner.observe("hrest"));
        assert!(!scanner.has_pending_sequence());
    }

    #[test]
    fn powershell_explicit_command_detection_accepts_abbreviations() {
        // 完整形式
        assert!(powershell_args_have_explicit_command(&[
            "-Command".to_string(),
            "Get-Date".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&[
            "-File".to_string(),
            "script.ps1".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&[
            "-EncodedCommand".to_string()
        ]));
        // PowerShell 唯一前缀缩写
        assert!(powershell_args_have_explicit_command(
            &["-Comm".to_string()]
        ));
        assert!(powershell_args_have_explicit_command(&[
            "-comma".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&["-fil".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-enc".to_string()]));
        assert!(powershell_args_have_explicit_command(&[
            "-CommandWithArgs".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&["-cwa".to_string()]));

        // 官方短写
        assert!(powershell_args_have_explicit_command(&["-c".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-f".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-e".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-ec".to_string()]));

        // 大小写不敏感
        assert!(powershell_args_have_explicit_command(&[
            "-COMMAND".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(
            &["-File".to_string()]
        ));
    }

    #[test]
    fn powershell_explicit_command_detection_rejects_unrelated_args() {
        // 无关参数
        assert!(!powershell_args_have_explicit_command(&[
            "-NoLogo".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[
            "-NoExit".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[]));
        // 非 flag 参数
        assert!(!powershell_args_have_explicit_command(&[
            "script.ps1".to_string()
        ]));
        // 形似但不匹配
        assert!(!powershell_args_have_explicit_command(&[
            "-NoCommand".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[
            "-ConfigurationFile".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[
            "-ConfigurationName".to_string()
        ]));
    }

    #[test]
    fn cmd_explicit_command_detection_preserves_user_command_modes() {
        assert!(cmd_args_have_explicit_command(&["/c".to_string()]));
        assert!(cmd_args_have_explicit_command(&["/K".to_string()]));
        assert!(!cmd_args_have_explicit_command(&["/q".to_string()]));
        assert!(!cmd_args_have_explicit_command(&[]));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn default_powershell_launch_skips_user_profile_and_keeps_utf8_setup() {
        let mut command = CommandBuilder::new("powershell.exe");
        configure_shell_command(&mut command, "powershell.exe", &[], &BTreeMap::new());

        let argv: Vec<String> = command
            .get_argv()
            .iter()
            .filter_map(|value| value.to_str().map(ToOwned::to_owned))
            .collect();
        assert_eq!(
            &argv[..4],
            &["powershell.exe", "-NoLogo", "-NoProfile", "-NoExit"]
        );
        assert!(argv.windows(2).any(|values| {
            values[0] == "-Command"
                && values[1].contains("Console]::InputEncoding")
                && values[1].contains("Console]::OutputEncoding")
        }));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn explicit_powershell_arguments_preserve_profile_behavior() {
        let mut command = CommandBuilder::new("powershell.exe");
        let arguments = vec!["-ExecutionPolicy".to_string(), "Bypass".to_string()];
        configure_shell_command(&mut command, "powershell.exe", &arguments, &BTreeMap::new());

        let argv: Vec<String> = command
            .get_argv()
            .iter()
            .filter_map(|value| value.to_str().map(ToOwned::to_owned))
            .collect();
        assert!(!argv.contains(&"-NoProfile".to_string()));
        assert!(argv
            .windows(2)
            .any(|values| { values[0] == "-ExecutionPolicy" && values[1] == "Bypass" }));
    }

    #[test]
    fn osc7_cwd_tracker_handles_split_and_percent_encoded_markers() {
        let mut tracker = LocalOsc7CwdTracker::default();
        assert_eq!(
            tracker.observe("\u{1b}]7;file://localhost/Users/stoffel/My%20Project"),
            None
        );
        assert_eq!(
            tracker.observe("\u{7}"),
            Some("/Users/stoffel/My Project".to_string())
        );

        assert_eq!(
            tracker.observe("\u{1b}]7;file:///tmp/project\u{1b}\\"),
            Some("/tmp/project".to_string())
        );
    }

    #[test]
    fn osc7_cwd_tracker_keeps_an_escape_prefix_split_across_reads() {
        let mut tracker = LocalOsc7CwdTracker::default();
        assert_eq!(tracker.observe("prompt\u{1b}]"), None);
        assert_eq!(
            tracker.observe("7;file:///tmp/next\u{7}"),
            Some("/tmp/next".to_string())
        );
    }

    #[test]
    fn local_terminal_query_scanner_replies_to_startup_queries_and_hides_them() {
        let mut scanner = LocalTerminalQueryScanner::default();

        let (display, replies) = scanner.consume("\u{1b}[6n");
        assert_eq!(display, "");
        assert_eq!(replies, vec!["\u{1b}[1;1R"]);

        let (display, replies) = scanner.consume("PS> ");
        assert_eq!(display, "PS> ");
        assert!(replies.is_empty());

        // Once the first prompt has arrived, later queries remain visible to
        // the renderer so xterm.js can answer them with the real cursor.
        let (display, replies) = scanner.consume("\u{1b}[6n");
        assert_eq!(display, "\u{1b}[6n");
        assert!(replies.is_empty());
    }

    #[test]
    fn local_terminal_query_scanner_handles_split_queries_and_device_status() {
        let mut scanner = LocalTerminalQueryScanner::default();

        let (display, replies) = scanner.consume("prefix\u{1b}[");
        assert_eq!(display, "prefix");
        assert!(replies.is_empty());

        let (display, replies) = scanner.consume("5n");
        assert_eq!(display, "");
        assert_eq!(replies, vec!["\u{1b}[0n"]);

        let (display, replies) = scanner.consume("prompt");
        assert_eq!(display, "prompt");
        assert!(replies.is_empty());

        let (display, replies) = scanner.consume("\u{1b}[c");
        assert_eq!(display, "\u{1b}[c");
        assert!(replies.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn posix_shell_is_started_as_a_login_shell_with_terminal_capabilities() {
        let mut command = CommandBuilder::new("/bin/zsh");
        configure_shell_command(&mut command, "/bin/zsh", &[], &BTreeMap::new());

        let argv = command.get_argv();
        assert!(argv.iter().any(|value| value.to_str() == Some("-l")));
        assert!(argv.windows(2).any(|values| {
            values[0].to_str() == Some("-o") && values[1].to_str() == Some("promptsubst")
        }));
        assert_eq!(
            command.get_env("TERM").and_then(|value| value.to_str()),
            Some("xterm-256color")
        );
        assert_eq!(
            command
                .get_env("COLORTERM")
                .and_then(|value| value.to_str()),
            Some("truecolor")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_local_environment_is_not_replaced_by_terminal_defaults() {
        let mut command = CommandBuilder::new("/bin/sh");
        let mut environment = BTreeMap::new();
        environment.insert("TERM".to_string(), "dumb".to_string());
        environment.insert("FILETERM_TEST".to_string(), "present".to_string());
        let arguments = vec!["-c".to_string(), "printf test".to_string()];

        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/sh", &arguments, &environment);

        assert_eq!(
            command.get_argv().get(1).and_then(|value| value.to_str()),
            Some("-l")
        );
        assert_eq!(
            command.get_argv().get(2).and_then(|value| value.to_str()),
            Some("-c")
        );
        assert_eq!(
            command.get_env("TERM").and_then(|value| value.to_str()),
            Some("dumb")
        );
        assert_eq!(
            command
                .get_env("FILETERM_TEST")
                .and_then(|value| value.to_str()),
            Some("present")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn bash_gets_osc7_prompt_command_injection() {
        let mut command = CommandBuilder::new("/bin/bash");
        configure_shell_command(&mut command, "/bin/bash", &[], &BTreeMap::new());

        let prompt_command = command
            .get_env("PROMPT_COMMAND")
            .and_then(|value| value.to_str())
            .expect("bash should receive a PROMPT_COMMAND");
        assert!(
            prompt_command.contains("\\033]7;"),
            "PROMPT_COMMAND should emit OSC 7: {prompt_command}"
        );
        assert!(
            prompt_command.contains("${PWD//"),
            "PROMPT_COMMAND should reference $PWD: {prompt_command}"
        );
        assert!(
            prompt_command.contains("${PWD//%/%25}"),
            "PROMPT_COMMAND should encode literal percent signs: {prompt_command}"
        );
        assert!(
            !prompt_command.contains("trap") && !prompt_command.contains("DEBUG"),
            "PROMPT_COMMAND should not install a high-frequency DEBUG trap: {prompt_command}"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_prompt_command_is_not_overwritten_for_bash() {
        let mut command = CommandBuilder::new("/bin/bash");
        let mut environment = BTreeMap::new();
        environment.insert("PROMPT_COMMAND".to_string(), "custom-hook".to_string());
        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/bash", &[], &environment);

        assert_eq!(
            command
                .get_env("PROMPT_COMMAND")
                .and_then(|value| value.to_str()),
            Some("custom-hook")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn zsh_gets_an_osc7_prompt_without_changing_default_prompt_style() {
        let mut command = CommandBuilder::new("/bin/zsh");
        configure_shell_command(&mut command, "/bin/zsh", &[], &BTreeMap::new());

        let prompt = command
            .get_env("PROMPT")
            .and_then(|value| value.to_str())
            .expect("zsh should receive a default-compatible PROMPT");
        assert!(
            prompt.contains("$(printf") && prompt.contains("${PWD//%/%25}"),
            "zsh prompt should emit encoded OSC 7 with the current path: {prompt:?}"
        );
        assert!(
            prompt.ends_with("%m%# "),
            "zsh prompt should retain the default visual suffix: {prompt:?}"
        );
        assert!(command.get_argv().windows(2).any(|values| {
            values[0].to_str() == Some("-o") && values[1].to_str() == Some("promptsubst")
        }));
        assert!(command.get_env("PROMPT_COMMAND").is_none());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_zsh_prompt_is_not_overwritten() {
        let mut command = CommandBuilder::new("/bin/zsh");
        let mut environment = BTreeMap::new();
        environment.insert("PROMPT".to_string(), "custom-prompt".to_string());
        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/zsh", &[], &environment);

        assert_eq!(
            command.get_env("PROMPT").and_then(|value| value.to_str()),
            Some("custom-prompt")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_zsh_ps1_is_not_overwritten() {
        let mut command = CommandBuilder::new("/bin/zsh");
        let mut environment = BTreeMap::new();
        environment.insert("PS1".to_string(), "custom-prompt".to_string());
        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/zsh", &[], &environment);

        assert!(command.get_env("PROMPT").is_none());
        assert_eq!(
            command.get_env("PS1").and_then(|value| value.to_str()),
            Some("custom-prompt")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn zsh_prompt_uses_the_terminal_program_marker_for_rc_overrides() {
        let mut command = CommandBuilder::new("/bin/zsh");
        configure_shell_command(&mut command, "/bin/zsh", &[], &BTreeMap::new());

        assert_eq!(
            command
                .get_env("TERM_PROGRAM")
                .and_then(|value| value.to_str()),
            Some("FileTerm")
        );
    }

    #[cfg(unix)]
    #[test]
    fn real_local_pty_preserves_utf8_output_and_exit_status() {
        use std::io::Read;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf 'FileTerm local 中文\\n'; exit 7"]);
        let mut child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        drop(slave);
        let mut reader = master
            .try_clone_reader()
            .expect("local PTY reader should clone");
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => output.extend_from_slice(&buffer[..size]),
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(output);
        });
        let _writer = master
            .take_writer()
            .expect("local PTY writer should be available");
        #[cfg(target_os = "macos")]
        std::thread::sleep(std::time::Duration::from_millis(20));
        drop(_writer);
        let status = child.wait().expect("local shell should exit");
        let output = output_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("local PTY reader should finish after shell exit");

        assert!(String::from_utf8_lossy(&output).contains("FileTerm local 中文"));
        assert_eq!(status.exit_code(), 7);
    }

    #[cfg(unix)]
    #[test]
    fn real_local_pty_routes_ctrl_c_to_the_foreground_shell() {
        let (master, child, output_rx) = spawn_posix_test_pty(
            "trap 'echo FileTerm-ctrl-c; exit 42' INT; while :; do sleep 1; done",
        );
        let writer = master
            .take_writer()
            .expect("local PTY writer should be available");
        let process_tree = LocalProcessTree::attach(child.as_ref());
        let (control_tx, control_rx) = std_mpsc::channel();
        let (result_tx, result_rx) = std_mpsc::channel();
        let runner = std::thread::spawn(move || {
            let mut child = child;
            let result = run_pty_loop(control_rx, &mut child, master, writer, &process_tree);
            let _ = result_tx.send(result);
        });

        std::thread::sleep(Duration::from_millis(100));
        control_tx
            .send(LocalPtyCommand::Input("\u{3}".to_string()))
            .expect("local PTY should accept Ctrl+C input");

        let (summary, status) = match result_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => result,
            Err(error) => {
                let _ = control_tx.send(LocalPtyCommand::Shutdown);
                let _ = result_rx.recv_timeout(Duration::from_secs(2));
                let _ = runner.join();
                panic!("Ctrl+C did not stop the local shell in time: {error}");
            }
        };
        runner
            .join()
            .expect("local PTY runner should finish after Ctrl+C");
        let output = output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("local PTY reader should finish after Ctrl+C");

        assert_eq!(status, WorkspaceTabStatus::Closed);
        assert!(
            summary.contains("42"),
            "unexpected shell summary: {summary}"
        );
        assert!(
            String::from_utf8_lossy(&output).contains("FileTerm-ctrl-c"),
            "unexpected Ctrl+C output: {:?}",
            String::from_utf8_lossy(&output)
        );
    }

    #[cfg(unix)]
    #[test]
    fn real_local_pty_can_restart_after_process_tree_shutdown() {
        let (first_master, first_child, first_output_rx) =
            spawn_posix_test_pty("printf 'FileTerm first\\n'; while :; do sleep 1; done");
        let first_writer = first_master
            .take_writer()
            .expect("first local PTY writer should be available");
        let first_process_tree = LocalProcessTree::attach(first_child.as_ref());
        let (first_control_tx, first_control_rx) = std_mpsc::channel();
        let (first_result_tx, first_result_rx) = std_mpsc::channel();
        let first_runner = std::thread::spawn(move || {
            let mut child = first_child;
            let result = run_pty_loop(
                first_control_rx,
                &mut child,
                first_master,
                first_writer,
                &first_process_tree,
            );
            let _ = first_result_tx.send(result);
        });

        std::thread::sleep(Duration::from_millis(100));
        first_control_tx
            .send(LocalPtyCommand::Shutdown)
            .expect("first local PTY should accept shutdown");
        let (first_summary, first_status) = first_result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first local PTY should stop in time");
        first_runner
            .join()
            .expect("first local PTY runner should finish");
        let first_output = first_output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first local PTY reader should finish");
        assert_eq!(first_status, WorkspaceTabStatus::Closed);
        assert_eq!(first_summary, "Local shell stopped");
        assert!(String::from_utf8_lossy(&first_output).contains("FileTerm first"));

        let (second_master, second_child, second_output_rx) =
            spawn_posix_test_pty("printf 'FileTerm second\\n'; exit 0");
        let second_writer = second_master
            .take_writer()
            .expect("second local PTY writer should be available");
        let second_process_tree = LocalProcessTree::attach(second_child.as_ref());
        let (second_control_tx, second_control_rx) = std_mpsc::channel();
        let (second_result_tx, second_result_rx) = std_mpsc::channel();
        let second_runner = std::thread::spawn(move || {
            let mut child = second_child;
            let result = run_pty_loop(
                second_control_rx,
                &mut child,
                second_master,
                second_writer,
                &second_process_tree,
            );
            let _ = second_result_tx.send(result);
        });

        let (second_summary, second_status) =
            match second_result_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(result) => result,
                Err(error) => {
                    let _ = second_control_tx.send(LocalPtyCommand::Shutdown);
                    let _ = second_runner.join();
                    panic!("second local PTY did not finish in time: {error}");
                }
            };
        second_runner
            .join()
            .expect("second local PTY runner should finish");
        let second_output = second_output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second local PTY reader should finish");

        assert_eq!(second_status, WorkspaceTabStatus::Closed);
        assert!(second_summary.contains("code 0"));
        assert!(String::from_utf8_lossy(&second_output).contains("FileTerm second"));
    }

    #[cfg(windows)]
    #[test]
    fn real_local_conpty_preserves_output_and_exit_status() {
        use std::io::{Read, Write};

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local ConPTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/C", "echo FileTerm local && exit /B 7"]);
        let mut child = slave
            .spawn_command(command)
            .expect("cmd.exe should start in ConPTY");
        drop(slave);
        let mut reader = master
            .try_clone_reader()
            .expect("ConPTY reader should clone");
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        output.extend_from_slice(&buffer[..size]);
                        let _ = output_tx.send(output.clone());
                    }
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(output);
        });
        // Keep the ConPTY input pipe open while cmd.exe runs: closing the
        // writer early makes conhost treat the client as gone and terminate
        // the child with STATUS_CONTROL_C_EXIT before it can echo anything.
        let mut writer = master
            .take_writer()
            .expect("ConPTY writer should be available");
        // cmd.exe under ConPTY emits ESC[6n (cursor position request) at
        // startup and blocks until the terminal replies. The production local
        // PTY reader answers this at the transport boundary; this standalone
        // test has to emulate that reply itself.
        let mut replied_cpr = false;
        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let status = loop {
            while let Ok(snapshot) = output_rx.try_recv() {
                output = snapshot;
            }
            if !replied_cpr && output.windows(4).any(|window| window == b"\x1b[6n") {
                replied_cpr = true;
                let _ = writer.write_all(b"\x1b[1;1R");
                let _ = writer.flush();
            }
            if let Ok(Some(status)) = child.try_wait() {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cmd.exe did not exit within 15s; output so far: {:?}",
                String::from_utf8_lossy(&output)
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        drop(writer);
        // ConPTY renders through conhost on a throttled refresh pass, so the
        // echo bytes can still be in flight after cmd.exe exits. Wait for the
        // payload to reach the reader before closing the master, otherwise
        // conhost is torn down with the data still buffered inside it.
        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if std::time::Instant::now() >= deadline {
                break;
            }
            match output_rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(snapshot) => {
                    output = snapshot;
                    if String::from_utf8_lossy(&output).contains("FileTerm local") {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        // Closing the master ends the conhost session; the reader then sees
        // EOF and publishes its final snapshot.
        drop(master);
        if let Ok(snapshot) = output_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            output = snapshot;
        }

        assert!(String::from_utf8_lossy(&output).contains("FileTerm local"));
        assert_eq!(status.exit_code(), 7);
    }

    #[cfg(unix)]
    #[test]
    fn local_process_tree_terminates_shell_process_group() {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master: _, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 30"]);
        let mut child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        let process_tree = LocalProcessTree::attach(child.as_ref());

        process_tree.terminate(child.as_mut());
        let status = child.wait().expect("terminated shell should be reapable");
        assert!(!status.success());
    }

    /// 验证进程组终止能收掉孙进程，而不只是直接子 shell。
    /// 修复前实现依赖 portable_pty 的 forkpty 调了 setsid()，但测试没显式
    /// 覆盖 grandchild，回归会漏掉。
    #[cfg(unix)]
    #[test]
    fn local_process_tree_terminates_grandchild_process() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let pid_file = std::env::temp_dir().join(format!(
            "fileterm-grandchild-{}-{}.pid",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_file(&pid_file);

        // 启动一个后台 sleep（grandchild），把 pid 写到文件，shell wait 它。
        let script = format!(
            "sleep 30 &\necho $! > {pid_file}\nwait\n",
            pid_file = pid_file.display()
        );

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master: _, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", &script]);
        let mut child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        let process_tree = LocalProcessTree::attach(child.as_ref());

        // 等 grandchild pid 落盘
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let grandchild_pid = loop {
            if let Ok(content) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = content.trim().parse::<libc::pid_t>() {
                    if pid > 0 {
                        break pid;
                    }
                }
            }
            if std::time::Instant::now() > deadline {
                let _ = fs::remove_file(&pid_file);
                panic!("grandchild pid was not recorded in time");
            }
            std::thread::sleep(Duration::from_millis(50));
        };

        process_tree.terminate(child.as_mut());
        child.wait().expect("terminated shell should be reapable");

        // 给 SIGHUP/SIGKILL 时间生效，并容忍 init 回收孤儿的延迟。
        // kill -0 对僵尸进程也返回 0，所以需要轮询直到进程真正消失，
        // 避免 CI 高负载下 init 回收慢导致误报。
        let mut still_alive = true;
        for attempt in 0..15 {
            if unsafe { libc::kill(grandchild_pid, 0) != 0 } {
                still_alive = false;
                break;
            }
            if attempt < 14 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let _ = fs::remove_file(&pid_file);
        assert!(
            !still_alive,
            "grandchild (pid={grandchild_pid}) survived process tree termination after 1.5s"
        );
    }

    #[cfg(windows)]
    #[test]
    fn local_process_tree_terminates_conpty_job() {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local ConPTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/C", "ping 127.0.0.1 -n 30 > nul"]);
        let mut child = slave
            .spawn_command(command)
            .expect("cmd.exe should start in ConPTY");
        drop(slave);
        let _master = master;
        let process_tree = LocalProcessTree::attach(child.as_ref());

        process_tree.terminate(child.as_mut());
        let status = child
            .wait()
            .expect("terminated ConPTY process should be reapable");
        assert!(!status.success());
    }
}
