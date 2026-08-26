use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::limits::TransferBudget;

const MIN_FREE_SPACE_RESERVE: u64 = 1024 * 1024;

/// Receive into a same-directory staging file and publish only after the
/// protocol has completed. This keeps partial data away from the user's final
/// path when a cable is unplugged, the process crashes, or the peer aborts.
#[derive(Debug)]
pub(super) struct StagedReceiveFile {
    file: Option<File>,
    target_path: PathBuf,
    temporary_path: PathBuf,
    bytes_written: u64,
    max_bytes: u64,
}

impl StagedReceiveFile {
    pub(super) async fn create(path: &Path, max_bytes: u64) -> Result<Self, String> {
        if path.exists() {
            return Err("串口接收目标文件已存在，请更换文件名".to_string());
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "串口接收目标文件名无效".to_string())?;

        ensure_free_space(parent, MIN_FREE_SPACE_RESERVE)?;
        for _ in 0..3 {
            let temporary_path = parent.join(format!(
                ".{file_name}.fileterm-part-{}",
                uuid::Uuid::new_v4()
            ));
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)
                .await
            {
                Ok(file) => {
                    return Ok(Self {
                        file: Some(file),
                        target_path: path.to_path_buf(),
                        temporary_path,
                        bytes_written: 0,
                        max_bytes,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("无法创建串口接收临时文件：{error}"));
                }
            }
        }
        Err("无法创建唯一的串口接收临时文件".to_string())
    }

    pub(super) async fn write_all(
        &mut self,
        buffer: &[u8],
        cancellation: &CancellationToken,
        budget: Option<&mut TransferBudget>,
    ) -> Result<(), String> {
        let count = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        let next = self
            .bytes_written
            .checked_add(count)
            .ok_or_else(|| "串口接收文件大小超出支持范围".to_string())?;
        if next > self.max_bytes {
            return Err(format!(
                "串口单个文件超过接收上限（{} 字节）",
                self.max_bytes
            ));
        }
        if let Some(budget) = budget {
            budget.account_unknown_bytes(count)?;
        }
        let parent = self.target_path.parent().unwrap_or_else(|| Path::new("."));
        ensure_free_space(parent, count.saturating_add(MIN_FREE_SPACE_RESERVE))?;
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| "串口接收文件已经关闭".to_string())?;
        tokio::select! {
            _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
            result = file.write_all(buffer) => {
                result.map_err(|error| format!("保存串口接收文件失败：{error}"))?;
                self.bytes_written = next;
                Ok(())
            }
        }
    }

    pub(super) async fn flush(&mut self) -> Result<(), String> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| "串口接收文件已经关闭".to_string())?;
        file.flush()
            .await
            .map_err(|error| format!("刷新串口接收文件失败：{error}"))?;
        file.sync_all()
            .await
            .map_err(|error| format!("同步串口接收文件失败：{error}"))
    }

    pub(super) async fn commit(mut self) -> Result<PathBuf, String> {
        if let Err(error) = self.flush().await {
            // The file handle can keep the staging inode open on Windows.
            // Drop it before removing the temporary path so a failed flush
            // cannot leave an orphaned `.fileterm-part-*` file behind.
            drop(self.file.take());
            self.remove_temporary().await;
            return Err(error);
        }
        drop(self.file.take());

        if self.target_path.exists() {
            self.remove_temporary().await;
            return Err("串口接收目标文件已存在，请更换文件名".to_string());
        }

        // A hard link publishes the fully written inode without replacing a
        // target created by another process between the existence check and
        // commit. Some removable filesystems do not support hard links; the
        // guarded rename fallback keeps those filesystems usable.
        match tokio::fs::hard_link(&self.temporary_path, &self.target_path).await {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
                ) =>
            {
                if self.target_path.exists() {
                    self.remove_temporary().await;
                    return Err("串口接收目标文件已存在，请更换文件名".to_string());
                }
                if let Err(rename_error) =
                    tokio::fs::rename(&self.temporary_path, &self.target_path).await
                {
                    self.remove_temporary().await;
                    return Err(format!("无法提交串口接收文件：{rename_error}"));
                }
                return Ok(self.target_path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                self.remove_temporary().await;
                return Err("串口接收目标文件已存在，请更换文件名".to_string());
            }
            Err(error) => {
                self.remove_temporary().await;
                return Err(format!("无法提交串口接收文件：{error}"));
            }
        }
        let _ = tokio::fs::remove_file(&self.temporary_path).await;
        Ok(self.target_path)
    }

    pub(super) async fn cleanup(mut self) {
        drop(self.file.take());
        self.remove_temporary().await;
    }

    async fn remove_temporary(&self) {
        let _ = tokio::fs::remove_file(&self.temporary_path).await;
    }
}

