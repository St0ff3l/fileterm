use super::*;
use russh_sftp::protocol::{Attrs, Status};

struct DifferentOwners {
    files: Arc<StdMutex<HashMap<String, (u32, &'static str)>>>,
    deny_remove: bool,
}

impl russh_sftp::server::Handler for DifferentOwners {
    type Error = StatusCode;
    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let files = self.files.lock().unwrap();
        let (uid, content) = files.get(&path).ok_or(StatusCode::NoSuchFile)?;
        let mut attrs = SftpMetadata::empty();
        attrs.permissions = Some(0o100644);
        attrs.uid = Some(*uid);
        attrs.size = Some(content.len() as u64);
        Ok(Attrs { id, attrs })
    }
    async fn setstat(
        &mut self,
        id: u32,
        _: String,
        attrs: SftpMetadata,
    ) -> Result<Status, Self::Error> {
        assert_eq!(attrs.permissions, Some(0o100644));
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let mut files = self.files.lock().unwrap();
        if files.contains_key(&newpath) {
            return Err(StatusCode::Failure);
        }
        let content = files.remove(&oldpath).ok_or(StatusCode::NoSuchFile)?;
        files.insert(newpath, content);
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        if self.deny_remove {
            return Err(StatusCode::PermissionDenied);
        }
        self.files
            .lock()
            .unwrap()
            .remove(&filename)
            .ok_or(StatusCode::NoSuchFile)?;
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
}

#[tokio::test]
async fn strict_commit_with_different_owners_uses_rename_without_opening_or_truncating_target() {
    let files = Arc::new(StdMutex::new(HashMap::from([
        ("/part".to_string(), (1000, "new")),
        ("/target".to_string(), (2000, "old")),
    ])));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        DifferentOwners {
            files: files.clone(),
            deny_remove: false,
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    // This server rejects all OPEN/WRITE requests. The old uid-mismatch fallback would fail here.
    assert!(
        replace_remote_file_with_policy(&sftp, "/part", "/target", Some(true))
            .await
            .unwrap()
    );
    assert_eq!(files.lock().unwrap().get("/target"), Some(&(1000, "new")));
    assert_eq!(files.lock().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn no_delete_mode_preserves_existing_target_and_checkpoint() {
    let files = Arc::new(StdMutex::new(HashMap::from([
        ("/part".to_string(), (1000, "new")),
        ("/target".to_string(), (1000, "old")),
    ])));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        DifferentOwners {
            files: files.clone(),
            deny_remove: false,
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    for policy in [None, Some(true)] {
        let error = replace_remote_file_with_policy_mode(&sftp, "/part", "/target", policy, true)
            .await
            .unwrap_err();
        assert!(error.contains("原文件未更改"), "{error}");
        assert!(error.contains("/target"), "{error}");
        assert_eq!(files.lock().unwrap().get("/target"), Some(&(1000, "old")));
        assert_eq!(files.lock().unwrap().get("/part"), Some(&(1000, "new")));
        assert_eq!(files.lock().unwrap().len(), 2);
    }
    server.abort();
}

#[tokio::test]
async fn no_delete_mode_commits_a_new_target() {
    let files = Arc::new(StdMutex::new(HashMap::from([(
        "/part".to_string(),
        (1000, "new"),
    )])));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        DifferentOwners {
            files: files.clone(),
            deny_remove: false,
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    assert!(
        replace_remote_file_with_policy_mode(&sftp, "/part", "/target", None, true)
            .await
            .unwrap()
    );
    assert_eq!(files.lock().unwrap().get("/target"), Some(&(1000, "new")));
    assert_eq!(files.lock().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn denied_backup_cleanup_reports_committed_target_and_retained_old_file() {
    let files = Arc::new(StdMutex::new(HashMap::from([
        ("/part".to_string(), (1000, "new")),
        ("/target".to_string(), (1000, "old")),
    ])));
    let (client, server) = tokio::io::duplex(4096);
    let server = tokio::spawn(russh_sftp::server::run(
        server,
        DifferentOwners {
            files: files.clone(),
            deny_remove: true,
        },
    ));
    let sftp = SftpSession::new(client).await.unwrap();
    let error = replace_remote_file_with_policy_mode(&sftp, "/part", "/target", None, false)
        .await
        .unwrap_err();
    assert!(error.contains("REMOVE backup"), "{error}");
    assert!(error.contains("目标已更新"), "{error}");
    assert!(error.contains("请勿重试"), "{error}");
    let files = files.lock().unwrap();
    assert_eq!(files.get("/target"), Some(&(1000, "new")));
    assert!(
        files
            .iter()
            .any(|(path, data)| path.starts_with("/target.fileterm-backup-")
                && *data == (1000, "old"))
    );
    drop(files);
    server.abort();
}
