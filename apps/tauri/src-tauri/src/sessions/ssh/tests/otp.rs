struct OtpFixture {
    prompt: &'static str,
    repeat: bool,
    attempts: Arc<AtomicUsize>,
    extra_prompt: bool,
}

const OTP_FIXTURE_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

impl server::Handler for OtpFixture {
    type Error = russh::Error;

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<server::Auth, Self::Error> {
        if let Some(response) = response {
            let answers: Vec<_> = response
                .map(|answer| String::from_utf8_lossy(&answer).into_owned())
                .collect();
            let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
            if !(self.repeat && attempt == 0) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                // The independent server oracle accepts the adjacent window at a clock boundary.
                let valid = [now.saturating_sub(30), now, now + 30]
                    .iter()
                    .any(|seconds| {
                        let expected = otp_fixture_code(*seconds);
                        answers == vec![expected; if self.extra_prompt { 2 } else { 1 }]
                    });
                return Ok(if valid {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                });
            }
        }
        let mut prompts = vec![(Cow::Borrowed(self.prompt), false)];
        if self.extra_prompt {
            prompts.push((Cow::Borrowed("OTP code:"), true));
        }
        Ok(server::Auth::Partial {
            name: Cow::Borrowed("FileTerm OTP fixture"),
            instructions: Cow::Borrowed("Authenticator code"),
            prompts: Cow::Owned(prompts),
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: russh::ChannelId,
        _term: &str,
        _cols: u32,
        _rows: u32,
        _width: u32,
        _height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, b"\r\nFileTerm OTP authentication passed.\r\nThis is a test echo terminal, not a system shell.\r\n".to_vec())?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, data.to_vec())?;
        Ok(())
    }
}

fn otp_fixture_code(seconds: u64) -> String {
    // Implement the server's HMAC with Digest so the protocol test does not call generate().
    use sha1::{Digest, Sha1};
    let mut inner_key = [0x36; 64];
    let mut outer_key = [0x5c; 64];
    for (index, byte) in b"12345678901234567890".iter().enumerate() {
        inner_key[index] ^= byte;
        outer_key[index] ^= byte;
    }
    let inner = Sha1::new()
        .chain_update(inner_key)
        .chain_update((seconds / 30).to_be_bytes())
        .finalize();
    let hash = Sha1::new()
        .chain_update(outer_key)
        .chain_update(inner)
        .finalize();
    let offset = (hash[19] & 15) as usize;
    let number = u32::from_be_bytes(hash[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    format!("{:06}", number % 1_000_000)
}

#[tokio::test]
async fn otp_autofill_real_ssh_success_rejection_and_manual_fallback() {
    assert_eq!(otp_fixture_code(59), "287082");
    for (secret, repeat, prompt, configured, extra_prompt, manual_count, succeeds) in [
        (
            Some(OTP_FIXTURE_SECRET),
            false,
            "OTP code:",
            None,
            false,
            0,
            true,
        ),
        (
            Some("JBSWY3DPEHPK3PXP"),
            false,
            "OTP code:",
            None,
            false,
            0,
            false,
        ),
        (
            Some(OTP_FIXTURE_SECRET),
            true,
            "OTP code:",
            None,
            false,
            1,
            true,
        ),
        (None, false, "OTP code:", None, false, 1, true),
        (
            Some(OTP_FIXTURE_SECRET),
            false,
            "SMS verification code:",
            None,
            false,
            1,
            true,
        ),
        (
            Some(OTP_FIXTURE_SECRET),
            false,
            "Company response:",
            Some("company response"),
            false,
            0,
            true,
        ),
        (
            Some(OTP_FIXTURE_SECRET),
            false,
            "OTP code:",
            Some("company response"),
            false,
            1,
            true,
        ),
        (
            Some(OTP_FIXTURE_SECRET),
            false,
            "OTP code:",
            None,
            true,
            1,
            true,
        ),
    ] {
        let attempts = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut config = server::Config {
            inactivity_timeout: None,
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        };
        config.keys.push(
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
        );
        let server_attempts = attempts.clone();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let running = server::run_stream(
                Arc::new(config),
                socket,
                OtpFixture {
                    prompt,
                    repeat,
                    attempts: server_attempts,
                    extra_prompt,
                },
            )
            .await
            .unwrap();
            let _ = running.await;
        });
        let mut handle = client::connect(
            Arc::new(client::Config::default()),
            address,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let manual = Arc::new(AtomicUsize::new(0));
        let manual_requests = manual.clone();
        let authenticated = timeout(
            Duration::from_secs(10),
            try_keyboard_interactive_with_responder(
                &mut handle,
                "alice",
                None,
                secret,
                configured,
                KeyboardInteractiveMode::AdditionalFactor,
                move |request| {
                    manual_requests.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(request.prompts[0].prompt, prompt);
                    let count = request.prompts.len();
                    async move {
                        Some(vec![
                            otp_fixture_code(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs(),
                            );
                            count
                        ])
                    }
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(authenticated, succeeds);
        assert_eq!(manual.load(Ordering::SeqCst), manual_count);
        assert_eq!(attempts.load(Ordering::SeqCst), if repeat { 2 } else { 1 });
        if authenticated {
            let mut shell = handle.channel_open_session().await.unwrap();
            shell
                .request_pty(true, "xterm", 80, 24, 0, 0, &[])
                .await
                .unwrap();
            shell.request_shell(true).await.unwrap();
            wait_for_channel_text(&mut shell, "OTP authentication passed").await;
        }
        drop(handle);
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }
}
