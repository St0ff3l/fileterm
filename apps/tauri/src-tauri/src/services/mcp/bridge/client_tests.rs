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
            let value = request
                .params
                .get("value")
                .and_then(Value::as_str)
                .expect("test request should carry a value");
            write_sync_frame(
                &mut writer,
                &BridgeFrame::Progress {
                    request_id: request_id.clone(),
                    progress: BridgeProgress {
                        kind: "progress".to_string(),
                        event: "test-progress".to_string(),
                        status: "working".to_string(),
                        code: "TEST_PROGRESS".to_string(),
                        message: format!("progress-{value}"),
                        progress_token: None,
                    },
                },
            )
            .expect("test server should write progress");
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
    let first_progress = Arc::new(Mutex::new(Vec::new()));
    let first_progress_for_call = Arc::clone(&first_progress);
    let first = thread::spawn(move || {
        let mut progress = |progress: &BridgeProgress| {
            first_progress_for_call
                .lock()
                .expect("first progress lock should work")
                .push(progress.message.clone());
        };
        first_client.call(request("first"), &mut progress, None)
    });
    let second_client = Arc::clone(&client);
    let second_progress = Arc::new(Mutex::new(Vec::new()));
    let second_progress_for_call = Arc::clone(&second_progress);
    let second = thread::spawn(move || {
        let mut progress = |progress: &BridgeProgress| {
            second_progress_for_call
                .lock()
                .expect("second progress lock should work")
                .push(progress.message.clone());
        };
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
    assert_eq!(
        *first_progress
            .lock()
            .expect("first progress lock should work"),
        vec!["progress-first".to_string()]
    );
    assert_eq!(
        *second_progress
            .lock()
            .expect("second progress lock should work"),
        vec!["progress-second".to_string()]
    );
    client.close();
    server.join().expect("test server should join");
}

#[test]
fn cancellation_is_routed_to_the_matching_bridge_request() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should expose an address");
    let descriptor = RuntimeDescriptor {
        protocol_version: MCP_PROTOCOL_VERSION,
        address: address.to_string(),
        token: "test-token".to_string(),
    };
    let (request_seen_sender, request_seen_receiver) = mpsc::channel();
    let (cancel_seen_sender, cancel_seen_receiver) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("test server should accept once");
        let reader_stream = stream.try_clone().expect("test server should clone reader");
        let mut reader = BufReader::new(reader_stream);
        let mut writer = BufWriter::new(stream);
        assert!(matches!(
            read_sync_frame(&mut reader).expect("test server should receive hello"),
            BridgeFrame::Hello { .. }
        ));
        write_sync_frame(
            &mut writer,
            &BridgeFrame::HelloAck {
                protocol_version: MCP_PROTOCOL_VERSION,
                session_id: "test-session".to_string(),
                error: None,
            },
        )
        .expect("test server should acknowledge hello");
        let request_id =
            match read_sync_frame(&mut reader).expect("test server should receive request") {
                BridgeFrame::Request { request_id, .. } => request_id,
                frame => panic!("unexpected request frame: {frame:?}"),
            };
        request_seen_sender
            .send(())
            .expect("test should observe request");
        match read_sync_frame(&mut reader).expect("test server should receive cancellation") {
            BridgeFrame::Cancel {
                request_id: cancelled_id,
            } => {
                assert_eq!(cancelled_id, request_id);
                cancel_seen_sender
                    .send(())
                    .expect("test should observe cancellation");
            }
            frame => panic!("unexpected cancellation frame: {frame:?}"),
        }
    });

    let connection = connect_bridge_to_endpoint("test-client", &descriptor)
        .expect("test client should connect");
    let client = Arc::new(BridgeClient::new());
    *client
        .connection
        .lock()
        .expect("test client connection lock should work") = Some(connection);
    let cancellation = Arc::new(AtomicBool::new(false));
    let call_cancellation = Arc::clone(&cancellation);
    let call_client = Arc::clone(&client);
    let call = thread::spawn(move || {
        let mut progress = |_progress: &BridgeProgress| {};
        call_client.call(
            request("cancel-me"),
            &mut progress,
            Some(&call_cancellation),
        )
    });
    request_seen_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("test request should reach the bridge");
    cancellation.store(true, Ordering::Release);
    assert_eq!(
        call.join()
            .expect("cancelled call should join")
            .unwrap_err(),
        super::super::super::FILETERM_CLI_JSONL_REQUEST_CANCELLED
    );
    cancel_seen_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("test cancellation should reach the bridge");
    client.close();
    server.join().expect("test server should join");
}

