use super::*;

#[cfg(unix)]
#[tokio::test]
async fn local_download_does_not_replace_a_symlink_destination() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "fileterm-download-replace-safety-{}",
        uuid::Uuid::new_v4()
    ));
    let partial = root.join("download.fileterm-part");
    let destination = root.join("download");
    let outside = root.join("outside");
    tokio::fs::create_dir_all(&root).await.unwrap();
    tokio::fs::write(&partial, b"new data").await.unwrap();
    tokio::fs::write(&outside, b"outside data").await.unwrap();
    symlink(&outside, &destination).unwrap();

    assert!(replace_local_file(&partial, &destination).await.is_err());
    assert_eq!(tokio::fs::read(&outside).await.unwrap(), b"outside data");
    assert_eq!(tokio::fs::read(&partial).await.unwrap(), b"new data");
    tokio::fs::remove_dir_all(root).await.unwrap();
}
