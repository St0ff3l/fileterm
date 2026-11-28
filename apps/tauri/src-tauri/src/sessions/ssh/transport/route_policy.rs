const SSH_ROUTE_POLICY_FILE: &str = "ssh-route-policy.json";
const MAX_SSH_ROUTE_CANDIDATES: usize = 4;
#[derive(Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum SshRouteCandidate {
    Direct,
    Proxy { proxy_id: String },
    Interface { adapter_id: uuid::Uuid },
}

#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
enum SshRouteRule {
    Auto {
        routes: Vec<SshRouteCandidate>,
    },
    Preferred {
        primary: SshRouteCandidate,
        fallback: Vec<SshRouteCandidate>,
    },
    Fixed {
        route: SshRouteCandidate,
    },
}

#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SshTagRule {
    priority: i32,
    rule: SshRouteRule,
}

#[derive(Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SshRoutePolicy {
    #[serde(default)]
    servers: HashMap<String, SshRouteRule>,
    #[serde(default)]
    tags: HashMap<String, SshTagRule>,
    default: Option<SshRouteRule>,
}

struct SshRouteSelection {
    scope: &'static str,
    label: String,
    rule: SshRouteRule,
    storage_root: PathBuf,
}

fn route_policy_eligible(profile: &Value) -> bool {
    profile["type"] == "ssh"
        && profile["authType"] != "sshConfig"
        && profile.get(SSH_CONFIG_RESOLVED_KEY).is_none()
        && match profile.get("jumpProfileIds") {
            Some(ids) => ids.as_array().is_some_and(Vec::is_empty),
            None => profile["jumpProfileId"].is_null(),
        }
}

