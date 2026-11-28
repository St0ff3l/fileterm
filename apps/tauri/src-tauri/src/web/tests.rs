use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::header::{COOKIE, HOST, ORIGIN, SET_COOKIE};
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite;
use tower::ServiceExt;

use crate::runtime::{AppHandle, WebRuntime};
use crate::web::{build_router, WebServerState, WEB_SESSION_COOKIE};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn test_state(temp: &TempDirGuard) -> WebServerState {
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let home_dir = temp.path().join("home");
    let dist_dir = temp.path().join("dist");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&home_dir).unwrap();
    std::fs::create_dir_all(&dist_dir).unwrap();
    std::fs::write(
        dist_dir.join("index.html"),
        "<!doctype html><html><body>ok</body></html>",
    )
    .unwrap();

    let runtime = WebRuntime::new(data_dir, config_dir, home_dir);
    let app = AppHandle::from_web(Arc::clone(&runtime));
    WebServerState::new(app, runtime, dist_dir, "test-secret-token".to_string())
}

#[tokio::test]
async fn healthz_and_bootstrap_and_origin_guards() {
    let temp = TempDirGuard::new("fileterm-web-test-http");
    let app = build_router(test_state(&temp));

    let health_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health_resp.status(), StatusCode::OK);
    let health_bytes = axum::body::to_bytes(health_resp.into_body(), 1024 * 64)
        .await
        .unwrap();
    let health_json: Value = serde_json::from_slice(&health_bytes).unwrap();
    assert_eq!(health_json["ok"], true);
    assert_eq!(health_json["runtime"], "web");

    let boot_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header(HOST, "127.0.0.1:18420")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot_resp.status(), StatusCode::OK);
    let set_cookie = boot_resp
        .headers()
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains(&format!("{WEB_SESSION_COOKIE}=test-secret-token")));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    let boot_bytes = axum::body::to_bytes(boot_resp.into_body(), 1024 * 64)
        .await
        .unwrap();
    let boot_json: Value = serde_json::from_slice(&boot_bytes).unwrap();
    assert_eq!(boot_json["isDesktop"], false);
    assert_eq!(boot_json["platform"], "web");
    assert_eq!(boot_json["runtimeName"], "Web");

    // Cross-origin POST /api/rpc must be rejected with 403 Forbidden.
    let cross_origin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header(HOST, "127.0.0.1:18420")
                .header(ORIGIN, "https://evil.example.com")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "command": "app_get_snapshot", "args": {} }))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_origin.status(), StatusCode::FORBIDDEN);

    // Same-origin POST /api/rpc succeeds.
    let rpc_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header(HOST, "127.0.0.1:18420")
                .header(ORIGIN, "http://127.0.0.1:18420")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "command": "app_get_snapshot", "args": {} }))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rpc_resp.status(), StatusCode::OK);
    let rpc_bytes = axum::body::to_bytes(rpc_resp.into_body(), 1024 * 256)
        .await
        .unwrap();
    let rpc_json: Value = serde_json::from_slice(&rpc_bytes).unwrap();
    assert!(rpc_json["data"]["tabs"].is_array());
}

