use super::*;

#[test]
fn checksum_requires_confirmed_exec_capability_except_for_root() {
    let supported = vec!["SHA-256 (SSH exec)".to_string()];
    let other = vec!["MD5".to_string()];
    assert!(transfer_checksum_available(true, "user", Some(&supported)));
    assert!(!transfer_checksum_available(
        false,
        "user",
        Some(&supported)
    ));
    assert!(!transfer_checksum_available(true, "user", Some(&other)));
    assert!(!transfer_checksum_available(true, "user", Some(&[])));
    assert!(!transfer_checksum_available(true, "user", None));
    assert!(transfer_checksum_available(false, "root", None));
}

#[tokio::test]
async fn download_finishes_without_reading_eof_and_bounds_last_read() {
    let cancel = CancellationToken::new();
    let (mut reader, mut server) = tokio::io::duplex(32);
    // Keep the server open: a read after the advertised size would hang.
    server.write_all(b"abcEXTRA").await.unwrap();
    let mut buffer = [0; 32];
    let timeout = Duration::from_millis(50);
    let count = read_remote_transfer_chunk(&mut reader, &mut buffer, 3, &cancel, timeout)
        .await
        .unwrap();
    assert_eq!(&buffer[..count], b"abc");
    assert_eq!(
        read_remote_transfer_chunk(&mut reader, &mut buffer, 0, &cancel, timeout)
            .await
            .unwrap(),
        0
    );
    // Same behavior for an empty file or an already complete resume checkpoint.
    let (mut empty, _server) = tokio::io::duplex(1);
    assert_eq!(
        read_remote_transfer_chunk(&mut empty, &mut buffer, 0, &cancel, timeout)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn download_rejects_short_sources_and_read_errors() {
    let cancel = CancellationToken::new();
    let mut buffer = [0; 8];
    let mut source = &b"ab"[..];
    let timeout = Duration::from_secs(1);
    assert_eq!(
        read_remote_transfer_chunk(&mut source, &mut buffer, 3, &cancel, timeout)
            .await
            .unwrap(),
        2
    );
    assert!(
        read_remote_transfer_chunk(&mut source, &mut buffer, 1, &cancel, timeout)
            .await
            .unwrap_err()
            .contains("下载源提前结束")
    );
    struct Denied;
    impl AsyncRead for Denied {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::ErrorKind::PermissionDenied.into()))
        }
    }
    assert!(
        read_remote_transfer_chunk(&mut Denied, &mut buffer, 1, &cancel, timeout)
            .await
            .unwrap_err()
            .contains("读取远端下载数据失败")
    );
}

