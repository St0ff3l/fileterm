#[test]
fn checksum_budget_scales_with_size_passes_and_the_profile_floor() {
    use std::time::Duration;
    let budget = |size, passes| {
        super::checksum_timeout_for_size(Duration::from_secs(60), size, passes, false).as_secs()
    };
    assert_eq!(budget(0, 1), 60);
    assert_eq!(budget(16 * 1024 * 1024 + 1, 1), 62);
    assert_eq!(budget(13 * 1024 * 1024 * 1024, 1), 892);
    assert_eq!(budget(13 * 1024 * 1024 * 1024, 2), 1724);
    assert_eq!(
        super::checksum_timeout_for_size(Duration::from_secs(3600), 1024, 1, false).as_secs(),
        3600
    );
}

#[test]
fn checksum_cap_can_be_disabled_without_changing_the_estimate() {
    use std::time::Duration;
    let size = 2 * 1024 * 1024 * 1024 * 1024;
    assert_eq!(
        super::checksum_timeout_for_size(Duration::from_secs(60), size, 1, false).as_secs(),
        86400
    );
    assert_eq!(
        super::checksum_timeout_for_size(Duration::from_secs(60), size, 1, true).as_secs(),
        131132
    );
    assert!(
        super::checksum_timeout_for_size(Duration::from_secs(60), u64::MAX, 2, true).as_secs()
            > 86400
    );
}

#[test]
fn checksum_requires_a_successful_remote_command_and_a_matching_digest() {
    let hash = "a".repeat(64);
    let output = format!("{hash}  file.bin\n");
    assert!(super::validate_transfer_sha256_output(&hash, &output, Some(0)).unwrap());
    assert!(super::validate_transfer_sha256_output(&hash, &output, Some(1)).is_err());
    assert!(super::validate_transfer_sha256_output(&hash, &output, None).is_err());
    assert!(super::validate_transfer_sha256_output(&"b".repeat(64), &output, Some(0)).is_err());
    assert!(super::validate_transfer_sha256_output(&hash, "no checksum", Some(0)).is_err());
}

#[test]
fn checksum_unavailable_marker_in_a_filename_does_not_skip_verification() {
    let hash = "a".repeat(64);
    let output = format!("{hash}  __FILETERM_SHA256_UNAVAILABLE__\n");
    assert!(super::validate_transfer_sha256_output(&"b".repeat(64), &output, Some(0)).is_err());
    assert!(super::validate_transfer_sha256_output(&hash, &output, Some(0)).unwrap());
    assert!(!super::validate_transfer_sha256_output(
        &hash,
        "__FILETERM_SHA256_UNAVAILABLE__\n",
        Some(0)
    )
    .unwrap());
    assert!(super::validate_transfer_sha256_output(
        &hash,
        "__FILETERM_SHA256_UNAVAILABLE__\n",
        Some(1)
    )
    .is_err());
}
