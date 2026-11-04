use super::SHELL_CWD_SETUP;
use std::io::Write;
use std::process::{Command, Stdio};

/// Exercise the real injected script; skip shells absent on the CI host.
fn interactive_output(shell: &str, args: &[&str], script: &str) -> Option<String> {
    let mut child = match Command::new(shell)
        .args(args)
        .env("TERM", "dumb")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("could not start {shell}: {error}"),
    };
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{shell} failed: {:?}",
        output.status
    );
    Some(String::from_utf8(output.stdout).unwrap())
}

#[test]
fn zsh_preexec_preserves_user_hooks_and_skips_blank_input_after_reinjection() {
    let script = format!(
        "PS1=; preexec() {{ printf USER_HOOK; }}\n{SHELL_CWD_SETUP}\n{SHELL_CWD_SETUP}\n\n\nprintf COMMAND_DONE\n\nexit\n"
    );
    let Some(output) = interactive_output("zsh", &["-dfi"], &script) else {
        return;
    };
    // Reinstallation itself, the command and exit. Blank lines emit no marker;
    // installing twice must not duplicate our preexec hook.
    assert_eq!(output.matches("\x1b]7777;FileTermCommand\x07").count(), 3);
    assert_eq!(output.matches("USER_HOOK").count(), 4);
    assert!(output.contains("COMMAND_DONE"));
}

#[test]
fn bash_ps0_preserves_user_content_and_works_with_history_disabled() {
    let script = format!(
        "PS1=; PS0=USER_PS0; PROMPT_COMMAND=\"printf USER_PROMPT\"; set +o history\n{SHELL_CWD_SETUP}\n{SHELL_CWD_SETUP}\n\n\nprintf COMMAND_DONE\n\ncase \"$PS0\" in *USER_PS0*) printf PS0_PRESERVED ;; esac\nexit\n"
    );
    let Some(output) = interactive_output("bash", &["--noprofile", "--norc", "-i"], &script) else {
        return;
    };
    assert!(output.contains("PS0_PRESERVED"));
    assert!(output.contains("USER_PROMPT"));
    assert!(output.contains("COMMAND_DONE"));
    if output.contains("\x1b]7777;FileTermCommandTracking=1\x07") {
        // PS0 output is written to stderr; test its value explicitly below.
        let script = format!(
            "PS0=USER_PS0\n{SHELL_CWD_SETUP}\n{SHELL_CWD_SETUP}\nprintf \"%s\" \"$PS0\"\nexit\n"
        );
        let output = interactive_output("bash", &["--noprofile", "--norc", "-i"], &script).unwrap();
        assert_eq!(output.matches("\x1b]7777;FileTermCommand\x07").count(), 1);
    } else {
        assert!(output.contains("\x1b]7777;FileTermCommandTracking=0\x07"));
    }
}

#[test]
fn missing_root_home_preserves_real_cwd_and_reports_root_identity() {
    let directory =
        std::env::temp_dir().join(format!("fileterm-missing-home-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let login_home = directory.join("stoffel");
    std::fs::create_dir(&login_home).unwrap();
    let missing_home = directory.join("root");
    let script = format!(
        "cd {}; HOME={}; id() {{ printf root; }}; {}; {}",
        super::shell_quote(login_home.to_str().unwrap()),
        super::shell_quote(missing_home.to_str().unwrap()),
        super::SHELL_HOME_DIRECTORY_RECOVERY,
        SHELL_CWD_SETUP,
    );
    let result = Command::new("bash")
        .args(["--noprofile", "--norc", "-c", &script])
        .output();
    let expected = login_home
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let missing = !missing_home.exists();
    std::fs::remove_dir_all(&directory).unwrap();
    let output = match result {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("bash fixture failed: {error}"),
    };
    assert!(missing);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (cwd, user) =
        super::track_cwd_and_user(&String::from_utf8_lossy(&output.stdout), &mut String::new());
    assert_eq!(cwd.as_deref(), Some(expected.as_str()));
    assert_eq!(user.as_deref(), Some("root"));
    assert_eq!(
        super::resolve_shell_file_access("stoffel", user.as_deref().unwrap()),
        ("root", Some("root".into()))
    );
}
