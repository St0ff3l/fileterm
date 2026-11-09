use tokio::net::TcpStream;

pub(crate) async fn connect_direct_telnet(host: &str, port: u16) -> Result<TcpStream, String> {
    TcpStream::connect((host, port))
        .await
        .map_err(|error| format!("Telnet connect failed: host={host:?} port={port}: {error}"))
}
