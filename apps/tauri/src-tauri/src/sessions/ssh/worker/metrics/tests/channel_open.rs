#[cfg(test)]
mod monitoring_channel_open_tests {
    use super::*;
    use russh::{client, server};

    struct DelayedServer {
        release: Arc<tokio::sync::Notify>,
        opened: mpsc::UnboundedSender<()>,
        closed: mpsc::UnboundedSender<()>,
    }

    impl server::Handler for DelayedServer {
        type Error = russh::Error;

        async fn auth_password(&mut self, _: &str, _: &str) -> Result<server::Auth, Self::Error> {
            Ok(server::Auth::Accept)
        }

        async fn channel_open_session(
            &mut self,
            _channel: Channel<server::Msg>,
            reply: server::ChannelOpenHandle,
            _: &mut server::Session,
        ) -> Result<(), Self::Error> {
            let _ = self.opened.send(());
            self.release.notified().await;
            reply.accept().await;
            Ok(())
        }

        async fn channel_close(
            &mut self,
            _: russh::ChannelId,
            _: &mut server::Session,
        ) -> Result<(), Self::Error> {
            let _ = self.closed.send(());
            Ok(())
        }
    }

    struct TestClient;

    impl client::Handler for TestClient {
        type Error = russh::Error;

        async fn check_server_key(
            &mut self,
            _: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn timed_out_and_cancelled_opens_close_late_channels_and_bound_pending_requests() {
        timeout(Duration::from_secs(10), async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let release = Arc::new(tokio::sync::Notify::new());
            let (opened, mut opens) = mpsc::unbounded_channel();
            let (closed, mut closes) = mpsc::unbounded_channel();
            let mut config = server::Config::default();
            config.keys.push(
                russh::keys::ssh_key::PrivateKey::random(
                    &mut rand::rng(),
                    russh::keys::ssh_key::Algorithm::Ed25519,
                )
                .unwrap(),
            );
            let fixture = DelayedServer {
                release: release.clone(),
                opened,
                closed,
            };
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let running = server::run_stream(Arc::new(config), socket, fixture)
                    .await
                    .unwrap();
                let _ = running.await;
            });
            let mut handle =
                client::connect(Arc::new(client::Config::default()), address, TestClient)
                    .await
                    .unwrap();
            assert!(handle
                .authenticate_password("fixture", "fixture")
                .await
                .unwrap()
                .success());
            let handle = Arc::new(handle);
            let gate = Arc::new(tokio::sync::Semaphore::new(1));
            for cancel_instead_of_timeout in [false, true] {
                let token = CancellationToken::new();
                let worker_token = token.clone();
                let worker_gate = gate.clone();
                let worker_handle = handle.clone();
                let pending = tokio::spawn(async move {
                    open_metrics_channel(
                        &worker_token,
                        CancellationToken::new(),
                        worker_gate,
                        worker_handle,
                        if cancel_instead_of_timeout {
                            Duration::from_secs(5)
                        } else {
                            Duration::from_millis(100)
                        },
                    )
                    .await
                });
                opens.recv().await.unwrap();
                if cancel_instead_of_timeout {
                    token.cancel();
                }
                assert!(pending.await.unwrap().is_err());
                // Retry while the original request is still unresolved: no new
                // SSH channel may be sent until its late result is cleaned up.
                assert!(open_metrics_channel(
                    &CancellationToken::new(),
                    CancellationToken::new(),
                    gate.clone(),
                    handle.clone(),
                    Duration::from_millis(30)
                )
                .await
                .is_err());
                assert!(opens.try_recv().is_err());
                release.notify_one();
                closes.recv().await.unwrap();
            }
            // A normal successful open is handed to its caller, not cleaned up.
            release.notify_one();
            let channel = open_metrics_channel(
                &CancellationToken::new(),
                CancellationToken::new(),
                gate.clone(),
                handle.clone(),
                Duration::from_secs(2),
            )
            .await
            .unwrap()
            .unwrap();
            opens.recv().await.unwrap();
            assert!(closes.try_recv().is_err());
            channel.close().await.unwrap();
            closes.recv().await.unwrap();
            // Closing the SSH worker releases a never-confirmed open as well.
            let connection = CancellationToken::new();
            let worker_connection = connection.clone();
            let worker_gate = gate.clone();
            let worker_handle = handle.clone();
            let pending = tokio::spawn(async move {
                open_metrics_channel(
                    &CancellationToken::new(),
                    worker_connection,
                    worker_gate,
                    worker_handle,
                    Duration::from_secs(5),
                )
                .await
            });
            opens.recv().await.unwrap();
            connection.cancel();
            assert!(pending.await.unwrap().is_err());
            assert_eq!(gate.available_permits(), 1);
            release.notify_one();
            handle
                .disconnect(russh::Disconnect::ByApplication, "done", "en")
                .await
                .unwrap();
            server.abort();
        })
        .await
        .unwrap();
    }
}
