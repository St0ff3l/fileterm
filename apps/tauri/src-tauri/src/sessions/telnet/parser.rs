#[derive(Clone, Copy)]
enum ParseState {
    Data,
    Iac,
    Option(u8),
    Subnegotiation,
    SubnegotiationIac,
}

struct TelnetParser {
    state: ParseState,
    subnegotiation: Vec<u8>,
    cols: u16,
    rows: u16,
    terminal_type: Vec<u8>,
    local_options: [TelnetOptionState; 256],
    remote_options: [TelnetOptionState; 256],
    receive_binary: bool,
    transmit_binary: bool,
    pending_cr: bool,
}

#[derive(Clone, Copy, Default)]
struct TelnetOptionState {
    enabled: bool,
    refused: bool,
}

impl TelnetParser {
    fn new(terminal_type: &str) -> Self {
        let terminal_type = terminal_type
            .bytes()
            .filter(|byte| (0x20..=0x7e).contains(byte))
            .take(40)
            .collect::<Vec<_>>();
        Self {
            state: ParseState::Data,
            subnegotiation: Vec::new(),
            cols: 80,
            rows: 24,
            terminal_type: if terminal_type.is_empty() {
                b"xterm-256color".to_vec()
            } else {
                terminal_type
            },
            local_options: [TelnetOptionState::default(); 256],
            remote_options: [TelnetOptionState::default(); 256],
            receive_binary: false,
            transmit_binary: false,
            pending_cr: false,
        }
    }

    fn set_size(&mut self, cols: u32, rows: u32) -> Vec<u8> {
        self.cols = cols.clamp(1, u16::MAX as u32) as u16;
        self.rows = rows.clamp(1, u16::MAX as u32) as u16;
        if self.local_options[NAWS as usize].enabled {
            self.naws()
        } else {
            Vec::new()
        }
    }

    fn naws(&self) -> Vec<u8> {
        let mut packet = vec![IAC, SB, NAWS];
        for byte in [
            (self.cols >> 8) as u8,
            self.cols as u8,
            (self.rows >> 8) as u8,
            self.rows as u8,
        ] {
            packet.push(byte);
            if byte == IAC {
                packet.push(IAC);
            }
        }
        packet.extend_from_slice(&[IAC, SE]);
        packet
    }

    fn feed(&mut self, input: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut output = Vec::new();
        let mut writes = Vec::new();
        for byte in input {
            match self.state {
                ParseState::Data => {
                    if *byte == IAC {
                        // An IAC starts a control sequence, so a preceding CR
                        // cannot be paired with a later NUL across it.
                        self.pending_cr = false;
                        self.state = ParseState::Iac;
                    } else if !self.receive_binary && self.pending_cr && *byte == 0 {
                        // NVT uses CR NUL for a literal carriage return. The
                        // CR itself was already emitted in the previous
                        // chunk; consume only the NUL terminator.
                        self.pending_cr = false;
                    } else {
                        output.push(*byte);
                        self.pending_cr = !self.receive_binary && *byte == b'\r';
                    }
                }
                ParseState::Iac => match *byte {
                    IAC => {
                        output.push(IAC);
                        self.state = ParseState::Data;
                    }
                    DO | DONT | WILL | WONT => self.state = ParseState::Option(*byte),
                    SB => {
                        self.subnegotiation.clear();
                        self.state = ParseState::Subnegotiation;
                    }
                    _ => self.state = ParseState::Data,
                },
                ParseState::Option(command) => {
                    writes.extend(self.negotiate(command, *byte));
                    self.state = ParseState::Data;
                }
                ParseState::Subnegotiation => {
                    if *byte == IAC {
                        self.state = ParseState::SubnegotiationIac;
                    } else if self.subnegotiation.len() < 4096 {
                        self.subnegotiation.push(*byte);
                    }
                }
                ParseState::SubnegotiationIac => {
                    if *byte == SE {
                        if self.subnegotiation.first() == Some(&TERMINAL_TYPE)
                            && self.subnegotiation.get(1) == Some(&1)
                        {
                            let mut reply = vec![IAC, SB, TERMINAL_TYPE, 0];
                            append_escaped(&mut reply, &self.terminal_type);
                            reply.extend_from_slice(&[IAC, SE]);
                            writes.push(reply);
                        }
                        self.subnegotiation.clear();
                        self.state = ParseState::Data;
                        continue;
                    } else if *byte == IAC && self.subnegotiation.len() < 4096 {
                        self.subnegotiation.push(IAC);
                    }
                    self.state = ParseState::Subnegotiation;
                }
            }
        }
        (output, writes)
    }

