//! Runtime coordination for externally requested connection opens.
//!
//! The desktop worker owns all connection credentials. This registry only
//! carries a short-lived operation id, the saved profile id, the spawned tab
//! id, and a non-secret terminal state so CLI/MCP callers can wait for the
//! result without polling the renderer or receiving a password.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use tokio::sync::{watch, RwLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionInteraction {
    request_id: String,
    pub kind: &'static str,
    pub status: &'static str,
}

pub const FILETERM_CONNECTION_FAILED: &str = "FILETERM_CONNECTION_FAILED";
pub const FILETERM_CONNECTION_WAIT_TIMEOUT: &str = "FILETERM_CONNECTION_WAIT_TIMEOUT";
pub const SSH_CREDENTIALS_NEEDED: &str = "SSH_CREDENTIALS_NEEDED";
pub const SSH_CREDENTIALS_CANCELLED: &str = "SSH_CREDENTIALS_CANCELLED";
pub const SSH_CREDENTIALS_TIMEOUT: &str = "SSH_CREDENTIALS_TIMEOUT";
pub const SSH_AUTH_FAILURE: &str = "SSH_AUTH_FAILURE";

const OPERATION_RETENTION: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionOperationState {
    Pending,
    Connecting,
    Connected,
    Failed { code: String },
}

impl ConnectionOperationState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Connected | Self::Failed { .. })
    }
}

pub struct ConnectionOperationHandle {
    pub id: String,
    pub profile_id: String,
    pub receiver: watch::Receiver<ConnectionOperationState>,
}

pub struct ConnectionOperationInfo {
    pub id: String,
    pub profile_id: String,
    pub tab_id: Option<String>,
    pub receiver: watch::Receiver<ConnectionOperationState>,
    pub interactions: Vec<ConnectionInteraction>,
}

struct OperationRecord {
    profile_id: String,
    tab_id: Option<String>,
    sender: watch::Sender<ConnectionOperationState>,
    updated_at: Instant,
    interactions: Vec<ConnectionInteraction>,
}

#[derive(Default)]
struct RegistryState {
    records: HashMap<String, OperationRecord>,
    tab_to_operation: HashMap<String, String>,
    profile_to_operation: HashMap<String, String>,
}

/// Keeps a bounded, event-driven view of connection attempts started for an
/// external client. Completed records remain briefly so a caller can resume a
/// wait after its original stdio/socket deadline has elapsed.
#[derive(Default)]
pub struct ConnectionOperationRegistry {
    state: RwLock<RegistryState>,
}