impl SshRoutePolicy {
    fn select(
        &self,
        profile: &Value,
        storage_root: PathBuf,
    ) -> Result<Option<SshRouteSelection>, String> {
        let id = profile["id"].as_str().unwrap_or("");
        let selected = if let Some(rule) = self.servers.get(id) {
            Some(("server", id, rule))
        } else if profile_has_proxy(profile)
            || crate::services::ssh_network_interface::has_preference(profile)
        {
            None
        } else {
            let matching = || {
                profile["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter_map(|tag| self.tags.get(tag).map(|entry| (tag, entry)))
            };
            let best = matching().max_by(|(a, left), (b, right)| {
                left.priority.cmp(&right.priority).then_with(|| b.cmp(a))
            });
            if let Some((tag, entry)) = best {
                if matching()
                    .any(|(_, other)| other.priority == entry.priority && other.rule != entry.rule)
                {
                    return Err(
                        "SSH route policy has conflicting tag rules at the highest priority".into(),
                    );
                }
                Some(("tag", tag, &entry.rule))
            } else {
                self.default.as_ref().map(|rule| ("default", "", rule))
            }
        };
        let Some((scope, label, rule)) = selected else {
            return Ok(None);
        };
        rule.validate()?;
        Ok(Some(SshRouteSelection {
            scope,
            label: label.to_owned(),
            rule: rule.clone(),
            storage_root,
        }))
    }
}

impl SshRouteRule {
    fn parts(&self) -> (Option<&SshRouteCandidate>, &[SshRouteCandidate]) {
        match self {
            Self::Auto { routes } => (None, routes),
            Self::Preferred { primary, fallback } => (Some(primary), fallback),
            Self::Fixed { route } => (Some(route), &[]),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let (primary, alternatives) = self.parts();
        let routes: Vec<_> = primary.into_iter().chain(alternatives).collect();
        if routes.is_empty() || routes.len() > MAX_SSH_ROUTE_CANDIDATES {
            return Err(format!(
                "SSH route policy requires 1–{MAX_SSH_ROUTE_CANDIDATES} routes"
            ));
        }
        for (index, route) in routes.iter().enumerate() {
            if routes[..index].contains(route) {
                return Err("SSH route policy contains a duplicate route".into());
            }
            if matches!(route, SshRouteCandidate::Proxy { proxy_id } if proxy_id.trim().is_empty())
                || matches!(route, SshRouteCandidate::Interface { adapter_id } if adapter_id.is_nil())
            {
                return Err("SSH route policy contains an invalid route ID".into());
            }
        }
        Ok(())
    }
}

fn parse_ssh_route_draft(profile: &Value, draft: Value) -> Result<SshRouteRule, String> {
    if !route_policy_eligible(profile) {
        return Err(
            "SSH route rules require an SSH connection without a jump host or OpenSSH alias".into(),
        );
    }
    let rule: SshRouteRule = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    rule.validate()?;
    Ok(rule)
}

async fn load_ssh_route_policy(
    app: &AppHandle,
    profile: &Value,
) -> Result<Option<SshRouteSelection>, String> {
    let path = crate::storage::workspace_file(app, SSH_ROUTE_POLICY_FILE)
        .map_err(|error| error.to_string())?;
    let content = match tokio::fs::read(&path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read {SSH_ROUTE_POLICY_FILE}: {error}")),
    };
    let policy = parse_ssh_route_policy(&content)?;
    let storage_root = path.parent().ok_or("Missing SSH route policy directory")?;
    policy.select(profile, storage_root.to_owned())
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SshRouteTarget {
    profile_id: String,
    host: String,
    port: u16,
}

struct SelectedSshRoute {
    route: SshRouteCandidate,
    dial_profile: Value,
    target: SshRouteTarget,
}

impl SelectedSshRoute {
    fn authenticated(self, profile: &Value) -> String {
        record_authenticated_ssh_route(self.target, self.route.clone());
        let mut actual = profile.clone();
        actual["proxy"] = self.dial_profile["proxy"].clone();
        let endpoint = crate::sessions::transfer_endpoint::fingerprint(&[actual]);
        if let SshRouteCandidate::Interface { adapter_id } = self.route {
            use sha2::Digest;
            return format!(
                "ssh-interface-v1:{:x}",
                sha2::Sha256::new()
                    .chain_update(endpoint)
                    .chain_update(adapter_id.as_bytes())
                    .finalize()
            );
        }
        endpoint
    }
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum SshRouteFailure {
    MissingProxy,
    InvalidProxy,
    ProxyStorage,
    UnsupportedInterface,
    TransportFailed,
    Timeout,
    HandshakeTransportFailed,
    HandshakeTimeout,
    HandshakeRejected,
}

impl SshRouteCandidate {
    fn dial_profile(&self, storage_root: &Path, host: &str) -> Result<Value, SshRouteFailure> {
        use crate::services::proxies;
        use serde_json::json;
        match self {
            Self::Direct => Ok(json!({})),
            Self::Proxy { proxy_id } => {
                let proxy = proxies::get_internal(storage_root, proxy_id)
                    .map_err(|_| SshRouteFailure::ProxyStorage)?
                    .ok_or(SshRouteFailure::MissingProxy)?;
                if !matches!(proxy.proxy_type.as_str(), "socks5" | "http") {
                    return Err(SshRouteFailure::InvalidProxy);
                }
                let password = proxies::get_secret_internal(storage_root, proxy_id)
                    .map_err(|_| SshRouteFailure::ProxyStorage)?;
                Ok(
                    json!({"proxy": {"type": proxy.proxy_type, "host": proxy.host,
                    "port": proxy.port, "username": proxy.username, "password": password}}),
                )
            }
            Self::Interface { adapter_id } => {
                let profile = json!({"type": "ssh", "host": host, "sshNetworkInterface": {"adapterId": adapter_id}});
                if !cfg!(windows)
                    || crate::services::ssh_network_interface::validate(&profile).is_err()
                {
                    return Err(SshRouteFailure::UnsupportedInterface);
                }
                Ok(profile)
            }
        }
    }
}

async fn dial_ssh_route(
    route: SshRouteCandidate,
    storage_root: &Path,
    host: &str,
    port: u16,
    budget: Duration,
) -> (
    SshRouteCandidate,
    Duration,
    Result<(BoxedSshTransport, Value), SshRouteFailure>,
) {
    let started = Instant::now();
    let result = timeout(budget, async {
        let _permit = SSH_ROUTE_DIAL_SLOTS
            .acquire()
            .await
            .expect("SSH route dial slots");
        let profile = route.dial_profile(storage_root, host)?;
        let stream = connect_ssh_transport(&profile, host, port)
            .await
            .map_err(|_| SshRouteFailure::TransportFailed)?;
        // Transfer endpoint identity uses proxy metadata only.
        let mut profile = profile;
        if let Some(proxy) = profile.get_mut("proxy").and_then(Value::as_object_mut) {
            proxy.remove("password");
        }
        Ok((stream, profile))
    })
    .await
    .unwrap_or(Err(SshRouteFailure::Timeout));
    (route, started.elapsed(), result)
}

impl SshRouteSelection {
    async fn connect_excluding(
        &self,
        app: &AppHandle,
        tab_id: &str,
        target: SshRouteTarget,
        budget: Duration,
        excluded: &mut std::collections::HashSet<SshRouteCandidate>,
    ) -> Result<(BoxedSshTransport, SelectedSshRoute), String> {
        let profile_id = target.profile_id.clone();
        self.connect_target_excluding(
            target,
            budget,
            excluded,
            |route, elapsed, failure, fallback| {
                self.report(
                    app,
                    tab_id,
                    &profile_id,
                    route,
                    elapsed,
                    failure,
                    fallback,
                    "transport",
                );
            },
        )
        .await
    }

    #[cfg(test)]
    async fn connect_target(
        &self,
        target: SshRouteTarget,
        budget: Duration,
        report: impl FnMut(&SshRouteCandidate, Duration, Option<SshRouteFailure>, bool),
    ) -> Result<(BoxedSshTransport, SelectedSshRoute), String> {
        let mut excluded = std::collections::HashSet::new();
        self.connect_target_excluding(target, budget, &mut excluded, report)
            .await
    }

    async fn connect_target_excluding(
        &self,
        target: SshRouteTarget,
        budget: Duration,
        excluded: &mut std::collections::HashSet<SshRouteCandidate>,
        mut report: impl FnMut(&SshRouteCandidate, Duration, Option<SshRouteFailure>, bool),
    ) -> Result<(BoxedSshTransport, SelectedSshRoute), String> {
        let (primary, alternatives) = self.rule.parts();
        let primary = primary.filter(|route| !excluded.contains(*route));
        let alternatives: Vec<_> = alternatives
            .iter()
            .filter(|route| !excluded.contains(*route))
            .cloned()
            .collect();
        let mut remaining = budget;
        if let Some(route) = primary {
            let first_budget = if alternatives.is_empty() {
                budget
            } else {
                budget / 2
            };
            let (route, elapsed, result) = dial_ssh_route(
                route.clone(),
                &self.storage_root,
                &target.host,
                target.port,
                first_budget,
            )
            .await;
            let failure = result.as_ref().err().copied();
            observe_ssh_route(&target, &route, elapsed, failure);
            report(&route, elapsed, failure, false);
            if let Ok((stream, dial_profile)) = result {
                return Ok((
                    stream,
                    SelectedSshRoute {
                        route,
                        dial_profile,
                        target,
                    },
                ));
            }
            excluded.insert(route);
            remaining = budget.saturating_sub(elapsed);
        }

        let mut attempts = tokio::task::JoinSet::new();
        for (route, delay) in schedule_ssh_routes(&target, &alternatives, remaining) {
            let storage_root = self.storage_root.clone();
            let host = target.host.clone();
            let port = target.port;
            attempts.spawn(async move {
                if !delay.is_zero() {
                    sleep(delay).await;
                }
                dial_ssh_route(route, &storage_root, &host, port, remaining - delay).await
            });
        }
        while let Some(attempt) = attempts.join_next().await {
            let (route, elapsed, result) =
                attempt.map_err(|_| "SSH route transport task failed".to_string())?;
            let failure = result.as_ref().err().copied();
            observe_ssh_route(&target, &route, elapsed, failure);
            report(
                &route,
                elapsed,
                failure,
                primary.is_some() || !excluded.is_empty(),
            );
            if let Ok((stream, dial_profile)) = result {
                return Ok((
                    stream,
                    SelectedSshRoute {
                        route,
                        dial_profile,
                        target,
                    },
                ));
            }
            excluded.insert(route);
        }
        Err("No SSH policy route reached the target. See the SSH route entries in the connection log.".into())
    }
}

#[cfg(test)]
include!("tests/route_policy.rs");
