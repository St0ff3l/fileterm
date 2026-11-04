use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

/// Calculate a local SHA-256 digest without loading a whole transfer into
/// memory. The digest is used only for optional post-transfer verification;
/// the protocol layer decides whether a remote peer can provide a digest.
pub(crate) async fn sha256_file(path: &str) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("open file for checksum failed: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("read file for checksum failed: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn parse_sha256_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .filter_map(|line| line.split_whitespace().next())
        .find(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(|value| value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests;
