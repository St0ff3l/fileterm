use super::*;
use crate::services::{backup_crypto::BackupDependencies, proxies, tunnels};
use std::collections::{HashMap, HashSet};

fn read_at(root: &Path, kind: &str, id: &str) -> Result<Option<Value>, AppError> {
    let value = if kind == "proxy" {
        let Some(profile) = proxies::get_internal(root, id)? else {
            return Ok(None);
        };
        let mut value =
            serde_json::to_value(profile).map_err(|error| command_error(error.to_string()))?;
        value["password"] = serde_json::to_value(proxies::get_secret_internal(root, id)?)
            .map_err(|error| command_error(error.to_string()))?;
        value
    } else {
        let Some(profile) = tunnels::get_internal(root, id)? else {
            return Ok(None);
        };
        let mut value =
            serde_json::to_value(profile).map_err(|error| command_error(error.to_string()))?;
        value["token"] = serde_json::to_value(tunnels::get_secret_internal(root, id)?)
            .map_err(|error| command_error(error.to_string()))?;
        value
    };
    Ok(Some(value))
}

pub(super) fn export_at(root: &Path, profiles: &[Value]) -> Result<BackupDependencies, AppError> {
    let mut result = BackupDependencies::default();
    for (field, kind, records) in [
        ("proxyProfileId", "proxy", &mut result.proxies),
        ("tunnelProfileId", "tunnel", &mut result.tunnels),
    ] {
        let ids: HashSet<_> = profiles
            .iter()
            .filter_map(|p| p[field].as_str())
            .filter(|id| !id.is_empty())
            .collect();
        for id in ids {
            records.push(
                read_at(root, kind, id)?
                    .ok_or_else(|| command_error(format!("备份引用的 {kind} 配置不存在：{id}")))?,
            );
        }
        records.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    Ok(result)
}

pub(super) fn merge(remote: &mut BackupDependencies, mut local: BackupDependencies) {
    for (remote, local) in [
        (&mut remote.proxies, &mut local.proxies),
        (&mut remote.tunnels, &mut local.tunnels),
    ] {
        for record in local.drain(..) {
            if let Some(index) = remote.iter().position(|old| old["id"] == record["id"]) {
                backup_crypto::zeroize_values(&mut remote[index..=index]);
                remote[index] = record;
            } else {
                remote.push(record);
            }
        }
    }
}

fn validate(kind: &str, record: &Value) -> Result<(), AppError> {
    if record["id"].as_str().is_none_or(str::is_empty) {
        return Err(command_error("备份中的代理或隧道缺少 ID"));
    }
    if kind == "proxy" {
        let input: proxies::SaveProxyInput = serde_json::from_value(record.clone())
            .map_err(|error| command_error(error.to_string()))?;
        if input.name.trim().is_empty()
            || input.host.trim().is_empty()
            || input.port == 0
            || !matches!(input.proxy_type.as_str(), "http" | "socks5")
        {
            return Err(command_error("备份中的代理配置无效"));
        }
    } else {
        let input: tunnels::SaveTunnelInput = serde_json::from_value(record.clone())
            .map_err(|error| command_error(error.to_string()))?;
        if input.name.trim().is_empty() {
            return Err(command_error("备份中的隧道名称为空"));
        }
        match input.tunnel_type.as_str() {
            "ssh" => tunnels::validate_forwards(input.forwards.as_deref().unwrap_or_default())?,
            "http" => {
                tunnels::validate_script_url(input.script_url.as_deref().unwrap_or_default())?;
                if !(1..=300).contains(&input.timeout_seconds.unwrap_or(30)) {
                    return Err(command_error("备份中的隧道超时无效"));
                }
            }
            _ => return Err(command_error("备份中的隧道类型无效")),
        }
    }
    Ok(())
}

fn canonical(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        for key in ["id", "createdAt", "updatedAt"] {
            object.remove(key);
        }
    }
    value
}

