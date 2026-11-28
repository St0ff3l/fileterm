use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::services::workspace::WorkspaceState;

const EVENT_BROADCAST_CAPACITY: usize = 512;
const DOWNLOAD_TOKEN_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug)]
pub struct RuntimeEvent {
    pub target_window: Option<String>,
    pub event: String,
    pub payload: Value,
}

#[derive(Clone, Debug)]
pub enum WebServerMessage {
    Event {
        event: String,
        payload: Value,
    },
    TerminalData {
        channel_id: u32,
        payload: Value,
    },
    ChannelEvent {
        channel_id: u32,
        payload: Value,
    },
    Pong,
}

pub struct StagedDownload {
    pub file_path: PathBuf,
    pub cleanup_dir: Option<PathBuf>,
    pub file_name: String,
    pub content_type: String,
    pub expires_at: Instant,
}

struct ConnectedWebClient {
    window_label: String,
    direct_tx: mpsc::Sender<WebServerMessage>,
    terminal_channel_ids: HashSet<u32>,
}

pub struct WebRuntime {
    pub workspace: WorkspaceState,
    events_tx: broadcast::Sender<RuntimeEvent>,
    clients: StdMutex<HashMap<String, ConnectedWebClient>>,
    pending_downloads: Mutex<HashMap<String, StagedDownload>>,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub home_dir: PathBuf,
}

impl WebRuntime {
    pub fn new(data_dir: PathBuf, config_dir: PathBuf, home_dir: PathBuf) -> Arc<Self> {
        let (events_tx, _) = broadcast::channel(EVENT_BROADCAST_CAPACITY);
        Arc::new(Self {
            workspace: WorkspaceState::default(),
            events_tx,
            clients: StdMutex::new(HashMap::new()),
            pending_downloads: Mutex::new(HashMap::new()),
            data_dir,
            config_dir,
            home_dir,
        })
    }

    pub fn emit_event(&self, target_window: Option<String>, event: &str, payload: Value) {
        let _ = self.events_tx.send(RuntimeEvent {
            target_window,
            event: event.to_string(),
            payload,
        });
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.events_tx.subscribe()
    }

    pub fn register_client(
        &self,
        client_id: String,
        window_label: String,
        direct_tx: mpsc::Sender<WebServerMessage>,
    ) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.insert(
                client_id,
                ConnectedWebClient {
                    window_label,
                    direct_tx,
                    terminal_channel_ids: HashSet::new(),
                },
            );
        }
    }

    pub fn track_terminal_channel(&self, client_id: &str, channel_id: u32) {
        if let Ok(mut clients) = self.clients.lock() {
            if let Some(client) = clients.get_mut(client_id) {
                client.terminal_channel_ids.insert(channel_id);
            }
        }
    }

    pub fn unregister_client(&self, client_id: &str) {
        let channel_ids = self
            .clients
            .lock()
            .ok()
            .and_then(|mut clients| clients.remove(client_id))
            .map(|client| client.terminal_channel_ids)
            .unwrap_or_default();

        if !channel_ids.is_empty() {
            if let Ok(mut channels) = self.workspace.terminal_output_channels.lock() {
                for channel_id in channel_ids {
                    if let Some(subscription) = channels.remove(&channel_id) {
                        subscription.close();
                    }
                }
            }
        }
    }

    pub fn has_connected_window(&self, label: &str) -> bool {
        self.clients
            .lock()
            .ok()
            .is_some_and(|clients| clients.values().any(|client| client.window_label == label))
    }

    pub fn send_direct(&self, client_id: &str, message: WebServerMessage) -> Result<(), String> {
        let sender = self
            .clients
            .lock()
            .map_err(|_| "Web client registry lock poisoned".to_string())?
            .get(client_id)
            .map(|client| client.direct_tx.clone())
            .ok_or_else(|| "Web client session is not connected".to_string())?;
        sender
            .try_send(message)
            .map_err(|error| format!("Web client stream closed or saturated: {error}"))
    }

    pub fn send_channel_event(
        &self,
        client_id: &str,
        channel_id: u32,
        payload: Value,
    ) -> Result<(), String> {
        self.send_direct(
            client_id,
            WebServerMessage::ChannelEvent {
                channel_id,
                payload,
            },
        )
    }

    pub async fn register_download(
        &self,
        file_path: PathBuf,
        cleanup_dir: Option<PathBuf>,
        file_name: String,
        content_type: String,
    ) -> String {
        let now = Instant::now();
        let token = uuid::Uuid::new_v4().to_string();
        let mut expired = Vec::new();
        let mut downloads = self.pending_downloads.lock().await;
        downloads.retain(|_, entry| {
            if entry.expires_at <= now {
                expired.push(entry.cleanup_dir.clone().unwrap_or_else(|| entry.file_path.clone()));
                false
            } else {
                true
            }
        });
        downloads.insert(
            token.clone(),
            StagedDownload {
                file_path,
                cleanup_dir,
                file_name,
                content_type,
                expires_at: now + DOWNLOAD_TOKEN_TTL,
            },
        );
        drop(downloads);
        for path in expired {
            remove_staged_path(path);
        }
        token
    }

    pub async fn take_download(&self, token: &str) -> Option<StagedDownload> {
        let now = Instant::now();
        let mut expired = Vec::new();
        let mut downloads = self.pending_downloads.lock().await;
        downloads.retain(|_, entry| {
            if entry.expires_at <= now {
                expired.push(entry.cleanup_dir.clone().unwrap_or_else(|| entry.file_path.clone()));
                false
            } else {
                true
            }
        });
        let entry = downloads.remove(token);
        drop(downloads);
        for path in expired {
            remove_staged_path(path);
        }
        entry
    }
}

pub fn remove_staged_path(path: PathBuf) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}
