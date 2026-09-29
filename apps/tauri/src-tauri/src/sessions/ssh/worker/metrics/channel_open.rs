/// Keep ownership of an in-flight open even after its caller times out.
/// A semaphore bounds unresolved opens to one per collector/SSH session.
async fn open_metrics_channel<H: russh::client::Handler + 'static>(
    cancellation: &CancellationToken,
    connection_cancellation: CancellationToken,
    gate: Arc<tokio::sync::Semaphore>,
    handle: Arc<Handle<H>>,
    deadline: Duration,
) -> Result<Result<Channel<russh::client::Msg>, russh::Error>, ()> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let operation = async {
        let permit = gate.acquire_owned().await.map_err(|_| ())?;
        // Do not cancel this task: russh cannot retract a sent channel-open
        // request. A late confirmation must still be received and closed.
        tokio::spawn(async move {
            let result = tokio::select! {
                biased;
                // The whole SSH worker is ending, so no further channel on
                // this connection will be used. Release the retained handle.
                _ = connection_cancellation.cancelled() => return,
                result = handle.channel_open_session() => result,
            }
            .map(|channel| PendingMetricsChannel {
                channel: Some(channel),
                permit: Some(permit),
                connection_cancellation,
            });
            // If the receiver disappeared, dropping the result closes the
            // channel. The guard also covers cancellation after send succeeds.
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| ())
            .map(|result| result.map(|mut pending| pending.channel.take().expect("opened channel")))
    };
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(()),
        result = timeout(deadline, operation) => result.unwrap_or(Err(())),
    }
}

struct PendingMetricsChannel {
    channel: Option<Channel<russh::client::Msg>>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    connection_cancellation: CancellationToken,
}

impl Drop for PendingMetricsChannel {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            let permit = self.permit.take();
            let connection_cancellation = self.connection_cancellation.clone();
            tokio::spawn(async move {
                // Hold the permit until the close is queued or SSH ends.
                // A timeout here would abandon cleanup again.
                tokio::select! {
                    _ = connection_cancellation.cancelled() => {},
                    _ = channel.close() => {},
                }
                drop(permit);
            });
        }
    }
}
