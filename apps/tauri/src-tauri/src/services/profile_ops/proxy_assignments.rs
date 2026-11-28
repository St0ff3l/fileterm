use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshProxyAssignmentInput {
    proxy_id: String,
    changes: Vec<ProxyAssignment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyAssignment {
    profile_id: String,
    assigned: bool,
    expected: ProxySource,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ProxySource {
    Direct,
    Saved {
        proxy_id: String,
    },
    Custom {
        #[serde(rename = "type")]
        proxy_type: String,
        host: Option<String>,
        port: Option<u64>,
        username: Option<String>,
    },
}

impl ProxySource {
    fn from_profile(profile: &Value) -> Self {
        if let Some(proxy_id) = saved_proxy_id(profile) {
            return Self::Saved {
                proxy_id: proxy_id.to_owned(),
            };
        }
        let proxy = &profile["proxy"];
        match proxy["type"].as_str() {
            Some(proxy_type) if proxy_type != "none" && !proxy_type.is_empty() => Self::Custom {
                proxy_type: proxy_type.to_owned(),
                host: proxy["host"].as_str().map(str::to_owned),
                port: proxy["port"].as_u64(),
                username: proxy["username"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            },
            _ => Self::Direct,
        }
    }
}

pub(super) fn saved_proxy_id(profile: &Value) -> Option<&str> {
    profile["proxyProfileId"]
        .as_str()
        .filter(|id| !id.trim().is_empty())
}

pub(crate) fn assign_ssh_proxy(
    app: &AppHandle,
    input: SshProxyAssignmentInput,
) -> Result<bool, AppError> {
    let path = workspace_file(app, "profiles.json")?;
    let root = profile_secret_storage_root(&path)?;
    let proxy =
        crate::services::proxies::get_internal(root, &input.proxy_id)?.ok_or_else(|| {
            AppError::Command(
                "The selected proxy no longer exists. Refresh the list and try again.".into(),
            )
        })?;
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    if !apply_assignments(&mut profiles, &proxy, &input.changes)? {
        return Ok(false);
    }
    persist_profiles(app, &profiles)?;
    Ok(true)
}

fn apply_assignments(
    profiles: &mut [Value],
    proxy: &crate::services::proxies::StoredProxyProfile,
    changes: &[ProxyAssignment],
) -> Result<bool, AppError> {
    let by_id: std::collections::HashMap<_, _> = profiles
        .iter()
        .enumerate()
        .filter_map(|(index, profile)| Some((profile["id"].as_str()?, index)))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut pending = Vec::new();
    for change in changes {
        let id = change.profile_id.as_str();
        if !seen.insert(id) {
            return Err(AppError::Command(format!("Duplicate connection ID: {id}")));
        }
        let stale = || {
            AppError::Command(format!(
                "Connection {id} changed. Refresh the list and review the assignments."
            ))
        };
        let index = *by_id.get(id).ok_or_else(stale)?;
        let profile = &profiles[index];
        if profile["type"] != "ssh" || profile["authType"] == "sshConfig" {
            return Err(stale());
        }
        let source = ProxySource::from_profile(profile);
        if source != change.expected {
            return Err(stale());
        }
        if change.assigned == (saved_proxy_id(profile) == Some(&proxy.id)) {
            continue;
        }
        if change.assigned && crate::services::ssh_network_interface::has_preference(profile) {
            let name = profile["name"].as_str().unwrap_or(id);
            return Err(AppError::Command(format!(
                "The SSH outbound interface for {name} conflicts with this proxy assignment."
            )));
        }
        if crate::sessions::ssh::has_proxy_assignment_jump_conflict(
            profile,
            profiles,
            change.assigned,
        ) {
            let name = profile["name"].as_str().unwrap_or(id);
            return Err(AppError::Command(format!(
                "The SSH jump route for {name} conflicts with this proxy assignment."
            )));
        }
        pending.push((index, change.assigned));
    }
    let changed = !pending.is_empty();
    for (index, assigned) in pending {
        let profile = profiles[index]
            .as_object_mut()
            .expect("validated SSH profile");
        profile.remove("proxyPassword");
        if assigned {
            profile.insert("proxyProfileId".into(), Value::String(proxy.id.clone()));
            profile.insert("proxy".into(), serde_json::json!({
                "type": proxy.proxy_type, "host": proxy.host, "port": proxy.port, "username": proxy.username
            }));
        } else {
            profile.remove("proxyProfileId");
            profile.remove("proxy");
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
