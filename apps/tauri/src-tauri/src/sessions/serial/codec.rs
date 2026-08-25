use std::fmt::Write as _;

use crate::sessions::terminal::{decode_terminal, encode_terminal};

pub(super) fn validate_modes(
    newline_mode: &str,
    input_mode: &str,
    output_mode: &str,
) -> Result<(), String> {
    if !matches!(newline_mode, "none" | "lf" | "cr" | "crlf") {
        return Err("串口换行模式无效".to_string());
    }
    if !matches!(input_mode, "text" | "hex") {
        return Err("串口输入模式无效".to_string());
    }
    if !matches!(output_mode, "text" | "hex") {
        return Err("串口输出模式无效".to_string());
    }
    Ok(())
}

pub(super) fn baud_rate(value: u64) -> Result<u32, String> {
    let baud_rate = u32::try_from(value).map_err(|_| "串口波特率超出支持范围".to_string())?;
    if baud_rate == 0 {
        return Err("串口波特率必须大于 0".to_string());
    }
    Ok(baud_rate)
}

pub(super) fn normalize_newlines(value: &str, mode: &str) -> Result<String, String> {
    let replacement = match mode {
        "none" => return Ok(value.to_string()),
        "lf" => "\n",
        "cr" => "\r",
        "crlf" => "\r\n",
        _ => return Err("串口换行模式无效".to_string()),
    };
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    Ok(normalized.replace('\n', replacement))
}

fn newline_bytes(mode: &str) -> &'static [u8] {
    match mode {
        "lf" => b"\n",
        "cr" => b"\r",
        "crlf" => b"\r\n",
        _ => b"",
    }
}

