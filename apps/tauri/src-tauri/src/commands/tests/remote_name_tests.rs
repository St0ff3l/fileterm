use super::{remote_parent_path, validate_remote_name};

#[test]
fn remote_name_operations_cannot_escape_the_parent_directory() {
    for name in [
        "",
        ".",
        "..",
        "../outside",
        "nested/file",
        "bad\0name",
        "line\nfeed",
    ] {
        assert!(validate_remote_name(name).is_err(), "{name:?}");
    }
    assert!(validate_remote_name("safe name.txt").is_ok());
    assert!(validate_remote_name(r"literal\name").is_ok());
}

#[test]
fn remote_parent_paths_ignore_host_platform_separators() {
    assert_eq!(remote_parent_path("/srv/literal\\name"), "/srv");
    assert_eq!(remote_parent_path("/srv/dir/file"), "/srv/dir");
    assert_eq!(remote_parent_path("file"), "/");
}