impl ConnectionOperationRegistry {
    pub async fn begin(&self, profile_id: impl Into<String>) -> ConnectionOperationHandle {
        let profile_id = profile_id.into();
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);
        Self::insert_new_operation(&mut state, profile_id)
    }

    /// Return the existing active operation for a profile when one is already
    /// in flight or still owns a connected tab. Otherwise reserve the profile
    /// and create one new operation. This is the single-flight boundary used
    /// by external Agent opens, so four concurrent requests cannot create four
    /// tabs while the first request is waiting for an SSH credential prompt.
    pub async fn begin_or_join(
        &self,
        profile_id: impl Into<String>,
    ) -> (ConnectionOperationHandle, bool) {
        let profile_id = profile_id.into();
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);

        if let Some(operation_id) = state.profile_to_operation.get(&profile_id).cloned() {
            let can_reuse = state.records.get(&operation_id).is_some_and(|record| {
                !matches!(
                    *record.sender.borrow(),
                    ConnectionOperationState::Failed { .. }
                )
            });
            if can_reuse {
                let record = state
                    .records
                    .get(&operation_id)
                    .expect("reusable connection operation must exist");
                return (
                    ConnectionOperationHandle {
                        id: operation_id,
                        profile_id: record.profile_id.clone(),
                        receiver: record.sender.subscribe(),
                    },
                    false,
                );
            }
            state.profile_to_operation.remove(&profile_id);
        }

        let handle = Self::insert_new_operation(&mut state, profile_id.clone());
        state
            .profile_to_operation
            .insert(profile_id, handle.id.clone());
        (handle, true)
    }

    fn insert_new_operation(
        state: &mut RegistryState,
        profile_id: String,
    ) -> ConnectionOperationHandle {
        let id = format!("connection-{}", uuid::Uuid::new_v4());
        let (sender, receiver) = watch::channel(ConnectionOperationState::Pending);
        let handle = ConnectionOperationHandle {
            id: id.clone(),
            profile_id: profile_id.clone(),
            receiver,
        };
        let record = OperationRecord {
            profile_id,
            tab_id: None,
            sender,
            updated_at: Instant::now(),
            interactions: Vec::new(),
        };
        state.records.insert(id, record);
        handle
    }

    pub async fn attach_tab(&self, operation_id: &str, tab_id: &str) -> Result<(), String> {
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);
        let record = state
            .records
            .get_mut(operation_id)
            .ok_or_else(|| "Connection operation was not found".to_string())?;
        record.tab_id = Some(tab_id.to_string());
        record.updated_at = Instant::now();
        record
            .sender
            .send_replace(ConnectionOperationState::Connecting);
        state
            .tab_to_operation
            .insert(tab_id.to_string(), operation_id.to_string());
        Ok(())
    }

    pub async fn info(&self, operation_id: &str) -> Result<ConnectionOperationInfo, String> {
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);
        let record = state
            .records
            .get(operation_id)
            .ok_or_else(|| "Connection operation was not found".to_string())?;
        Ok(ConnectionOperationInfo {
            id: operation_id.to_string(),
            profile_id: record.profile_id.clone(),
            tab_id: record.tab_id.clone(),
            receiver: record.sender.subscribe(),
            interactions: record.interactions.clone(),
        })
    }

    /// Keep only interaction type and outcome; no prompt text or response values.
    pub async fn request_interaction(&self, tab_id: &str, request_id: &str, kind: &str) {
        let kind = match kind {
            "credentials" => "credentials",
            "host-verification" => "host-verification",
            "key-passphrase" => "key-passphrase",
            "keyboard-interactive" => "keyboard-interactive",
            _ => return,
        };
        let mut state = self.state.write().await;
        let Some(operation_id) = state.tab_to_operation.get(tab_id).cloned() else {
            return;
        };
        let Some(record) = state.records.get_mut(&operation_id) else {
            return;
        };
        if record.sender.borrow().is_terminal() {
            return;
        }
        // Keep the complete normal handshake bounded below the bridge event queue.
        if record.interactions.len() == 16 {
            record.interactions.push(ConnectionInteraction {
                request_id: String::new(),
                kind: "other",
                status: "history-truncated",
            });
        } else if record.interactions.len() < 16 {
            record.interactions.push(ConnectionInteraction {
                request_id: request_id.to_string(),
                kind,
                status: "requested",
            });
        }
        record.sender.send_modify(|_| {});
    }

    pub async fn resolve_interaction(&self, request_id: &str, cancelled: bool) {
        let mut state = self.state.write().await;
        for record in state.records.values_mut() {
            if let Some(interaction) = record
                .interactions
                .iter_mut()
                .find(|item| item.request_id == request_id)
            {
                interaction.status = if cancelled {
                    "cancelled"
                } else {
                    "input-completed"
                };
                record.sender.send_modify(|_| {});
                break;
            }
        }
    }

    pub async fn publish_for_tab(&self, tab_id: &str, next: ConnectionOperationState) {
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);
        let Some(operation_id) = state.tab_to_operation.get(tab_id).cloned() else {
            return;
        };
        let (profile_id, remove_profile_flight) = {
            let Some(record) = state.records.get_mut(&operation_id) else {
                state.tab_to_operation.remove(tab_id);
                return;
            };

            // A connection operation is single-flight. Once the initial attempt
            // has connected or failed, later disconnect/reconnect events must not
            // rewrite the result that the original caller is waiting for.
            if record.sender.borrow().is_terminal() {
                return;
            }
            let remove_profile_flight = matches!(&next, ConnectionOperationState::Failed { .. });
            record.updated_at = Instant::now();
            record.sender.send_replace(next);
            (record.profile_id.clone(), remove_profile_flight)
        };
        if remove_profile_flight
            && state.profile_to_operation.get(&profile_id) == Some(&operation_id)
        {
            state.profile_to_operation.remove(&profile_id);
        }
    }

    pub async fn fail_for_tab(&self, tab_id: &str, code: &'static str) {
        self.publish_for_tab(
            tab_id,
            ConnectionOperationState::Failed {
                code: code.to_string(),
            },
        )
        .await;
    }

    pub async fn fail_for_operation(&self, operation_id: &str, code: &'static str) {
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);
        let Some(record) = state.records.get_mut(operation_id) else {
            return;
        };
        if record.sender.borrow().is_terminal() {
            return;
        }
        let profile_id = record.profile_id.clone();
        record.updated_at = Instant::now();
        record
            .sender
            .send_replace(ConnectionOperationState::Failed {
                code: code.to_string(),
            });
        if state
            .profile_to_operation
            .get(&profile_id)
            .is_some_and(|id| id == operation_id)
        {
            state.profile_to_operation.remove(&profile_id);
        }
    }

    /// Stop reusing a tab that the user disconnected or closed. Pending
    /// operations are failed so external waiters do not sit until the full
    /// deadline after the visible session has already gone away.
    pub async fn forget_tab(&self, tab_id: &str) {
        let mut state = self.state.write().await;
        Self::prune_expired(&mut state);
        let Some(operation_id) = state.tab_to_operation.remove(tab_id) else {
            return;
        };
        let Some(record) = state.records.get_mut(&operation_id) else {
            return;
        };
        let profile_id = record.profile_id.clone();
        if !record.sender.borrow().is_terminal() {
            record.updated_at = Instant::now();
            record
                .sender
                .send_replace(ConnectionOperationState::Failed {
                    code: FILETERM_CONNECTION_FAILED.to_string(),
                });
        }
        if state.profile_to_operation.get(&profile_id) == Some(&operation_id) {
            state.profile_to_operation.remove(&profile_id);
        }
    }

    fn prune_expired(state: &mut RegistryState) {
        let now = Instant::now();
        let expired = state
            .records
            .iter()
            .filter(|(_, record)| {
                record.sender.borrow().is_terminal()
                    && now.duration_since(record.updated_at) >= OPERATION_RETENTION
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for operation_id in expired {
            if let Some(record) = state.records.remove(&operation_id) {
                if state.profile_to_operation.get(&record.profile_id) == Some(&operation_id) {
                    state.profile_to_operation.remove(&record.profile_id);
                }
                if let Some(tab_id) = record.tab_id {
                    if state.tab_to_operation.get(&tab_id) == Some(&operation_id) {
                        state.tab_to_operation.remove(&tab_id);
                    }
                }
            }
        }
    }
}

/// Convert internal SSH errors into a stable, non-secret operation code.
pub fn ssh_connection_error_code(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("credentials request canceled") {
        SSH_CREDENTIALS_CANCELLED
    } else if lower.contains("credentials request timed out") {
        SSH_CREDENTIALS_TIMEOUT
    } else if lower.contains("username and password are required")
        || lower.contains("credentials are required")
    {
        SSH_CREDENTIALS_NEEDED
    } else if lower.contains("authentication")
        || lower.contains("auth failed")
        || lower.contains("permission denied")
    {
        SSH_AUTH_FAILURE
    } else {
        FILETERM_CONNECTION_FAILED
    }
}

#[cfg(test)]
#[path = "connection_operations/tests/authentication_interaction_tests.rs"]
mod authentication_interaction_tests;
#[cfg(test)]
#[path = "connection_operations/tests/mod.rs"]
mod tests;
