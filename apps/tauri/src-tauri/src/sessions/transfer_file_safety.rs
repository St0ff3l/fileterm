use std::path::Path;

use tokio::fs::{self, File, OpenOptions};

/// A download checkpoint is application-owned state. Never follow a link or
/// write a special file here: a sibling `.fileterm-part` can otherwise redirect
/// an SSH/FTP download outside the selected directory or block on a FIFO.
pub(crate) async fn existing_regular_local_transfer_file(
    path: &Path,
    description: &str,
) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("无法检查{description} {}：{error}", path.display())),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("{description}不能是符号链接：{}", path.display()));
    }
    if !metadata.is_file() {
        return Err(format!("{description}必须是普通文件：{}", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(format!("{description}不能是硬链接：{}", path.display()));
        }
    }
    Ok(true)
}

pub(crate) async fn open_local_download_checkpoint(
    path: &str,
    resume_offset: u64,
) -> Result<File, String> {
    let path = Path::new(path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("无法创建本地下载目录 {}：{error}", parent.display()))?;
    }

    let exists = existing_regular_local_transfer_file(path, "本地下载断点").await?;
    if !exists && resume_offset > 0 {
        return Err(format!(
            "本地下载断点不存在，不能从 {resume_offset} bytes 继续：{}",
            path.display()
        ));
    }

    let mut options = OpenOptions::new();
    // Do not pass `truncate` to open: a hard link inserted after the metadata
    // check would otherwise be cleared before the opened handle can be
    // inspected. Validate the handle first, then truncate it below.
    options.write(true).create(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);

    let file = options
        .open(path)
        .await
        .map_err(|error| format!("无法打开本地下载断点 {}：{error}", path.display()))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| format!("无法读取本地下载断点 {}：{error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("本地下载断点必须是普通文件：{}", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(format!("本地下载断点不能是硬链接：{}", path.display()));
        }
    }
    if resume_offset > 0 && metadata.len() != resume_offset {
        return Err(format!(
            "本地下载断点大小已变化，不能从 {resume_offset} bytes 继续：{}",
            path.display()
        ));
    }
    if resume_offset == 0 {
        file.set_len(0)
            .await
            .map_err(|error| format!("无法清空本地下载断点 {}：{error}", path.display()))?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn checkpoint_rejects_links_special_files_and_changed_resume_offsets() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "fileterm-download-checkpoint-safety-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let outside = root.join("outside");
        let link = root.join("link.fileterm-part");
        let fifo = root.join("pipe.fileterm-part");
        let hard_link = root.join("hard-link.fileterm-part");
        fs::write(&outside, b"outside data").await.unwrap();
        symlink(&outside, &link).unwrap();
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        std::fs::hard_link(&outside, &hard_link).unwrap();

        assert!(open_local_download_checkpoint(&link.to_string_lossy(), 0)
            .await
            .is_err());
        assert!(open_local_download_checkpoint(&fifo.to_string_lossy(), 0)
            .await
            .is_err());
        assert!(
            open_local_download_checkpoint(&hard_link.to_string_lossy(), 0)
                .await
                .is_err()
        );
        assert!(
            open_local_download_checkpoint(&outside.to_string_lossy(), 1)
                .await
                .is_err()
        );
        assert_eq!(fs::read(&outside).await.unwrap(), b"outside data");
        fs::remove_dir_all(root).await.unwrap();
    }
}
