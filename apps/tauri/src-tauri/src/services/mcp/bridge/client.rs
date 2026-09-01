use super::super::{
    runtime_descriptor_path, BridgeProgress, BridgeRequest, BridgeResponse, RuntimeDescriptor,
    AGENT_CANCEL_POLL_INTERVAL, MCP_BRIDGE_TIMEOUT, MCP_CLIENT_TIMEOUT, MCP_MAX_MESSAGE_BYTES,
    MCP_PROTOCOL_VERSION,
};
use super::wire::BridgeFrame;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

const BRIDGE_WRITER_QUEUE_SIZE: usize = 128;
const BRIDGE_PENDING_EVENT_QUEUE_SIZE: usize = 32;
const BRIDGE_RECONNECT_DELAYS: [Duration; 4] = [
    Duration::from_millis(100),
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
];
const BRIDGE_DISCONNECTED: &str = "FILETERM_MCP_BRIDGE_DISCONNECTED";
const BRIDGE_BACKPRESSURE: &str = "FILETERM_MCP_BRIDGE_BACKPRESSURE";

#[derive(Debug)]
enum BridgeEvent {
    Progress(BridgeProgress),
    Response(BridgeResponse),
    Disconnected(String),
}

type PendingSender = SyncSender<BridgeEvent>;

/// A process-scoped, multiplexed client for the desktop-owned bridge.
///
/// The client owns at most one live TCP session. Calls from the MCP/CLI worker
/// pool share the session and are routed by an internal request id. A failed
/// in-flight call is never replayed; the next call performs a single-flight
/// reconnect instead.
pub(crate) struct BridgeClient {
    client_id: String,
    next_request_id: AtomicU64,
    connection: Mutex<Option<Arc<BridgeConnection>>>,
    reconnect_lock: Mutex<()>,
}

impl BridgeClient {
    pub(crate) fn new() -> Self {
        Self {
            client_id: format!("fileterm-{}", Uuid::new_v4().simple()),
            next_request_id: AtomicU64::new(1),
            connection: Mutex::new(None),
            reconnect_lock: Mutex::new(()),
        }
    }

