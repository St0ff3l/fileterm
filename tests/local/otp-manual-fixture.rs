// Interactive fixture only: it never starts during the normal test suite.
#[tokio::test]
#[ignore = "Local interactive SSH OTP server; stop with Ctrl-C"]
async fn otp_manual_fixture() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = server::Config {
        inactivity_timeout: None,
        auth_rejection_time: Duration::from_millis(1),
        ..Default::default()
    };
    config.keys.push(
        PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
    );
    let config = Arc::new(config);
    println!("\nOTP test SSH: {}\nAuth: keyboard-interactive / MFA\nUsername: alice\nOTP test key: {OTP_FIXTURE_SECRET}\nDisable SFTP, exec and monitoring. Ctrl-C stops this fixture.\n", listener.local_addr().unwrap());
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            accepted = listener.accept() => {
                let (socket, _) = accepted.unwrap();
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(running) = server::run_stream(config, socket, OtpFixture {
                        prompt: "Verification code:", repeat: false, extra_prompt: false, attempts: Arc::new(AtomicUsize::new(0)),
                    }).await { let _ = running.await; }
                });
            }
        }
    }
}
