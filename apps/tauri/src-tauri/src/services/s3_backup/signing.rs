fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    bytes_to_hex(&hasher.finalize())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

fn signature(
    secret_access_key: &str,
    date_stamp: &str,
    region: &str,
    string_to_sign: &str,
) -> String {
    let date_key = hmac(
        format!("AWS4{secret_access_key}").as_bytes(),
        date_stamp.as_bytes(),
    );
    let region_key = hmac(&date_key, region.as_bytes());
    let service_key = hmac(&region_key, b"s3");
    let signing_key = hmac(&service_key, b"aws4_request");
    bytes_to_hex(&hmac(&signing_key, string_to_sign.as_bytes()))
}

fn aws_timestamp() -> (String, String) {
    let timestamp = webdav::export_timestamp();
    let amz_date = timestamp.replace(['-', ':'], "");
    let date_stamp = amz_date[..8].to_string();
    (amz_date, date_stamp)
}

fn signed_request(
    client: &Client,
    config: &StoredConfig,
    method: Method,
    target: ObjectTarget,
    body: Vec<u8>,
    extra_headers: BTreeMap<&str, String>,
) -> Result<reqwest::RequestBuilder, AppError> {
    let access_key_id = config
        .access_key_id
        .as_deref()
        .ok_or_else(|| command_error("缺少 S3 Access Key ID"))?;
    let secret_access_key = config
        .secret_access_key
        .as_deref()
        .ok_or_else(|| command_error("缺少 S3 Secret Access Key"))?;
    let (amz_date, date_stamp) = aws_timestamp();
    let payload_hash = sha256_hex(&body);
    let mut headers = BTreeMap::new();
    headers.insert("host", target.host.clone());
    headers.insert("x-amz-content-sha256", payload_hash.clone());
    headers.insert("x-amz-date", amz_date.clone());
    for (name, value) in extra_headers {
        headers.insert(name, value);
    }
    let canonical_headers = headers
        .iter()
        .map(|(name, value)| {
            format!(
                "{name}:{}\n",
                value.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect::<String>();
    let signed_headers = headers.keys().copied().collect::<Vec<_>>().join(";");
    let canonical_request = format!(
        "{}\n{}\n\n{}\n{}\n{}",
        method.as_str(),
        target.canonical_uri,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", config.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = signature(
        secret_access_key,
        &date_stamp,
        &config.region,
        &string_to_sign,
    );
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let mut request = client
        .request(method, target.url)
        .header("authorization", authorization)
        .header("host", target.host)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date);
    for (name, value) in headers {
        if !matches!(name, "host" | "x-amz-content-sha256" | "x-amz-date") {
            request = request.header(name, value);
        }
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    Ok(request)
}

fn etag(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn response_error(action: &str, status: StatusCode) -> AppError {
    command_error(format!("S3 {action}失败 ({status})"))
}

async fn head_object(client: &Client, config: &StoredConfig) -> Result<Option<String>, AppError> {
    let response = signed_request(
        client,
        config,
        Method::HEAD,
        object_target(config)?,
        Vec::new(),
        BTreeMap::new(),
    )?
    .send()
    .await
    .map_err(|error| command_error(format!("S3 预检失败: {error}")))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(response_error("预检", response.status()));
    }
    Ok(etag(response.headers()))
}

async fn head_bucket(client: &Client, config: &StoredConfig) -> Result<(), AppError> {
    let response = signed_request(
        client,
        config,
        Method::HEAD,
        bucket_target(config)?,
        Vec::new(),
        BTreeMap::new(),
    )?
    .send()
    .await
    .map_err(|error| command_error(format!("S3 Bucket 预检失败: {error}")))?;
    if !response.status().is_success() {
        return Err(response_error("Bucket 预检", response.status()));
    }
    Ok(())
}
