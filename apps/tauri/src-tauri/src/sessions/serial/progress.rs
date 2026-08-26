use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use super::super::{SerialTransferDirection, SerialTransferMode, SerialTransferProgress};

const PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(200);

pub(super) struct SerialTransferReporter {
    app: Option<AppHandle>,
    tab_id: String,
    direction: SerialTransferDirection,
    mode: SerialTransferMode,
    local_path: String,
    started_at: Instant,
    last_emitted_at: Option<Instant>,
    total_bytes: Option<u64>,
    last_bytes: u64,
}

impl SerialTransferReporter {
    pub(super) fn new(
        app: &AppHandle,
        tab_id: &str,
        direction: SerialTransferDirection,
        mode: SerialTransferMode,
        local_path: &str,
        total_bytes: Option<u64>,
    ) -> Self {
        Self {
            app: Some(app.clone()),
            tab_id: tab_id.to_string(),
            direction,
            mode,
            local_path: local_path.to_string(),
            started_at: Instant::now(),
            last_emitted_at: None,
            total_bytes,
            last_bytes: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn disabled(
        direction: SerialTransferDirection,
        mode: SerialTransferMode,
        local_path: &str,
    ) -> Self {
        Self {
            app: None,
            tab_id: String::new(),
            direction,
            mode,
            local_path: local_path.to_string(),
            started_at: Instant::now(),
            last_emitted_at: None,
            total_bytes: None,
            last_bytes: 0,
        }
    }

    pub(super) fn set_total(&mut self, total_bytes: Option<u64>) {
        self.total_bytes = total_bytes;
    }

    pub(super) fn report(&mut self, bytes: u64, block: Option<u64>) {
        self.last_bytes = self.last_bytes.max(bytes);
        self.emit_if_due("running", bytes, block, None, false);
    }

    pub(super) fn finish(
        &mut self,
        status: &str,
        bytes: u64,
        block: Option<u64>,
        message: Option<String>,
    ) {
        let bytes = bytes.max(self.last_bytes);
        self.last_bytes = bytes;
        self.emit_if_due(status, bytes, block, message, true);
    }

    fn emit_if_due(
        &mut self,
        status: &str,
        bytes: u64,
        block: Option<u64>,
        message: Option<String>,
        force: bool,
    ) {
        let now = Instant::now();
        if !force
            && self
                .last_emitted_at
                .is_some_and(|last| now.saturating_duration_since(last) < PROGRESS_EVENT_INTERVAL)
        {
            return;
        }
        self.last_emitted_at = Some(now);
        let elapsed = now.saturating_duration_since(self.started_at).as_secs_f64();
        let speed = (elapsed > 0.0).then(|| (bytes as f64 / elapsed) as u64);
        let Some(app) = &self.app else {
            return;
        };
        let _ = app.emit(
            "serial:transfer-progress",
            SerialTransferProgress {
                tab_id: self.tab_id.clone(),
                direction: direction_name(self.direction).to_string(),
                mode: mode_name(self.mode).to_string(),
                local_path: self.local_path.clone(),
                status: status.to_string(),
                bytes_transferred: bytes,
                total_bytes: self.total_bytes,
                speed_bytes_per_second: speed,
                block,
                message,
            },
        );
    }
}

fn direction_name(direction: SerialTransferDirection) -> &'static str {
    match direction {
        SerialTransferDirection::Send => "send",
        SerialTransferDirection::Receive => "receive",
    }
}

fn mode_name(mode: SerialTransferMode) -> &'static str {
    match mode {
        SerialTransferMode::Raw => "raw",
        SerialTransferMode::Xmodem => "xmodem",
        SerialTransferMode::Ymodem => "ymodem",
        SerialTransferMode::Zmodem => "zmodem",
        SerialTransferMode::Kermit => "kermit",
    }
}
