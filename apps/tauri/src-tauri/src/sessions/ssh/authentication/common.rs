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
    interaction: &SshInteractionContext,
    interaction_timeout: Duration,
) -> Result<bool, String> {
    interaction.log_interaction(
        app,
        "DEBUG",
        "-",
        "authentication",
        "primary",
        0,
        format!("started auth_type={auth_type}"),
    );
    let result = try_authenticate(
        handle,
        username,
        auth_type,
        profile,
        app,
        interaction,
        interaction_timeout,
    )
    .await?;
    interaction.log_interaction(
        app,
        match result {
            AuthenticationResult::Authenticated => "INFO",
            AuthenticationResult::KeyboardInteractiveAvailable { .. } => "INFO",
            AuthenticationResult::Rejected => "WARN",
        },
        "-",
        "authentication",
        "primary",
        0,
        format!("completed result={result:?}"),
    );
    match result {
        AuthenticationResult::Authenticated => Ok(true),
        AuthenticationResult::KeyboardInteractiveAvailable { mode } => {
            interaction.log_interaction(
                app,
                "INFO",
                "-",
                "authentication",
                "keyboard-interactive",
                0,
                format!(
                    "continuing mode={mode:?}"
                ),
            );
            try_keyboard_interactive(
                handle,
                username,
                password_for_authentication(profile),
                app,
                interaction,
                mode,
                interaction_timeout,
            )
            .await
        }
        AuthenticationResult::Rejected => Ok(false),
    }
}
