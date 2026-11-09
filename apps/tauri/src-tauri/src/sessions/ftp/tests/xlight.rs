use super::*;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

async fn fixture(
    opts_response: &'static str,
    listing_mode: &str,
    encoding: &'static encoding_rs::Encoding,
) -> (FtpClient, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let listing_mode = listing_mode.to_string();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        let mut files = HashMap::from([
            ("/中文.txt".to_string(), b"utf8\r\n".to_vec()),
            ("/protected".to_string(), b"original".to_vec()),
        ]);
        let mut commands = Vec::new();
        let mut data_listener = None;
        let mut from = String::new();
        let mut cwd = "/".to_string();
        write.write_all(b"220 Xlight fixture\r\n").await.unwrap();
        loop {
            let mut raw = Vec::new();
            if reader.read_until(b'\n', &mut raw).await.unwrap() == 0 {
                break;
            }
            let (line, _, errors) = encoding.decode(&raw);
            assert!(!errors, "invalid command bytes for configured encoding");
            let line = line.trim_end_matches(['\r', '\n']).to_string();
            commands.push(line.clone());
            let (verb, arg) = line.split_once(' ').unwrap_or((&line, ""));
            let response = match verb {
                "USER" => "331 password\r\n".to_string(),
                "PASS" => "230 logged in\r\n".to_string(),
                "OPTS" => {
                    assert_eq!(
                        arg,
                        if encoding == encoding_rs::GBK {
                            "UTF8 OFF"
                        } else {
                            "UTF8 ON"
                        }
                    );
                    opts_response.to_string()
                }
                "TYPE" => "200 binary\r\n".to_string(),
                "MKD" => "521 Cannot create directory\r\n".to_string(),
                "PWD" => format!("257 \"{cwd}\"\r\n"),
                "CWD" => {
                    if matches!(arg, "/" | "/uploads" | "/目录") {
                        cwd = arg.to_string();
                        "250 changed\r\n".to_string()
                    } else {
                        "550 Cannot change directory\r\n".to_string()
                    }
                }
                "EPSV" | "PASV" => {
                    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let port = listener.local_addr().unwrap().port();
                    data_listener = Some(listener);
                    if verb == "EPSV" {
                        format!("229 (|||{port}|)\r\n")
                    } else {
                        format!("227 (127,0,0,1,{},{})\r\n", port / 256, port % 256)
                    }
                }
                "MLSD" | "LIST" => {
                    if listing_mode == "denied" {
                        "550 Cannot list directory\r\n".to_string()
                    } else {
                        write.write_all(b"150 listing\r\n").await.unwrap();
                        let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                        let mut listing =
                            "type=cdir;modify=20261109000000; /\r\ntype=pdir; /\r\n".to_string();
                        if listing_mode == "malformed" {
                            listing.push_str("unstructured\r\n");
                        }
                        for (path, bytes) in files
                            .iter()
                            .filter(|(path, _)| parent_remote_path(path) == arg)
                        {
                            listing.push_str(&format!(
                                "type=file;size={};modify=20261109000000; {}\r\n",
                                bytes.len(),
                                path.rsplit('/').next().unwrap()
                            ));
                        }
                        let (encoded, _, errors) = encoding.encode(&listing);
                        assert!(!errors);
                        data.write_all(&encoded).await.unwrap();
                        data.shutdown().await.unwrap();
                        "226 done\r\n".to_string()
                    }
                }
                "DELE" => {
                    if arg == "/protected" || files.remove(arg).is_none() {
                        "550 Cannot delete file\r\n".to_string()
                    } else {
                        "250 deleted\r\n".to_string()
                    }
                }
                "SIZE" => match files.get(arg) {
                    Some(bytes) => format!("213 {}\r\n", bytes.len()),
                    None => "550 Cannot get file size\r\n".to_string(),
                },
                "STOR" => {
                    write.write_all(b"150 upload\r\n").await.unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut bytes = Vec::new();
                    data.read_to_end(&mut bytes).await.unwrap();
                    files.insert(arg.to_string(), bytes);
                    "226 done\r\n".to_string()
                }
                "RETR" => {
                    write.write_all(b"150 download\r\n").await.unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    data.write_all(files.get(arg).unwrap()).await.unwrap();
                    data.shutdown().await.unwrap();
                    "226 done\r\n".to_string()
                }
                "RNFR" => {
                    if files.contains_key(arg) {
                        from = arg.to_string();
                        "350 rename\r\n".to_string()
                    } else {
                        "550 Cannot rename file\r\n".to_string()
                    }
                }
                "RNTO" => {
                    let bytes = files.remove(&from).unwrap();
                    files.insert(arg.to_string(), bytes);
                    "250 renamed\r\n".to_string()
                }
                "QUIT" => {
                    write.write_all(b"221 bye\r\n").await.unwrap();
                    break;
                }
                _ => panic!("Unexpected FTP command: {line}"),
            };
            let (encoded, _, errors) = encoding.encode(&response);
            assert!(!errors);
            write.write_all(&encoded).await.unwrap();
        }
        assert_eq!(files["/protected"], b"original");
        commands
    });
    let profile = serde_json::json!({"type":"ftp", "username":"test", "password":"test", "encoding":encoding.name()});
    (
        connect_ftp(&profile, "127.0.0.1", port).await.unwrap(),
        server,
    )
}

