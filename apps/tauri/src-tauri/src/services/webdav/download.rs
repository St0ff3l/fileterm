/// Enforce the limit while receiving, including chunked bodies without a
/// Content-Length. Never collect an unbounded response before validating it.
pub(crate) async fn read_bounded_bundle(
    mut response: reqwest::Response,
    limit: usize,
    provider: &str,
) -> Result<Vec<u8>, AppError> {
    let too_large = || command_error(format!("{provider} 配置包超过大小限制（{limit} 字节）"));
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| command_error(format!("{provider} 下载内容失败: {error}")))?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "tests/download_limit_tests.rs"]
mod download_limit_tests;
