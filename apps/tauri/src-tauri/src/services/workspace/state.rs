impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            tabs: Arc::new(RwLock::new(Vec::new())),
            active_tab_id: Arc::new(RwLock::new(None)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
            session_log_writers: Arc::new(RwLock::new(HashMap::new())),
            terminal_inputs: Arc::new(RwLock::new(HashMap::new())),
            terminal_output_channels: Arc::new(StdMutex::new(HashMap::new())),
            worker_controls: Arc::new(RwLock::new(HashMap::new())),
            serial_reconnect_attempts: Arc::new(RwLock::new(HashMap::new())),
            serial_transfer_cancellations: Arc::new(RwLock::new(HashMap::new())),
            local_terminal_runtime_ids: Arc::new(RwLock::new(HashMap::new())),
            local_terminal_runtime_gates: Arc::new(RwLock::new(HashMap::new())),
            local_terminal_launches: Arc::new(RwLock::new(HashMap::new())),
            pending_interactions: Arc::new(RwLock::new(HashMap::new())),
            connection_operations: Arc::new(ConnectionOperationRegistry::default()),
            pending_backup_passwords: Arc::new(RwLock::new(HashMap::new())),
            backup_password_renderer_registration: Arc::new(RwLock::new(None)),
            pending_sudo_passwords: Arc::new(RwLock::new(HashMap::new())),
            sudo_password_renderer_registration: Arc::new(RwLock::new(None)),
            pending_action_approvals: Arc::new(RwLock::new(HashMap::new())),
            remote_forwards: Arc::new(RwLock::new(HashMap::new())),
            transfers: Arc::new(RwLock::new(Vec::new())),
            transfer_runs: Arc::new(RwLock::new(HashMap::new())),
            transfer_lifecycle: Arc::new(Mutex::new(())),
            next_transfer_generation: Arc::new(AtomicU64::new(0)),
            transfer_journal_loaded: Arc::new(Mutex::new(false)),
            transfer_journal_write: Arc::new(Mutex::new(())),
            transfer_last_event: Arc::new(Mutex::new(HashMap::new())),
            transfer_progress_samples: Arc::new(Mutex::new(HashMap::new())),
            connection_import_plans: Arc::new(RwLock::new(HashMap::new())),
            connection_tests_in_flight: Arc::new(Mutex::new(HashSet::new())),
            connection_tests_last_started: Arc::new(Mutex::new(HashMap::new())),
            library_mutation: Arc::new(Mutex::new(())),
            update_status: Arc::new(RwLock::new(None)),
            update_check: Arc::new(Mutex::new(())),
            update_operation: Arc::new(Mutex::new(())),
            workspace_snapshot_lock: Arc::new(Mutex::new(())),
            next_workspace_snapshot_revision: Arc::new(AtomicU64::new(0)),
            active_pane_tab_id_by_root: Arc::new(RwLock::new(HashMap::new())),
            ai_session_revisions: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(target_os = "windows")]
            windows_downloaded_update: Arc::new(Mutex::new(None)),
        }
    }
}

impl WorkspaceState {
    pub async fn set_backup_password_renderer_ready(&self, registration_id: &str, ready: bool) {
        let registration_id = registration_id.trim();
        if registration_id.is_empty() || registration_id.len() > 200 {
            return;
        }
        let mut active = self.backup_password_renderer_registration.write().await;
        if ready {
            *active = Some(registration_id.to_string());
            return;
        }
        if active.as_deref() != Some(registration_id) {
            return;
        }
        *active = None;
        self.pending_backup_passwords.write().await.clear();
    }

    pub async fn insert_pending_backup_password(
        &self,
        request_id: String,
        pending: PendingBackupPassword,
    ) -> bool {
        let active = self.backup_password_renderer_registration.read().await;
        if active.is_none() {
            return false;
        }
        self.pending_backup_passwords
            .write()
            .await
            .insert(request_id, pending);
        true
    }

    pub async fn set_sudo_password_renderer_ready(&self, registration_id: &str, ready: bool) {
        let registration_id = registration_id.trim();
        if registration_id.is_empty() || registration_id.len() > 200 {
            return;
        }
        let mut active = self.sudo_password_renderer_registration.write().await;
        if ready {
            *active = Some(registration_id.to_string());
            return;
        }
        if active.as_deref() != Some(registration_id) {
            return;
        }
        *active = None;
        self.pending_sudo_passwords.write().await.clear();
    }

    pub async fn insert_pending_sudo_password(
        &self,
        request_id: String,
        pending: PendingSudoPassword,
    ) -> bool {
        // Keep the registration write lock across the readiness check and
        // pending-map insertion. Renderer teardown takes the same lock before
        // clearing pending senders, so a prompt can never be inserted after
        // readiness has been withdrawn.
        let active = self.sudo_password_renderer_registration.write().await;
        if active.is_none() {
            return false;
        }
        self.pending_sudo_passwords
            .write()
            .await
            .insert(request_id, pending);
        true
    }

    pub async fn has_sudo_password_renderer(&self) -> bool {
        self.sudo_password_renderer_registration
            .read()
            .await
            .is_some()
    }

    pub async fn ai_session_revision(&self, tab_id: &str) -> u64 {
        self.ai_session_revisions
            .read()
            .await
            .get(tab_id)
            .copied()
            .unwrap_or_default()
    }

    pub async fn touch_ai_session_revision(&self, tab_id: &str) -> u64 {
        let mut revisions = self.ai_session_revisions.write().await;
        let revision = revisions.entry(tab_id.to_string()).or_default();
        *revision = revision.saturating_add(1);
        *revision
    }

    pub async fn remove_ai_session_revision(&self, tab_id: &str) {
        self.ai_session_revisions.write().await.remove(tab_id);
    }

    pub fn register_terminal_output_channel(&self, channel: Channel<serde_json::Value>) {
        if let Ok(mut channels) = self.terminal_output_channels.lock() {
            channels.insert(channel.id(), channel);
        }
    }

    /// Broadcast a terminal output chunk to every registered renderer channel.
    ///
    /// The std Mutex is held only long enough to clone the channel list out;
    /// the per-channel `send` (which serializes the JSON payload and pushes
    /// it through Tauri's IPC bridge) runs **outside** the lock. Holding the
    /// lock during `send` was the original cause of multi-second worker-loop
    /// stalls when the webview fell behind on high-throughput output (e.g.
    /// `pacman-key --populate`): a single slow `channel.send` blocked the
    /// Tokio worker thread, which blocked `flush_batch`, which blocked the
    /// SSH `select!` from polling `terminal_input_rx` — so Ctrl+C stopped
    /// responding until the webview caught up.
    pub fn publish_terminal_output(&self, tab_id: &str, chunk: &str) {
        let payload = serde_json::json!({ "tabId": tab_id, "chunk": chunk });
        let snapshot: Vec<Channel<serde_json::Value>> = match self.terminal_output_channels.lock() {
            Ok(channels) => channels.values().cloned().collect(),
            Err(_) => return,
        };
        let mut dead_ids: Vec<u32> = Vec::new();
        for channel in &snapshot {
            if channel.send(payload.clone()).is_err() {
                dead_ids.push(channel.id());
            }
        }
        if !dead_ids.is_empty() {
            if let Ok(mut channels) = self.terminal_output_channels.lock() {
                for id in &dead_ids {
                    channels.remove(id);
                }
            }
        }
    }
}
