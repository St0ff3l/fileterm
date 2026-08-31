#[cfg(test)]
mod tests {
    use base64::Engine;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[cfg(unix)]
    use super::connect_ftp_with_tls_connector;
    use super::{
        client_list, client_quit, client_read, client_write, connect_ftp,
        ftp_capabilities_from_features, ftp_listing_permission, ftp_sha256_command,
        is_ftp_existing_path, is_ftp_file_not_found, join_remote_path,
        normalize_ftp_certificate_fingerprint, parent_remote_path, parse_ftp_listing_line,
        parse_ftp_sha256_response, upload_file, FtpClient, FtpListingState,
        DEFAULT_FTP_OPERATION_TIMEOUT,
    };
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[test]
    fn cleanup_treats_only_missing_ftp_files_as_idempotent() {
        let missing = suppaftp::FtpError::UnexpectedResponse(suppaftp::types::Response {
            status: suppaftp::Status::FileUnavailable,
            body: b"No such file or directory".to_vec(),
        });
        let denied = suppaftp::FtpError::UnexpectedResponse(suppaftp::types::Response {
            status: suppaftp::Status::FileUnavailable,
            body: b"Permission denied".to_vec(),
        });

        assert!(is_ftp_file_not_found(&missing));
        assert!(!is_ftp_file_not_found(&denied));

        let existing = suppaftp::FtpError::UnexpectedResponse(suppaftp::types::Response {
            status: suppaftp::Status::FileUnavailable,
            body: b"Can't create directory: File exists".to_vec(),
        });
        assert!(is_ftp_existing_path(&existing));
        assert!(!is_ftp_existing_path(&denied));
    }

    #[test]
    fn normalizes_ftps_certificate_fingerprint_formats() {
        let digest = [0xab; 32];
        let hex = "ab".repeat(32);
        let colon_hex = hex
            .as_bytes()
            .chunks(2)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        let base64 = base64::engine::general_purpose::STANDARD.encode(digest);
        assert_eq!(
            normalize_ftp_certificate_fingerprint(&format!("SHA256:{colon_hex}")),
            Ok(format!("sha256:{hex}"))
        );
        assert_eq!(
            normalize_ftp_certificate_fingerprint(&base64),
            Ok(format!("sha256:{hex}"))
        );
        assert!(normalize_ftp_certificate_fingerprint("not-a-fingerprint").is_err());
    }

