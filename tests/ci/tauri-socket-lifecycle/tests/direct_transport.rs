#[path = "../../../../apps/tauri/src-tauri/src/sessions/telnet_direct.rs"]
mod telnet_direct;

use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn direct_transport_drop_releases_socket_on_windows() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut byte = [0_u8; 1];
        socket.read(&mut byte).await.unwrap()
    });

    let transport = telnet_direct::connect_direct_telnet("127.0.0.1", address.port())
        .await
        .unwrap();
    drop(transport);

    assert_eq!(
        timeout(Duration::from_secs(2), peer)
            .await
            .unwrap()
            .unwrap(),
        0
    );
}