pub(super) fn encode_input(
    value: &str,
    encoding: &str,
    input_mode: &str,
    newline_mode: &str,
) -> Result<Vec<u8>, String> {
    let has_line_break = value.contains('\r') || value.contains('\n');
    let transformed = normalize_newlines(value, newline_mode)?;
    match input_mode {
        "text" => Ok(encode_terminal(&transformed, encoding)),
        "hex" => {
            // In hex mode Enter terminates the editor. Keep the actual line
            // ending when newlineMode is `none`; dropping it made Hex line
            // mode silently send no terminator at all.
            let content = transformed.replace(['\r', '\n'], "");
            let mut bytes = parse_hex(&content)?;
            if has_line_break {
                let line_ending = if newline_mode == "none" {
                    if value.contains("\r\n") {
                        b"\r\n".as_slice()
                    } else if value.contains('\r') {
                        b"\r".as_slice()
                    } else {
                        b"\n".as_slice()
                    }
                } else {
                    newline_bytes(newline_mode)
                };
                bytes.extend_from_slice(line_ending);
            }
            Ok(bytes)
        }
        _ => Err("串口输入模式无效".to_string()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SerialInputChunk {
    Line { value: String, terminator: String },
    LineContinuation(String),
    Immediate(u8),
}

fn consume_input(
    buffer: &mut String,
    data: &str,
    pending_lf_after_cr: &mut bool,
) -> Vec<SerialInputChunk> {
    let mut ready_lines = Vec::new();
    for character in data.chars() {
        if *pending_lf_after_cr {
            *pending_lf_after_cr = false;
            if character == '\n' {
                if let Some(SerialInputChunk::Line { terminator, .. }) = ready_lines.last_mut() {
                    *terminator = "\r\n".to_string();
                } else {
                    // CR and LF can arrive in separate renderer input events.
                    // Keep the second byte instead of dropping it; the worker
                    // decides whether newline normalization should apply.
                    ready_lines.push(SerialInputChunk::LineContinuation("\n".to_string()));
                }
                continue;
            }
        }
        match character {
            '\r' => {
                ready_lines.push(SerialInputChunk::Line {
                    value: std::mem::take(buffer),
                    terminator: "\r".to_string(),
                });
                *pending_lf_after_cr = true;
            }
            '\n' => ready_lines.push(SerialInputChunk::Line {
                value: std::mem::take(buffer),
                terminator: "\n".to_string(),
            }),
            '\u{8}' | '\u{7f}' => {
                buffer.pop();
            }
            character if character.is_ascii_control() && character != '\t' => {
                // Ctrl+C is the conventional line-cancel key. Send the byte
                // immediately, but do not leave the canceled command in the
                // local line editor for the next Enter press.
                if character == '\u{3}' {
                    buffer.clear();
                }
                ready_lines.push(SerialInputChunk::Immediate(character as u8));
            }
            _ => buffer.push(character),
        }
    }
    ready_lines
}

pub(super) fn consume_hex_input(
    buffer: &mut String,
    data: &str,
    pending_lf_after_cr: &mut bool,
) -> Vec<SerialInputChunk> {
    consume_input(buffer, data, pending_lf_after_cr)
}

pub(super) fn consume_line_input(
    buffer: &mut String,
    data: &str,
    pending_lf_after_cr: &mut bool,
) -> Vec<SerialInputChunk> {
    consume_input(buffer, data, pending_lf_after_cr)
}

pub(super) fn parse_hex(value: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for token in value.split(|character: char| {
        character.is_ascii_whitespace() || matches!(character, ':' | ',' | '_')
    }) {
        if token.is_empty() {
            continue;
        }
        let digits = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        if digits.is_empty() || digits.len() % 2 != 0 {
            return Err(format!("Hex 输入必须按两个字符表示一个字节：{token}"));
        }
        for pair in digits.as_bytes().chunks_exact(2) {
            let pair = std::str::from_utf8(pair).expect("hex input is ASCII-compatible");
            let byte = u8::from_str_radix(pair, 16)
                .map_err(|_| format!("Hex 输入包含无效字节：{pair}"))?;
            bytes.push(byte);
        }
    }
    Ok(bytes)
}

pub(super) struct TextDecoder {
    decoder: encoding_rs::Decoder,
}

impl TextDecoder {
    pub(super) fn new(encoding: &str) -> Self {
        let encoding = encoding_rs::Encoding::for_label(encoding.trim().as_bytes())
            .unwrap_or(encoding_rs::UTF_8);
        Self {
            decoder: encoding.new_decoder_without_bom_handling(),
        }
    }

    pub(super) fn decode(&mut self, bytes: &[u8]) -> String {
        let capacity = self
            .decoder
            .max_utf8_buffer_length(bytes.len())
            .unwrap_or(4)
            .max(4);
        let mut output = String::with_capacity(capacity);
        let _ = self.decoder.decode_to_string(bytes, &mut output, false);
        output
    }

    pub(super) fn finish(&mut self) -> String {
        let mut output = String::with_capacity(4);
        let _ = self.decoder.decode_to_string(&[], &mut output, true);
        output
    }
}

pub(super) fn stream_display(
    decoder: &mut TextDecoder,
    bytes: &[u8],
    output_mode: &str,
) -> Result<String, String> {
    match output_mode {
        "text" => Ok(decoder.decode(bytes)),
        "hex" => Ok(format_hex(bytes)),
        _ => Err("串口输出模式无效".to_string()),
    }
}

pub(super) fn display(bytes: &[u8], encoding: &str, output_mode: &str) -> Result<String, String> {
    match output_mode {
        "text" => Ok(decode_terminal(bytes, encoding)),
        "hex" => Ok(format_hex(bytes)),
        _ => Err("串口输出模式无效".to_string()),
    }
}

pub(super) fn format_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        let _ = write!(output, "{byte:02X}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_serial_codecs_and_line_endings() {
        assert_eq!(baud_rate(115_200).unwrap(), 115_200);
        assert!(baud_rate(0).is_err());
        assert!(baud_rate(u64::from(u32::MAX) + 1).is_err());
        assert!(validate_modes("crlf", "hex", "text").is_ok());
        assert!(validate_modes("bad", "text", "text").is_err());
        assert_eq!(
            normalize_newlines("a\r\nb\rc\nd", "crlf").unwrap(),
            "a\r\nb\r\nc\r\nd"
        );
    }

    #[test]
    fn encodes_text_and_hex_input() {
        assert_eq!(
            encode_input("ping\r", "UTF-8", "text", "lf").unwrap(),
            b"ping\n"
        );
        assert_eq!(
            encode_input("48 65 0x6C6C6F\r", "UTF-8", "hex", "crlf").unwrap(),
            b"Hello\r\n"
        );
        assert_eq!(
            encode_input("41\r", "UTF-8", "hex", "none").unwrap(),
            b"A\r"
        );
        assert!(encode_input("ABC", "UTF-8", "hex", "none").is_err());
    }

    #[test]
    fn buffers_hex_keystrokes_and_handles_split_crlf() {
        let mut buffer = String::new();
        let mut pending_lf = false;
        assert!(consume_hex_input(&mut buffer, "4", &mut pending_lf).is_empty());
        assert!(consume_hex_input(&mut buffer, "8", &mut pending_lf).is_empty());
        assert_eq!(buffer, "48");
        assert_eq!(
            consume_hex_input(&mut buffer, "\u{7f}8\r", &mut pending_lf),
            vec![SerialInputChunk::Line {
                value: "48".to_string(),
                terminator: "\r".to_string()
            }]
        );
        assert_eq!(
            consume_hex_input(&mut buffer, "48\r", &mut pending_lf),
            vec![SerialInputChunk::Line {
                value: "48".to_string(),
                terminator: "\r".to_string()
            }]
        );
        assert_eq!(
            consume_hex_input(&mut buffer, "\n69\r\n", &mut pending_lf),
            vec![
                SerialInputChunk::LineContinuation("\n".to_string()),
                SerialInputChunk::Line {
                    value: "69".to_string(),
                    terminator: "\r\n".to_string()
                }
            ]
        );
        assert!(!pending_lf);
    }

    #[test]
    fn buffers_text_lines_and_removes_backspace() {
        let mut buffer = String::new();
        let mut pending_lf = false;
        assert!(consume_line_input(&mut buffer, "pi", &mut pending_lf).is_empty());
        assert_eq!(
            consume_line_input(&mut buffer, "n\u{8}ng\r\n", &mut pending_lf),
            vec![SerialInputChunk::Line {
                value: "ping".to_string(),
                terminator: "\r\n".to_string()
            }]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn emits_control_bytes_immediately_in_line_mode() {
        let mut buffer = String::from("pending");
        let mut pending_lf = false;
        assert_eq!(
            consume_line_input(&mut buffer, "\u{3}", &mut pending_lf),
            vec![SerialInputChunk::Immediate(3)]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn keeps_a_split_lf_for_newline_none_mode() {
        let mut buffer = String::new();
        let mut pending_lf = false;
        assert_eq!(
            consume_line_input(&mut buffer, "ok\r", &mut pending_lf),
            vec![SerialInputChunk::Line {
                value: "ok".to_string(),
                terminator: "\r".to_string()
            }]
        );
        assert_eq!(
            consume_line_input(&mut buffer, "\n", &mut pending_lf),
            vec![SerialInputChunk::LineContinuation("\n".to_string())]
        );
    }

    #[test]
    fn parses_and_formats_hex() {
        assert_eq!(parse_hex("0x41:42,43_44").unwrap(), b"ABCD");
        assert_eq!(format_hex(&[0x00, 0x0a, 0xff]), "00 0A FF");
        assert_eq!(
            display(&[0x00, 0x0a, 0xff], "UTF-8", "hex").unwrap(),
            "00 0A FF"
        );
    }

    #[test]
    fn preserves_multibyte_text_when_reads_split_a_character() {
        let mut decoder = TextDecoder::new("UTF-8");
        assert_eq!(decoder.decode(&[0xe4]), "");
        assert_eq!(decoder.decode(&[0xb8]), "");
        assert_eq!(decoder.decode(&[0xad]), "中");
        assert_eq!(decoder.finish(), "");

        let mut decoder = TextDecoder::new("GBK");
        assert_eq!(decoder.decode(&[0xd6]), "");
        assert_eq!(decoder.decode(&[0xd0]), "中");
        assert_eq!(decoder.finish(), "");
    }
}
