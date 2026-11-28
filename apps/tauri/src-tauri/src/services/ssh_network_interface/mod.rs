use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use uuid::Uuid;

#[cfg(windows)]
mod windows;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Preference {
    adapter_id: Uuid,
    display_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    id: Uuid,
    name: String,
    online: bool,
    ipv4: bool,
    ipv6: bool,
}

pub(crate) fn has_preference(profile: &Value) -> bool {
    profile
        .get("sshNetworkInterface")
        .is_some_and(|value| !value.is_null())
}

fn has_jump(profile: &Value) -> bool {
    match profile.get("jumpProfileIds") {
        Some(hops) => hops.as_array().is_none_or(|hops| !hops.is_empty()),
        None => profile
            .get("jumpProfileId")
            .is_some_and(|value| !value.is_null()),
    }
}

fn references_jump(profile: &Value, id: &str) -> bool {
    match profile.get("jumpProfileIds") {
        Some(hops) => hops
            .as_array()
            .is_some_and(|hops| hops.iter().any(|hop| hop == id)),
        None => profile["jumpProfileId"] == id,
    }
}

fn target_ip(host: &str) -> Result<IpAddr, String> {
    host.parse::<IpAddr>().ok().filter(|ip| {
        !ip.is_unspecified() && !ip.is_multicast() && !ip.is_loopback() &&
            match ip {
                IpAddr::V4(ip) => !ip.is_broadcast(),
                IpAddr::V6(ip) => !ip.is_unicast_link_local() && ip.to_ipv4_mapped().is_none(),
            }
    }).ok_or_else(|| "SSH outbound interface requires a supported IPv4 or IPv6 literal (no loopback or scoped IPv6)".into())
}

pub(crate) fn validate(profile: &Value) -> Result<Option<Preference>, String> {
    if !has_preference(profile) {
        return Ok(None);
    }
    if profile["type"] != "ssh" || profile["authType"] == "sshConfig" {
        return Err("SSH outbound interface requires a regular SSH profile".into());
    }
    if profile["proxyProfileId"]
        .as_str()
        .is_some_and(|id| !id.trim().is_empty())
        || profile["proxy"]["type"]
            .as_str()
            .is_some_and(|kind| kind != "none")
        || has_jump(profile)
    {
        return Err("SSH outbound interface conflicts with a proxy or Jump Host".into());
    }
    target_ip(profile["host"].as_str().unwrap_or_default())?;
    let preference: Preference = serde_json::from_value(profile["sshNetworkInterface"].clone())
        .map_err(|_| "Invalid SSH outbound interface adapter ID".to_string())?;
    if preference.adapter_id.is_nil() {
        return Err("Invalid SSH outbound interface adapter ID".into());
    }
    Ok(Some(preference))
}

pub(crate) fn validate_save(profile: &Value, profiles: &[Value]) -> Result<(), String> {
    if validate(profile)?.is_some() {
        let dependent = profile["id"].as_str().and_then(|id| {
            profiles
                .iter()
                .find(|candidate| references_jump(candidate, id))
        });
        if let Some(dependent) = dependent {
            let name = dependent["name"].as_str().unwrap_or("another connection");
            return Err(format!(
                "SSH outbound interface conflicts with use as a Jump Host by {name}"
            ));
        }
    }
    if let Some(hop) = profiles.iter().find(|candidate| {
        has_preference(candidate)
            && candidate["id"]
                .as_str()
                .is_some_and(|id| references_jump(profile, id))
    }) {
        return Err(jump_conflict(hop));
    }
    Ok(())
}

pub(crate) fn jump_conflict(hop: &Value) -> String {
    let name = hop["name"]
        .as_str()
        .or_else(|| hop["id"].as_str())
        .unwrap_or("unknown");
    format!("Jump Host {name} has a configured SSH outbound interface")
}

pub(crate) fn list() -> Result<Vec<AdapterInfo>, String> {
    #[cfg(windows)]
    return windows::list();
    #[cfg(not(windows))]
    Ok(Vec::new())
}

pub(crate) async fn connect(
    preference: Preference,
    host: &str,
    port: u16,
) -> Result<tokio::net::TcpStream, String> {
    let address = SocketAddr::new(target_ip(host)?, port);
    #[cfg(windows)]
    return windows::connect(preference, address).await;
    #[cfg(not(windows))]
    {
        let label = preference
            .display_name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| preference.adapter_id.to_string());
        Err(format!(
            "SSH outbound interface {label} for {address} is only supported on Windows"
        ))
    }
}

#[cfg(test)]
mod tests;
