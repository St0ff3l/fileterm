enum LocalPtyCommand {
    Input(String),
    Resize {
        cols: u32,
        rows: u32,
        width: u32,
        height: u32,
    },
    Shutdown,
}

struct LocalOutputChunk {
    data: String,
    dropped_bytes_before: usize,
    /// 丢帧期间被丢的数据里是否可能包含 alternate screen 切换序列。
    /// renderer 可据此提示用户终端状态可能不一致（参考 Netcatty 的
    /// droppedOutputMayAffectTerminalState 语义）。
    dropped_alt_screen_change: bool,
}

#[derive(Default)]
struct LocalOutputDropState {
    bytes: usize,
    logged: bool,
    saw_alt_screen_change: bool,
    alt_screen_scanner: AltScreenTransitionScanner,
}

#[derive(Default)]
struct AltScreenTransitionScanner {
    state: AltScreenScanState,
}

#[derive(Default)]
enum AltScreenScanState {
    #[default]
    Ground,
    Escape,
    Csi {
        params: Vec<u8>,
        has_intermediate: bool,
        overflowed: bool,
    },
}

impl AltScreenTransitionScanner {
    /// Scan a PTY chunk while retaining an incomplete CSI sequence for the next chunk.
    fn observe(&mut self, data: &str) -> bool {
        let mut found_transition = false;
        for byte in data.bytes() {
            let (state, transition) = match std::mem::take(&mut self.state) {
                AltScreenScanState::Ground => {
                    if byte == 0x1b {
                        (AltScreenScanState::Escape, false)
                    } else {
                        (AltScreenScanState::Ground, false)
                    }
                }
                AltScreenScanState::Escape => match byte {
                    b'[' => (
                        AltScreenScanState::Csi {
                            params: Vec::new(),
                            has_intermediate: false,
                            overflowed: false,
                        },
                        false,
                    ),
                    0x1b => (AltScreenScanState::Escape, false),
                    _ => (AltScreenScanState::Ground, false),
                },
                AltScreenScanState::Csi {
                    mut params,
                    mut has_intermediate,
                    mut overflowed,
                } => {
                    if (0x30..=0x3f).contains(&byte) {
                        if params.len() < 128 {
                            params.push(byte);
                        } else {
                            overflowed = true;
                        }
                        (
                            AltScreenScanState::Csi {
                                params,
                                has_intermediate,
                                overflowed,
                            },
                            false,
                        )
                    } else if (0x20..=0x2f).contains(&byte) {
                        has_intermediate = true;
                        (
                            AltScreenScanState::Csi {
                                params,
                                has_intermediate,
                                overflowed,
                            },
                            false,
                        )
                    } else if (0x40..=0x7e).contains(&byte) {
                        let transition = !overflowed
                            && !has_intermediate
                            && (byte == b'h' || byte == b'l')
                            && alt_screen_params_match(&params);
                        (AltScreenScanState::Ground, transition)
                    } else if byte == 0x1b {
                        (AltScreenScanState::Escape, false)
                    } else {
                        (AltScreenScanState::Ground, false)
                    }
                }
            };
            self.state = state;
            found_transition |= transition;
        }
        found_transition
    }

    fn has_pending_sequence(&self) -> bool {
        !matches!(&self.state, AltScreenScanState::Ground)
    }
}

fn alt_screen_params_match(params: &[u8]) -> bool {
    let params = if params.first() == Some(&b'?') {
        &params[1..]
    } else {
        params
    };
    params.split(|byte| *byte == b';').any(|token| {
        let token = std::str::from_utf8(token).unwrap_or("");
        matches!(token, "47" | "1047" | "1049")
    })
}

/// 检测数据里是否包含 DECSET/DECRST 风格的 alternate screen 切换序列。
/// 这个无状态入口保留给单 chunk 场景和单元测试；真实 PTY reader 使用
/// `AltScreenTransitionScanner`，以便处理 ANSI 序列跨 read 边界的情况。
#[cfg(test)]
fn scan_alt_screen_transition(data: &str) -> bool {
    let mut scanner = AltScreenTransitionScanner::default();
    scanner.observe(data)
}

const LOCAL_OSC7_BUFFER_LIMIT: usize = 16 * 1024;
const LOCAL_OSC7_MARKER: &str = "\x1b]7;";

#[derive(Default)]
struct LocalOsc7CwdTracker {
    buffer: String,
}

