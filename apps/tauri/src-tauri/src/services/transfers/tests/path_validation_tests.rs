use super::{download_target_name, validate_transfer_name};

#[test]
fn target_names_cannot_replace_or_escape_the_selected_directory() {
    for windows in [false, true] {
        for name in [
            "",
            ".",
            "..",
            "/tmp/outside",
            "../outside",
            "nested/file",
            "bad\0name",
        ] {
            assert!(
                download_target_name("/srv/file", Some(name.into()), windows).is_err(),
                "{name:?}"
            );
            assert!(validate_transfer_name(name, windows).is_err());
        }
    }
}

#[test]
fn windows_downloads_reject_path_prefixes_streams_and_name_aliases() {
    for name in [
        r"..\outside",
        r"C:\outside",
        "C:outside",
        r"\\server\share",
        "file:stream",
        "NUL.txt",
        "COM1",
        "lpt².log",
        "name.",
        "name ",
    ] {
        assert!(validate_transfer_name(name, true).is_err(), "{name:?}");
        assert!(
            download_target_name(&format!("/srv/{name}"), None, true).is_err(),
            "{name:?}"
        );
    }
}

#[test]
fn downloads_preserve_valid_names_and_parse_remote_paths_independently_of_host() {
    for windows in [false, true] {
        for name in ["中文 空格.txt", ".hidden", "COM10.txt", "report (1).csv"] {
            assert_eq!(
                download_target_name(&format!("/srv/{name}/"), None, windows).unwrap(),
                name
            );
        }
        assert!(download_target_name("/", None, windows).is_err());
        assert_eq!(
            download_target_name("/", Some("root-copy".into()), windows).unwrap(),
            "root-copy"
        );
    }
    for name in [r"literal\name", "report:2026", "NUL.txt"] {
        assert_eq!(
            download_target_name(&format!("/srv/{name}"), None, false).unwrap(),
            name
        );
    }
}
