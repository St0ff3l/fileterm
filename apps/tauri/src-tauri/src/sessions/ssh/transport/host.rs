/// Compute the OpenSSH-style SHA256 fingerprint of a host key.
///
/// Matches Electron's `computeHostFingerprint`:
/// `SHA256:` + base64(sha256(ssh_wire_encoded_public_key)) with `=` padding
/// stripped. The `ssh-key` crate's `Fingerprint` `Display` impl produces
/// exactly this format, so we defer to it instead of re-encoding manually.
fn fingerprint_sha256_base64(key: &russh::keys::PublicKey) -> String {
    format!("{}", key.fingerprint(russh::keys::HashAlg::Sha256))
}

/// Open an SSH session using the profile credentials. `trusted_fingerprint`
/// flows into the Handler's `check_server_key` so it can short-circuit the
/// accept/reject prompt when the fingerprint already matches.
/// Load a jump host profile from the profiles.json storage by its id.
/// Mirrors Electron's `resolveProfile(jumpProfileId)`.
/// 校验 profile 类型必须为 ssh：UI 层已过滤，但存储层可能被篡改或残留
/// 旧数据，FTP/Serial/ Telnet profile 无法作为 SSH 跳板，提前拒绝避免
/// 在 russh 握手阶段才失败、错误信息不清晰。
fn load_jump_profile(app: &AppHandle, profile_id: &str) -> Result<Value, String> {
    let profiles = crate::storage::read_json_array(app, "profiles.json")
        .map_err(|e| format!("Failed to read profiles.json for jump host: {}", e))?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(profile_id))
        .cloned()
        .ok_or_else(|| format!("Jump Host profile '{}' not found", profile_id))?;
    let profile_type = profile.get("type").and_then(Value::as_str).unwrap_or("");
    if profile_type != "ssh" {
        return Err(format!(
            "Jump Host profile '{}' must be an SSH profile, got '{}'",
            profile_id, profile_type
        ));
    }
    Ok(profile)
}

trait SshTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> SshTransport for T {}

type BoxedSshTransport = Box<dyn SshTransport>;

#[allow(clippy::too_many_arguments)] // SSH handler construction keeps the connection identity and interaction policy explicit.
fn new_client_handler(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    host: &str,
    port: u16,
    trusted_fingerprint: Option<String>,
    host_verification_waiting: Arc<AtomicBool>,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    remote_sshid: SharedRemoteSshId,
) -> ClientHandler {
    ClientHandler {
        app: app.clone(),
        tab_id: tab_id.to_string(),
        profile_id: profile_id.to_string(),
        host: host.to_string(),
        port,
        trusted_fingerprint,
        host_verification_waiting,
        interaction_timeout,
        interaction_window_label,
        remote_sshid,
    }
}
