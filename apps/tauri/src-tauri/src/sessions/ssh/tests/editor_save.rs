use super::*;
use russh_sftp::protocol::{Attrs, FileAttributes, Status};
use std::collections::HashMap;

type Files = Arc<StdMutex<HashMap<String, (Vec<u8>, u32)>>>;
struct EditorServer {
    files: Files,
    reject_permissions: bool,
    cancellation: tokio_util::sync::CancellationToken,
    different_owner: bool,
    cancel_commit: bool,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}

impl russh_sftp::server::Handler for EditorServer {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let files = self.files.lock().unwrap();
        let (bytes, mode) = files.get(&path).ok_or(StatusCode::NoSuchFile)?;
        let mut attrs = FileAttributes::empty();
        attrs.size = Some(bytes.len() as u64);
        attrs.permissions = Some(*mode);
        attrs.uid = Some(if self.different_owner && path == "/target" {
            2000
        } else {
            1000
        });
        attrs.gid = Some(1000);
        Ok(Attrs { id, attrs })
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        flags: OpenFlags,
        attrs: FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, StatusCode> {
        if flags.contains(OpenFlags::CREATE) {
            assert!(flags.contains(OpenFlags::EXCLUDE));
            assert_eq!(
                attrs.permissions,
                Some(0o600),
                "sensitive content must be private from creation"
            );
            let mut files = self.files.lock().unwrap();
            assert!(!files.contains_key(&path));
            files.insert(path.clone(), (Vec::new(), 0o100600));
        }
        Ok(russh_sftp::protocol::Handle { id, handle: path })
    }
    async fn write(
        &mut self,
        id: u32,
        path: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let mut files = self.files.lock().unwrap();
        let (bytes, mode) = files.get_mut(&path).unwrap();
        assert_eq!(*mode & 0o777, 0o600);
        bytes.resize(offset as usize + data.len(), 0);
        bytes[offset as usize..].copy_from_slice(&data);
        Ok(ok(id))
    }
    async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }
    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        if self.reject_permissions {
            return Err(StatusCode::PermissionDenied);
        }
        self.files.lock().unwrap().get_mut(&path).unwrap().1 = attrs.permissions.unwrap();
        Ok(ok(id))
    }
    async fn rename(&mut self, id: u32, from: String, to: String) -> Result<Status, StatusCode> {
        {
            let mut files = self.files.lock().unwrap();
            let entry = files.remove(&from).unwrap();
            files.insert(to.clone(), entry);
        }
        if to.contains(".fileterm-backup-") {
            if self.cancel_commit {
                self.cancellation.cancel();
            }
            tokio::time::sleep(Duration::from_millis(1100)).await;
        }
        Ok(ok(id))
    }
    async fn remove(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        self.files.lock().unwrap().remove(&path);
        Ok(ok(id))
    }
}

#[tokio::test]
async fn editor_creates_private_staging_and_never_commits_when_permissions_fail() {
    for (reject_permissions, different_owner, cancel_commit) in [
        (true, false, false), (false, false, false), (false, false, true), (false, true, false),
    ] {
        let cancellation = tokio_util::sync::CancellationToken::new();
        let files: Files = Arc::new(StdMutex::new(HashMap::from([(
            "/target".into(),
            (b"old".to_vec(), 0o100600),
        )])));
        let (client, server) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(russh_sftp::server::run(
            server,
            EditorServer {
                files: files.clone(),
                reject_permissions,
                cancellation: cancellation.clone(),
                different_owner,
                cancel_commit,
            },
        ));
        let sftp = SftpSession::new(client).await.unwrap();
        let result = write_file(
            &sftp,
            "/target",
            "new",
            "utf-8",
            &cancellation,
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(
            result.is_err(),
            reject_permissions || different_owner,
            "{result:?}"
        );
        assert_eq!(
            files.lock().unwrap()["/target"].0,
            if reject_permissions || different_owner {
                b"old"
            } else {
                b"new"
            }
        );
        assert_eq!(files.lock().unwrap()["/target"].1 & 0o777, 0o600);
        server.abort();
    }
}


#[cfg(unix)]
#[test]
fn privileged_save_rejects_unresolvable_symlinks_without_truncating_the_target() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = std::env::temp_dir().join(format!("fileterm-root-symlink-{}", uuid::Uuid::new_v4()));
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let target = root.join("target");
    let link = root.join("link");
    let staging = root.join("staging");
    std::fs::write(&target, b"original").unwrap();
    std::fs::write(&staging, b"new").unwrap();
    symlink(&target, &link).unwrap();
    let readlink = bin.join("readlink");
    std::fs::write(&readlink, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&readlink, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = root_replace_remote_file_command(root.to_str().unwrap(), staging.to_str().unwrap(), link.to_str().unwrap());
    let result = std::process::Command::new("/bin/sh").args(["-c", &command]).env("PATH", format!("{}:/usr/bin:/bin", bin.display())).output().unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read(&staging).unwrap(), b"new");
    assert!(link.is_symlink());
    std::fs::remove_dir_all(root).unwrap();
}


#[cfg(unix)]
#[test]
fn privileged_editor_staging_is_private_even_with_a_permissive_shell_umask() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("fileterm-root-mode-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let staging = root.join("staging");
    let command = format!("umask 000\n{}", root_editor_write_shell_command(staging.to_str().unwrap(), 3));
    let mut child = std::process::Command::new("/bin/sh").args(["-c", &command]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"bmV3\n").unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(std::fs::read(&staging).unwrap(), b"new");
    assert_eq!(std::fs::metadata(&staging).unwrap().permissions().mode() & 0o777, 0o600);
    std::fs::remove_dir_all(root).unwrap();
}