#[tokio::test]
async fn xlight_missing_checkpoint_upload_commit_and_filename_roundtrip() {
    for encoding in [encoding_rs::UTF_8, encoding_rs::GBK] {
        let (mut client, server) = fixture("200 option set\r\n", "normal", encoding).await;
        let listing = client_list(&mut client, "/", &mut FtpListingState::default())
            .await
            .unwrap();
        assert!(listing.iter().any(|entry| entry["name"] == "中文.txt"));
        assert!(!listing.iter().any(|entry| entry["name"] == "/"));
        let FtpClient::Plain(ref mut ftp) = client else {
            panic!("plain fixture");
        };
        if encoding == encoding_rs::GBK {
            let error = ftp.size("/😀").await.unwrap_err();
            assert!(error
                .to_string()
                .contains("unsupported by the configured filename encoding"));
        }
        assert!(ensure_dir(ftp, "/denied")
            .await
            .unwrap_err()
            .contains("CWD verification"));
        assert!(stat_file(ftp, "/missing").await.unwrap().is_none());
        remove_file(ftp, "/missing").await.unwrap();
        assert!(remove_file(ftp, "/protected")
            .await
            .unwrap_err()
            .contains("DELE"));
        let local = std::env::temp_dir().join(format!("fileterm-ftp-{}", uuid::Uuid::new_v4()));
        let bytes = "roundtrip 中文\r\n".as_bytes();
        tokio::fs::write(&local, bytes).await.unwrap();
        // Include both new-file and overwrite commits, all through the shared upload implementation.
        for target in [
            "/uploads/probe.txt",
            "/probe.txt.fileterm-part",
            "/中文.txt",
            "/目录/中文.txt",
            "/probe.txt",
        ] {
            let partial = format!("{target}.fileterm-part");
            remove_file(ftp, &partial).await.unwrap();
            assert!(stat_file(ftp, &partial).await.unwrap().is_none());
            upload_file(
                ftp,
                local.to_str().unwrap(),
                bytes.len() as u64,
                &partial,
                0,
                "test",
                CancellationToken::new(),
                None,
                Duration::from_secs(2),
            )
            .await
            .unwrap();
            replace_file(ftp, &partial, target).await.unwrap();
            let mut stream = ftp.retr_as_stream(target).await.unwrap();
            let mut actual = Vec::new();
            stream.read_to_end(&mut actual).await.unwrap();
            ftp.finalize_retr_stream(stream).await.unwrap();
            assert_eq!(actual, bytes);
            assert_eq!(ftp.pwd().await.unwrap(), "/");
        }
        tokio::fs::remove_file(local).await.unwrap();
        client_quit(&mut client).await.unwrap();
        let commands = server.await.unwrap();
        assert!(
            commands
                .iter()
                .position(|line| line
                    == if encoding == encoding_rs::GBK {
                        "OPTS UTF8 OFF"
                    } else {
                        "OPTS UTF8 ON"
                    })
                .unwrap()
                < commands.iter().position(|line| line == "MLSD /").unwrap()
        );
    }
}

#[tokio::test]
async fn ambiguous_550_stays_an_error_when_listing_is_denied_or_unreliable() {
    for mode in ["denied", "malformed"] {
        let (mut client, server) = fixture("200 UTF8 enabled\r\n", mode, encoding_rs::UTF_8).await;
        let FtpClient::Plain(ref mut ftp) = client else {
            panic!("plain fixture");
        };
        assert!(remove_file(ftp, "/missing").await.is_err());
        assert!(stat_file(ftp, "/missing").await.is_err());
        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
    }
}

#[tokio::test]
async fn legacy_ftp_without_opts_still_connects() {
    let (mut client, server) = fixture("502 Unsupported\r\n", "normal", encoding_rs::UTF_8).await;
    client_quit(&mut client).await.unwrap();
    assert!(server
        .await
        .unwrap()
        .iter()
        .any(|line| line == "OPTS UTF8 ON"));
}