    pub(crate) fn call<F>(
        &self,
        request: BridgeRequest,
        on_progress: &mut F,
        cancellation: Option<&AtomicBool>,
    ) -> Result<Value, String>
    where
        F: FnMut(&BridgeProgress),
    {
        if cancellation_requested(cancellation) {
            return Err(super::super::FILETERM_CLI_JSONL_REQUEST_CANCELLED.to_string());
        }

        let connection = self.ensure_connection()?;
        let request_id = self.next_request_id();
        let (events_tx, events_rx) = mpsc::sync_channel(BRIDGE_PENDING_EVENT_QUEUE_SIZE);
        connection.register_pending(request_id.clone(), events_tx)?;

        if let Err(error) = connection.send(BridgeFrame::Request {
            request_id: request_id.clone(),
            request,
        }) {
            connection.remove_pending(&request_id);
            self.clear_connection_if_current(&connection);
            return Err(error);
        }

        let deadline = Instant::now() + MCP_CLIENT_TIMEOUT;
        loop {
            if cancellation_requested(cancellation) {
                connection.cancel(&request_id);
                connection.remove_pending(&request_id);
                return Err(super::super::FILETERM_CLI_JSONL_REQUEST_CANCELLED.to_string());
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                connection.cancel(&request_id);
                connection.remove_pending(&request_id);
                return Err(
                    "FileTerm did not respond to the MCP request. Retry shortly.".to_string(),
                );
            }
            let wait_for = cancellation
                .map(|_| remaining.min(AGENT_CANCEL_POLL_INTERVAL))
                .unwrap_or(remaining);
            match events_rx.recv_timeout(wait_for) {
                Ok(BridgeEvent::Progress(progress)) => on_progress(&progress),
                Ok(BridgeEvent::Response(response)) => {
                    let result = if response.ok {
                        response
                            .result
                            .ok_or_else(|| "FileTerm returned an empty MCP response.".to_string())
                    } else {
                        Err(response.error.unwrap_or_else(|| {
                            "FileTerm could not complete the MCP request.".to_string()
                        }))
                    };
                    return result;
                }
                Ok(BridgeEvent::Disconnected(reason)) => {
                    self.clear_connection_if_current(&connection);
                    return Err(reason);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if cancellation.is_none() {
                        connection.cancel(&request_id);
                        connection.remove_pending(&request_id);
                        return Err(
                            "FileTerm did not respond to the MCP request. Retry shortly."
                                .to_string(),
                        );
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.clear_connection_if_current(&connection);
                    return Err(format!("{BRIDGE_DISCONNECTED}: bridge reader stopped"));
                }
            }
        }
    }

    pub(crate) fn close(&self) {
        let connection = self
            .connection
            .lock()
            .ok()
            .and_then(|mut connection| connection.take());
        if let Some(connection) = connection {
            connection.close();
        }
    }

    fn ensure_connection(&self) -> Result<Arc<BridgeConnection>, String> {
        if let Some(connection) = self.current_connection() {
            return Ok(connection);
        }

        let _reconnect_guard = self
            .reconnect_lock
            .lock()
            .map_err(|_| "FileTerm MCP bridge recovery is unavailable".to_string())?;
        if let Some(connection) = self.current_connection() {
            return Ok(connection);
        }

        let mut last_error = None;
        for (attempt, delay) in BRIDGE_RECONNECT_DELAYS.iter().enumerate() {
            if attempt > 0 {
                thread::sleep(*delay);
            }
            match connect_bridge(&self.client_id) {
                Ok(connection) => {
                    if let Ok(mut current) = self.connection.lock() {
                        *current = Some(Arc::clone(&connection));
                    }
                    return Ok(connection);
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| format!("{BRIDGE_DISCONNECTED}: unable to connect")))
    }

    fn current_connection(&self) -> Option<Arc<BridgeConnection>> {
        self.connection
            .lock()
            .ok()
            .and_then(|connection| connection.as_ref().cloned())
            .filter(|connection| !connection.is_closed())
    }

    fn clear_connection_if_current(&self, connection: &Arc<BridgeConnection>) {
        if let Ok(mut current) = self.connection.lock() {
            if current
                .as_ref()
                .is_some_and(|candidate| Arc::ptr_eq(candidate, connection))
            {
                *current = None;
            }
        }
    }

    fn next_request_id(&self) -> String {
        let sequence = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        format!("{}-{sequence}", self.client_id)
    }
}

impl Drop for BridgeClient {
    fn drop(&mut self) {
        self.close();
    }
}

struct BridgeConnection {
    writer: Mutex<Option<SyncSender<BridgeFrame>>>,
    pending: Mutex<HashMap<String, PendingSender>>,
    closed: AtomicBool,
    control_stream: Mutex<Option<TcpStream>>,
    reader_thread: Mutex<Option<JoinHandle<()>>>,
    writer_thread: Mutex<Option<JoinHandle<()>>>,
}

impl BridgeConnection {
    fn register_pending(&self, request_id: String, sender: PendingSender) -> Result<(), String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| format!("{BRIDGE_DISCONNECTED}: pending registry unavailable"))?;
        if self.is_closed() {
            return Err(format!("{BRIDGE_DISCONNECTED}: bridge session is closed"));
        }
        if pending.insert(request_id, sender).is_some() {
            return Err(format!(
                "{BRIDGE_DISCONNECTED}: duplicate bridge request id"
            ));
        }
        Ok(())
    }

