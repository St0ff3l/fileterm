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
