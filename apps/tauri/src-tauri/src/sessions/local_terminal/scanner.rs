#[derive(Default)]
struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        if bytes.is_empty() && self.pending.is_empty() {
            return String::new();
        }

        let mut combined = Vec::with_capacity(self.pending.len() + bytes.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(bytes);
        self.pending.clear();

        match std::str::from_utf8(&combined) {
            Ok(value) => value.to_string(),
            Err(error) if error.error_len().is_none() => {
                let valid_up_to = error.valid_up_to();
                self.pending.extend_from_slice(&combined[valid_up_to..]);
                String::from_utf8_lossy(&combined[..valid_up_to]).into_owned()
            }
            Err(_) => String::from_utf8_lossy(&combined).into_owned(),
        }
    }

    fn finish(&mut self) -> String {
        let pending = std::mem::take(&mut self.pending);
        String::from_utf8_lossy(&pending).into_owned()
    }
}

/// Local shells can ask the terminal to report its device status or cursor
/// position before they print their first prompt.  A real terminal emulator
/// answers these queries internally; a PTY-backed desktop terminal must do the
/// same on the PTY boundary or the shell can remain blocked before the
/// renderer has subscribed to output.
struct LocalTerminalQueryScanner {
    active: bool,
    saw_query: bool,
    state: LocalTerminalQueryScanState,
    pending: Vec<u8>,
}

impl Default for LocalTerminalQueryScanner {
    fn default() -> Self {
        Self {
            active: true,
            saw_query: false,
            state: LocalTerminalQueryScanState::Ground,
            pending: Vec::new(),
        }
    }
}

#[derive(Default)]
enum LocalTerminalQueryScanState {
    #[default]
    Ground,
    Escape,
    Csi,
}

impl LocalTerminalQueryScanner {
    fn consume(&mut self, input: &str) -> (String, Vec<String>) {
        if !self.active {
            return (input.to_string(), Vec::new());
        }

        let mut display = Vec::with_capacity(input.len());
        let mut replies = Vec::new();
        let bytes = input.as_bytes();
        let mut index = 0;

        while index < bytes.len() {
            if !self.active {
                display.extend_from_slice(&bytes[index..]);
                break;
            }

            let byte = bytes[index];
            match self.state {
                LocalTerminalQueryScanState::Ground => {
                    if byte == 0x1b {
                        self.pending.clear();
                        self.pending.push(byte);
                        self.state = LocalTerminalQueryScanState::Escape;
                    } else {
                        self.emit_non_query_byte(byte, &mut display);
                    }
                }
                LocalTerminalQueryScanState::Escape => {
                    if byte == b'[' {
                        self.pending.push(byte);
                        self.state = LocalTerminalQueryScanState::Csi;
                    } else {
                        self.flush_pending(&mut display);
                        if byte == 0x1b {
                            self.pending.push(byte);
                            self.state = LocalTerminalQueryScanState::Escape;
                        } else {
                            self.emit_non_query_byte(byte, &mut display);
                            self.state = LocalTerminalQueryScanState::Ground;
                        }
                    }
                }
                LocalTerminalQueryScanState::Csi => {
                    self.pending.push(byte);
                    if (0x40..=0x7e).contains(&byte) {
                        if let Some(reply) = Self::reply_for_query(&self.pending) {
                            self.saw_query = true;
                            replies.push(reply.to_string());
                            self.pending.clear();
                            self.state = LocalTerminalQueryScanState::Ground;
                        } else {
                            self.flush_pending(&mut display);
                        }
                    } else if self.pending.len() > 64 {
                        self.flush_pending(&mut display);
                    }
                }
            }
            index += 1;
        }

        (String::from_utf8_lossy(&display).into_owned(), replies)
    }

    fn finish(&mut self) -> (String, Vec<String>) {
        let mut display = Vec::new();
        self.flush_pending(&mut display);
        (String::from_utf8_lossy(&display).into_owned(), Vec::new())
    }

    fn reply_for_query(sequence: &[u8]) -> Option<&'static str> {
        match sequence {
            b"\x1b[5n" => Some("\x1b[0n"),
            b"\x1b[6n" => Some("\x1b[1;1R"),
            b"\x1b[?6n" => Some("\x1b[?1;1R"),
            b"\x1b[c" | b"\x1b[0c" => Some("\x1b[?1;2c"),
            _ => None,
        }
    }

    fn emit_non_query_byte(&mut self, byte: u8, display: &mut Vec<u8>) {
        if self.saw_query {
            self.active = false;
        }
        display.push(byte);
    }

    fn flush_pending(&mut self, display: &mut Vec<u8>) {
        if self.saw_query {
            self.active = false;
        }
        display.extend_from_slice(&self.pending);
        self.pending.clear();
        self.state = LocalTerminalQueryScanState::Ground;
    }
}