impl LocalOsc7CwdTracker {
    fn observe(&mut self, chunk: &str) -> Option<String> {
        self.buffer.push_str(chunk);
        let mut latest_cwd = None;

        loop {
            let Some(start) = self.buffer.find(LOCAL_OSC7_MARKER) else {
                retain_osc7_prefix(&mut self.buffer);
                break;
            };
            if start > 0 {
                self.buffer.drain(..start);
            }

            let payload = &self.buffer[LOCAL_OSC7_MARKER.len()..];
            let bel_end = payload.find('\u{7}').map(|index| (index + 1, 1));
            let st_end = payload.find("\x1b\\").map(|index| (index + 2, 2));
            let Some((end, terminator_len)) = [bel_end, st_end]
                .into_iter()
                .flatten()
                .min_by_key(|(end, _)| *end)
            else {
                break;
            };

            if let Some(cwd) = decode_osc7_cwd(&payload[..end - terminator_len]) {
                latest_cwd = Some(cwd);
            }
            self.buffer.drain(..LOCAL_OSC7_MARKER.len() + end);
        }

        if self.buffer.len() > LOCAL_OSC7_BUFFER_LIMIT {
            if let Some(start) = self.buffer.rfind(LOCAL_OSC7_MARKER) {
                self.buffer.drain(..start);
            } else {
                self.buffer.clear();
            }
        }
        latest_cwd
    }
}

fn retain_osc7_prefix(buffer: &mut String) {
    let keep = ["\x1b", "\x1b]", "\x1b]7"]
        .into_iter()
        .filter(|prefix| buffer.ends_with(prefix))
        .map(str::len)
        .max()
        .unwrap_or(0);
    if keep == 0 {
        buffer.clear();
    } else {
        let start = buffer.len() - keep;
        buffer.drain(..start);
    }
}

fn decode_osc7_cwd(payload: &str) -> Option<String> {
    let uri = payload.strip_prefix("file://")?;
    let path_start = uri.find('/')?;
    let path = percent_decode(&uri[path_start..]);
    if path.is_empty() || path.contains('\0') {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        let mut path = path;
        if path.starts_with('/') && path.as_bytes().get(2) == Some(&b':') {
            path.remove(0);
        }
        Some(path)
    }

    #[cfg(not(target_os = "windows"))]
    Some(path)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}


fn queue_local_terminal_output(
    app: &AppHandle,
    tab_id: &str,
    gate: &Arc<LocalTerminalRuntimeGate>,
    output_tx: &mpsc::Sender<LocalOutputChunk>,
    chunk: String,
    output_drop_state: &mut LocalOutputDropState,
) -> bool {
    if !gate.active.load(Ordering::Acquire) {
        crate::services::logging::debug(
            app,
            "local",
            format!(
                "discarding PTY output tab={} reason=runtime-inactive",
                tab_id
            ),
        );
        return false;
    }

    // Feed every reader chunk into the scanner, not only chunks that are already
    // being dropped. A CSI sequence can start in a successfully delivered chunk
    // and finish after the output queue becomes full.
    let alt_screen_transition = output_drop_state.alt_screen_scanner.observe(&chunk);
    let dropped_bytes_before = output_drop_state.bytes;
    let dropped_alt_screen_change = output_drop_state.saw_alt_screen_change
        || (dropped_bytes_before > 0 && alt_screen_transition);
    match output_tx.try_send(LocalOutputChunk {
        data: chunk.clone(),
        dropped_bytes_before,
        dropped_alt_screen_change,
    }) {
        Ok(()) => {
            output_drop_state.bytes = 0;
            output_drop_state.logged = false;
            output_drop_state.saw_alt_screen_change = false;
            true
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            output_drop_state.bytes = output_drop_state.bytes.saturating_add(chunk.len());
            if alt_screen_transition || output_drop_state.alt_screen_scanner.has_pending_sequence()
            {
                output_drop_state.saw_alt_screen_change = true;
            }
            if !output_drop_state.logged {
                output_drop_state.logged = true;
                crate::services::logging::session(
                    app,
                    "WARN",
                    "local",
                    tab_id,
                    "terminal output pump saturated; dropping local PTY output",
                );
            }
            true
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            crate::services::logging::debug(
                app,
                "local",
                format!("PTY output queue closed tab={tab_id}"),
            );
            false
        }
    }
}

fn flush_local_output_drop_notice(
    output_tx: &mpsc::Sender<LocalOutputChunk>,
    output_drop_state: &mut LocalOutputDropState,
) {
    if output_drop_state.bytes == 0 {
        return;
    }

    let dropped_bytes_before = output_drop_state.bytes;
    let dropped_alt_screen_change = output_drop_state.saw_alt_screen_change;
    if output_tx
        .try_send(LocalOutputChunk {
            data: String::new(),
            dropped_bytes_before,
            dropped_alt_screen_change,
        })
        .is_ok()
    {
        output_drop_state.bytes = 0;
        output_drop_state.logged = false;
        output_drop_state.saw_alt_screen_change = false;
    }
}

fn append_local_output_chunk(batch: &mut String, chunk: &LocalOutputChunk) {
    if chunk.dropped_bytes_before > 0 {
        batch.push_str(&format!(
            "\r\n[FileTerm: local terminal output dropped {} bytes while the renderer was busy]\r\n",
            chunk.dropped_bytes_before
        ));
        if chunk.dropped_alt_screen_change {
            batch.push_str("\r\n[FileTerm: dropped output may include alternate screen transitions; terminal state may be inconsistent — run `reset` or Ctrl+L to resync]\r\n");
        }
    }
    batch.push_str(&chunk.data);
}
