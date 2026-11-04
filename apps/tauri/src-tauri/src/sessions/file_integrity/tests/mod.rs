use super::parse_sha256_output;

#[test]
fn extracts_a_sha256_digest_from_common_command_output() {
    assert_eq!(
        parse_sha256_output(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  file.bin\n"
        ),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
    );
}

#[test]
fn ignores_short_or_non_hex_tokens() {
    assert_eq!(parse_sha256_output("not-a-hash\n1234 file"), None);
}
