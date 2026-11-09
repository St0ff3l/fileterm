use super::*;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn editor_save_preserves_original_on_failure_and_finishes_commit_after_cancel_or_deadline() {
    for (fail_upload, different_group, finish_commit, cancel_commit) in [
        (true, false, false, false),
        (false, false, false, false),
        (false, true, false, false),
        (false, false, true, true),
        (false, false, true, false),
    ] {
        let cancellation = CancellationToken::new();
        let server_cancellation = cancellation.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut lines = BufReader::new(read).lines();
            let mut files = HashMap::from([("/target".to_owned(), b"original".to_vec())]);
            let mut data = None;
            let mut from = String::new();
            write.write_all(b"220 fixture\r\n").await.unwrap();
            while let Some(line) = lines.next_line().await.unwrap() {
                let (verb, arg) = line.split_once(' ').unwrap_or((&line, ""));
                let response = match verb {
                    "USER" => "331 password\r\n".to_owned(),
                    "PASS" => "230 logged in\r\n".to_owned(),
                    "TYPE" | "OPTS" => "200 binary\r\n".to_owned(),
                    "EPSV" | "PASV" => {
                        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                        let port = listener.local_addr().unwrap().port();
                        data = Some(listener);
                        if verb == "EPSV" {
                            format!("229 (|||{port}|)\r\n")
                        } else {
                            format!("227 (127,0,0,1,{},{})\r\n", port / 256, port % 256)
                        }
                    }
                    "MLSD" => "500 unsupported\r\n".to_owned(),
                    "SITE" => "200 permissions set\r\n".to_owned(),
                    "LIST" => {
                        write.write_all(b"150 listing\r\n").await.unwrap();
                        let (mut socket, _) = data.take().unwrap().accept().await.unwrap();
                        for (path, bytes) in &files {
                            let group = if different_group && path == "/target" {
                                2000
                            } else {
                                1000
                            };
                            socket
                                .write_all(
                                    format!(
                                        "-rw------- 1 1000 {group} {} Jun 18 23:00 {}\r\n",
                                        bytes.len(),
                                        path.trim_start_matches('/')
                                    )
                                    .as_bytes(),
                                )
                                .await
                                .unwrap();
                        }
                        socket.shutdown().await.unwrap();
                        "226 listed\r\n".to_owned()
                    }
                    "STOR" => {
                        assert!(
                            arg.starts_with("/target.fileterm-edit-"),
                            "the original must never be truncated"
                        );
                        write.write_all(b"150 upload\r\n").await.unwrap();
                        let (mut socket, _) = data.take().unwrap().accept().await.unwrap();
                        let mut bytes = Vec::new();
                        socket.read_to_end(&mut bytes).await.unwrap();
                        files.insert(arg.to_owned(), bytes);
                        if fail_upload && !files[arg].is_empty() {
                            "451 injected upload failure\r\n"
                        } else {
                            "226 done\r\n"
                        }
                        .to_owned()
                    }
                    "SIZE" => format!("213 {}\r\n", files[arg].len()),
                    "RNFR" => {
                        from = arg.to_owned();
                        "350 rename\r\n".to_owned()
                    }
                    "RNTO" => {
                        if from.contains(".fileterm-edit-") && !finish_commit && !different_group {
                            "550 injected commit failure\r\n".to_owned()
                        } else {
                            let bytes = files.remove(&from).unwrap();
                            files.insert(arg.to_owned(), bytes);
                            if arg.contains(".fileterm-backup-") && finish_commit {
                                if cancel_commit {
                                    server_cancellation.cancel();
                                    sleep(Duration::from_millis(100)).await;
                                } else {
                                    sleep(Duration::from_secs(6)).await;
                                }
                            }
                            "250 renamed\r\n".to_owned()
                        }
                    }
                    "DELE" => {
                        files.remove(arg);
                        "250 deleted\r\n".to_owned()
                    }
                    "QUIT" => {
                        write.write_all(b"221 bye\r\n").await.unwrap();
                        break;
                    }
                    _ => panic!("unexpected editor command: {line}"),
                };
                write.write_all(response.as_bytes()).await.unwrap();
            }
            files
        });
        let profile = serde_json::json!({"type":"ftp","username":"test","password":"test","securityMode":"none","operationTimeoutSeconds":5});
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        let result = client_write(
            &mut client,
            "/target",
            "new",
            "utf-8",
            &profile,
            cancellation,
        )
        .await;
        assert_eq!(result.is_ok(), finish_commit, "{result:?}");
        client_quit(&mut client).await.unwrap();
        let files = server.await.unwrap();
        assert_eq!(
            files["/target"],
            if finish_commit {
                b"new".as_slice()
            } else {
                b"original".as_slice()
            }
        );
        assert_eq!(
            files.len(),
            if finish_commit { 1 } else { 2 },
            "completed saves remove staging and backup; failed saves retain recovery data"
        );
    }
}

#[test]
fn editor_permissions_preserve_special_bits_and_unknown_formats() {
    assert_eq!(editor_file_mode("-rw-------"), Some(0o600));
    assert_eq!(editor_file_mode("-rwsr-Sr-t"), Some(0o7745));
    assert_eq!(editor_file_mode(""), None);
}

#[tokio::test]
async fn canceled_staging_requires_reconnecting_the_control_stream() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = ftp_with_cancellation(
        &serde_json::json!({}),
        "write",
        cancellation,
        std::future::pending::<Result<(), String>>(),
    )
    .await
    .unwrap_err();
    assert!(ftp_error_requires_reconnect(&error));
}
