// Provider normalization, default repair, secret patching, and provider CRUD.
fn read_normalized_store(
    app: &AppHandle,
) -> Result<(StoredProviderConfig, StoredProviderSecrets), AppError> {
    let mut config = read_public_config(app)?;
    let secrets = read_secret_config(app)?;
    config.schema_version = CONFIG_SCHEMA_VERSION;
    if repair_default_provider(&mut config, &secrets) {
        write_public_config(app, &config)?;
    }
    Ok((config, secrets))
}

fn normalize_text(value: &str, field: &str, maximum_length: usize) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            format!("{field} 不能为空"),
        ));
    }
    if value.len() > maximum_length {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            format!("{field} 超过长度限制"),
        ));
    }
    Ok(value.to_string())
}

fn normalize_base_url(value: &str, allow_insecure_http: bool) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_BASE_URL_LENGTH {
        return Err(ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"));
    }

    let url = Url::parse(value).map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"))?;
    if !matches!(url.scheme(), "https" | "http") || url.host().is_none() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "API 地址必须是包含主机名的 HTTP(S) 地址",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "API 地址不得内嵌用户名或密码",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "API 地址不得包含 query 或 fragment",
        ));
    }
    if url.scheme() == "http" && !allow_insecure_http {
        return Err(ai_error(
            "AI_PROVIDER_INSECURE_HTTP",
            "HTTP 连接需要明确启用不安全连接选项",
        ));
    }

    let path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if ["/chat/completions", "/responses", "/messages"]
        .iter()
        .any(|endpoint| path.ends_with(endpoint))
    {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "请填写 API root，不要填写具体的请求 endpoint",
        ));
    }

    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn is_trusted_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(host)) => host.octets()[0] == 127,
        Some(Host::Ipv6(host)) => host.is_loopback(),
        None => false,
    }
}

fn normalize_provider(draft: AiProviderDraft, id: String) -> Result<StoredAiProvider, AppError> {
    let name = normalize_text(&draft.name, "Provider 名称", MAX_PROVIDER_NAME_LENGTH)?;
    let model = normalize_text(&draft.model, "模型名称", MAX_MODEL_LENGTH)?;
    let base_url = normalize_base_url(&draft.base_url, draft.allow_insecure_http)?;
    let parsed =
        Url::parse(&base_url).map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"))?;
    if draft.allow_no_auth && !is_trusted_loopback(&parsed) {
        return Err(ai_error(
            "AI_PROVIDER_AUTH_REQUIRED",
            "无 API Key 仅允许可信 loopback 地址",
        ));
    }

    let mut models = Vec::new();
    for m in &draft.models {
        if let Ok(normalized_m) = normalize_text(m, "模型名称", MAX_MODEL_LENGTH) {
            if !normalized_m.is_empty() && !models.contains(&normalized_m) {
                models.push(normalized_m);
            }
        }
    }
    if !model.is_empty() && !models.contains(&model) {
        models.push(model.clone());
    }

    Ok(StoredAiProvider {
        id,
        name,
        kind: draft.kind,
        base_url,
        model,
        models,
        enabled: draft.enabled,
        is_default: draft.is_default,
        allow_no_auth: draft.allow_no_auth,
        allow_insecure_http: draft.allow_insecure_http,
    })
}

fn has_api_key(secrets: &StoredProviderSecrets, provider_id: &str) -> bool {
    secrets
        .providers
        .get(provider_id)
        .is_some_and(|secret| !secret.api_key.trim().is_empty())
}

fn provider_is_usable(provider: &StoredAiProvider, secrets: &StoredProviderSecrets) -> bool {
    if !provider.enabled {
        return false;
    }
    let Ok(base_url) = normalize_base_url(&provider.base_url, provider.allow_insecure_http) else {
        return false;
    };
    let Ok(parsed) = Url::parse(&base_url) else {
        return false;
    };
    if provider.allow_no_auth && !is_trusted_loopback(&parsed) {
        return false;
    }
    if provider.model.trim().is_empty() {
        return false;
    }
    if has_api_key(secrets, &provider.id) {
        return true;
    }
    provider.allow_no_auth
}

fn validate_secret_patch(patch: Option<&AiProviderSecretPatch>) -> Result<(), AppError> {
    let Some(AiProviderSecretPatch {
        api_key: SecretPatchValue::Replace(api_key),
    }) = patch
    else {
        return Ok(());
    };
    if api_key
        .chars()
        .any(|character| matches!(character, '\r' | '\n' | '\0'))
    {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            "API Key 不得包含控制换行符",
        ));
    }
    Ok(())
}

fn provider_summary(
    provider: &StoredAiProvider,
    secrets: &StoredProviderSecrets,
) -> AiProviderSummary {
    AiProviderSummary {
        id: provider.id.clone(),
        name: provider.name.clone(),
        kind: provider.kind.clone(),
        base_url: provider.base_url.clone(),
        model: provider.model.clone(),
        models: provider.models.clone(),
        enabled: provider.enabled,
        has_api_key: has_api_key(secrets, &provider.id),
        usable: provider_is_usable(provider, secrets),
        is_default: provider.is_default,
        allow_no_auth: provider.allow_no_auth,
        allow_insecure_http: provider.allow_insecure_http,
    }
}

fn default_candidate_key(provider: &StoredAiProvider) -> (String, String) {
    (provider.name.to_ascii_lowercase(), provider.id.clone())
}

