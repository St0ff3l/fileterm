const SSH_ROUTE_ACTIVITY_LIMIT: usize = 160;
static SSH_ROUTE_ACTIVITY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static SSH_ROUTE_ACTIVITY: LazyLock<StdMutex<std::collections::VecDeque<SshRouteActivity>>> =
    LazyLock::new(|| StdMutex::new(std::collections::VecDeque::new()));

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SshRouteActivity {
    id: u64,
    profile_id: String,
    tab_id: String,
    scope: &'static str,
    rule: String,
    route: SshRouteCandidate,
    stage: &'static str,
    elapsed_ms: u64,
    observed_at: u64,
    failure: Option<SshRouteFailure>,
    fallback: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SshRoutePolicySnapshot {
    policy: SshRoutePolicy,
    activity: Vec<SshRouteActivity>,
    clients: Vec<SshRouteClientStatus>,
}

impl SshRouteSelection {
    #[allow(clippy::too_many_arguments)]
    fn report(
        &self,
        app: &AppHandle,
        tab_id: &str,
        profile_id: &str,
        route: &SshRouteCandidate,
        elapsed: Duration,
        failure: Option<SshRouteFailure>,
        fallback: bool,
        stage: &'static str,
    ) {
        let event = SshRouteActivity {
            id: SSH_ROUTE_ACTIVITY_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            profile_id: profile_id.to_owned(),
            tab_id: tab_id.to_owned(),
            scope: self.scope,
            rule: self.label.clone(),
            route: route.clone(),
            stage,
            elapsed_ms: elapsed.as_millis() as u64,
            observed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            failure,
            fallback,
        };
        let json = serde_json::to_string(&event).expect("SSH route event serialization");
        crate::services::logging::session(
            app,
            if failure.is_some() { "WARN" } else { "INFO" },
            "ssh",
            tab_id,
            format!("SSH route {json}"),
        );
        let mut activity = SSH_ROUTE_ACTIVITY.lock().expect("SSH route activity lock");
        if activity.len() == SSH_ROUTE_ACTIVITY_LIMIT {
            activity.pop_back();
        }
        activity.push_front(event.clone());
        drop(activity);
        let _ = app.emit("ssh-route:activity", event);
    }
}

fn parse_ssh_route_policy(content: &[u8]) -> Result<SshRoutePolicy, String> {
    serde_json::from_slice(content).map_err(|error| {
        format!(
            "Invalid {SSH_ROUTE_POLICY_FILE} at line {}, column {}",
            error.line(),
            error.column()
        )
    })
}

fn read_route_policy_file(path: &Path) -> Result<SshRoutePolicy, crate::AppError> {
    match std::fs::read(path) {
        Ok(content) => parse_ssh_route_policy(&content).map_err(crate::AppError::Storage),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SshRoutePolicy::default()),
        Err(error) => Err(crate::AppError::Storage(format!(
            "Cannot read {SSH_ROUTE_POLICY_FILE}: {error}"
        ))),
    }
}

pub(crate) fn validate_ssh_route_profile(
    app: &AppHandle,
    profile: &Value,
) -> Result<(), crate::AppError> {
    if !route_policy_eligible(profile) {
        return Ok(());
    }
    let path = crate::storage::workspace_file(app, SSH_ROUTE_POLICY_FILE)?;
    let policy = read_route_policy_file(&path)?;
    policy
        .select(profile, PathBuf::new())
        .map_err(crate::AppError::Command)?;
    Ok(())
}

pub(crate) fn ssh_route_policy_snapshot(
    app: &AppHandle,
) -> Result<SshRoutePolicySnapshot, crate::AppError> {
    let path = crate::storage::workspace_file(app, SSH_ROUTE_POLICY_FILE)?;
    let policy = read_route_policy_file(&path)?;
    Ok(SshRoutePolicySnapshot {
        clients: ssh_route_client_status(&policy),
        policy,
        activity: SSH_ROUTE_ACTIVITY
            .lock()
            .expect("SSH route activity lock")
            .iter()
            .cloned()
            .collect(),
    })
}

