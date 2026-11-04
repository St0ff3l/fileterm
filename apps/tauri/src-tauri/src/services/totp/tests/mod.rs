use super::*;

#[test]
fn otp_matches_rfc6238_sha1_vectors_and_window_boundaries() {
    let key = BASE32_NOPAD.encode(b"12345678901234567890");
    for (seconds, expected) in [
        (59, "287082"),
        (1_111_111_109, "081804"),
        (1_111_111_111, "050471"),
        (1_234_567_890, "005924"),
        (2_000_000_000, "279037"),
        (20_000_000_000, "353130"),
    ] {
        assert_eq!(generate_at(&key, seconds).unwrap(), expected);
    }
    assert_eq!(generate_at(&key, 29).unwrap(), "755224");
    assert_eq!(generate_at(&key, 30).unwrap(), "287082");
}

#[test]
fn otp_validates_keys_without_echoing_them_in_errors() {
    let key = BASE32_NOPAD.encode(b"12345678901234567890");
    assert_eq!(
        generate_at(&format!(" {} \n", key.to_lowercase()), 59).unwrap(),
        "287082"
    );
    let padded = BASE32.encode(b"12345678901");
    assert!(decode_secret(&padded).is_ok());
    for invalid in [
        "",
        "123456",
        "otpauth://totp/test?secret=secret",
        "!!!!!!!!",
        "MY",
        "MZXW6===",
        "AAAAAAAAAAAAAAAA=",
        &"A".repeat(513),
    ] {
        assert!(decode_secret(invalid).is_err());
    }
    assert!(validate_profile(serde_json::json!({"otpSecret": 123}).as_object().unwrap()).is_err());
}

#[test]
fn otp_only_matches_authenticator_prompts() {
    assert!(is_otp_prompt(
        "Enter company response:",
        Some("company response")
    ));
    assert!(!is_otp_prompt("OTP code:", Some("company response")));
    assert!(!is_otp_prompt("SMS code:", Some("SMS code")));

    for prompt in [
        "OTP code:",
        "Verification code:",
        "请输入动态验证码",
        "Token:",
        "One-time password:",
        "Please enter the mobile app OTP verification code:",
    ] {
        assert!(is_otp_prompt(prompt, None), "{prompt}");
    }
    for prompt in [
        "Password:",
        "MFA:",
        "Duo passcode or option:",
        "SMS verification code:",
        "Email OTP:",
        "Recovery code:",
        "请输入短信验证码",
        "请输入备用验证码",
        "Select OTP method:",
        "Press enter for OTP help",
        "1. OTP 2. Push",
    ] {
        assert!(!is_otp_prompt(prompt, None), "{prompt}");
    }
}
