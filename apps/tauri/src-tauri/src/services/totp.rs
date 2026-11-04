//! Standard SSH authenticator codes: Base32, HMAC-SHA1, six digits, 30 seconds.
use data_encoding::{BASE32, BASE32_NOPAD};
use hmac_next::{Hmac, KeyInit, Mac};
use serde_json::{Map, Value};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

const INVALID_KEY: &str = "OTP 密钥无效：请输入 Base32 密钥（至少 16 个字符），不是 6 位验证码或二维码链接 / Invalid Base32 OTP secret";

fn decode_secret(secret: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    if secret.len() > 512 {
        return Err(INVALID_KEY.to_string());
    }
    let mut normalized = Zeroizing::new(
        secret
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>(),
    );
    normalized.make_ascii_uppercase();
    let encoding = if normalized.contains('=') {
        BASE32
    } else {
        BASE32_NOPAD
    };
    let key = Zeroizing::new(
        encoding
            .decode(normalized.as_bytes())
            .map_err(|_| INVALID_KEY.to_string())?,
    );
    if !(10..=128).contains(&key.len()) {
        return Err(INVALID_KEY.to_string());
    }
    Ok(key)
}

pub fn validate_profile(profile: &Map<String, Value>) -> Result<(), crate::AppError> {
    if profile
        .get("otpPrompt")
        .is_some_and(|value| !value.as_str().is_some_and(|text| text.len() <= 512))
    {
        return Err(crate::AppError::Command(
            "OTP 识别文本无效（最多 512 字节） / Invalid OTP prompt text".to_string(),
        ));
    }
    if let Some(value) = profile.get("otpSecret") {
        if value.is_null() || value.as_str() == Some("") {
            return Ok(());
        }
        let secret = value
            .as_str()
            .ok_or_else(|| crate::AppError::Command(INVALID_KEY.to_string()))?;
        decode_secret(secret).map_err(crate::AppError::Command)?;
    }
    Ok(())
}

pub fn generate(secret: &str) -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is before Unix epoch".to_string())?
        .as_secs();
    generate_at(secret, seconds)
}

fn generate_at(secret: &str, seconds: u64) -> Result<String, String> {
    let key = decode_secret(secret)?;
    let mut mac = Hmac::<Sha1>::new_from_slice(&key).map_err(|_| INVALID_KEY.to_string())?;
    mac.update(&(seconds / 30).to_be_bytes());
    let hash = mac.finalize().into_bytes();
    let offset = (hash[19] & 15) as usize;
    let value = u32::from_be_bytes(hash[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    Ok(format!("{:06}", value % 1_000_000))
}

pub fn is_otp_prompt(prompt: &str, configured: Option<&str>) -> bool {
    let prompt = prompt.to_lowercase();
    if [
        "recovery", "backup", "sms", "email", "e-mail", "push", "duo", "短信", "邮件", "邮箱",
        "恢复", "备用", "推送",
    ]
    .iter()
    .any(|word| prompt.contains(word))
    {
        return false;
    }
    if let Some(configured) = configured.map(str::trim).filter(|text| !text.is_empty()) {
        return prompt.contains(&configured.to_lowercase());
    }
    let text = prompt
        .trim()
        .trim_matches(|c: char| c.is_whitespace() || "[]:：".contains(c));
    let text = text
        .strip_prefix("please enter ")
        .or_else(|| text.strip_prefix("enter "))
        .or_else(|| text.strip_prefix("请输入"))
        .unwrap_or(text);
    let text = text
        .strip_prefix("the ")
        .or_else(|| text.strip_prefix("your "))
        .unwrap_or(text);
    let text = text.strip_prefix("mobile app ").unwrap_or(text).trim();
    matches!(
        text,
        "otp"
            | "otp code"
            | "otp verification code"
            | "totp"
            | "totp code"
            | "verification code"
            | "authentication code"
            | "authenticator code"
            | "security code"
            | "mfa code"
            | "one-time password"
            | "one time password"
            | "one-time code"
            | "one time code"
            | "passcode"
            | "token"
            | "验证码"
            | "动态验证码"
            | "动态口令"
            | "动态密码"
    )
}

#[cfg(test)]
mod tests;
