//! Identity of the route actually used by a connected worker, never the editable profile store.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) fn fingerprint(route: &[Value]) -> String {
    let route: Vec<_> = route.iter().map(|profile| {
        let kind = profile["type"].as_str().unwrap_or("ssh");
        let proxy = &profile["proxy"];
        json!({
            "type": kind,
            "host": profile["host"].as_str().unwrap_or("127.0.0.1").to_ascii_lowercase(),
            "port": profile["port"].as_u64().unwrap_or(if kind == "ftp" {21} else {22}),
            "username": profile["username"].as_str().unwrap_or(if kind == "ftp" {"anonymous"} else {"root"}),
            "proxy": [proxy["type"].clone(), proxy["host"].clone(), proxy["port"].clone(), proxy["username"].clone()],
            "secure": profile["secure"], "securityMode": profile["securityMode"],
            "hostKey": profile["trustedHostFingerprint"], "certificate": profile["certificateFingerprint"]
        })
    }).collect();
    format!(
        "route-v1:{:x}",
        Sha256::digest(serde_json::to_vec(&route).unwrap())
    )
}

#[cfg(test)]
mod tests;
