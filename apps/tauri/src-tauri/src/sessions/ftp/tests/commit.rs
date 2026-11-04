use super::*;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn strict_ftp_commit_skips_conflicts_and_restores_old_file_when_final_rename_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut lines = BufReader::new(read).lines();
        let mut files =
            HashMap::from([("/part".to_string(), "new"), ("/target".to_string(), "old")]);
        let mut data = None;
        let mut from = String::new();
        write.write_all(b"220 fixture\r\n").await.unwrap();
        while let Some(line) = lines.next_line().await.unwrap() {
            let (verb, arg) = line.split_once(' ').unwrap_or((&line, ""));
            let response = match verb {
                "USER" => "331 password\r\n".to_string(),
                "PASS" => "230 logged in\r\n".to_string(),
                "TYPE" => "200 binary\r\n".to_string(),
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
                "MLSD" => "500 unsupported\r\n".to_string(),
                "LIST" => {
                    write.write_all(b"150 listing\r\n").await.unwrap();
                    let (mut socket, _) = data.take().unwrap().accept().await.unwrap();
                    for (name, content) in &files {
                        socket
                            .write_all(
                                format!(
                                    "-rw-r--r-- 1 0 0 {} Jun 18 23:00 {}\r\n",
                                    content.len(),
                                    &name[1..]
                                )
                                .as_bytes(),
                            )
                            .await
                            .unwrap();
                    }
                    socket.shutdown().await.unwrap();
                    "226 done\r\n".to_string()
                }
                "RNFR" => {
                    from = arg.to_string();
                    "350 rename\r\n".to_string()
                }
                "RNTO" => {
                    if from == "/part" {
                        "550 injected commit failure\r\n".to_string()
                    } else if let Some(content) = files.remove(&from) {
                        files.insert(arg.to_string(), content);
                        "250 renamed\r\n".to_string()
                    } else {
                        "550 No such file\r\n".to_string()
                    }
                }
                "QUIT" => {
                    write.write_all(b"221 bye\r\n").await.unwrap();
                    break;
                }
                _ => panic!("strict commit must never stream/truncate data: {line}"),
            };
            write.write_all(response.as_bytes()).await.unwrap();
        }
        files
    });
    let profile = serde_json::json!({"type":"ftp", "username":"test", "password":"test", "securityMode":"none"});
    let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
    assert!(!client_commit(&mut client, "/part", "/target", false)
        .await
        .unwrap());
    assert!(client_commit(&mut client, "/part", "/target", true)
        .await
        .is_err());
    client_quit(&mut client).await.unwrap();
    let files = server.await.unwrap();
    assert_eq!(files.get("/target"), Some(&"old"));
    assert_eq!(files.get("/part"), Some(&"new"));
    assert_eq!(files.len(), 2, "rollback must not leave an orphan backup");
}
