use super::read_bounded_bundle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn response(
    headers: &str,
    body: &[u8],
) -> (
    reqwest::Response,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let data = [headers.as_bytes(), body].concat();
    let (release, wait) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut byte = [0];
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
        }
        socket.write_all(&data).await.unwrap();
        let _ = wait.await;
    });
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}/backup"))
        .send()
        .await
        .unwrap();
    (response, release, server)
}

#[tokio::test]
async fn rejects_oversized_content_length_without_waiting_for_body() {
    let (response, release, server) =
        response("HTTP/1.1 200 OK\r\nContent-Length: 999\r\n\r\n", b"").await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        read_bounded_bundle(response, 4, "WebDAV"),
    )
    .await;
    release.send(()).unwrap();
    server.await.unwrap();
    assert!(result
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("大小限制"));
}

#[tokio::test]
async fn rejects_chunked_overflow_before_response_finishes() {
    // Deliberately omit the final zero chunk and leave the socket open.
    let (response, release, server) = response(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
        b"3\r\nabc\r\n2\r\nde\r\n",
    )
    .await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        read_bounded_bundle(response, 4, "S3"),
    )
    .await;
    release.send(()).unwrap();
    server.await.unwrap();
    assert!(result
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("大小限制"));
}

#[tokio::test]
async fn accepts_chunked_body_exactly_at_limit() {
    let (response, release, server) = response(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
        b"2\r\nab\r\n2\r\ncd\r\n0\r\n\r\n",
    )
    .await;
    let bytes = read_bounded_bundle(response, 4, "S3").await.unwrap();
    release.send(()).unwrap();
    server.await.unwrap();
    assert_eq!(bytes, b"abcd");
}
