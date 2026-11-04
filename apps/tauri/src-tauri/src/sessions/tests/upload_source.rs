use super::*;

struct SourceFile(std::path::PathBuf);

impl SourceFile {
    fn new(bytes: &[u8]) -> Self {
        let file = Self(
            std::env::temp_dir().join(format!("fileterm-upload-test-{}", uuid::Uuid::new_v4())),
        );
        std::fs::write(&file.0, bytes).unwrap();
        file
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

async fn drain(source: &mut UploadSource, cancel: &CancellationToken) -> Vec<u8> {
    let mut sent = Vec::new();
    let mut buffer = [0; 2];
    loop {
        let count = source.read(&mut buffer, cancel).await.unwrap();
        if count == 0 {
            break;
        }
        sent.extend_from_slice(&buffer[..count]);
    }
    sent
}

#[tokio::test]
async fn append_during_upload_is_bounded_and_post_upload_rewrites_do_not_change_hash() {
    use std::io::Write;
    let file = SourceFile::new(b"abcdef");
    let cancel = CancellationToken::new();
    let mut source = UploadSource::open(file.path(), 6, 0, &cancel)
        .await
        .unwrap();
    let mut buffer = [0; 2];
    assert_eq!(source.read(&mut buffer, &cancel).await.unwrap(), 2);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file.0)
        .unwrap()
        .write_all(b"more logs")
        .unwrap();
    let mut sent = buffer.to_vec();
    sent.extend(drain(&mut source, &cancel).await);
    assert_eq!(sent, b"abcdef");
    std::fs::write(&file.0, b"rewritten").unwrap();
    assert_eq!(source.hash(), hash(&sent));
}

#[tokio::test]
async fn resume_hash_includes_prefix_once_and_does_not_send_it_again() {
    let file = SourceFile::new(b"abcdefmore logs");
    let cancel = CancellationToken::new();
    let mut source = UploadSource::open(file.path(), 6, 3, &cancel)
        .await
        .unwrap();
    assert_eq!(source.hash(), hash(b"abc"));
    verify_resume_prefix(
        &mut &b"abc"[..],
        3,
        &source.hash(),
        &cancel,
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(drain(&mut source, &cancel).await, b"def");
    assert_eq!(source.hash(), hash(b"abcdef"));
    let mut complete = UploadSource::open(file.path(), 6, 6, &cancel)
        .await
        .unwrap();
    assert!(drain(&mut complete, &cancel).await.is_empty());
    assert_eq!(complete.hash(), hash(b"abcdef"));
}

#[tokio::test]
async fn resume_rejects_corrupted_short_or_extended_checkpoints() {
    let cancel = CancellationToken::new();
    for bytes in [b"abX".as_slice(), b"ab", b"abcd"] {
        assert!(verify_resume_prefix(
            &mut &bytes[..],
            3,
            &hash(b"abc"),
            &cancel,
            Duration::from_secs(1)
        )
        .await
        .is_err());
    }
}

#[tokio::test]
async fn empty_files_invalid_offsets_and_short_sources() {
    let file = SourceFile::new(b"");
    let cancel = CancellationToken::new();
    let mut empty = UploadSource::open(file.path(), 0, 0, &cancel)
        .await
        .unwrap();
    assert!(drain(&mut empty, &cancel).await.is_empty());
    assert_eq!(empty.hash(), hash(b""));
    assert!(UploadSource::open(file.path(), 0, 1, &cancel)
        .await
        .is_err());
    assert!(UploadSource::open(file.path(), 1, 0, &cancel)
        .await
        .is_err());
}

#[tokio::test]
async fn truncation_during_upload_fails_instead_of_finishing_early() {
    let file = SourceFile::new(&vec![b'x'; super::super::TRANSFER_IO_BUFFER_BYTES * 3]);
    let size = std::fs::metadata(&file.0).unwrap().len();
    let cancel = CancellationToken::new();
    let mut source = UploadSource::open(file.path(), size, 0, &cancel)
        .await
        .unwrap();
    let mut buffer = vec![0; super::super::TRANSFER_IO_BUFFER_BYTES];
    source.read(&mut buffer, &cancel).await.unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file.0)
        .unwrap()
        .set_len(0)
        .unwrap();
    assert!(source
        .read(&mut buffer, &cancel)
        .await
        .unwrap_err()
        .contains("截短"));
}

#[tokio::test]
async fn canceled_reads_and_stalled_checkpoint_checks_stop() {
    let file = SourceFile::new(b"abc");
    let cancel = CancellationToken::new();
    let mut source = UploadSource::open(file.path(), 3, 0, &cancel)
        .await
        .unwrap();
    cancel.cancel();
    assert!(source
        .read(&mut [0; 3], &cancel)
        .await
        .unwrap_err()
        .contains("canceled"));
    assert!(UploadSource::open(file.path(), 3, 3, &cancel)
        .await
        .is_err());
    assert!(verify_resume_prefix(
        &mut &b"abc"[..],
        3,
        &hash(b"abc"),
        &cancel,
        Duration::from_secs(1)
    )
    .await
    .is_err());
    let (mut reader, _writer) = tokio::io::duplex(8);
    assert!(verify_resume_prefix(
        &mut reader,
        3,
        &hash(b"abc"),
        &CancellationToken::new(),
        Duration::from_millis(5)
    )
    .await
    .unwrap_err()
    .contains("超时"));
}

#[tokio::test]
async fn checkpoint_verification_can_be_canceled_while_waiting_for_data() {
    let cancel = CancellationToken::new();
    let (mut reader, _writer) = tokio::io::duplex(8);
    let expected = hash(b"abc");
    let (result, ()) = tokio::join!(
        verify_resume_prefix(&mut reader, 3, &expected, &cancel, Duration::from_secs(30)),
        async {
            tokio::task::yield_now().await;
            cancel.cancel();
        },
    );
    assert!(result.unwrap_err().contains("canceled"));
}

#[cfg(unix)]
#[tokio::test]
async fn upload_source_rejects_symlinks_and_does_not_block_on_fifo() {
    use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
    let file = SourceFile::new(b"private data");
    let link = SourceFile(file.0.with_extension("link"));
    std::os::unix::fs::symlink(&file.0, &link.0).unwrap();
    assert!(
        UploadSource::open(link.path(), 12, 0, &CancellationToken::new())
            .await
            .is_err()
    );

    let fifo = SourceFile(file.0.with_extension("fifo"));
    let path = std::ffi::CString::new(fifo.0.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let fifo_path = fifo.path().to_string();
    let mut request = tokio::spawn(async move {
        UploadSource::open(&fifo_path, 0, 0, &CancellationToken::new()).await
    });
    let result = timeout(Duration::from_millis(200), &mut request).await;
    if result.is_err() {
        // Unblock a regressed blocking open so the test runtime can still shut down.
        let _unblock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&fifo.0)
            .unwrap();
        let _ = request.await;
        panic!("upload source blocked on a FIFO");
    }
    assert!(result.unwrap().unwrap().is_err());
}
