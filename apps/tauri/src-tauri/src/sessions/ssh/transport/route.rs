const MAX_SSH_JUMP_HOSTS: usize = 8;

fn apply_legacy_connection_overrides(profile: &mut Value) {
    for field in ["useEmptyPassword", "legacyAlgorithms"] {
        if let Some(value) = profile
            .get("connectionOverrides")
            .and_then(|overrides| overrides.get(field))
            .cloned()
        {
            profile[field] = value;
        }
    }
}

fn profile_has_proxy(profile: &Value) -> bool {
    profile
        .get("proxyProfileId")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty())
        || profile
            .get("proxy")
            .and_then(|proxy| proxy.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("none")
            != "none"
}

pub(crate) fn has_proxy_assignment_jump_conflict(
    profile: &Value,
    profiles: &[Value],
    assigning: bool,
) -> bool {
    let has_own_jump = match profile.get("jumpProfileIds") {
        Some(hops) => hops.as_array().is_none_or(|hops| !hops.is_empty()),
        None => profile
            .get("jumpProfileId")
            .is_some_and(|jump| !jump.is_null()),
    };
    has_own_jump
        || assigning
            && profiles.iter().any(|dependent| {
                matches!(dependent["type"].as_str(), Some("ssh" | "sftp"))
                    && dependent["jumpProfileIds"].as_array().is_some_and(|hops| {
                        hops.iter()
                            .skip(1)
                            .any(|hop| hop.as_str() == profile["id"].as_str())
                    })
            })
}

fn resolve_ssh_route(profile: &Value, profiles: &[Value]) -> Result<Vec<Value>, String> {
    use crate::services::ssh_network_interface::{has_preference, jump_conflict, validate};
    validate(profile)?;
    if let Some(jump_ids) = profile.get("jumpProfileIds") {
        let jump_ids = jump_ids.as_array().ok_or_else(|| {
            "SSH Jump Host route must be an array of profile IDs".to_string()
        })?;
        if jump_ids.len() > MAX_SSH_JUMP_HOSTS {
            return Err(format!(
                "SSH route supports at most {MAX_SSH_JUMP_HOSTS} Jump Hosts"
            ));
        }
        let target_id = profile.get("id").and_then(Value::as_str).unwrap_or("");
        let mut visited = std::collections::HashSet::new();
        if !target_id.is_empty() {
            visited.insert(target_id.to_string());
        }
        let mut route = Vec::with_capacity(jump_ids.len() + 1);
        if !jump_ids.is_empty() && profile_has_proxy(profile) {
            return Err(format!(
                "Proxy and Jump Host are mutually exclusive for SSH profile '{target_id}'"
            ));
        }
        for (index, jump_id) in jump_ids.iter().enumerate() {
            let jump_id = jump_id
                .as_str()
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| {
                    format!("SSH profile '{target_id}' has an invalid Jump Host reference")
                })?;
            if !visited.insert(jump_id.to_string()) {
                return Err(format!(
                    "SSH Jump Host route contains a cycle at profile '{jump_id}'"
                ));
            }
            let mut hop = profiles
                .iter()
                .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(jump_id))
                .cloned()
                .ok_or_else(|| format!("Jump Host profile '{jump_id}' not found"))?;
            if hop.get("type").and_then(Value::as_str) != Some("ssh") {
                return Err(format!("Jump Host profile '{jump_id}' must be an SSH profile"));
            }
            if has_preference(&hop) {
                return Err(jump_conflict(&hop));
            }
            port_from_profile(&hop, 22, "SSH")?;
            if index > 0 && profile_has_proxy(&hop) {
                return Err(format!(
                    "Proxy and Jump Host are mutually exclusive for SSH profile '{jump_id}'"
                ));
            }
            // The target's ordered list is authoritative; nested route fields on a selected hop are ignored.
            apply_legacy_connection_overrides(&mut hop);
            route.push(hop);
        }
        let mut target = profile.clone();
        if target.get("type").and_then(Value::as_str) != Some("ssh") {
            return Err(format!("Jump Host profile '{target_id}' must be an SSH profile"));
        }
        port_from_profile(&target, 22, "SSH")?;
        apply_legacy_connection_overrides(&mut target);
        route.push(target);
        return Ok(route);
    }

    let mut route = Vec::new();
    let mut current = profile.clone();
    let mut visited = std::collections::HashSet::new();
    loop {
        let id = current.get("id").and_then(Value::as_str).unwrap_or("");
        if !id.is_empty() && !visited.insert(id.to_string()) {
            return Err(format!(
                "SSH Jump Host route contains a cycle at profile '{id}'"
            ));
        }
        if current.get("type").and_then(Value::as_str) != Some("ssh") {
            return Err(format!("Jump Host profile '{id}' must be an SSH profile"));
        }
        if !route.is_empty() && has_preference(&current) {
            return Err(jump_conflict(&current));
        }
        port_from_profile(&current, 22, "SSH")?;
        let jump_id = match current
            .get("jumpProfileId")
            .filter(|value| !value.is_null())
        {
            Some(value) => Some(
                value
                    .as_str()
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| {
                        format!("SSH profile '{id}' has an invalid Jump Host reference")
                    })?
                    .to_string(),
            ),
            None => None,
        };
        if jump_id.is_some() && profile_has_proxy(&current) {
            return Err(format!(
                "Proxy and Jump Host are mutually exclusive for SSH profile '{id}'"
            ));
        }
        // Legacy per-profile overrides also belong to an intermediate hop's transport.
        apply_legacy_connection_overrides(&mut current);
        route.push(current);
        let Some(jump_id) = jump_id else { break };
        if visited.contains(&jump_id) {
            return Err(format!(
                "SSH Jump Host route contains a cycle at profile '{jump_id}'"
            ));
        }
        if route.len() > MAX_SSH_JUMP_HOSTS {
            return Err(format!(
                "SSH route supports at most {MAX_SSH_JUMP_HOSTS} Jump Hosts"
            ));
        }
        current = profiles
            .iter()
            .find(|profile| profile.get("id").and_then(Value::as_str) == Some(&jump_id))
            .cloned()
            .ok_or_else(|| format!("Jump Host profile '{jump_id}' not found"))?;
    }
    route.reverse();
    Ok(route)
}

