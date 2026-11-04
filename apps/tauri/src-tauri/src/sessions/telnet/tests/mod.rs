#[cfg(test)]
mod tests {
    use super::{
        connect_transport, encode_telnet_input, TelnetParser, BINARY, DO, DONT, ECHO, IAC, NAWS,
        SB, SE, WILL, WONT,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};

    async fn read_http_headers(socket: &mut tokio::net::TcpStream) -> String {
        let mut headers = Vec::new();
        let mut byte = [0_u8; 1];
        while !headers.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut byte).await.unwrap();
            assert_eq!(count, 1, "client closed before completing CONNECT headers");
            headers.push(byte[0]);
        }
        String::from_utf8(headers).unwrap()
    }

    async fn read_socks5_connect_request(socket: &mut tokio::net::TcpStream) -> (String, u16) {
        let mut greeting = [0_u8; 2];
        socket.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting[0], 5);
        let mut methods = vec![0_u8; greeting[1] as usize];
        socket.read_exact(&mut methods).await.unwrap();
        assert!(methods.contains(&0));
        socket.write_all(&[5, 0]).await.unwrap();

        let mut request = [0_u8; 4];
        socket.read_exact(&mut request).await.unwrap();
        assert_eq!(&request[..3], &[5, 1, 0]);
        let host = match request[3] {
            1 => {
                let mut address = [0_u8; 4];
                socket.read_exact(&mut address).await.unwrap();
                std::net::Ipv4Addr::from(address).to_string()
            }
            3 => {
                let mut length = [0_u8; 1];
                socket.read_exact(&mut length).await.unwrap();
                let mut hostname = vec![0_u8; length[0] as usize];
                socket.read_exact(&mut hostname).await.unwrap();
                String::from_utf8(hostname).unwrap()
            }
            other => panic!("unexpected SOCKS5 address type: {other}"),
        };
        let mut port = [0_u8; 2];
        socket.read_exact(&mut port).await.unwrap();
        (host, u16::from_be_bytes(port))
    }

    #[test]
    fn negotiates_naws_and_hides_iac_control_bytes() {
        let mut parser = TelnetParser::new("xterm-256color");
        let (output, writes) = parser.feed(&[IAC, DO, NAWS, b'o', b'k']);
        assert_eq!(output, b"ok");
        assert_eq!(writes[0], vec![IAC, WILL, NAWS]);
        assert_eq!(writes[1], vec![IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]);
    }

    #[test]
    fn binary_transmit_mode_only_escapes_iac() {
        assert_eq!(
            encode_telnet_input(b"a\r\n\xffb", "cr", true, true),
            b"a\r\n\xff\xffb"
        );
    }

    #[test]
    fn nvt_transmit_mode_applies_cr_nul_without_touching_binary_mode() {
        assert_eq!(
            encode_telnet_input(b"\r\n", "cr", true, false),
            vec![b'\r', 0]
        );
        assert_eq!(encode_telnet_input(b"\r\n", "crlf", true, false), b"\r\n");
    }

    #[test]
    fn option_state_is_directional_and_repeated_will_is_quiet() {
        let mut parser = TelnetParser::new("vt220");
        let (_, first) = parser.feed(&[IAC, WILL, ECHO]);
        assert_eq!(first, vec![vec![IAC, DO, ECHO]]);
        let (_, repeated) = parser.feed(&[IAC, WILL, ECHO]);
        assert!(repeated.is_empty());
        let (_, disabled) = parser.feed(&[IAC, WONT, ECHO]);
        assert_eq!(disabled, vec![vec![IAC, DONT, ECHO]]);
    }

    #[test]
    fn cr_nul_pair_does_not_cross_a_telnet_control_sequence() {
        let mut parser = TelnetParser::new("ansi");
        let (first, _) = parser.feed(b"\r");
        assert_eq!(first, b"\r");
        let (second, writes) = parser.feed(&[IAC, DO, BINARY, 0]);
        assert_eq!(second, vec![0]);
        assert_eq!(writes, vec![vec![IAC, WILL, BINARY]]);
    }

    #[test]
    fn terminal_type_reply_uses_the_selected_type() {
        let mut parser = TelnetParser::new("vt100");
        let (_, writes) = parser.feed(&[IAC, SB, 24, 1, IAC, SE]);
        assert_eq!(
            writes,
            vec![vec![IAC, SB, 24, 0, b'v', b't', b'1', b'0', b'0', IAC, SE]]
        );
    }

    #[tokio::test]
    async fn direct_transport_drop_releases_socket_on_every_desktop_platform() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut byte = [0_u8; 1];
            socket.read(&mut byte).await.unwrap()
        });
        let profile = serde_json::json!({ "proxy": { "type": "none" } });
        let transport = connect_transport(&profile, "127.0.0.1", address.port())
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

    #[tokio::test]
    async fn http_connect_proxy_reaches_a_real_telnet_peer_and_relays_bytes() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut request = [0_u8; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let request = read_http_headers(&mut client).await;
            assert!(request.starts_with(&format!(
                "CONNECT 127.0.0.1:{} HTTP/1.1\r\n",
                target_address.port()
            )));
            assert!(request.contains("Proxy-Authorization: Basic cHJveHktdXNlcjpwcm94eS1wYXNz\r\n"));
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(target_address)
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut client, &mut target)
                .await
                .unwrap();
        });

        let profile = serde_json::json!({
            "proxy": {
                "type": "http",
                "host": "127.0.0.1",
                "port": proxy_address.port(),
                "username": "proxy-user",
                "password": "proxy-pass"
            }
        });
        let mut transport = connect_transport(&profile, "127.0.0.1", target_address.port())
            .await
            .unwrap();
        transport.write_all(b"ping").await.unwrap();
        let mut response = [0_u8; 4];
        transport.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        drop(transport);

        target.await.unwrap();
        proxy.await.unwrap();
    }

    #[tokio::test]
    async fn socks5_proxy_reaches_a_real_telnet_peer_and_relays_bytes() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut request = [0_u8; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let (host, port) = read_socks5_connect_request(&mut client).await;
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, target_address.port());
            client
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(target_address)
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut client, &mut target)
                .await
                .unwrap();
        });

        let profile = serde_json::json!({
            "proxy": {
                "type": "socks5",
                "host": "127.0.0.1",
                "port": proxy_address.port()
            }
        });
        let mut transport = connect_transport(&profile, "127.0.0.1", target_address.port())
            .await
            .unwrap();
        transport.write_all(b"ping").await.unwrap();
        let mut response = [0_u8; 4];
        transport.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        drop(transport);

        target.await.unwrap();
        proxy.await.unwrap();
    }
}
