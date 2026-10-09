use super::*;

/// Follow the worker's real capture -> per-method-cache update boundary.
#[test]
fn same_password_populates_each_auth_method_cache_in_both_packet_orders() {
    for (old_method, command, new_method) in [
        (
            RootFileAccessMethod::Sudo,
            "su -\r",
            RootFileAccessMethod::Su,
        ),
        (
            RootFileAccessMethod::Su,
            "sudo -i\r",
            RootFileAccessMethod::Sudo,
        ),
    ] {
        for delayed_prompt in [false, true] {
            let mut awaiting = None;
            let mut pending = String::new();
            let mut recent = String::new();
            let mut active = Some("shared-test-password".to_string());
            let mut last = Some(PendingRootAccessAuth {
                method: old_method,
                target_user: "root".into(),
                interactive_shell: true,
            });
            let mut pending_command = None;
            let mut sudo =
                (old_method == RootFileAccessMethod::Sudo).then(|| active.clone().unwrap());
            let mut su = (old_method == RootFileAccessMethod::Su).then(|| active.clone().unwrap());
            assert!(!capture_root_access_password_input(
                command,
                &mut awaiting,
                &mut pending,
                &mut recent,
                &mut active,
                &mut last,
                &mut pending_command,
            ));
            let mut prompt = String::new();
            let updated = if delayed_prompt {
                assert!(!capture_root_access_password_input(
                    "shared-test-password\r",
                    &mut awaiting,
                    &mut pending,
                    &mut recent,
                    &mut active,
                    &mut last,
                    &mut pending_command,
                ));
                track_root_access_prompt_from_terminal(
                    "Password: ",
                    &mut prompt,
                    &mut awaiting,
                    &mut pending,
                    &mut active,
                    &mut last,
                    &mut pending_command,
                )
            } else {
                assert!(!track_root_access_prompt_from_terminal(
                    "Password: ",
                    &mut prompt,
                    &mut awaiting,
                    &mut pending,
                    &mut active,
                    &mut last,
                    &mut pending_command,
                ));
                capture_root_access_password_input(
                    "shared-test-password\r",
                    &mut awaiting,
                    &mut pending,
                    &mut recent,
                    &mut active,
                    &mut last,
                    &mut pending_command,
                )
            };
            assert!(updated, "method transition must update the worker cache");
            cache_root_password_for_auth(last.as_ref(), &active, &mut sudo, &mut su);
            assert_eq!(root_password_for_method(new_method, &sudo, &su), active);
            assert_eq!(last.as_ref().unwrap().method, new_method);
            assert_eq!(sudo.as_deref(), Some("shared-test-password"));
            assert_eq!(su.as_deref(), Some("shared-test-password"));
        }
    }
}

#[test]
fn history_recalled_sudo_and_su_are_tracked_from_completed_pty_echo() {
    for (command, method) in [
        ("sudo -i", RootFileAccessMethod::Sudo),
        ("su -", RootFileAccessMethod::Su),
    ] {
        for delayed_prompt in [false, true] {
            let mut awaiting = None;
            let mut pending_password = String::new();
            let mut recent_input = String::new();
            let mut active = None;
            let mut last = None;
            let mut pending_command = None;
            capture_root_access_password_input(
                "\x1b[A\r",
                &mut awaiting,
                &mut pending_password,
                &mut recent_input,
                &mut active,
                &mut last,
                &mut pending_command,
            );
            assert!(pending_command.is_none());
            assert!(pending_password.is_empty());
            if delayed_prompt {
                capture_root_access_password_input(
                    "fixture-password\r",
                    &mut awaiting,
                    &mut pending_password,
                    &mut recent_input,
                    &mut active,
                    &mut last,
                    &mut pending_command,
                );
            }
            let mut prompt = String::new();
            let output = format!("\r\x1b[K[alice@host ~]$ {command}\r\nPassword: ");
            let observed = track_root_access_prompt_from_terminal(
                &output,
                &mut prompt,
                &mut awaiting,
                &mut pending_password,
                &mut active,
                &mut last,
                &mut pending_command,
            );
            assert_eq!(pending_command.as_ref().unwrap().method, method);
            if delayed_prompt {
                assert!(observed);
            } else {
                assert!(!observed);
                assert_eq!(awaiting.as_ref().unwrap().method, method);
                assert!(capture_root_access_password_input(
                    "fixture-password\r",
                    &mut awaiting,
                    &mut pending_password,
                    &mut recent_input,
                    &mut active,
                    &mut last,
                    &mut pending_command,
                ));
            }
            assert_eq!(active.as_deref(), Some("fixture-password"));
            assert_eq!(last.as_ref().unwrap().method, method);
        }
    }
}

#[test]
fn only_submitted_privilege_commands_arm_one_root_hook_installation() {
    assert!(!root_shell_setup_requested("sudo -i", "", false));
    assert!(root_shell_setup_requested("i\r", "sudo -", false));
    assert!(root_shell_setup_requested(
        "sudo -i\rfixture-password\r",
        "",
        false
    ));
    assert!(!root_shell_setup_requested("sudo -i\r", "", true));
    assert!(!root_shell_setup_requested("#\r", "", false));
    assert!(!root_shell_setup_requested("echo hi\r", "", false));
    assert!(should_reinject_root_shell_setup(
        true,
        false,
        false,
        true,
        "[root@host ~]# "
    ));
    assert!(!should_reinject_root_shell_setup(
        true,
        false,
        false,
        false,
        "[root@host ~]# "
    ));
    assert!(!should_reinject_root_shell_setup(
        true,
        true,
        false,
        true,
        "[root@host ~]# "
    ));
}

#[test]
fn history_privilege_echo_handles_every_control_sequence_packet_boundary() {
    for command in ["su -", "sudo -i"] {
        let echo = format!("\x1b[?2004l{command}\r\nPassword: ");
        for split in 0..=echo.len() {
            let mut buffer = String::new();
            let mut awaiting = None;
            let mut password = String::new();
            let mut active = None;
            let mut last = None;
            let mut pending = None;
            for packet in [&echo[..split], &echo[split..]] {
                track_root_access_prompt_from_terminal(
                    packet,
                    &mut buffer,
                    &mut awaiting,
                    &mut password,
                    &mut active,
                    &mut last,
                    &mut pending,
                );
            }
            assert_eq!(
                pending,
                privilege_command_from_terminal_input(command),
                "split={split}"
            );
            assert_eq!(awaiting, pending, "split={split}");
        }
    }
}

#[test]
fn privilege_input_ignores_focus_events_and_applies_backspace_edits() {
    for input in ["\x1b[Isu -\r", "sud\x7f -\r", "wrong\x15su -\r"] {
        assert_eq!(
            privilege_command_from_terminal_input(input),
            privilege_command_from_terminal_input("su -")
        );
    }
}
