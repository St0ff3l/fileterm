async fn connect_target_through_jump(
    jump_handle: &Handle<ClientHandler>,
    config: Arc<russh::client::Config>,
    handler: ClientHandler,
    host: &str,
    port: u16,
    connect_timeout: Duration,
    interaction_timeout: Duration,
) -> Result<Handle<ClientHandler>, String> {
    let host_verification_waiting = handler.host_verification_waiting.clone();
    let log_app = handler.app.clone();
    let log_tab_id = handler.tab_id.clone();
    let channel = wait_for_ssh_stage("SSH jump-host channel setup", connect_timeout, async {
        jump_handle
            .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
            .await
            .map_err(|error| format!("Jump Host direct-tcpip failed: {error}"))
    })
    .await?;
    crate::services::logging::session(
        &log_app,
        "INFO",
        "ssh",
        &log_tab_id,
        format!("socket connected via jump host target={host}:{port}"),
    );
    let result = wait_for_ssh_handshake_with_network_timeout(
        "SSH handshake via jump host",
        host_verification_waiting,
        connect_timeout,
        interaction_timeout,
        async {
            russh::client::connect_stream(config, channel.into_stream(), handler)
                .await
                .map_err(|error| format!("SSH connect via jump host failed: {error}"))
        },
    )
    .await;
    if result.is_ok() {
        crate::services::logging::session(
            &log_app,
            "INFO",
            "ssh",
            &log_tab_id,
            "SSH handshake completed via jump host",
        );
    }
    result
}
