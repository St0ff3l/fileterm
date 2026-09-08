/// Decode each SSH stream independently, retaining only an incomplete UTF-8
/// suffix. Invalid bytes are replaced, but never consume a later partial char.
#[derive(Default)]
struct SshUtf8Decoder {
    pending: Vec<u8>,
}

impl SshUtf8Decoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut text = String::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(valid) => {
                    text.push_str(valid);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let valid_end = consumed + error.valid_up_to();
                    text.push_str(std::str::from_utf8(&self.pending[consumed..valid_end]).unwrap());
                    consumed = valid_end;
                    match error.error_len() {
                        Some(length) => {
                            text.push('\u{fffd}');
                            consumed += length;
                        }
                        None => break,
                    }
                }
            }
        }
        self.pending.drain(..consumed);
        text
    }

    fn finish(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
    }
}

#[cfg(test)]
mod ssh_utf8_tests {
    use super::SshUtf8Decoder;

    #[test]
    fn every_packet_split_preserves_terminal_columns_and_osc() {
        let text = "\x1b[32m──中文 🦀\x1b[0m\x1b]7;file://host/目录\x07";
        for split in 0..=text.len() {
            let mut decoder = SshUtf8Decoder::default();
            let mut actual = decoder.decode(&text.as_bytes()[..split]);
            actual.push_str(&decoder.decode(&[])); // a timer flush does not finish a stream
            actual.push_str(&decoder.decode(&text.as_bytes()[split..]));
            actual.push_str(&decoder.finish());
            assert_eq!(actual, text, "split={split}");
        }
        let mut decoder = SshUtf8Decoder::default();
        let actual: String = text
            .as_bytes()
            .iter()
            .map(|byte| decoder.decode(&[*byte]))
            .collect();
        assert_eq!(actual, text);
    }

    #[test]
    fn invalid_byte_before_partial_char_does_not_corrupt_next_packet() {
        let mut decoder = SshUtf8Decoder::default();
        assert_eq!(decoder.decode(&[0xff, 0xe2]), "�");
        assert_eq!(decoder.decode(&[0x94, 0x80]), "─");
        assert_eq!(decoder.decode(&[0xf0, 0x9f]), "");
        assert_eq!(decoder.finish(), "�");
        assert_eq!(decoder.decode(b"new session"), "new session");
    }

    #[test]
    fn stderr_cannot_complete_a_stdout_character() {
        let mut stdout = SshUtf8Decoder::default();
        let mut stderr = SshUtf8Decoder::default();
        assert_eq!(stdout.decode(&[0xe2]), "");
        assert_eq!(stderr.decode(b"warning"), "warning");
        assert_eq!(stdout.decode(&[0x94, 0x80]), "─");
    }
}
