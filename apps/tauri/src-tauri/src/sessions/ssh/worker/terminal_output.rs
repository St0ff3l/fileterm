/// Start the bounded terminal output pump used by the SSH worker loop.
///
/// The worker only performs a non-blocking `try_send`; this task owns the
/// renderer-facing async emit so terminal input and command dispatch stay
/// responsive while a remote process produces a large amount of output.
fn spawn_terminal_output_pump(app: &AppHandle, tab_id: &str) -> mpsc::Sender<String> {
    let (terminal_output_tx, mut terminal_output_rx) = mpsc::channel::<String>(128);
    let pump_app = app.clone();
    let pump_tab_id = tab_id.to_string();
    tokio::spawn(async move {
        while let Some(chunk) = terminal_output_rx.recv().await {
            emit_terminal_data(&pump_app, &pump_tab_id, &chunk).await;
        }
    });

    terminal_output_tx
}