fn repair_default_provider(
    config: &mut StoredProviderConfig,
    secrets: &StoredProviderSecrets,
) -> bool {
    let candidate_indices = config
        .providers
        .iter()
        .enumerate()
        .filter_map(|(index, provider)| provider_is_usable(provider, secrets).then_some(index))
        .collect::<Vec<_>>();

    let preferred = candidate_indices
        .iter()
        .copied()
        .filter(|index| config.providers[*index].is_default)
        .min_by_key(|index| default_candidate_key(&config.providers[*index]));
    let fallback = candidate_indices
        .iter()
        .copied()
        .min_by_key(|index| default_candidate_key(&config.providers[*index]));
    let winner = preferred.or(fallback);

    let mut changed = false;
    for (index, provider) in config.providers.iter_mut().enumerate() {
        let should_be_default = winner == Some(index);
        if provider.is_default != should_be_default {
            provider.is_default = should_be_default;
            changed = true;
        }
    }
    changed
}

fn selected_existing_id(
    config: &StoredProviderConfig,
    draft_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(draft_id) = draft_id else {
        return Ok(None);
    };
    let id = draft_id.trim();
    if id.is_empty() {
        return Err(ai_error("AI_PROVIDER_INVALID_CONFIG", "Provider ID 无效"));
    }
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| Some(provider.id.clone()))
        .ok_or_else(|| ai_error("AI_PROVIDER_NOT_FOUND", "找不到指定的 AI Provider"))
}

fn apply_secret_patch(
    secrets: &mut StoredProviderSecrets,
    provider_id: &str,
    patch: Option<&AiProviderSecretPatch>,
) -> bool {
    let Some(patch) = patch else {
        return false;
    };
    match &patch.api_key {
        SecretPatchValue::Unchanged => false,
        SecretPatchValue::Clear => secrets.providers.remove(provider_id).is_some(),
        SecretPatchValue::Replace(api_key) => {
            let previous = secrets.providers.insert(
                provider_id.to_string(),
                StoredProviderSecret {
                    api_key: api_key.clone(),
                },
            );
            previous
                .as_ref()
                .is_none_or(|previous| previous.api_key != *api_key)
        }
    }
}

pub fn list_providers(app: &AppHandle) -> Result<Vec<AiProviderSummary>, AppError> {
    let _guard = store_lock()?;
    let (config, secrets) = read_normalized_store(app)?;
    Ok(config
        .providers
        .iter()
        .map(|provider| provider_summary(provider, &secrets))
        .collect())
}

pub fn save_provider(
    app: &AppHandle,
    input: SaveAiProviderInput,
) -> Result<AiProviderSummary, AppError> {
    let _guard = store_lock()?;
    let (mut config, mut secrets) = read_normalized_store(app)?;
    let existing_id = selected_existing_id(&config, input.provider.id.as_deref())?;
    let provider_id = existing_id
        .clone()
        .unwrap_or_else(|| crate::storage::new_id("ai-provider"));
    let provider = normalize_provider(input.provider, provider_id.clone())?;

    let name_conflict = config.providers.iter().any(|existing| {
        existing.id != provider_id
            && existing
                .name
                .trim()
                .eq_ignore_ascii_case(provider.name.trim())
    });
    if name_conflict {
        return Err(ai_error(
            "AI_PROVIDER_DUPLICATE_NAME",
            format!(
                "Provider 名称 \"{}\" 已存在，请使用其他唯一名称",
                provider.name.trim()
            ),
        ));
    }

    validate_secret_patch(input.secrets.as_ref())?;

    let secret_changed = apply_secret_patch(&mut secrets, &provider_id, input.secrets.as_ref());
    if let Some(existing_id) = existing_id {
        let index = config
            .providers
            .iter()
            .position(|existing| existing.id == existing_id)
            .ok_or_else(|| ai_error("AI_PROVIDER_NOT_FOUND", "找不到指定的 AI Provider"))?;
        config.providers[index] = provider;
    } else {
        config.providers.push(provider);
    }
    config.schema_version = CONFIG_SCHEMA_VERSION;
    repair_default_provider(&mut config, &secrets);

    // Secrets are written before the public reference. If the second write
    // fails, a potential orphan key is still unreachable from the public
    // provider list and can be safely replaced on the next save.
    if secret_changed {
        write_secret_config(app, &secrets)?;
    }
    write_public_config(app, &config)?;

    config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider_summary(provider, &secrets))
        .ok_or_else(|| AppError::Storage("已保存的 AI Provider 不存在".to_string()))
}

pub fn delete_provider(
    app: &AppHandle,
    provider_id: &str,
) -> Result<Vec<AiProviderSummary>, AppError> {
    let _guard = store_lock()?;
    let (mut config, mut secrets) = read_normalized_store(app)?;
    let original_length = config.providers.len();
    config
        .providers
        .retain(|provider| provider.id != provider_id);
    if config.providers.len() == original_length {
        return Err(ai_error(
            "AI_PROVIDER_NOT_FOUND",
            "找不到指定的 AI Provider",
        ));
    }
    let secret_changed = secrets.providers.remove(provider_id).is_some();
    repair_default_provider(&mut config, &secrets);
    write_public_config(app, &config)?;
    if secret_changed {
        write_secret_config(app, &secrets)?;
    }
    Ok(config
        .providers
        .iter()
        .map(|provider| provider_summary(provider, &secrets))
        .collect())
}