    fn remove_pending(&self, request_id: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(request_id);
        }
    }

    fn send(&self, frame: BridgeFrame) -> Result<(), String> {
        if self.is_closed() {
            return Err(format!("{BRIDGE_DISCONNECTED}: bridge session is closed"));
        }
        let writer = self
            .writer
            .lock()
            .map_err(|_| format!("{BRIDGE_DISCONNECTED}: bridge writer unavailable"))?;
        let Some(writer) = writer.as_ref() else {
            return Err(format!("{BRIDGE_DISCONNECTED}: bridge writer is closed"));
        };
        match writer.try_send(frame) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(format!(
                "{BRIDGE_BACKPRESSURE}: bridge writer queue is full; retry shortly"
            )),
            Err(TrySendError::Disconnected(_)) => {
                Err(format!("{BRIDGE_DISCONNECTED}: bridge writer stopped"))
            }
        }
    }

    fn cancel(&self, request_id: &str) {
        let _ = self.send(BridgeFrame::Cancel {
            request_id: request_id.to_string(),
        });
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn mark_disconnected(&self, reason: String) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
        if let Ok(mut stream) = self.control_stream.lock() {
            if let Some(stream) = stream.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        let pending = self
            .pending
            .lock()
            .map(|mut pending| {
                pending
                    .drain()
                    .map(|(_, sender)| sender)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for sender in pending {
            let _ = sender.try_send(BridgeEvent::Disconnected(reason.clone()));
        }
    }

    fn close(&self) {
        if !self.is_closed() {
            let _ = self.send(BridgeFrame::Close);
        }
        self.mark_disconnected(format!("{BRIDGE_DISCONNECTED}: bridge session closed"));
        join_thread(&self.reader_thread);
        join_thread(&self.writer_thread);
    }
}

fn connect_bridge(client_id: &str) -> Result<Arc<BridgeConnection>, String> {
    let descriptor = load_runtime_descriptor()?;
    connect_bridge_to_endpoint(client_id, &descriptor)
}

fn connect_bridge_to_endpoint(
    client_id: &str,
    descriptor: &RuntimeDescriptor,
) -> Result<Arc<BridgeConnection>, String> {
    let address: SocketAddr = descriptor.address.parse().map_err(|_| {
        "FileTerm MCP runtime address is invalid. Restart FileTerm, then retry this MCP tool."
            .to_string()
    })?;
    if !address.ip().is_loopback() {
        return Err("FileTerm MCP rejected a non-local runtime address.".to_string());
    }

    let stream = TcpStream::connect_timeout(&address, MCP_BRIDGE_TIMEOUT).map_err(|_| {
        "FileTerm desktop app is unavailable. Open or restart FileTerm, then retry this MCP tool."
            .to_string()
    })?;
    stream
        .set_nodelay(true)
        .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    let mut handshake_writer = BufWriter::new(
        stream
            .try_clone()
            .map_err(|_| "Unable to initialize FileTerm MCP connection".to_string())?,
    );
    handshake_writer
        .get_ref()
        .set_write_timeout(Some(MCP_BRIDGE_TIMEOUT))
        .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    write_sync_frame(
        &mut handshake_writer,
        &BridgeFrame::Hello {
            protocol_version: MCP_PROTOCOL_VERSION,
            token: descriptor.token.clone(),
            client_id: client_id.to_string(),
        },
    )?;

    let mut handshake_reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|_| "Unable to initialize FileTerm MCP connection".to_string())?,
    );
    handshake_reader
        .get_ref()
        .set_read_timeout(Some(MCP_BRIDGE_TIMEOUT))
        .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    let ack = read_sync_frame(&mut handshake_reader)?;
    match ack {
        BridgeFrame::HelloAck {
            protocol_version,
            session_id,
            error,
        } if protocol_version == MCP_PROTOCOL_VERSION
            && error.is_none()
            && !session_id.is_empty() => {}
        BridgeFrame::HelloAck { error, .. } => {
            return Err(error.unwrap_or_else(|| {
                "FileTerm MCP bridge protocol version is unsupported".to_string()
            }))
        }
        _ => return Err("FileTerm MCP bridge returned an invalid handshake".to_string()),
    }

    let reader_stream = stream
        .try_clone()
        .map_err(|_| "Unable to initialize FileTerm MCP reader".to_string())?;
    let writer_stream = stream
        .try_clone()
        .map_err(|_| "Unable to initialize FileTerm MCP writer".to_string())?;
    let control_stream = stream
        .try_clone()
        .map_err(|_| "Unable to initialize FileTerm MCP control".to_string())?;
    for socket in [&stream, &reader_stream, &writer_stream, &control_stream] {
        socket
            .set_read_timeout(None)
            .and_then(|()| socket.set_write_timeout(None))
            .map_err(|_| "Unable to configure FileTerm MCP connection".to_string())?;
    }

    let (writer_sender, writer_receiver) = mpsc::sync_channel(BRIDGE_WRITER_QUEUE_SIZE);
    let connection = BridgeConnection {
        writer: Mutex::new(Some(writer_sender)),
        pending: Mutex::new(HashMap::new()),
        closed: AtomicBool::new(false),
        control_stream: Mutex::new(Some(control_stream)),
        reader_thread: Mutex::new(None),
        writer_thread: Mutex::new(None),
    };
    let connection = Arc::new(connection);
    let reader_connection = Arc::clone(&connection);
    let reader_thread = thread::Builder::new()
        .name("fileterm-mcp-bridge-reader".to_string())
        .spawn(move || reader_loop(reader_stream, reader_connection))
        .map_err(|_| "Unable to start FileTerm MCP bridge reader".to_string())?;
    let writer_connection = Arc::clone(&connection);
    let writer_thread = thread::Builder::new()
        .name("fileterm-mcp-bridge-writer".to_string())
        .spawn(move || writer_loop(writer_stream, writer_receiver, writer_connection))
        .map_err(|_| "Unable to start FileTerm MCP bridge writer".to_string())?;
    if let Ok(mut reader) = connection.reader_thread.lock() {
        *reader = Some(reader_thread);
    }
    if let Ok(mut writer) = connection.writer_thread.lock() {
        *writer = Some(writer_thread);
    }

    Ok(connection)
}

