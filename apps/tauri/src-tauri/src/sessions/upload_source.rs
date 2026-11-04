use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

/// Hash the resume prefix once, then hash only the bounded bytes read for sending.
pub(super) struct UploadSource {
    file: tokio::fs::File,
    remaining: u64,
    hasher: Sha256,
}

impl UploadSource {
    pub(super) async fn open(
        path: &str,
        size: u64,
        offset: u64,
        cancel: &CancellationToken,
    ) -> Result<Self, String> {
        if cancel.is_cancelled() {
            return Err("transfer canceled".to_string());
        }
        if offset > size {
            return Err("上传断点大于源文件".to_string());
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        // The path can change after transfer-service validation; never follow links or block on a FIFO.
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = options.open(path).await.map_err(|e| e.to_string())?;
        let metadata = file.metadata().await.map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() < size {
            return Err("上传源文件已被截短或不是普通文件，请重新传输".to_string());
        }
        let mut source = Self {
            file,
            remaining: size,
            hasher: Sha256::new(),
        };
        let mut buffer = vec![0; super::TRANSFER_IO_BUFFER_BYTES];
        let mut prefix = offset;
        while prefix > 0 {
            let length = prefix.min(buffer.len() as u64) as usize;
            let count = source.read(&mut buffer[..length], cancel).await?;
            prefix -= count as u64;
        }
        Ok(source)
    }

    pub(super) fn hash(&self) -> String {
        format!("{:x}", self.hasher.clone().finalize())
    }

    pub(super) async fn read(
        &mut self,
        buffer: &mut [u8],
        cancel: &CancellationToken,
    ) -> Result<usize, String> {
        let length = self.remaining.min(buffer.len() as u64) as usize;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("transfer canceled".to_string()),
            result = self.file.read(&mut buffer[..length]) => {
                let count = result.map_err(|e| e.to_string())?;
                if count == 0 && self.remaining > 0 {
                    return Err("上传源文件在传输中被截短，请重新传输".to_string());
                }
                self.remaining -= count as u64;
                self.hasher.update(&buffer[..count]);
                Ok(count)
            }
        }
    }
}

/// Verify the entire existing checkpoint before appending, including peers without HASH.
pub(super) async fn verify_resume_prefix(
    remote: &mut (impl AsyncRead + Unpin),
    size: u64,
    expected_hash: &str,
    cancel: &CancellationToken,
    io_timeout: Duration,
) -> Result<(), String> {
    let mut hasher = Sha256::new();
    let mut remaining = size;
    let mut buffer = vec![0; super::TRANSFER_IO_BUFFER_BYTES];
    loop {
        // Read one byte past the prefix to reject a checkpoint that grew meanwhile.
        let length = if remaining == 0 {
            1
        } else {
            remaining.min(buffer.len() as u64) as usize
        };
        let count = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("transfer canceled".to_string()),
            result = timeout(io_timeout, remote.read(&mut buffer[..length])) => {
                result.map_err(|_| "校验上传断点超时".to_string())?
                    .map_err(|e| e.to_string())?
            }
        };
        if count == 0 {
            break;
        }
        if remaining == 0 {
            return Err("上传断点大小已变化，请丢弃后重新传输".to_string());
        }
        remaining -= count as u64;
        hasher.update(&buffer[..count]);
    }
    if remaining != 0 || format!("{:x}", hasher.finalize()) != expected_hash {
        return Err("上传断点内容与源文件不一致，请丢弃后重新传输".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/upload_source.rs"]
mod tests;
