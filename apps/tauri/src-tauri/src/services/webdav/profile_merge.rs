fn merge_profiles(
    existing: &[Value],
    incoming: Vec<Value>,
    mode: DownloadMode,
) -> Result<(Vec<Value>, u64, u64), AppError> {
    use std::collections::{HashMap, HashSet};
    let mut merged = if mode == DownloadMode::MergeLocal {
        existing.to_vec()
    } else {
        Vec::new()
    };
    let mut identities: HashMap<_, Vec<&Value>> = HashMap::new();
    let mut incoming_identities = HashMap::new();
    for profile in existing {
        if let Some(identity) = profile_fingerprint(profile) {
            identities.entry(identity).or_default().push(profile);
        }
    }
    for profile in &incoming {
        if let Some(identity) = profile_fingerprint(profile) {
            *incoming_identities.entry(identity).or_insert(0) += 1;
        }
    }
    let mut id_map = HashMap::new();
    let mut backup_identities = HashSet::new();
    let mut changed: HashMap<usize, HashSet<&str>> = HashMap::new();
    let mut imported = 0;
    let mut updated = 0;
    for profile in incoming {
        let reference_fields = ["jumpProfileId", "jumpProfileIds"]
            .into_iter()
            .filter(|field| profile.get(*field).is_some())
            .collect::<Vec<_>>();
        let identity =
            profile_fingerprint(&profile).ok_or_else(|| command_error("备份连接缺少有效标识"))?;
        let source_id = profile
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let backup_identity = profile
            .get(profile_ops::BACKUP_IDENTITY_FIELD)
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .or(source_id)
            .map(ToOwned::to_owned);
        if backup_identity
            .as_ref()
            .is_some_and(|id| !backup_identities.insert((id.clone(), identity.clone())))
        {
            return Err(command_error("备份包含重复的连接恢复身份，已停止恢复"));
        }
        let previous = source_id
            .and_then(|id| {
                existing.iter().find(|candidate| {
                    candidate["id"].as_str() == Some(id)
                        && profile_fingerprint(candidate).as_ref() == Some(&identity)
                })
            })
            .or_else(|| {
                backup_identity.as_deref().and_then(|origin| {
                    existing.iter().find(|candidate| {
                        candidate
                            .get(profile_ops::BACKUP_IDENTITY_FIELD)
                            .and_then(Value::as_str)
                            .or_else(|| candidate["id"].as_str())
                            == Some(origin)
                            && profile_fingerprint(candidate).as_ref() == Some(&identity)
                    })
                })
            })
            .or_else(|| {
                identities
                    .get(&identity)
                    .filter(|matches| {
                        matches.len() == 1 && incoming_identities.get(&identity) == Some(&1)
                    })
                    .map(|matches| matches[0])
            });
        let id = previous
            .and_then(|p| p.get("id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| crate::storage::new_id("profile"));
        if let Some(old_id) = source_id {
            if id_map.insert(old_id.to_owned(), id.clone()).is_some() {
                return Err(command_error("备份包含重复连接 ID，已停止恢复"));
            }
        }
        let mut saved = if mode == DownloadMode::MergeLocal {
            match previous {
                Some(previous) => {
                    merge_synced_profile(previous, &profile).map_err(command_error)?
                }
                None => profile,
            }
        } else {
            let mut saved = profile;
            if let Some(previous) = previous {
                for key in ["order", "lastUsedAt"] {
                    if let Some(value) = previous.get(key) {
                        saved[key] = value.clone();
                    }
                }
            }
            saved
        };
        saved[profile_ops::BACKUP_IDENTITY_FIELD] =
            Value::String(backup_identity.unwrap_or_else(|| id.clone()));
        saved["id"] = Value::String(id.clone());
        if let Some(index) = merged
            .iter()
            .position(|p| p.get("id").and_then(Value::as_str) == Some(&id))
        {
            merged[index] = saved;
            changed.entry(index).or_default().extend(reference_fields);
            updated += 1;
        } else {
            changed
                .entry(merged.len())
                .or_default()
                .extend(reference_fields);
            merged.push(saved);
            imported += 1;
        }
    }
    let ssh_ids = merged
        .iter()
        .filter(|p| p["type"] == "ssh")
        .filter_map(|p| p["id"].as_str().map(ToOwned::to_owned))
        .collect::<HashSet<_>>();
    for (index, reference_fields) in changed {
        let profile = &mut merged[index];
        for key in reference_fields {
            let Some(value) = profile.get_mut(key).filter(|value| !value.is_null()) else {
                continue;
            };
            let refs: Vec<&mut Value> = match value {
                Value::Array(values) if key == "jumpProfileIds" => values.iter_mut().collect(),
                Value::String(_) if key == "jumpProfileId" => vec![value],
                _ => return Err(command_error("备份中的 Jump Host 引用格式无效")),
            };
            for reference in refs {
                let old = reference
                    .as_str()
                    .ok_or_else(|| command_error("备份中的 Jump Host ID 无效"))?;
                if old.is_empty() && key == "jumpProfileId" {
                    continue;
                }
                let id = id_map.get(old).map(String::as_str).unwrap_or(old);
                if !ssh_ids.contains(id) {
                    return Err(command_error(format!(
                        "备份中的 Jump Host 引用无法恢复：{old}"
                    )));
                }
                *reference = Value::String(id.to_owned());
            }
        }
    }
    Ok((merged, imported, updated))
}

#[cfg(test)]
#[path = "tests/profile_merge.rs"]
mod profile_merge_tests;