fn reader_loop(stream: TcpStream, connection: Arc<BridgeConnection>) {
    let mut reader = BufReader::new(stream);
    loop {
        match read_sync_frame(&mut reader) {
            Ok(BridgeFrame::Progress {
                request_id,
                progress,
            }) => {
                let sender = connection
                    .pending
                    .lock()
                    .ok()
                    .and_then(|pending| pending.get(&request_id).cloned());
                if let Some(sender) = sender {
                    let _ = sender.try_send(BridgeEvent::Progress(progress));
                }
            }
            Ok(BridgeFrame::Response {
                request_id,
                response,
            }) => {
                let sender = connection
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.remove(&request_id));
                if let Some(sender) = sender {
                    let _ = sender.send(BridgeEvent::Response(response));
                }
            }
            Ok(BridgeFrame::Pong { .. }) => {}
            Ok(BridgeFrame::Close) => {
                connection.mark_disconnected(format!(
                    "{BRIDGE_DISCONNECTED}: desktop bridge closed the session"
                ));
                return;
            }
            Ok(_) => {
                connection.mark_disconnected(format!(
                    "{BRIDGE_DISCONNECTED}: invalid frame from desktop bridge"
                ));
                return;
            }
            Err(error) => {
                connection.mark_disconnected(format!("{BRIDGE_DISCONNECTED}: {error}"));
                return;
            }
        }
    }
}

fn writer_loop(
    stream: TcpStream,
    receiver: Receiver<BridgeFrame>,
    connection: Arc<BridgeConnection>,
) {
    let mut writer = BufWriter::new(stream);
    while let Ok(frame) = receiver.recv() {
        if let Err(error) = write_sync_frame(&mut writer, &frame) {
            connection.mark_disconnected(format!("{BRIDGE_DISCONNECTED}: {error}"));
            return;
        }
        if matches!(frame, BridgeFrame::Close) {
            return;
        }
    }
}

fn load_runtime_descriptor() -> Result<RuntimeDescriptor, String> {
    let runtime_path = runtime_descriptor_path()?;
    let descriptor_content = std::fs::read_to_string(&runtime_path).map_err(|_| {
        "FileTerm desktop app is not running. Open FileTerm, then retry this MCP tool.".to_string()
    })?;
    let descriptor: RuntimeDescriptor = serde_json::from_str(&descriptor_content).map_err(|_| {
        "FileTerm MCP runtime information is invalid. Restart FileTerm, then retry this MCP tool."
            .to_string()
    })?;
    if descriptor.protocol_version != MCP_PROTOCOL_VERSION || descriptor.token.is_empty() {
        return Err(
            "FileTerm MCP runtime version is unsupported. Restart FileTerm and retry.".to_string(),
        );
    }
    Ok(descriptor)
}