#[tokio::test]
async fn stalled_download_times_out_or_cancels() {
    let cancel = CancellationToken::new();
    let (mut reader, _server) = tokio::io::duplex(1);
    let mut buffer = [0; 8];
    assert_eq!(
        read_remote_transfer_chunk(
            &mut reader,
            &mut buffer,
            1,
            &cancel,
            Duration::from_millis(5)
        )
        .await
        .unwrap_err(),
        "读取远端下载数据超时"
    );
    let read =
        read_remote_transfer_chunk(&mut reader, &mut buffer, 1, &cancel, Duration::from_secs(1));
    let stop = async {
        tokio::task::yield_now().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(read, stop);
    assert_eq!(result.unwrap_err(), TRANSFER_CANCELED);
}

struct LegacyEofServer {
    eof_error: Option<StatusCode>,
    reads: Arc<StdMutex<Vec<(u64, u32)>>>,
}

impl russh_sftp::server::Handler for LegacyEofServer {
    type Error = StatusCode;
    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }
    async fn open(
        &mut self,
        id: u32,
        _: String,
        _: OpenFlags,
        _: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, Self::Error> {
        Ok(russh_sftp::protocol::Handle {
            id,
            handle: "file".into(),
        })
    }
    async fn read(
        &mut self,
        id: u32,
        _: String,
        offset: u64,
        len: u32,
    ) -> Result<russh_sftp::protocol::Data, Self::Error> {
        self.reads.lock().unwrap().push((offset, len));
        if offset >= 3 {
            if let Some(error) = self.eof_error {
                return Err(error);
            }
            return std::future::pending().await;
        }
        let end = (offset as usize + len as usize).min(3);
        Ok(russh_sftp::protocol::Data {
            id,
            data: b"abc"[offset as usize..end].to_vec(),
        })
    }
}

#[tokio::test]
async fn sftp_resume_does_not_request_eof_from_a_server_that_never_answers_it() {
    let reads = Arc::new(StdMutex::new(Vec::new()));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        LegacyEofServer {
            eof_error: None,
            reads: reads.clone(),
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    let mut file = sftp.open("file").await.unwrap();
    file.seek(std::io::SeekFrom::Start(1)).await.unwrap();
    let cancel = CancellationToken::new();
    let mut buffer = [0; 32];
    assert_eq!(
        read_remote_transfer_chunk(&mut file, &mut buffer, 2, &cancel, Duration::from_secs(1))
            .await
            .unwrap(),
        2
    );
    assert_eq!(&buffer[..2], b"bc");
    assert_eq!(
        read_remote_transfer_chunk(&mut file, &mut buffer, 0, &cancel, Duration::from_secs(1))
            .await
            .unwrap(),
        0
    );
    assert_eq!(*reads.lock().unwrap(), vec![(1, 2)]);
    server.abort();
}

#[tokio::test]
async fn sftp_download_finishes_when_the_server_denies_reads_past_eof() {
    let reads = Arc::new(StdMutex::new(Vec::new()));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        LegacyEofServer {
            eof_error: Some(StatusCode::PermissionDenied),
            reads: reads.clone(),
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    let mut file = sftp.open("file").await.unwrap();
    let cancel = CancellationToken::new();
    let mut buffer = [0; 32];
    let mut downloaded = Vec::new();
    loop {
        let count = read_remote_transfer_chunk(
            &mut file,
            &mut buffer,
            3 - downloaded.len() as u64,
            &cancel,
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        if count == 0 {
            break;
        }
        downloaded.extend_from_slice(&buffer[..count]);
    }
    assert_eq!(downloaded, b"abc");
    assert_eq!(*reads.lock().unwrap(), vec![(0, 3)]);

    // Prove the extra READ used by the old loop produces the reported SFTP error.
    let error = timeout(Duration::from_secs(1), file.read(&mut buffer[..1]))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.to_string(), "Permission denied: Permission denied");
    assert_eq!(*reads.lock().unwrap(), vec![(0, 3), (3, 1)]);
    server.abort();
}

struct ListOnlyServer {
    entry: Option<russh_sftp::protocol::FileAttributes>,
    listed: bool,
    deny_list: bool,
    deny_open: bool,
}

impl russh_sftp::server::Handler for ListOnlyServer {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn stat(&mut self, _: u32, _: String) -> Result<russh_sftp::protocol::Attrs, StatusCode> {
        Err(StatusCode::PermissionDenied)
    }
    async fn opendir(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Handle, StatusCode> {
        if self.deny_list {
            return Err(StatusCode::PermissionDenied);
        }
        assert_eq!(path, "/dir");
        self.listed = false;
        Ok(russh_sftp::protocol::Handle { id, handle: path })
    }
    async fn readdir(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Name, StatusCode> {
        if self.listed {
            return Err(StatusCode::Eof);
        }
        self.listed = true;
        Ok(russh_sftp::protocol::Name {
            id,
            files: self
                .entry
                .clone()
                .map(|attrs| russh_sftp::protocol::File::new("file", attrs))
                .into_iter()
                .collect(),
        })
    }
    async fn close(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        _: OpenFlags,
        _: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, StatusCode> {
        if self.deny_open {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(russh_sftp::protocol::Handle { id, handle: path })
    }
    async fn write(
        &mut self,
        _: u32,
        _: String,
        _: u64,
        _: Vec<u8>,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        Err(StatusCode::PermissionDenied)
    }
}

#[tokio::test]
async fn stat_permission_fallback_requires_a_confirmed_non_symlink_checkpoint() {
    for (mode, size, present, deny_list, succeeds) in [
        (0o100600, Some(3), true, false, true),
        (0o120777, Some(3), true, false, false),
        (0o100600, None, true, false, false),
        (0o100600, Some(3), false, false, false),
        (0o100600, Some(3), true, true, false),
    ] {
        let mut attrs = russh_sftp::protocol::FileAttributes::empty();
        attrs.permissions = Some(mode);
        attrs.size = size;
        let (client, server) = tokio::io::duplex(4096);
        let server = tokio::spawn(russh_sftp::server::run(
            server,
            ListOnlyServer {
                entry: present.then_some(attrs),
                listed: false,
                deny_list,
                deny_open: false,
            },
        ));
        let sftp = SftpSession::new(client).await.unwrap();
        let result = transfer_remote_metadata(&sftp, "/dir/file").await;
        if succeeds {
            assert_eq!(result.unwrap().size, Some(3));
            // STAT denial must not cause MKDIR on this existing virtual directory.
            ensure_transfer_parent_dir(&sftp, "/dir/new-file")
                .await
                .unwrap();
        } else {
            let error = result.unwrap_err();
            assert!(is_sftp_permission_denied(&error));
            assert!(!is_sftp_not_found(&error));
        }
        server.abort();
    }
}

#[tokio::test]
async fn upload_errors_identify_open_or_write_and_remote_path() {
    let local = std::env::temp_dir().join(format!("fileterm-upload-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&local, b"abc").await.unwrap();
    for deny_open in [true, false] {
        let (client, server) = tokio::io::duplex(4096);
        let server = tokio::spawn(russh_sftp::server::run(
            server,
            ListOnlyServer {
                entry: None,
                listed: false,
                deny_list: false,
                deny_open,
            },
        ));
        let sftp = SftpSession::new(client).await.unwrap();
        let error = upload_local_file(
            &sftp,
            local.to_str().unwrap(),
            3,
            "/dir/file",
            0,
            "test",
            CancellationToken::new(),
            None,
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(
            error.contains(if deny_open {
                "SFTP OPEN write"
            } else {
                "SFTP WRITE"
            }),
            "{error}"
        );
        assert!(error.contains("path=\"/dir/file\""), "{error}");
        assert!(error.contains("Permission denied"), "{error}");
        server.abort();
    }
    tokio::fs::remove_file(local).await.unwrap();
    assert_eq!(
        sftp_transfer_error("STAT", "/dir/a\nb", "denied"),
        "SFTP STAT path=\"/dir/a\\nb\": denied"
    );
}

struct DeleteDeniedServer {
    metadata_error: Option<StatusCode>,
    removes: Arc<StdMutex<usize>>,
}

impl russh_sftp::server::Handler for DeleteDeniedServer {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn lstat(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Attrs, StatusCode> {
        if path == "/dir/final" {
            return Err(StatusCode::NoSuchFile);
        }
        if let Some(error) = self.metadata_error {
            return Err(error);
        }
        let mut attrs = russh_sftp::protocol::FileAttributes::empty();
        attrs.permissions = Some(0o100600);
        attrs.size = Some(3);
        Ok(russh_sftp::protocol::Attrs { id, attrs })
    }
    async fn stat(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Attrs, StatusCode> {
        let mut attrs = russh_sftp::protocol::FileAttributes::empty();
        attrs.permissions = Some(0o040755);
        Ok(russh_sftp::protocol::Attrs { id, attrs })
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        _: OpenFlags,
        _: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, StatusCode> {
        self.metadata_error = None;
        Ok(russh_sftp::protocol::Handle { id, handle: path })
    }
    async fn write(
        &mut self,
        id: u32,
        _: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        assert_eq!(offset, 0);
        assert_eq!(data, b"abc");
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn close(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn rename(
        &mut self,
        id: u32,
        old: String,
        new: String,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        assert_eq!(old, "/dir/probe.txt.fileterm-part");
        assert_eq!(new, "/dir/final");
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn remove(
        &mut self,
        _: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        *self.removes.lock().unwrap() += 1;
        Err(StatusCode::PermissionDenied)
    }
}

#[tokio::test]
async fn cleanup_skips_missing_checkpoint_and_reports_existing_or_uninspectable_paths() {
    for (metadata_error, expected_removes, expected_stage) in [
        (Some(StatusCode::NoSuchFile), 0, None),
        (None, 1, Some("REMOVE cleanup")),
        (Some(StatusCode::PermissionDenied), 0, Some("LSTAT cleanup")),
    ] {
        let removes = Arc::new(StdMutex::new(0));
        let (client, server) = tokio::io::duplex(4096);
        let server = tokio::spawn(russh_sftp::server::run(
            server,
            DeleteDeniedServer {
                metadata_error,
                removes: removes.clone(),
            },
        ));
        let sftp = SftpSession::new(client).await.unwrap();
        let path = "/dir/probe.txt.fileterm-part";
        let result = remove_transfer_file_if_present(&sftp, path).await;
        if let Some(stage) = expected_stage {
            let error = result.unwrap_err();
            assert!(error.contains(stage), "{error}");
            assert!(error.contains(path), "{error}");
            assert!(error.contains("Permission denied"), "{error}");
        } else {
            result.unwrap();
        }
        assert_eq!(*removes.lock().unwrap(), expected_removes);
        server.abort();
    }
}

#[tokio::test]
async fn new_upload_commits_without_delete_permission() {
    let removes = Arc::new(StdMutex::new(0));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        DeleteDeniedServer {
            metadata_error: Some(StatusCode::NoSuchFile),
            removes: removes.clone(),
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    let path = "/dir/probe.txt.fileterm-part";
    remove_transfer_file_if_present(&sftp, path).await.unwrap();
    let root = std::env::temp_dir().join(format!(
        "fileterm-upload-no-delete-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::write(&root, b"abc").await.unwrap();
    upload_local_file(
        &sftp,
        root.to_str().unwrap(),
        3,
        path,
        0,
        "test",
        CancellationToken::new(),
        None,
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    replace_remote_file(&sftp, path, "/dir/final")
        .await
        .unwrap();
    assert_eq!(*removes.lock().unwrap(), 0);
    tokio::fs::remove_file(root).await.unwrap();
    server.abort();
}

#[test]
fn permission_errors_identify_operation_path_and_action() {
    for (operation, expected) in [
        ("OPEN resume-read", "读取权限"),
        ("OPEN write", "追加/覆盖权限"),
        ("MKDIR", "创建目录权限"),
        ("REMOVE cleanup", "删除权限"),
        ("RENAME", "重命名及覆盖权限"),
        ("READDIR", "列表权限"),
        ("STAT download", "属性查询权限"),
        ("SETSTAT preserve-permissions", "原目标未替换"),
    ] {
        let result = sftp_transfer_error(operation, "/restricted/file", "Permission denied");
        assert!(result.contains(operation), "{result}");
        assert!(result.contains("/restricted/file"), "{result}");
        assert!(result.contains("Permission denied"), "{result}");
        assert!(result.contains(expected), "{result}");
    }
}

struct ResumeWriteDeniedServer;

impl russh_sftp::server::Handler for ResumeWriteDeniedServer {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn stat(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Attrs, StatusCode> {
        let mut attrs = SftpMetadata::empty();
        attrs.permissions = Some(0o040755);
        Ok(russh_sftp::protocol::Attrs { id, attrs })
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        flags: OpenFlags,
        _: SftpMetadata,
    ) -> Result<russh_sftp::protocol::Handle, StatusCode> {
        if flags.contains(OpenFlags::WRITE) {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(russh_sftp::protocol::Handle { id, handle: path })
    }
    async fn read(
        &mut self,
        id: u32,
        _: String,
        offset: u64,
        len: u32,
    ) -> Result<russh_sftp::protocol::Data, StatusCode> {
        let start = offset as usize;
        let end = (start + len as usize).min(3);
        if start >= end {
            return Err(StatusCode::Eof);
        }
        Ok(russh_sftp::protocol::Data {
            id,
            data: b"abc"[start..end].to_vec(),
        })
    }
    async fn close(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Status, StatusCode> {
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
}

#[tokio::test]
async fn denied_resume_write_reports_append_permission_and_preserves_checkpoint() {
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(server, ResumeWriteDeniedServer));
    let sftp = SftpSession::new(client).await.unwrap();
    let local =
        std::env::temp_dir().join(format!("fileterm-resume-denied-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&local, b"abcdef").await.unwrap();
    let error = upload_local_file(
        &sftp,
        local.to_str().unwrap(),
        6,
        "/dir/file.fileterm-part",
        3,
        "test",
        CancellationToken::new(),
        None,
        Duration::from_secs(1),
    )
    .await
    .unwrap_err();
    assert!(error.contains("OPEN write"), "{error}");
    assert!(error.contains("offset=3"), "{error}");
    assert!(error.contains("追加/覆盖权限"), "{error}");
    tokio::fs::remove_file(local).await.unwrap();
    server.abort();
}
