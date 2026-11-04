use super::{is_newer, select_release, windows_update_target_for_portable, GithubRelease};

#[test]
fn portable_updates_cannot_select_the_installer_payload() {
    assert_eq!(
        windows_update_target_for_portable(true, "x86_64"),
        Some("windows-x86_64-portable")
    );
    assert_eq!(
        windows_update_target_for_portable(true, "aarch64"),
        Some("windows-aarch64-portable")
    );
    assert_eq!(windows_update_target_for_portable(false, "x86_64"), None);
}

#[test]
fn compares_release_tags_numerically() {
    assert!(is_newer("v1.10.0", "1.9.9"));
    assert!(!is_newer("v1.1.1", "1.1.1"));
    assert!(is_newer("v2.2.0-beta.3", "2.2.0-beta.2"));
    assert!(is_newer("v2.2.0", "2.2.0-beta.2"));
    assert!(!is_newer("v2.2.0-beta.2", "2.2.0"));
}

fn release(tag_name: &str, prerelease: bool) -> GithubRelease {
    GithubRelease {
        tag_name: tag_name.to_string(),
        html_url: format!("https://github.com/St0ff3l/fileterm/releases/tag/{tag_name}"),
        prerelease,
        draft: false,
    }
}

#[test]
fn stable_channel_ignores_prereleases() {
    let selected = select_release(
        vec![
            release("v2.2.0-beta.3", true),
            release("v2.2.0-beta.2", false),
            release("v2.1.8", false),
        ],
        "stable",
    )
    .expect("stable release should be selected");

    assert_eq!(selected.tag_name, "v2.1.8");
}

#[test]
fn beta_channel_includes_stable_releases() {
    let selected = select_release(
        vec![release("v2.2.0-beta.3", true), release("v2.2.0", false)],
        "beta",
    )
    .expect("beta release should be selected");

    assert_eq!(selected.tag_name, "v2.2.0");
}

#[test]
fn draft_and_invalid_tags_are_ignored() {
    let mut draft = release("v9.0.0", false);
    draft.draft = true;
    let selected = select_release(
        vec![
            draft,
            release("not-a-version", true),
            release("v2.2.0-beta.2", true),
        ],
        "beta",
    )
    .expect("valid prerelease should be selected");

    assert_eq!(selected.tag_name, "v2.2.0-beta.2");
}
