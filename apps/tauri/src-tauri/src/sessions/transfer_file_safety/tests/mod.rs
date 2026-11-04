#[cfg(unix)]
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