    fn negotiate(&mut self, command: u8, option: u8) -> Vec<Vec<u8>> {
        let index = option as usize;
        let mut writes = Vec::new();
        match command {
            DO => {
                let supported = supports_local(option);
                let state = &mut self.local_options[index];
                if supported {
                    state.refused = false;
                    if !state.enabled {
                        state.enabled = true;
                        writes.push(vec![IAC, WILL, option]);
                        if option == NAWS {
                            writes.push(self.naws());
                        }
                    }
                } else if !state.refused {
                    state.enabled = false;
                    state.refused = true;
                    writes.push(vec![IAC, WONT, option]);
                }
            }
            DONT => {
                let state = &mut self.local_options[index];
                if state.enabled {
                    state.enabled = false;
                    writes.push(vec![IAC, WONT, option]);
                }
                state.refused = false;
            }
            WILL => {
                let supported = supports_remote(option);
                let state = &mut self.remote_options[index];
                if supported {
                    state.refused = false;
                    if !state.enabled {
                        state.enabled = true;
                        writes.push(vec![IAC, DO, option]);
                        if option == BINARY {
                            self.receive_binary = true;
                        }
                    }
                } else if !state.refused {
                    state.enabled = false;
                    state.refused = true;
                    writes.push(vec![IAC, DONT, option]);
                }
            }
            WONT => {
                let state = &mut self.remote_options[index];
                if state.enabled {
                    state.enabled = false;
                    writes.push(vec![IAC, DONT, option]);
                }
                state.refused = false;
                if option == BINARY {
                    self.receive_binary = false;
                }
            }
            _ => {}
        }
        if option == BINARY {
            self.transmit_binary = self.local_options[index].enabled;
        }
        writes
    }
}

fn supports_local(option: u8) -> bool {
    matches!(option, BINARY | SUPPRESS_GO_AHEAD | TERMINAL_TYPE | NAWS)
}

fn supports_remote(option: u8) -> bool {
    matches!(option, BINARY | ECHO | SUPPRESS_GO_AHEAD)
}

fn append_escaped(output: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        output.push(*byte);
        if *byte == IAC {
            output.push(IAC);
        }
    }
}

fn encode_telnet_input(
    input: &[u8],
    newline_mode: &str,
    cr_nul: bool,
    transmit_binary: bool,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() + 8);
    if transmit_binary {
        // RFC 856 binary mode carries octets as-is.  The only Telnet framing
        // still required is IAC doubling; applying NVT newline conversion in
        // this branch would silently corrupt file/protocol payloads.
        append_escaped(&mut output, input);
        return output;
    }
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        if (byte == b'\r' || byte == b'\n') && input.get(index + 1) == Some(&b'\n') && byte == b'\r'
        {
            index += 1;
        }
        let translated = match byte {
            b'\r' | b'\n' => match newline_mode {
                "lf" => vec![b'\n'],
                "cr" => vec![b'\r'],
                "crlf" => vec![b'\r', b'\n'],
                _ => vec![byte],
            },
            _ => vec![byte],
        };
        for translated_byte in translated {
            output.push(translated_byte);
            if translated_byte == b'\r' && cr_nul && !transmit_binary && newline_mode != "crlf" {
                output.push(0);
            }
            if translated_byte == IAC {
                output.push(IAC);
            }
        }
        index += 1;
    }
    output
}