    #[test]
    fn discovers_ftp_checksum_extensions_and_commands() {
        let features = HashMap::from([
            ("HASH".to_string(), Some("SHA-256 SHA-1".to_string())),
            ("UTF8".to_string(), None),
        ]);
        let capabilities = ftp_capabilities_from_features(features.clone());
        assert_eq!(capabilities.extensions, vec!["HASH", "UTF8"]);
        assert_eq!(capabilities.checksum_algorithms, vec!["SHA-1", "SHA-256"]);
        assert_eq!(ftp_sha256_command(&features), Some("HASH".to_string()));

        let xsha = HashMap::from([("XSHA256".to_string(), None)]);
        assert_eq!(ftp_sha256_command(&xsha), Some("XSHA256".to_string()));
        assert!(super::ftp_hash_requires_algorithm_selection("HASH"));
        assert!(!super::ftp_hash_requires_algorithm_selection("XSHA256"));
        assert_eq!(
            parse_ftp_sha256_response(
                "213 /tmp/file 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            ),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );
    }

    #[test]
    fn parses_classic_unix_listing_before_mlsd_fallback() {
        let line = "drwxr-xr-x 5 0 0 4096 Jun 18 23:00 anydesk";
        let parsed = parse_ftp_listing_line(line).expect("classic LIST row should parse");
        let entry = parsed.entry;

        assert!(parsed.type_is_trusted);
        assert_eq!(entry.name(), "anydesk");
        assert!(entry.is_directory());
        assert_eq!(entry.size(), 4096);
        assert_eq!(ftp_listing_permission(line), "drwxr-xr-x");
    }

    #[test]
    fn keeps_standard_mlsd_listing_support() {
        let line = "type=file;size=8192;modify=20260715163248;UNIX.mode=0644;UNIX.uid=0;UNIX.gid=0; readme.txt";
        let parsed = parse_ftp_listing_line(line).expect("MLSD row should parse");
        let entry = parsed.entry;

        assert!(parsed.type_is_trusted);
        assert_eq!(entry.name(), "readme.txt");
        assert!(!entry.is_directory());
        assert_eq!(entry.size(), 8192);
        assert_eq!(ftp_listing_permission(line), "-rw-r--r--");
    }

    #[test]
    fn marks_unstructured_serv_u_rows_for_capability_probe() {
        let parsed =
            parse_ftp_listing_line("reports").expect("name-only row should remain visible");

        assert_eq!(parsed.entry.name(), "reports");
        assert!(!parsed.type_is_trusted);
    }

    #[tokio::test]
    async fn remembers_mlsd_failure_and_uses_fast_classic_list_afterward() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn(run_classic_listing_server(listener, commands.clone()));
        let profile = serde_json::json!({
            "type": "ftp", "username": "test", "password": "test", "securityMode": "none"
        });
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        let mut state = FtpListingState::default();

        let first = client_list(&mut client, "/", &mut state).await.unwrap();
        let second = client_list(&mut client, "/", &mut state).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(first[0]["name"], "folder");
        assert_eq!(first[0]["type"], "folder");
        assert_eq!(first[1]["name"], "payload.bin");
        assert_eq!(first[1]["size"], "2.0 KB");

        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
        let commands = commands.lock().await;
        assert_eq!(
            commands.iter().filter(|command| *command == "MLSD").count(),
            1
        );
        assert_eq!(
            commands.iter().filter(|command| *command == "LIST").count(),
            2
        );
    }

    async fn run_classic_listing_server(listener: TcpListener, commands: Arc<Mutex<Vec<String>>>) {
        let (control, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = control.into_split();
        let mut reader = BufReader::new(reader);
        let mut data_listener = None;
        writer
            .write_all(b"220 Serv-U compatible fixture\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, _) = command.split_once(' ').unwrap_or((command, ""));
            let verb = verb.to_ascii_uppercase();
            commands.lock().await.push(verb.clone());
            match verb.as_str() {
                "USER" => writer
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 Logged in\r\n").await.unwrap(),
                "TYPE" | "OPTS" => writer.write_all(b"200 OK\r\n").await.unwrap(),
                "EPSV" | "PASV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let data_port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb == "EPSV" {
                        format!("229 Entering Extended Passive Mode (|||{data_port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            data_port / 256,
                            data_port % 256
                        )
                    };
                    writer.write_all(response.as_bytes()).await.unwrap();
                }
                "MLSD" => writer.write_all(b"500 Unknown command\r\n").await.unwrap(),
                "LIST" => {
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    data.write_all(
                        b"drwxr-xr-x 2 0 0 4096 Jun 18 23:00 folder\r\n-rw-r--r-- 1 0 0 2048 Jun 18 23:00 payload.bin\r\n",
                    )
                    .await
                    .unwrap();
                    data.shutdown().await.unwrap();
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "QUIT" => {
                    writer.write_all(b"221 Goodbye\r\n").await.unwrap();
                    return;
                }
                _ => writer.write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }

    async fn run_resumable_upload_server(
        listener: TcpListener,
        supports_appe: bool,
        stored: Arc<Mutex<Vec<u8>>>,
        commands: Arc<Mutex<Vec<String>>>,
    ) {
        let (control, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = control.into_split();
        let mut reader = BufReader::new(reader);
        let mut data_listener = None;
        let mut rest_offset = 0_usize;
        writer
            .write_all(b"220 FileTerm resumable upload fixture\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = command.split_once(' ').unwrap_or((command, ""));
            let verb = verb.to_ascii_uppercase();
            commands.lock().await.push(verb.clone());
            match verb.as_str() {
                "USER" => writer
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 Logged in\r\n").await.unwrap(),
                "TYPE" | "OPTS" => writer.write_all(b"200 OK\r\n").await.unwrap(),
                "EPSV" | "PASV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let data_port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb == "EPSV" {
                        format!("229 Entering Extended Passive Mode (|||{data_port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            data_port / 256,
                            data_port % 256
                        )
                    };
                    writer.write_all(response.as_bytes()).await.unwrap();
                }
                "APPE" if supports_appe => {
                    assert_eq!(argument, "/resume.bin");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut suffix = Vec::new();
                    data.read_to_end(&mut suffix).await.unwrap();
                    stored.lock().await.extend_from_slice(&suffix);
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "APPE" => {
                    let _ = data_listener.take().unwrap().accept().await.unwrap();
                    writer.write_all(b"502 APPE unsupported\r\n").await.unwrap();
                }
                "REST" => {
                    rest_offset = argument.parse().unwrap();
                    writer
                        .write_all(b"350 Restarting at offset\r\n")
                        .await
                        .unwrap();
                }
                "STOR" => {
                    assert_eq!(argument, "/resume.bin");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut suffix = Vec::new();
                    data.read_to_end(&mut suffix).await.unwrap();
                    let mut bytes = stored.lock().await;
                    bytes.truncate(rest_offset);
                    bytes.extend_from_slice(&suffix);
                    rest_offset = 0;
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "SIZE" => {
                    let size = stored.lock().await.len();
                    writer
                        .write_all(format!("213 {size}\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                "DELE" => {
                    stored.lock().await.clear();
                    writer.write_all(b"250 Deleted\r\n").await.unwrap();
                }
                "QUIT" => {
                    writer.write_all(b"221 Goodbye\r\n").await.unwrap();
                    return;
                }
                _ => writer.write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }

    async fn assert_resumable_upload_strategy(supports_appe: bool) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let stored = Arc::new(Mutex::new(b"abc".to_vec()));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn(run_resumable_upload_server(
            listener,
            supports_appe,
            stored.clone(),
            commands.clone(),
        ));
        let root =
            std::env::temp_dir().join(format!("fileterm-ftp-resume-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("resume.bin");
        tokio::fs::write(&source, b"abcdef").await.unwrap();
        let profile = serde_json::json!({
            "type": "ftp", "username": "test", "password": "test", "securityMode": "none"
        });
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        match &mut client {
            FtpClient::Plain(ftp) => upload_file(
                ftp,
                source.to_str().unwrap(),
                "/resume.bin",
                3,
                "transfer-test",
                tokio_util::sync::CancellationToken::new(),
                None,
                DEFAULT_FTP_OPERATION_TIMEOUT,
            )
            .await
            .unwrap(),
            FtpClient::Secure(_) => panic!("plain fixture returned a secure client"),
        }
        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
        assert_eq!(*stored.lock().await, b"abcdef");

        let commands = commands.lock().await;
        let appe = commands
            .iter()
            .position(|command| command == "APPE")
            .unwrap();
        if supports_appe {
            assert!(!commands.iter().any(|command| command == "REST"));
            assert!(!commands.iter().any(|command| command == "STOR"));
        } else {
            let rest = commands
                .iter()
                .position(|command| command == "REST")
                .unwrap();
            let stor = commands
                .iter()
                .position(|command| command == "STOR")
                .unwrap();
            assert!(appe < rest && rest < stor);
        }
        assert!(commands.iter().any(|command| command == "SIZE"));
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn resumable_upload_prefers_appe_and_verifies_size() {
        assert_resumable_upload_strategy(true).await;
    }

    #[tokio::test]
    async fn resumable_upload_falls_back_to_rest_and_stor() {
        assert_resumable_upload_strategy(false).await;
    }

    #[cfg(unix)]
    async fn run_secured_ftps_session<S>(
        stream: S,
        acceptor: &suppaftp::async_native_tls::TlsAcceptor,
        stored: Arc<Mutex<Vec<u8>>>,
        send_greeting: bool,
    ) where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let mut control = BufReader::new(stream);
        let mut data_listener = None;
        if send_greeting {
            control
                .get_mut()
                .write_all(b"220 FileTerm real FTPS fixture\r\n")
                .await
                .unwrap();
        }
        let mut line = String::new();
        loop {
            line.clear();
            if control.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = command.split_once(' ').unwrap_or((command, ""));
            match verb.to_ascii_uppercase().as_str() {
                "USER" => control
                    .get_mut()
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => control
                    .get_mut()
                    .write_all(b"230 Logged in\r\n")
                    .await
                    .unwrap(),
                "PBSZ" | "PROT" | "TYPE" | "OPTS" => {
                    control.get_mut().write_all(b"200 OK\r\n").await.unwrap()
                }
                "PASV" | "EPSV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb.eq_ignore_ascii_case("EPSV") {
                        format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            port / 256,
                            port % 256
                        )
                    };
                    control
                        .get_mut()
                        .write_all(response.as_bytes())
                        .await
                        .unwrap();
                }
                "STOR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    control
                        .get_mut()
                        .write_all(b"150 Opening protected data connection\r\n")
                        .await
                        .unwrap();
                    let (data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut data = acceptor.accept(data).await.unwrap();
                    let mut bytes = Vec::new();
                    data.read_to_end(&mut bytes).await.unwrap();
                    *stored.lock().await = bytes;
                    control
                        .get_mut()
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "RETR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    control
                        .get_mut()
                        .write_all(b"150 Opening protected data connection\r\n")
                        .await
                        .unwrap();
                    let (data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut data = acceptor.accept(data).await.unwrap();
                    let bytes = stored.lock().await.clone();
                    data.write_all(&bytes).await.unwrap();
                    data.shutdown().await.unwrap();
                    control
                        .get_mut()
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "QUIT" => {
                    control
                        .get_mut()
                        .write_all(b"221 Goodbye\r\n")
                        .await
                        .unwrap();
                    return;
                }
                _ => control.get_mut().write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }

    #[cfg(unix)]
    async fn run_explicit_ftps_server(
        listener: TcpListener,
        acceptor: suppaftp::async_native_tls::TlsAcceptor,
        stored: Arc<Mutex<Vec<u8>>>,
    ) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut control = BufReader::new(stream);
        control
            .get_mut()
            .write_all(b"220 FileTerm explicit FTPS fixture\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            assert!(control.read_line(&mut line).await.unwrap() > 0);
            let command = line.trim_end_matches(['\r', '\n']);
            if command.eq_ignore_ascii_case("AUTH TLS") {
                control
                    .get_mut()
                    .write_all(b"234 Begin TLS negotiation\r\n")
                    .await
                    .unwrap();
                let secured = acceptor.accept(control.into_inner()).await.unwrap();
                run_secured_ftps_session(secured, &acceptor, stored, false).await;
                return;
            }
            control
                .get_mut()
                .write_all(b"500 Send AUTH TLS first\r\n")
                .await
                .unwrap();
        }
    }

    #[cfg(unix)]
    async fn run_implicit_ftps_server(
        listener: TcpListener,
        acceptor: suppaftp::async_native_tls::TlsAcceptor,
        stored: Arc<Mutex<Vec<u8>>>,
    ) {
        let (stream, _) = listener.accept().await.unwrap();
        let secured = acceptor.accept(stream).await.unwrap();
        run_secured_ftps_session(secured, &acceptor, stored, true).await;
    }

    #[cfg(unix)]
    fn create_ftps_identity() -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("fileterm-ftps-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let key = root.join("key.pem");
        let cert = root.join("cert.pem");
        let identity = root.join("identity.p12");
        let openssl = "/usr/bin/openssl";
        assert!(
            std::path::Path::new(openssl).exists(),
            "real FTPS fixture requires {openssl}"
        );
        let certificate = std::process::Command::new(openssl)
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
            .arg(&key)
            .args(["-out"])
            .arg(&cert)
            .args(["-subj", "/CN=localhost", "-days", "1"])
            .output()
            .unwrap();
        assert!(
            certificate.status.success(),
            "openssl certificate generation failed: {}",
            String::from_utf8_lossy(&certificate.stderr)
        );
        let package = std::process::Command::new(openssl)
            .args(["pkcs12", "-export", "-out"])
            .arg(&identity)
            .args(["-inkey"])
            .arg(&key)
            .args(["-in"])
            .arg(&cert)
            .args(["-passout", "pass:fileterm-test"])
            .output()
            .unwrap();
        assert!(
            package.status.success(),
            "openssl PKCS#12 generation failed: {}",
            String::from_utf8_lossy(&package.stderr)
        );
        (root, identity)
    }

    #[test]
    fn keeps_ftp_paths_posix_normalized() {
        assert_eq!(parent_remote_path("/one/file"), "/one");
        assert_eq!(join_remote_path("/", "file"), "/file");
    }

    #[tokio::test]
    async fn plain_ftp_client_round_trips_against_a_real_tcp_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let stored = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn(run_minimal_ftp_server(listener, stored.clone()));
        let profile = serde_json::json!({
            "securityMode": "none", "username": "fileterm", "password": "test",
        });
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        client_write(&mut client, "/roundtrip.txt", "Tauri FTP", "utf-8")
            .await
            .unwrap();
        assert_eq!(
            client_read(&mut client, "/roundtrip.txt", "utf-8")
                .await
                .unwrap(),
            "Tauri FTP"
        );
        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
        assert_eq!(&*stored.lock().await, b"Tauri FTP");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn explicit_and_implicit_ftps_round_trip_over_real_tls_control_and_data_channels() {
        let (root, identity) = create_ftps_identity();
        for security_mode in ["explicit", "implicit"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let acceptor = suppaftp::async_native_tls::TlsAcceptor::new(
                tokio::fs::File::open(&identity).await.unwrap(),
                "fileterm-test",
            )
            .await
            .unwrap();
            let stored = Arc::new(Mutex::new(Vec::new()));
            let server = if security_mode == "explicit" {
                tokio::spawn(run_explicit_ftps_server(listener, acceptor, stored.clone()))
            } else {
                tokio::spawn(run_implicit_ftps_server(listener, acceptor, stored.clone()))
            };
            let insecure_connector = suppaftp::async_native_tls::TlsConnector::new()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true);
            let profile = serde_json::json!({
                "securityMode": security_mode,
                "username": "fileterm",
                "password": "test",
            });
            let mut client = connect_ftp_with_tls_connector(
                &profile,
                "localhost",
                port,
                suppaftp::tokio::AsyncNativeTlsConnector::from(insecure_connector),
            )
            .await
            .unwrap();
            client_write(&mut client, "/roundtrip.txt", "Tauri FTPS", "utf-8")
                .await
                .unwrap();
            assert_eq!(
                client_read(&mut client, "/roundtrip.txt", "utf-8")
                    .await
                    .unwrap(),
                "Tauri FTPS"
            );
            client_quit(&mut client).await.unwrap();
            server.await.unwrap();
            assert_eq!(&*stored.lock().await, b"Tauri FTPS");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    async fn run_minimal_ftp_server(listener: TcpListener, stored: Arc<Mutex<Vec<u8>>>) {
        let (control, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = control.into_split();
        let mut reader = BufReader::new(reader);
        let mut data_listener = None;
        writer
            .write_all(b"220 FileTerm Tauri test FTP\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = command.split_once(' ').unwrap_or((command, ""));
            match verb.to_ascii_uppercase().as_str() {
                "USER" => writer
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 Logged in\r\n").await.unwrap(),
                "TYPE" | "OPTS" => writer.write_all(b"200 OK\r\n").await.unwrap(),
                "PASV" | "EPSV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb.eq_ignore_ascii_case("EPSV") {
                        format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            port / 256,
                            port % 256
                        )
                    };
                    writer.write_all(response.as_bytes()).await.unwrap();
                }
                "STOR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut bytes = Vec::new();
                    data.read_to_end(&mut bytes).await.unwrap();
                    *stored.lock().await = bytes;
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "RETR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let bytes = stored.lock().await.clone();
                    data.write_all(&bytes).await.unwrap();
                    data.shutdown().await.unwrap();
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "QUIT" => {
                    writer.write_all(b"221 Goodbye\r\n").await.unwrap();
                    return;
                }
                _ => writer.write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }
}
