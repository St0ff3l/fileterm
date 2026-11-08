use tokio::io::{AsyncRead, AsyncReadExt};

pub(crate) const TEXT_EDITOR_MAX_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn check_size(size: usize, max_bytes: usize) -> Result<(), String> {
    if size > max_bytes {
        return Err(format!(
            "File exceeds the {max_bytes} byte text read limit; download it instead"
        ));
    }
    Ok(())
}

pub(crate) async fn read_bounded(
    reader: &mut (impl AsyncRead + Unpin),
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    check_size(bytes.len(), max_bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_at_most_limit_plus_one_and_never_returns_a_partial_file() {
        let mut source = &b"abcdef"[..];
        assert!(read_bounded(&mut source, 3).await.is_err());
        assert_eq!(source, b"ef");
        assert_eq!(read_bounded(&mut &b"abc"[..], 3).await.unwrap(), b"abc");
        assert!(read_bounded(&mut &b""[..], 0).await.unwrap().is_empty());
    }
}
