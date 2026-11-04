/// Only connection identity enters this hash, never credentials or display preferences.
fn monitoring_target_key(profile: &Value, host: &str, port: u16, username: &str) -> Option<String> {
    use sha2::{Digest, Sha256};

    let id = profile
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?;
    let proxy = profile.get("proxy");
    let identity = serde_json::json!([
        id,
        host,
        port,
        username,
        profile.get("jumpProfileId"),
        profile.get("jumpProfileIds"),
        proxy.and_then(|p| p.get("type")),
        proxy.and_then(|p| p.get("host")),
        proxy.and_then(|p| p.get("port")),
        proxy.and_then(|p| p.get("username")),
    ]);
    Some(format!(
        "{:x}",
        Sha256::digest(identity.to_string().as_bytes())
    ))
}

#[cfg(test)]
#[path = "tests/monitoring_target_tests.rs"]
mod monitoring_target_tests;
