/// Register and start the SSH tunnel rules owned by this session.
///
/// Tunnel control has its own serialized queue so server-side forwarding
/// requests cannot block the terminal worker. Invalid rules are reported in
/// the terminal, while valid `autoStart` rules are queued after registration.
async fn start_tunnel_command_runtime(
    profile: &Value,
    tab_id: &str,
    app: &AppHandle,
    handle: &Arc<Handle<ClientHandler>>,
) -> mpsc::UnboundedSender<TunnelCommand> {
    let mut tunnel_manager = TunnelManager::new(tab_id, app, Arc::clone(handle));
    let mut auto_start_tunnel_ids = Vec::new();
    if let Some(rules) = profile.get("forwards").and_then(Value::as_array) {
        for raw_rule in rules {
            match serde_json::from_value::<SshTunnelRule>(raw_rule.clone()) {
                Ok(rule) => {
                    let should_start = rule.auto_start;
                    if let Err(error) = tunnel_manager.register(rule.clone(), false) {
                        emit_terminal_data(
                            app,
                            tab_id,
                            &format!("[tunnel] 忽略无效规则: {error}\r\n"),
                        )
                        .await;
                    } else if should_start {
                        auto_start_tunnel_ids.push(rule.id);
                    }
                }
                Err(error) => {
                    emit_terminal_data(app, tab_id, &format!("[tunnel] 解析规则失败: {error}\r\n"))
                        .await
                }
            }
        }
    }

    let (tunnel_command_tx, tunnel_command_rx) = mpsc::unbounded_channel();
    tokio::spawn(run_tunnel_command_loop(tunnel_manager, tunnel_command_rx));
    for rule_id in auto_start_tunnel_ids {
        let (respond_to, response_rx) = oneshot::channel();
        enqueue_tunnel_command(
            &tunnel_command_tx,
            TunnelCommand::Start {
                rule_id: rule_id.clone(),
                respond_to,
            },
        );
        let auto_tunnel_app = app.clone();
        let auto_tunnel_tab_id = tab_id.to_string();
        tokio::spawn(async move {
            match response_rx.await {
                Ok(Err(error)) => {
                    emit_terminal_data(
                        &auto_tunnel_app,
                        &auto_tunnel_tab_id,
                        &format!("[tunnel] 自动启动 {rule_id} 失败: {error}\r\n"),
                    )
                    .await;
                }
                Err(_) => {
                    crate::services::logging::session(
                        &auto_tunnel_app,
                        "WARN",
                        "tunnel",
                        &auto_tunnel_tab_id,
                        format!("auto-start response dropped id={rule_id}"),
                    );
                }
                Ok(Ok(_)) => {}
            }
        });
    }

    tunnel_command_tx
}