fn ensure_free_space(path: &Path, required: u64) -> Result<(), String> {
    match available_space(path) {
        Ok(free) if free < required => Err(format!(
            "串口接收目录可用空间不足，需要至少 {} 字节",
            required
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::Unsupported => Ok(()),
        Err(error) => Err(format!("无法检查串口接收目录可用空间：{error}")),
    }
}

#[cfg(unix)]
fn available_space(path: &Path) -> io::Result<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL 字节"))?;
    let mut stats = MaybeUninit::<libc::statvfs>::zeroed();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    let stats = unsafe { stats.assume_init() };
    let free_blocks = u128::from(stats.f_bavail);
    let block_size = u128::from(stats.f_frsize.max(stats.f_bsize));
    u64::try_from(free_blocks.saturating_mul(block_size))
        .map_err(|_| io::Error::other("可用空间超出支持范围"))
}

#[cfg(target_os = "windows")]
fn available_space(path: &Path) -> io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut free = 0_u64;
    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(free)
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
fn available_space(_path: &Path) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "当前平台没有可用空间查询实现",
    ))
}

pub(super) fn is_safe_transfer_file_name(file_name: &str) -> bool {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        || file_name.ends_with('.')
        || file_name.ends_with(' ')
    {
        return false;
    }

    let stem = file_name
        .trim_end_matches(['.', ' '])
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_windows_numbered_device_name(&stem, "COM")
        && !is_windows_numbered_device_name(&stem, "LPT")
}

fn is_windows_numbered_device_name(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit() && suffix != "0"
    })
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::StagedReceiveFile;
    use crate::sessions::serial::limits::{SerialTransferLimits, TransferBudget};

    #[tokio::test]
    async fn staged_receive_publishes_only_after_commit() {
        let target = std::env::temp_dir().join(format!("fileterm-staged-{}", uuid::Uuid::new_v4()));
        let mut staged = StagedReceiveFile::create(&target, 1024 * 1024)
            .await
            .unwrap();
        let mut budget = TransferBudget::new(SerialTransferLimits::default());
        budget.begin_file(None).unwrap();
        staged
            .write_all(b"partial", &CancellationToken::new(), Some(&mut budget))
            .await
            .unwrap();
        assert!(!target.exists());
        let committed = staged.commit().await.unwrap();
        assert_eq!(committed, target);
        assert_eq!(tokio::fs::read(&target).await.unwrap(), b"partial");
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn staged_receive_cleanup_does_not_remove_existing_target() {
        let target =
            std::env::temp_dir().join(format!("fileterm-staged-existing-{}", uuid::Uuid::new_v4()));
        tokio::fs::write(&target, b"keep").await.unwrap();
        assert!(StagedReceiveFile::create(&target, 1024).await.is_err());
        assert_eq!(tokio::fs::read(&target).await.unwrap(), b"keep");
        let _ = tokio::fs::remove_file(target).await;
    }

    #[test]
    fn safe_names_reject_paths_and_device_names() {
        for name in ["../escape", "folder\\escape", "CON", "COM1", "report:"] {
            assert!(!super::is_safe_transfer_file_name(name));
        }
        assert!(super::is_safe_transfer_file_name("report.bin"));
    }
}