// Channels retain russh senders after Handle::drop; explicitly disconnect every hop.
struct SshSessionCleanup<H: Handler + 'static> {
    handles: Vec<Arc<Handle<H>>>,
}

impl<H: Handler + 'static> Drop for SshSessionCleanup<H> {
    fn drop(&mut self) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let handles = std::mem::take(&mut self.handles);
            runtime.spawn(async move {
                for handle in handles.into_iter().rev() {
                    let _ = timeout(
                        Duration::from_secs(3),
                        handle.disconnect(Disconnect::ByApplication, "SSH route closed", "en"),
                    )
                    .await;
                }
            });
        }
    }
}

async fn open_session(
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    flow: SshInteractionFlow,
    cancellation: CancellationToken,
) -> Result<OpenSshSession, String> {
    let has_jump_route = profile
        .get("jumpProfileIds")
        .and_then(Value::as_array)
        .map(|ids| !ids.is_empty())
        .unwrap_or_else(|| {
            profile
                .get("jumpProfileId")
                .is_some_and(|value| !value.is_null())
        });
    let profiles = if has_jump_route {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let _guard = state.library_mutation.lock().await;
        crate::services::profile_ops::read_and_heal_profiles(app)
            .map(|(profiles, _)| profiles)
            .map_err(|error| format!("Failed to read SSH Jump Host profiles: {error}"))?
    } else {
        Vec::new()
    };
    let route = expand_ssh_config_route(resolve_ssh_route(profile, &profiles)?, app)?;
    let mut cleanup = SshSessionCleanup {
        handles: Vec::new(),
    };
    let mut remote_sshid = Vec::new();
    let mut disconnect_reason = Arc::new(StdMutex::new(None));
    for (index, hop) in route.iter().enumerate() {
        let authentication_target = if route.len() == 1 {
            SshAuthenticationTarget::Direct
        } else if index + 1 == route.len() {
            SshAuthenticationTarget::Target(index as u32)
        } else {
            SshAuthenticationTarget::JumpHost(index as u32)
        };
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err("SSH connection canceled".to_string()),
            result = open_session_hop(hop, app, tab_id, interaction_timeout,
                interaction_window_label.clone(), authentication_target, flow.clone(),
                cancellation.clone(), &mut cleanup) => result,
        };
        (remote_sshid, disconnect_reason) = result.map_err(|error| {
            let name = hop.get("name").and_then(Value::as_str).unwrap_or("SSH");
            format!("SSH hop {} ({name}): {error}", index + 1)
        })?;
    }
    let agent_forwarding = route
        .last()
        .is_some_and(|target| ssh_config_agent_forwarding(target).is_some());
    Ok(OpenSshSession {
        transfer_endpoint: crate::sessions::transfer_endpoint::fingerprint(&route),
        agent_forwarding,
        handle: cleanup.handles.last().unwrap().clone(),
        cleanup,
        remote_sshid,
        disconnect_reason,
    })
}