pub(crate) fn save_ssh_route_policy(
    app: &AppHandle,
    expected: SshRoutePolicy,
    policy: SshRoutePolicy,
) -> Result<SshRoutePolicySnapshot, crate::AppError> {
    use crate::AppError;
    let path = crate::storage::workspace_file(app, SSH_ROUTE_POLICY_FILE)?;
    if read_route_policy_file(&path)? != expected {
        return Err(AppError::Command(
            "SSH route rules changed in another window. Reload and review them before saving."
                .into(),
        ));
    }
    for (kind, definition) in &policy.clients {
        if expected.clients.get(kind) != Some(definition) {
            make_ssh_route_client_command(*kind, definition)?;
        }
    }
    for rule in policy
        .servers
        .values()
        .chain(policy.tags.values().map(|tag| &tag.rule))
        .chain(policy.default.iter())
    {
        rule.validate().map_err(AppError::Command)?;
    }
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("Missing SSH route policy directory".into()))?;
    let (profiles, _) = crate::services::profile_ops::read_and_heal_profiles(app)?;
    for profile in profiles
        .iter()
        .filter(|profile| route_policy_eligible(profile))
    {
        policy
            .select(profile, storage_root.to_path_buf())
            .map_err(|error| {
                let name = profile["name"].as_str().unwrap_or("SSH");
                AppError::Command(format!("{name}: {error}"))
            })?;
    }
    write_ssh_route_policy(&path, &policy)?;
    let _ = app.emit("ssh-route:policy-changed", ());
    ssh_route_policy_snapshot(app)
}

fn write_ssh_route_policy(path: &Path, policy: &SshRoutePolicy) -> Result<(), crate::AppError> {
    use crate::AppError;
    let staged = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let json = serde_json::to_vec_pretty(policy)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let result = std::fs::write(&staged, json)
        .map_err(|error| AppError::Storage(error.to_string()))
        .and_then(|()| crate::storage::replace_file_atomically(&staged, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

pub(crate) fn persist_new_profile_with_ssh_route(
    app: &AppHandle,
    profile: &Value,
    draft: Option<Value>,
    persist: impl FnOnce() -> Result<(), crate::AppError>,
) -> Result<(), crate::AppError> {
    let Some(draft) = draft.filter(|value| !value.is_null()) else {
        validate_ssh_route_profile(app, profile)?;
        return persist();
    };
    let rule = parse_ssh_route_draft(profile, draft).map_err(crate::AppError::Command)?;
    let path = crate::storage::workspace_file(app, SSH_ROUTE_POLICY_FILE)?;
    let previous = read_route_policy_file(&path)?;
    let mut policy = previous.clone();
    let id = profile["id"]
        .as_str()
        .ok_or_else(|| crate::AppError::Command("Missing SSH profile ID".into()))?;
    policy.servers.insert(id.to_owned(), rule);
    // Write the rule first so a failed rule save cannot leave a partially created connection.
    write_ssh_route_policy(&path, &policy)?;
    if let Err(error) = persist() {
        write_ssh_route_policy(&path, &previous)?;
        return Err(error);
    }
    let _ = app.emit("ssh-route:policy-changed", ());
    Ok(())
}

pub(crate) async fn probe_ssh_route_policy(
    app: &AppHandle,
    profile_id: &str,
) -> Result<SshRoutePolicySnapshot, crate::AppError> {
    use crate::AppError;
    let profiles = crate::storage::read_json_array(app, "profiles.json")?;
    let profile = profiles
        .iter()
        .find(|profile| profile["id"] == profile_id && route_policy_eligible(profile))
        .ok_or_else(|| {
            AppError::Command(
                "Choose a saved SSH connection without a jump host or OpenSSH alias".into(),
            )
        })?;
    let selection = load_ssh_route_policy(app, profile)
        .await
        .map_err(AppError::Command)?
        .ok_or_else(|| AppError::Command("This connection has no SSH route rule".into()))?;
    let host = profile["host"]
        .as_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| AppError::Command("Missing SSH destination".into()))?;
    let port = port_from_profile(profile, 22, "SSH").map_err(AppError::Command)?;
    let target = SshRouteTarget {
        profile_id: profile_id.into(),
        host: host.into(),
        port,
    };
    let (primary, alternatives) = selection.rule.parts();
    let mut probes = tokio::task::JoinSet::new();
    for route in primary.into_iter().chain(alternatives).cloned() {
        let storage_root = selection.storage_root.clone();
        let host = host.to_owned();
        probes.spawn(async move {
            dial_ssh_route(route, &storage_root, &host, port, Duration::from_secs(5)).await
        });
    }
    while let Some(probe) = probes.join_next().await {
        let (route, elapsed, result) =
            probe.map_err(|error| AppError::Command(error.to_string()))?;
        let failure = result.err();
        observe_ssh_route(&target, &route, elapsed, failure);
        selection.report(
            app, "-", profile_id, &route, elapsed, failure, false, "probe",
        );
    }
    ssh_route_policy_snapshot(app)
}
