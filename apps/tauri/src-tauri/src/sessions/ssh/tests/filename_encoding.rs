use super::*;
use russh_sftp::protocol::{File, Name, Packet, Status, StatusCode, Version};

async fn receive(stream: &mut tokio::io::DuplexStream) -> Vec<u8> {
    let len = stream.read_u32().await.unwrap();
    let mut bytes = vec![0; len as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    bytes
}

async fn send(stream: &mut tokio::io::DuplexStream, packet: Packet) {
    let (kind, payload) = match packet {
        Packet::Version(p) => (2, russh_sftp::ser::to_bytes(&p).unwrap()),
        Packet::Handle(p) => (102, russh_sftp::ser::to_bytes(&p).unwrap()),
        Packet::Status(p) => (101, russh_sftp::ser::to_bytes(&p).unwrap()),
        Packet::Attrs(p) => (105, russh_sftp::ser::to_bytes(&p).unwrap()),
        _ => panic!("unexpected fixture response"),
    };
    stream.write_u32((payload.len() + 1) as u32).await.unwrap();
    stream.write_u8(kind).await.unwrap();
    stream.write_all(&payload).await.unwrap();
}

async fn names(
    stream: &mut tokio::io::DuplexStream,
    id: u32,
    name: &str,
    encoding: &'static encoding_rs::Encoding,
) {
    let payload = russh_sftp::ser::to_bytes_with_encoding(
        &Name {
            id,
            files: vec![File::dummy(name)],
        },
        encoding,
    )
    .unwrap();
    stream.write_u32((payload.len() + 1) as u32).await.unwrap();
    stream.write_u8(104).await.unwrap();
    stream.write_all(&payload).await.unwrap();
}

#[tokio::test]
async fn filenames_detect_legacy_encoding_and_open_the_original_directory_bytes() {
    for (wire, configured) in [
        (encoding_rs::UTF_8, "UTF-8"),
        (encoding_rs::GBK, "UTF-8"),
        (encoding_rs::GBK, "GBK"),
    ] {
        let (client, mut server) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            assert_eq!(receive(&mut server).await[0], 1);
            send(
                &mut server,
                Packet::Version(Version {
                    version: 3,
                    extensions: Default::default(),
                }),
            )
            .await;
            let realpath = receive(&mut server).await;
            assert_eq!(realpath[0], 16);
            let id = u32::from_be_bytes(realpath[1..5].try_into().unwrap());
            names(&mut server, id, "/测试目录", wire).await;
            let open = receive(&mut server).await;
            assert_eq!(open[0], 11);
            assert_eq!(&open[9..], wire.encode("/测试目录").0.as_ref());
            let id = u32::from_be_bytes(open[1..5].try_into().unwrap());
            send(
                &mut server,
                Packet::Handle(russh_sftp::protocol::Handle {
                    id,
                    handle: "opaque-中文".into(),
                }),
            )
            .await;
            for index in 0..3 {
                let request = receive(&mut server).await;
                assert_eq!(request[0], if index < 2 { 12 } else { 4 });
                assert_eq!(&request[9..], "opaque-中文".as_bytes());
                let id = u32::from_be_bytes(request[1..5].try_into().unwrap());
                if index == 0 {
                    names(&mut server, id, "测试.txt", wire).await;
                } else {
                    send(
                        &mut server,
                        Packet::Status(Status {
                            id,
                            status_code: if index == 1 {
                                StatusCode::Eof
                            } else {
                                StatusCode::Ok
                            },
                            error_message: String::new(),
                            language_tag: String::new(),
                        }),
                    )
                    .await;
                }
            }
            let stat = receive(&mut server).await;
            assert_eq!(stat[0], 17);
            assert_eq!(&stat[9..], wire.encode("/测试目录/测试.txt").0.as_ref());
            let id = u32::from_be_bytes(stat[1..5].try_into().unwrap());
            send(
                &mut server,
                Packet::Attrs(russh_sftp::protocol::Attrs {
                    id,
                    attrs: Default::default(),
                }),
            )
            .await;
        });
        let sftp = SftpSession::new_with_config(
            client,
            SftpConfig {
                filename_encoding: configured.into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let directory = sftp.canonicalize("/").await.unwrap();
        assert_eq!(directory, "/测试目录");
        assert_eq!(sftp.filename_encoding(), wire.name());
        let file = sftp.read_dir(directory).await.unwrap().next().unwrap();
        assert_eq!(file.file_name(), "测试.txt");
        sftp.metadata(file.path()).await.unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn invalid_filename_bytes_fail_without_replacement_or_request_timeout() {
    let (client, mut server) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move {
        receive(&mut server).await;
        send(
            &mut server,
            Packet::Version(Version {
                version: 3,
                extensions: Default::default(),
            }),
        )
        .await;
        let request = receive(&mut server).await;
        let id = u32::from_be_bytes(request[1..5].try_into().unwrap());
        let payload = russh_sftp::ser::to_bytes(&Name {
            id,
            files: vec![File::dummy("x")],
        })
        .unwrap();
        let mut payload = payload.to_vec();
        payload[12] = 0xff; // Invalid in both UTF-8 and GBK.
        server.write_u32((payload.len() + 1) as u32).await.unwrap();
        server.write_u8(104).await.unwrap();
        server.write_all(&payload).await.unwrap();
        std::future::pending::<()>().await;
    });
    let sftp = SftpSession::new(client).await.unwrap();
    let error = timeout(Duration::from_secs(1), sftp.canonicalize("/"))
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        error.to_string().contains("cannot be decoded losslessly"),
        "{error}"
    );
    assert_eq!(sftp.filename_encoding(), "UTF-8");
    task.abort();
}