fn matching_id(
    root: &Path,
    kind: &str,
    record: &Value,
    existing: &[Value],
) -> Result<Option<String>, AppError> {
    for existing in existing {
        if let Some(id) = existing["id"].as_str() {
            if read_at(root, kind, id)?.map(canonical) == Some(canonical(record.clone())) {
                return Ok(Some(id.to_owned()));
            }
        }
    }
    Ok(None)
}

pub(super) struct RestoredDependencies<'a> {
    app: &'a AppHandle,
    created: Vec<(&'static str, String)>,
    committed: bool,
}

impl RestoredDependencies<'_> {
    pub(super) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for RestoredDependencies<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for (kind, id) in self.created.iter().rev() {
            let result = if *kind == "proxy" {
                proxies::delete(self.app, id)
            } else {
                tunnels::delete(self.app, id)
            };
            if let Err(error) = result {
                tracing::warn!(%error, "unable to roll back imported connection dependency");
            }
        }
    }
}

pub(super) fn restore<'a>(
    app: &'a AppHandle,
    dependencies: &BackupDependencies,
    profiles: &mut [Value],
) -> Result<RestoredDependencies<'a>, AppError> {
    let path = workspace_file(app, "profiles.json")?;
    let root = path
        .parent()
        .ok_or_else(|| command_error("无法解析备份目录"))?;
    let mut result = RestoredDependencies {
        app,
        created: Vec::new(),
        committed: false,
    };
    // Validate every record and reference before creating anything in the local library.
    for (field, kind, records) in [
        ("proxyProfileId", "proxy", &dependencies.proxies),
        ("tunnelProfileId", "tunnel", &dependencies.tunnels),
    ] {
        let mut ids = HashSet::new();
        for record in records {
            validate(kind, record)?;
            if !ids.insert(record["id"].as_str().unwrap()) {
                return Err(command_error("备份中的代理或隧道 ID 重复"));
            }
        }
        for profile in profiles.iter() {
            if let Some(id) = profile[field].as_str().filter(|id| !id.is_empty()) {
                if !ids.contains(id) && read_at(root, kind, id)?.is_none() {
                    return Err(command_error(format!("备份缺少所引用的 {kind} 配置：{id}")));
                }
            }
        }
    }
    for (field, kind, file, records) in [
        (
            "proxyProfileId",
            "proxy",
            "proxies.json",
            &dependencies.proxies,
        ),
        (
            "tunnelProfileId",
            "tunnel",
            "tunnels.json",
            &dependencies.tunnels,
        ),
    ] {
        let existing = crate::storage::read_json_array(app, file)?;
        let mut mapping = HashMap::new();
        for record in records {
            let old_id = record["id"].as_str().unwrap();
            if !profiles
                .iter()
                .any(|profile| profile[field].as_str() == Some(old_id))
            {
                continue;
            }
            let matched = matching_id(root, kind, record, &existing)?;
            let id = match matched {
                Some(id) => id,
                None => {
                    let saved = if kind == "proxy" {
                        let mut input: proxies::SaveProxyInput =
                            serde_json::from_value(record.clone())
                                .map_err(|error| command_error(error.to_string()))?;
                        input.id = None;
                        proxies::save(app, input)?
                    } else {
                        let mut input: tunnels::SaveTunnelInput =
                            serde_json::from_value(record.clone())
                                .map_err(|error| command_error(error.to_string()))?;
                        input.id = None;
                        // Nested Option uses null for clearing; new records only need the present token.
                        input.token = Some(
                            record
                                .get("token")
                                .and_then(Value::as_str)
                                .map(ToOwned::to_owned),
                        );
                        tunnels::save(app, input)?
                    };
                    let id = saved["id"]
                        .as_str()
                        .ok_or_else(|| command_error("导入的代理或隧道缺少 ID"))?
                        .to_owned();
                    result.created.push((kind, id.clone()));
                    id
                }
            };
            mapping.insert(old_id.to_owned(), id);
        }
        for profile in profiles.iter_mut() {
            if let Some(id) = profile[field]
                .as_str()
                .and_then(|id| mapping.get(id))
                .cloned()
            {
                profile[field] = Value::String(id);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "tests/dependencies.rs"]
mod tests;
