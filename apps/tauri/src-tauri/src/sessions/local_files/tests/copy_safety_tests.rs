use super::*;

#[test]
fn creating_a_file_never_truncates_an_existing_file() {
    let root =
        std::env::temp_dir().join(format!("fileterm-create-safety-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let directory = root.to_string_lossy().into_owned();
    app_create_local_file(directory.clone(), "file".into()).unwrap();
    fs::write(root.join("file"), b"keep existing contents").unwrap();
    assert!(app_create_local_file(directory, "file".into()).is_err());
    assert_eq!(
        fs::read(root.join("file")).unwrap(),
        b"keep existing contents"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_name_operations_cannot_escape_the_current_directory() {
    for name in ["", ".", "..", "../outside", "nested/file", "bad\0name"] {
        assert!(validate_local_name(name).is_err(), "{name:?}");
    }
    assert!(validate_local_name("safe name.txt").is_ok());
    #[cfg(unix)]
    assert!(validate_local_name(r"literal\name").is_ok());
}

#[test]
fn copy_rejects_self_alias_and_nested_destination() {
    let root =
        std::env::temp_dir().join(format!("fileterm-copy-safety-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("source")).unwrap();
    fs::write(root.join("source/file"), b"keep").unwrap();
    assert!(app_copy_local_path(
        root.join("source").to_string_lossy().into_owned(),
        root.join("source/nested").to_string_lossy().into_owned()
    )
    .is_err());
    assert!(app_copy_local_path(
        root.join("source/file").to_string_lossy().into_owned(),
        root.join("source/./file").to_string_lossy().into_owned()
    )
    .is_err());
    assert_eq!(fs::read(root.join("source/file")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn copy_rejects_a_hard_link_to_the_source_file() {
    let root =
        std::env::temp_dir().join(format!("fileterm-copy-hard-link-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source");
    let alias = root.join("alias");
    fs::write(&source, b"keep").unwrap();
    fs::hard_link(&source, &alias).unwrap();
    assert!(app_copy_local_path(
        source.to_string_lossy().into_owned(),
        alias.to_string_lossy().into_owned()
    )
    .is_err());
    assert_eq!(fs::read(&source).unwrap(), b"keep");
    assert_eq!(fs::read(&alias).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn copy_rejects_a_nested_hard_link_destination_without_mutating_it() {
    let root = std::env::temp_dir().join(format!(
        "fileterm-copy-nested-hard-link-{}",
        uuid::Uuid::new_v4()
    ));
    let source = root.join("source");
    let destination = root.join("destination");
    let outside = root.join("outside");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("config"), b"new data").unwrap();
    fs::write(&outside, b"keep existing contents").unwrap();
    fs::hard_link(&outside, destination.join("config")).unwrap();

    assert!(app_copy_local_path(
        source.to_string_lossy().into_owned(),
        destination.to_string_lossy().into_owned()
    )
    .is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep existing contents");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn copy_rejects_a_symlink_destination_instead_of_following_it() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!(
        "fileterm-copy-symlink-destination-{}",
        uuid::Uuid::new_v4()
    ));
    let source = root.join("source");
    let outside = root.join("outside");
    let alias = root.join("alias");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"safe source").unwrap();
    fs::write(&outside, b"must stay unchanged").unwrap();
    symlink(&outside, &alias).unwrap();
    assert!(app_copy_local_path(
        source.to_string_lossy().into_owned(),
        alias.to_string_lossy().into_owned()
    )
    .is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"must stay unchanged");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn copy_rejects_special_files_instead_of_opening_them() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let root = std::env::temp_dir().join(format!(
        "fileterm-copy-special-file-{}",
        uuid::Uuid::new_v4()
    ));
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    let fifo = source.join("pipe");
    let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    assert!(app_copy_local_path(
        source.to_string_lossy().into_owned(),
        destination.to_string_lossy().into_owned()
    )
    .is_err());
    assert!(!destination.join("pipe").exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn copy_rejects_special_destinations_instead_of_opening_them() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileTypeExt;
    let root = std::env::temp_dir().join(format!(
        "fileterm-copy-special-destination-{}",
        uuid::Uuid::new_v4()
    ));
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"safe source").unwrap();
    let destination_c = CString::new(destination.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(destination_c.as_ptr(), 0o600) }, 0);
    assert!(app_copy_local_path(
        source.to_string_lossy().into_owned(),
        destination.to_string_lossy().into_owned()
    )
    .is_err());
    assert!(fs::symlink_metadata(&destination)
        .unwrap()
        .file_type()
        .is_fifo());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn cross_volume_copy_preserves_dangling_and_cyclic_links() {
    use std::os::unix::fs::symlink;
    let root =
        std::env::temp_dir().join(format!("fileterm-move-links-{}", uuid::Uuid::new_v4()));
    let src = root.join("src");
    let dst = root.join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("file"), b"keep").unwrap();
    symlink("missing", src.join("dangling")).unwrap();
    symlink(".", src.join("loop")).unwrap();
    copy_for_move(&src, &dst).unwrap();
    assert_eq!(
        fs::read_link(dst.join("dangling")).unwrap(),
        Path::new("missing")
    );
    assert_eq!(fs::read_link(dst.join("loop")).unwrap(), Path::new("."));
    assert_eq!(fs::read(dst.join("file")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn mount_cleanup_never_removes_contents_of_a_remaining_share() {
    let root =
        std::env::temp_dir().join(format!("fileterm-mount-safety-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("keep"), b"remote data").unwrap();
    remove_empty_mount_directory(&root);
    assert_eq!(fs::read(root.join("keep")).unwrap(), b"remote data");
    fs::remove_file(root.join("keep")).unwrap();
    remove_empty_mount_directory(&root);
    assert!(!root.exists());
}
