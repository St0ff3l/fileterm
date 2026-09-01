/// Normalize an encoding label to a canonical name understood by
/// `encoding_rs`. Mirrors Electron's `normalizeEncoding` alias table.
fn normalize_encoding(encoding: &str) -> &'static str {
    let normalized = encoding.trim().to_lowercase();
    match normalized.as_str() {
        "utf8" | "utf-8" | "" => "utf-8",
        "utf-8-bom" => "utf-8-bom",
        "utf16" | "utf-16" | "utf16le" | "utf-16le" => "utf-16le",
        "utf16be" | "utf-16be" => "utf-16be",
        "gb18030" => "gb18030",
        "gbk" => "gbk",
        "big5" | "cp950" => "big5",
        "euc-jp" | "eucjp" => "euc-jp",
        "shift-jis" | "shiftjis" | "shift_jis" | "sjis" => "shift_jis",
        "iso-2022-jp" => "iso-2022-jp",
        "euc-kr" | "euckr" | "cp949" => "euc-kr",
        "windows-1252" | "cp1252" => "windows-1252",
        "latin1" | "iso-8859-1" => "iso-8859-1",
        "windows-1251" | "cp1251" => "windows-1251",
        _ => "utf-8",
    }
}

/// Decode raw bytes into a string using the given encoding. Mirrors
/// Electron's `decodeBuffer` (iconv-lite + BOM stripping).
fn decode_bytes(buf: &[u8], encoding: &str) -> Result<String, String> {
    let normalized = normalize_encoding(encoding);
    match normalized {
        "utf-8" => {
            let text = std::str::from_utf8(buf)
                .map_err(|error| format!("utf-8 decode failed: {error}"))?;
            Ok(text.strip_prefix('\u{feff}').unwrap_or(text).to_string())
        }
        "utf-8-bom" => {
            let start = if buf.starts_with(&[0xef, 0xbb, 0xbf]) {
                3
            } else {
                0
            };
            String::from_utf8(buf[start..].to_vec())
                .map_err(|e| format!("utf-8 decode failed: {}", e))
        }
        "utf-16le" => {
            let start = if buf.starts_with(&[0xff, 0xfe]) { 2 } else { 0 };
            decode_utf16(&buf[start..], true)
        }
        "utf-16be" => {
            let start = if buf.starts_with(&[0xfe, 0xff]) { 2 } else { 0 };
            decode_utf16(&buf[start..], false)
        }
        "gb18030" => decode_with_encoding(encoding_rs::GB18030, buf, normalized),
        "gbk" => decode_with_encoding(encoding_rs::GBK, buf, normalized),
        "big5" => decode_with_encoding(encoding_rs::BIG5, buf, normalized),
        "euc-jp" => decode_with_encoding(encoding_rs::EUC_JP, buf, normalized),
        "shift_jis" => decode_with_encoding(encoding_rs::SHIFT_JIS, buf, normalized),
        "iso-2022-jp" => decode_with_encoding(encoding_rs::ISO_2022_JP, buf, normalized),
        "euc-kr" => decode_with_encoding(encoding_rs::EUC_KR, buf, normalized),
        "windows-1252" => decode_with_encoding(encoding_rs::WINDOWS_1252, buf, normalized),
        "iso-8859-1" => decode_with_encoding(encoding_rs::WINDOWS_1252, buf, normalized),
        "windows-1251" => decode_with_encoding(encoding_rs::WINDOWS_1251, buf, normalized),
        _ => Err(format!("unsupported text encoding: {normalized}")),
    }
}

fn decode_with_encoding(
    encoding: &'static encoding_rs::Encoding,
    bytes: &[u8],
    label: &str,
) -> Result<String, String> {
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(format!("{label} decode failed: invalid byte sequence"));
    }
    Ok(text.into_owned())
}

/// Decode UTF-16 bytes (little-endian or big-endian) into a string.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("utf-16 data length is odd".to_string());
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units).map_err(|e| format!("utf-16 decode failed: {}", e))
}

/// Encode a string into bytes using the given encoding. Mirrors Electron's
/// `encodeText` (iconv-lite + BOM prefixing for utf-8-bom / utf-16le / utf-16be).
fn encode_text(content: &str, encoding: &str) -> Result<Vec<u8>, String> {
    let normalized = normalize_encoding(encoding);
    match normalized {
        "utf-8" => Ok(content.as_bytes().to_vec()),
        "utf-8-bom" => {
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend_from_slice(content.as_bytes());
            Ok(bytes)
        }
        "utf-16le" => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(bytes)
        }
        "utf-16be" => {
            let mut bytes = vec![0xfe, 0xff];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            Ok(bytes)
        }
        "gb18030" => encode_with_encoding(encoding_rs::GB18030, content, normalized),
        "gbk" => encode_with_encoding(encoding_rs::GBK, content, normalized),
        "big5" => encode_with_encoding(encoding_rs::BIG5, content, normalized),
        "euc-jp" => encode_with_encoding(encoding_rs::EUC_JP, content, normalized),
        "shift_jis" => encode_with_encoding(encoding_rs::SHIFT_JIS, content, normalized),
        "iso-2022-jp" => encode_with_encoding(encoding_rs::ISO_2022_JP, content, normalized),
        "euc-kr" => encode_with_encoding(encoding_rs::EUC_KR, content, normalized),
        "windows-1252" => encode_with_encoding(encoding_rs::WINDOWS_1252, content, normalized),
        "iso-8859-1" => encode_with_encoding(encoding_rs::WINDOWS_1252, content, normalized),
        "windows-1251" => encode_with_encoding(encoding_rs::WINDOWS_1251, content, normalized),
        _ => Err(format!("unsupported text encoding: {normalized}")),
    }
}

fn encode_with_encoding(
    encoding: &'static encoding_rs::Encoding,
    content: &str,
    label: &str,
) -> Result<Vec<u8>, String> {
    let (bytes, _, had_errors) = encoding.encode(content);
    if had_errors {
        return Err(format!("{label} cannot encode one or more characters"));
    }
    Ok(bytes.into_owned())
}
