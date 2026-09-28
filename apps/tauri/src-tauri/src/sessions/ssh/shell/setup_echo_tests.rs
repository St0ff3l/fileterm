use super::*;

#[test]
fn highlighted_setup_redraws_stay_hidden_after_buffer_limit() {
    for release_prompt in [false, true] {
        let mut pending = Some(ShellSetupEchoSuppression::with_prompt_policy(
            false,
            release_prompt,
        ));
        // A line editor can redraw the full, syntax-highlighted setup command
        // repeatedly before executing it. This exceeds the retention budget.
        let redraw = format!("\r\x1b[38;2;249;226;175m {SHELL_CWD_SETUP}\x1b[0m");
        for _ in 0..32 {
            assert_eq!(suppress_shell_setup_echo(&mut pending, &redraw), "");
            assert!(pending.is_some(), "output volume must not end suppression");
            assert!(pending.as_ref().unwrap().buffer.len() <= MAX_SHELL_SETUP_BUFFER_BYTES);
        }
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, "\r\n\x1b]7777;FileTerm"),
            ""
        );
        let prompt = "\x1b[32muser@host\x1b[0m:~$ ";
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, &format!("Ready\x07{prompt}")),
            if release_prompt { prompt } else { "" }
        );
        assert!(pending.is_none());
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, "echo user-command\r\n"),
            "echo user-command\r\n"
        );
    }
}

#[test]
fn oversized_setup_chunk_keeps_fragmented_ready_marker_and_utf8_safe_bound() {
    for terminator in ["\x07", "\x1b\\"] {
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        let oversized = format!("{}\x1b]7777;FileTermReady", "目录\x1b[33m".repeat(8192));
        assert_eq!(suppress_shell_setup_echo(&mut pending, &oversized), "");
        assert!(pending.as_ref().unwrap().buffer.len() <= MAX_SHELL_SETUP_BUFFER_BYTES);
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, &format!("{terminator}host% ")),
            "host% "
        );
        assert!(pending.is_none());
    }
}

#[test]
fn oversized_setup_echo_preserves_banner_and_timeout_fallback() {
    let mut pending = Some(ShellSetupEchoSuppression::new(true));
    let echo = format!("Welcome\r\nuser$ {SHELL_CWD_SETUP}{}", "x".repeat(32768));
    assert_eq!(suppress_shell_setup_echo(&mut pending, &echo), "");
    assert_eq!(
        suppress_shell_setup_echo(&mut pending, "\x1b]7777;FileTermReady\x07user$ "),
        ""
    );
    assert_eq!(
        finish_shell_setup_suppression(&mut pending),
        "Welcome\r\nuser$ "
    );

    let mut pending = Some(ShellSetupEchoSuppression::with_fallback("root# ".into()));
    assert_eq!(
        suppress_shell_setup_echo(&mut pending, &"x".repeat(32768)),
        ""
    );
    pending.as_mut().unwrap().started_at = Instant::now() - SHELL_SETUP_TIMEOUT;
    assert_eq!(
        suppress_shell_setup_echo(&mut pending, "user output"),
        "root# user output"
    );
    assert!(pending.is_none());
}

#[test]
fn completed_setup_preserves_custom_prompt_and_fragmented_escape_on_timeout() {
    for prompt in ["➜ 12:23 root ~ ", "\x1b[32mroot@主机", "\x1b[38;2;249;"] {
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                &format!("hidden setup echo\r\n\x1b]7777;FileTermReady\x07{prompt}")
            ),
            ""
        );
        pending.as_mut().unwrap().marker_seen_at = Some(Instant::now() - SHELL_SETUP_SETTLE_DELAY);
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, "next packet"),
            format!("{prompt}next packet")
        );
        assert!(pending.is_none());
    }
}

#[test]
fn output_after_ready_is_not_trimmed_with_the_hidden_echo() {
    let mut pending = Some(ShellSetupEchoSuppression::new(false));
    let prompt = format!("\x1b[32m{}\x1b[0m$ ", "long prompt ".repeat(2000));
    assert_eq!(
        suppress_shell_setup_echo(
            &mut pending,
            &format!("hidden echo\x1b]7777;FileTermReady\x07{prompt}")
        ),
        prompt
    );
    assert!(pending.is_none());
}

#[test]
fn setup_output_from_either_stream_releases_queued_input_once() {
    let mut pending = Some(ShellSetupEchoSuppression::new(false));
    let mut deferred = vec![b"echo user-command\r".to_vec()];
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Exercise the shared Data/ExtendedData filter, including stderr echo or
    // diagnostics followed by a ready marker split across deliveries.
    for chunk in ["stdout echo", "stderr echo", "\x1b]7777;FileTerm"] {
        assert_eq!(
            filter_shell_setup_output(&mut pending, &mut deferred, &tx, chunk).unwrap(),
            ""
        );
        assert!(rx.try_recv().is_err());
    }
    assert_eq!(
        filter_shell_setup_output(&mut pending, &mut deferred, &tx, "Ready\x07user$ ").unwrap(),
        "user$ "
    );
    assert_eq!(rx.try_recv().unwrap(), b"echo user-command\r");
    assert!(deferred.is_empty());
    assert!(pending.is_none());
    assert_eq!(
        filter_shell_setup_output(&mut pending, &mut deferred, &tx, "normal stderr\r\n").unwrap(),
        "normal stderr\r\n"
    );
    assert!(rx.try_recv().is_err());
}