#[tokio::test]
async fn websocket_enforces_cookie_streams_terminal_and_cleans_up_per_client() {
    let temp = TempDirGuard::new("fileterm-web-test-ws");
    let state = test_state(&temp);
    let router = build_router(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    // Missing session cookie -> 401.
    let no_cookie_req = http_ws_request(&format!("ws://{addr}/api/ws?clientId=c1"), None);
    let err = tokio_tungstenite::connect_async(no_cookie_req)
        .await
        .unwrap_err();
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(resp.status(), StatusCode::UNAUTHORIZED),
        other => panic!("unexpected ws error: {other:?}"),
    }

    // Valid cookie -> connects two clients independently.
    let req1 = http_ws_request(
        &format!("ws://{addr}/api/ws?clientId=client-1&windowLabel=main"),
        Some(&format!("{WEB_SESSION_COOKIE}=test-secret-token")),
    );
    let req2 = http_ws_request(
        &format!("ws://{addr}/api/ws?clientId=client-2&windowLabel=main"),
        Some(&format!("{WEB_SESSION_COOKIE}=test-secret-token")),
    );
    let (mut ws1, _) = tokio_tungstenite::connect_async(req1).await.unwrap();
    let (mut ws2, _) = tokio_tungstenite::connect_async(req2).await.unwrap();

    ws1.send(tungstenite::Message::Text(
        json!({ "type": "subscribe_terminal" }).to_string().into(),
    ))
    .await
    .unwrap();
    ws2.send(tungstenite::Message::Text(
        json!({ "type": "subscribe_terminal" }).to_string().into(),
    ))
    .await
    .unwrap();

    let sub1 = next_ws_json(&mut ws1).await;
    assert_eq!(sub1["event"], "web:terminal-subscribed");
    let chan1 = sub1["payload"]["channelId"].as_u64().unwrap() as u32;

    let sub2 = next_ws_json(&mut ws2).await;
    assert_eq!(sub2["event"], "web:terminal-subscribed");
    let chan2 = sub2["payload"]["channelId"].as_u64().unwrap() as u32;
    assert_ne!(chan1, chan2);
    assert_eq!(
        state
            .runtime
            .workspace
            .terminal_output_channels
            .lock()
            .unwrap()
            .len(),
        2
    );

    // Emit an event and verify broadcast + terminal output delivery.
    state
        .app
        .emit("workspace:test-ping", json!({ "ok": true }))
        .unwrap();
    state
        .runtime
        .workspace
        .publish_terminal_output("tab-1", "hello web");

    let mut saw_event = false;
    let mut saw_terminal = false;
    for _ in 0..4 {
        let msg = next_ws_json(&mut ws1).await;
        if msg["type"] == "event" && msg["event"] == "workspace:test-ping" {
            saw_event = true;
        }
        if msg["type"] == "terminal_data" && msg["payload"]["chunk"] == "hello web" {
            saw_terminal = true;
            let delivery_id = msg["payload"]["deliveryId"].as_u64().unwrap();
            ws1.send(tungstenite::Message::Text(
                json!({ "type": "ack_terminal", "channelId": chan1, "deliveryId": delivery_id })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
        if saw_event && saw_terminal {
            break;
        }
    }
    assert!(saw_event);
    assert!(saw_terminal);

    // Closing client 1 cleans up only client 1's terminal subscription.
    drop(ws1);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state
            .runtime
            .workspace
            .terminal_output_channels
            .lock()
            .unwrap()
            .len()
            != 1
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(state
        .runtime
        .workspace
        .terminal_output_channels
        .lock()
        .unwrap()
        .contains_key(&chan2));

    drop(ws2);
    server.abort();
}

#[tokio::test]
async fn stage_upload_export_connections_and_logs_work() {
    let temp = TempDirGuard::new("fileterm-web-test-files");
    let state = test_state(&temp);
    let app = build_router(state.clone());

    let boundary = "----FileTermTestBoundary";
    let multipart_body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\nContent-Type: text/plain\r\n\r\nhello-upload\r\n--{boundary}--\r\n"
    );
    let upload_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/web/stage-upload")
                .header(HOST, "127.0.0.1:18420")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload_resp.status(), StatusCode::OK);
    let upload_bytes = axum::body::to_bytes(upload_resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let upload_json: Value = serde_json::from_slice(&upload_bytes).unwrap();
    let staged_path = PathBuf::from(upload_json["paths"][0].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(&staged_path).unwrap(),
        "hello-upload"
    );

    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/web/export-connections?format=fileterm")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_resp.status(), StatusCode::OK);

    let log_resp = app
        .oneshot(
            Request::builder()
                .uri("/api/web/logs/app.log")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(log_resp.status(), StatusCode::OK);
}

fn http_ws_request(url: &str, cookie: Option<&str>) -> tungstenite::handshake::client::Request {
    use tungstenite::client::IntoClientRequest;
    let mut req = url.into_client_request().unwrap();
    if let Some(cookie) = cookie {
        req.headers_mut().insert(COOKIE, cookie.parse().unwrap());
    }
    req
}

async fn next_ws_json<S>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<tungstenite::Message, tungstenite::Error>> + Unpin,
{
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Some(Ok(msg)) = ws.next().await {
            if let tungstenite::Message::Text(text) = msg {
                return serde_json::from_str::<Value>(&text).unwrap();
            }
        }
        panic!("websocket closed before receiving JSON frame");
    })
    .await
    .unwrap()
}
