/// Runtime shell markers, independent of terminal input and command history.
#[derive(Default)]
struct ShellPromptRefresh {
    buffer: String,
    command_tracking: bool,
    command_pending: bool,
    pending_cwd: Option<(String, bool)>,
}

impl ShellPromptRefresh {
    fn observe_output(&mut self, text: &str) {
        self.buffer.push_str(text);
        if self.buffer.len() > 8192 {
            trim_string_front(&mut self.buffer, 4096);
        }
        // Consume complete OSCs in order and retain only an incomplete suffix.
        while let Some(start) = self.buffer.find("\x1b]") {
            let body_start = start + 2;
            let tail = &self.buffer[body_start..];
            let end = tail.find('\x07').map(|index| (index, 1));
            let st = tail.find("\x1b\\").map(|index| (index, 2));
            let Some((end, terminator_length)) = end.into_iter().chain(st).min() else {
                self.buffer.drain(..start);
                return;
            };
            match &tail[..end] {
                "7777;FileTermCommandTracking=1" => self.command_tracking = true,
                "7777;FileTermCommandTracking=0" => {
                    self.command_tracking = false;
                    self.command_pending = false;
                }
                "7777;FileTermCommand" => self.command_pending = true,
                _ => {}
            }
            self.buffer.drain(..body_start + end + terminator_length);
        }
        // Preserve a split ESC introducer without retaining ordinary output.
        let escape = self.buffer.ends_with('\x1b');
        self.buffer.clear();
        if escape {
            self.buffer.push('\x1b');
        }
    }

    /// Returns (CWD, silent). Pair CWD and identity before consuming a command.
    fn take_prompt(
        &mut self,
        cwd: Option<String>,
        previous_cwd: Option<&str>,
        has_user: bool,
    ) -> Option<(String, bool)> {
        if let Some(cwd) = cwd {
            let changed = previous_cwd != Some(cwd.as_str());
            self.pending_cwd = Some((cwd, changed));
        }
        if !has_user {
            return None;
        }
        let (cwd, changed) = self.pending_cwd.take()?;
        let command = std::mem::take(&mut self.command_pending);
        (changed || command || !self.command_tracking).then_some((cwd, !changed))
    }
}

#[cfg(test)]
#[path = "tests/prompt_refresh_tests.rs"]
mod prompt_refresh_tests;