fn write_sync_frame<W: Write>(writer: &mut W, frame: &BridgeFrame) -> Result<(), String> {
    let payload = serde_json::to_vec(frame)
        .map_err(|_| "Unable to encode FileTerm MCP bridge frame".to_string())?;
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP bridge frame exceeds the size limit".to_string());
    }
    writer
        .write_all(&payload)
        .and_then(|()| writer.write_all(b"\n"))
        .and_then(|()| writer.flush())
        .map_err(|_| "Unable to write FileTerm MCP bridge frame".to_string())
}

fn read_sync_frame<R: BufRead>(reader: &mut R) -> Result<BridgeFrame, String> {
    let mut line = String::new();
    let count = reader
        .read_line(&mut line)
        .map_err(|_| "Unable to read FileTerm MCP bridge frame".to_string())?;
    if count == 0 {
        return Err("bridge connection reached EOF".to_string());
    }
    if line.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP bridge frame exceeds the size limit".to_string());
    }
    serde_json::from_str(&line).map_err(|_| "invalid FileTerm MCP bridge frame".to_string())
}

fn cancellation_requested(cancellation: Option<&AtomicBool>) -> bool {
    cancellation.is_some_and(|cancellation| cancellation.load(Ordering::Acquire))
}

fn join_thread(handle: &Mutex<Option<JoinHandle<()>>>) {
    let current = thread::current().id();
    let Some(handle) = handle.lock().ok().and_then(|mut handle| handle.take()) else {
        return;
    };
    if handle.thread().id() != current {
        let _ = handle.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::TcpListener;

    fn request(value: &str) -> BridgeRequest {
        BridgeRequest {
            action: "test_bridge_request".to_string(),
            params: json!({ "value": value }),
            source: super::super::super::WorkspaceSessionSource::Cli,
            requires_approval: false,
            progress_token: None,
        }
    }

    #[test]
    fn one_bridge_session_routes_multiple_out_of_order_responses() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should expose an address");
        let descriptor = RuntimeDescriptor {
            protocol_version: MCP_PROTOCOL_VERSION,
            address: address.to_string(),
            token: "test-token".to_string(),
        };
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("test server should accept once");
            let reader_stream = stream.try_clone().expect("test server should clone reader");
            let mut reader = BufReader::new(reader_stream);
            let mut writer = BufWriter::new(stream);
            match read_sync_frame(&mut reader).expect("test server should receive hello") {
                BridgeFrame::Hello { .. } => {}
                frame => panic!("unexpected hello frame: {frame:?}"),
            }
            write_sync_frame(
                &mut writer,
                &BridgeFrame::HelloAck {
                    protocol_version: MCP_PROTOCOL_VERSION,
                    session_id: "test-session".to_string(),
                    error: None,
                },
            )
            .expect("test server should acknowledge hello");

            let mut requests = Vec::new();
            while requests.len() < 2 {
                match read_sync_frame(&mut reader).expect("test server should receive request") {
                    BridgeFrame::Request {
                        request_id,
                        request,
                    } => requests.push((request_id, request)),
                    frame => panic!("unexpected request frame: {frame:?}"),
                }
            }

            for (request_id, request) in requests.into_iter().rev() {
                write_sync_frame(
                    &mut writer,
                    &BridgeFrame::Response {
                        request_id,
                        response: BridgeResponse::success(request.params),
                    },
                )
                .expect("test server should write response");
            }
        });

        let connection = connect_bridge_to_endpoint("test-client", &descriptor)
            .expect("test client should connect");
        let client = Arc::new(BridgeClient::new());
        *client
            .connection
            .lock()
            .expect("test client connection lock should work") = Some(connection);

        let first_client = Arc::clone(&client);
        let first = thread::spawn(move || {
            let mut progress = |_progress: &BridgeProgress| {};
            first_client.call(request("first"), &mut progress, None)
        });
        let second_client = Arc::clone(&client);
        let second = thread::spawn(move || {
            let mut progress = |_progress: &BridgeProgress| {};
            second_client.call(request("second"), &mut progress, None)
        });

        assert_eq!(
            first.join().expect("first call should join").unwrap(),
            json!({"value": "first"})
        );
        assert_eq!(
            second.join().expect("second call should join").unwrap(),
            json!({"value": "second"})
        );
        client.close();
        server.join().expect("test server should join");
    }
}
