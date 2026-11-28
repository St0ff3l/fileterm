use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::runtime::{StreamChannel, WebServerMessage};
use crate::web::WebServerState;

const WS_OUTBOX_CAPACITY: usize = 256;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsQuery {
    pub client_id: Option<String>,
    pub window_label: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientWsMessage {
    SubscribeTerminal,
    AckTerminal {
        #[serde(rename = "channelId")]
        channel_id: u32,
        #[serde(rename = "deliveryId")]
        delivery_id: u64,
    },
    Ping,
}

pub async fn handle_ws(
    State(state): State<WebServerState>,
    Query(query): Query<WsQuery>,
    session: Option<axum::Extension<tokio::sync::watch::Receiver<String>>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let client_id = query
        .client_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let window_label = query
        .window_label
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());

    ws.on_upgrade(move |socket| {
        run_ws_session(
            state,
            client_id,
            window_label,
            socket,
            session.map(|session| session.0),
        )
    })
}

fn serialize_server_message(message: WebServerMessage) -> Option<String> {
    let value = match message {
        WebServerMessage::Event { event, payload } => json!({
            "type": "event",
            "event": event,
            "payload": payload,
        }),
        WebServerMessage::TerminalData {
            channel_id,
            payload,
        } => json!({
            "type": "terminal_data",
            "channelId": channel_id,
            "payload": payload,
        }),
        WebServerMessage::ChannelEvent {
            channel_id,
            payload,
        } => json!({
            "type": "channel",
            "channelId": channel_id,
            "payload": payload,
        }),
        WebServerMessage::Pong => json!({ "type": "pong" }),
    };
    serde_json::to_string(&value).ok()
}

async fn run_ws_session(
    state: WebServerState,
    client_id: String,
    window_label: String,
    socket: WebSocket,
    mut session: Option<tokio::sync::watch::Receiver<String>>,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (direct_tx, mut direct_rx) = mpsc::channel::<WebServerMessage>(WS_OUTBOX_CAPACITY);

    state
        .runtime
        .register_client(client_id.clone(), window_label.clone(), direct_tx.clone());
    let mut events_rx = state.runtime.subscribe_events();
    let writer_window_label = window_label.clone();

    let writer_task = tokio::spawn(async move {
        loop {
            let outbound = tokio::select! {
                direct = direct_rx.recv() => {
                    match direct {
                        Some(msg) => serialize_server_message(msg),
                        None => break,
                    }
                }
                event = events_rx.recv() => {
                    match event {
                        Ok(runtime_event) => {
                            let target_matches = runtime_event
                                .target_window
                                .as_deref()
                                .is_none_or(|target| target == writer_window_label);
                            if target_matches {
                                serialize_server_message(WebServerMessage::Event {
                                    event: runtime_event.event,
                                    payload: runtime_event.payload,
                                })
                            } else {
                                None
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            serialize_server_message(WebServerMessage::Event {
                                event: "web:events-lagged".to_string(),
                                payload: Value::Null,
                            })
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            };

            if let Some(text) = outbound {
                if ws_sender.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    loop {
        let frame_result = tokio::select! {
            biased;
            _ = async {
                match &mut session {
                    Some(session) => { let _ = session.changed().await; }
                    None => std::future::pending().await,
                }
            } => break,
            frame = ws_receiver.next() => match frame {
                Some(frame) => frame,
                None => break,
            },
        };
        let Ok(frame) = frame_result else {
            break;
        };
        match frame {
            Message::Text(text) => {
                let Ok(message) = serde_json::from_str::<ClientWsMessage>(&text) else {
                    continue;
                };
                match message {
                    ClientWsMessage::SubscribeTerminal => {
                        let stream_tx = direct_tx.clone();
                        let channel_id = StreamChannel::<Value>::allocate_id();
                        let channel = StreamChannel::from_callback(channel_id, move |payload| {
                            stream_tx
                                .try_send(WebServerMessage::TerminalData {
                                    channel_id,
                                    payload,
                                })
                                .map_err(|error| error.to_string())
                        });
                        // Notify the browser client before registering so it owns `channelId` ahead of queued output frames.
                        let _ = direct_tx.try_send(WebServerMessage::Event {
                            event: "web:terminal-subscribed".to_string(),
                            payload: json!({ "channelId": channel_id }),
                        });
                        let registered_id = crate::commands::app_subscribe_terminal_data(
                            state.app.clone(),
                            client_id.clone(),
                            channel,
                        );
                        debug_assert_eq!(registered_id, channel_id);
                        state
                            .runtime
                            .track_terminal_channel(&client_id, registered_id);
                    }
                    ClientWsMessage::AckTerminal {
                        channel_id,
                        delivery_id,
                    } => {
                        crate::commands::app_ack_terminal_data(
                            state.app.clone(),
                            client_id.clone(),
                            channel_id,
                            delivery_id,
                        );
                    }
                    ClientWsMessage::Ping => {
                        let _ = direct_tx.try_send(WebServerMessage::Pong);
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer_task.abort();
    state.runtime.unregister_client(&client_id);
}
