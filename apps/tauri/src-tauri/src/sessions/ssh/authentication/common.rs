#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyboardInteractiveMode {
    /// A previous password/key/agent factor succeeded. Do not reuse its
    /// secret for any later KBI challenge, even if the prompt says Password.
    AdditionalFactor,
    /// KBI is the normal password-like fallback (for example, a server that
    /// does not advertise the SSH `password` method).
    PasswordFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthenticationResult {
    Authenticated,
    /// The server kept this SSH transport open and advertised
    /// keyboard-interactive as the next authentication method.
    KeyboardInteractiveAvailable {
        mode: KeyboardInteractiveMode,
    },
    Rejected,
}

fn authentication_result_from_auth_result(result: &AuthResult) -> AuthenticationResult {
    match result {
        AuthResult::Success => AuthenticationResult::Authenticated,
        AuthResult::Failure {
            remaining_methods,
            partial_success,
        } if remaining_methods.contains(&MethodKind::KeyboardInteractive) => {
            // `partial_success` distinguishes an additional MFA factor from
            // a normal method fallback, but both cases keep the SSH handle
            // alive and advertise keyboard-interactive. The latter is common
            // on devices that implement password login only as KBI (there is
            // no `password` method and partial_success is false).
            AuthenticationResult::KeyboardInteractiveAvailable {
                mode: if *partial_success {
                    KeyboardInteractiveMode::AdditionalFactor
                } else {
                    KeyboardInteractiveMode::PasswordFallback
                },
            }
        }
        AuthResult::Failure { .. } => AuthenticationResult::Rejected,
    }
}

fn should_restart_keyboard_interactive(
    partial_success: bool,
    remaining_methods: &MethodSet,
    restart_count: usize,
) -> bool {
    partial_success
        && remaining_methods.contains(&MethodKind::KeyboardInteractive)
        && restart_count < MAX_KEYBOARD_INTERACTIVE_RESTARTS
}

/// Complete the configured primary authentication and, when the server
/// advertises it, continue keyboard-interactive on the same authenticated
/// transport. Reconnecting here loses partial-success state and can make a
/// jump-host target ask for its host key a second time.
async fn authenticate_session(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    auth_type: &str,
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
    authentication_target: SshAuthenticationTarget,
) -> Result<bool, String> {
    match try_authenticate(handle, username, auth_type, profile, app, tab_id).await? {
        AuthenticationResult::Authenticated => Ok(true),
        AuthenticationResult::KeyboardInteractiveAvailable { mode } => {
            let profile_id = profile.get("id").and_then(Value::as_str).unwrap_or("");
            let host = profile.get("host").and_then(Value::as_str).unwrap_or("");
            let port = port_from_profile(profile, 22, "SSH")?;
            let connection_name = profile
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(host);
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                format!(
                    "continuing authentication with keyboard-interactive target={} host={host}:{port}",
                    authentication_target.as_str()
                ),
            );
            try_keyboard_interactive(
                handle,
                username,
                password_for_authentication(profile),
                app,
                tab_id,
                profile_id,
                host,
                port,
                connection_name,
                authentication_target,
                mode,
            )
            .await
        }
        AuthenticationResult::Rejected => Ok(false),
    }
}