#[test]
fn disconnected_in_flight_request_is_not_replayed_and_next_call_reconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("test listener should expose an address");
    let descriptor = RuntimeDescriptor {
        protocol_version: MCP_PROTOCOL_VERSION,
        address: address.to_string(),
        token: "test-token".to_string(),
    };
    let (first_request_sender, first_request_receiver) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener
            .accept()
            .expect("test server should accept the first session");
        let reader_stream = stream.try_clone().expect("test server should clone reader");
        let mut reader = BufReader::new(reader_stream);
        let mut writer = BufWriter::new(stream);
        assert!(matches!(
            read_sync_frame(&mut reader).expect("test server should receive first hello"),
            BridgeFrame::Hello { .. }
        ));
        write_sync_frame(
            &mut writer,
            &BridgeFrame::HelloAck {
                protocol_version: MCP_PROTOCOL_VERSION,
                session_id: "first-session".to_string(),
                error: None,
            },
        )
        .expect("test server should acknowledge the first session");
        let first_request_id = match read_sync_frame(&mut reader)
            .expect("test server should receive the first request")
        {
            BridgeFrame::Request {
                request_id,
                request,
            } => {
                assert_eq!(
                    request.params.get("value").and_then(Value::as_str),
                    Some("first")
                );
                request_id
            }
            frame => panic!("unexpected first request frame: {frame:?}"),
        };
        first_request_sender
            .send(first_request_id)
            .expect("test should observe the first request");
        drop(reader);
        drop(writer);

        let (stream, _) = listener
            .accept()
            .expect("test server should accept the recovered session");
        let reader_stream = stream.try_clone().expect("test server should clone reader");
        let mut reader = BufReader::new(reader_stream);
        let mut writer = BufWriter::new(stream);
        assert!(matches!(
            read_sync_frame(&mut reader).expect("test server should receive second hello"),
            BridgeFrame::Hello { .. }
        ));
        write_sync_frame(
            &mut writer,
            &BridgeFrame::HelloAck {
                protocol_version: MCP_PROTOCOL_VERSION,
                session_id: "second-session".to_string(),
                error: None,
            },
        )
        .expect("test server should acknowledge the recovered session");
        let second_request_id = match read_sync_frame(&mut reader)
            .expect("test server should receive the recovered request")
        {
            BridgeFrame::Request {
                request_id,
                request,
            } => {
                assert_eq!(
                    request.params.get("value").and_then(Value::as_str),
                    Some("second")
                );
                request_id
            }
            frame => panic!("unexpected recovered request frame: {frame:?}"),
        };
        write_sync_frame(
            &mut writer,
            &BridgeFrame::Response {
                request_id: second_request_id,
                response: BridgeResponse::success(json!({ "value": "second" })),
            },
        )
        .expect("test server should respond after recovery");
    });

    let client = Arc::new(BridgeClient::new_for_descriptor(descriptor));
    let first_client = Arc::clone(&client);
    let first = thread::spawn(move || {
        let mut progress = |_progress: &BridgeProgress| {};
        first_client.call(request("first"), &mut progress, None)
    });
    first_request_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("first request should reach the bridge");
    let first_error = first
        .join()
        .expect("first call should join")
        .expect_err("a disconnected request must not be replayed");
    assert!(first_error.starts_with(BRIDGE_DISCONNECTED));

    let mut progress = |_progress: &BridgeProgress| {};
    assert_eq!(
        client
            .call(request("second"), &mut progress, None)
            .expect("the next explicit request should recover the session"),
        json!({ "value": "second" })
    );
    client.close();
    server.join().expect("test server should join");
}

#[test]
fn failed_recovery_enters_a_short_circuit_breaker_window() {
    let client = BridgeClient::new();
    *client
        .reconnect_cooldown_until
        .lock()
        .expect("test cooldown lock should work") =
        Some(Instant::now() + Duration::from_secs(1));

    let error = match client.ensure_connection() {
        Ok(_) => panic!("cooling down bridge should not reconnect"),
        Err(error) => error,
    };
    assert!(error.starts_with(BRIDGE_UNAVAILABLE));
}
