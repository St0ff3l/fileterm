async fn send_raw<S>(
    stream: &mut S,
    path: &Path,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncWrite + Unpin,
{
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件信息：{error}"))?;
    budget.begin_file(Some(metadata.len()))?;
    let mut file = File::open(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件：{error}"))?;
    reporter.set_total(Some(metadata.len()));
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut total = 0_u64;
    loop {
        let count = read_file(&mut file, &mut buffer, cancellation).await?;
        if count == 0 {
            break;
        }
        write_all(stream, &buffer[..count], timing.write_timeout, cancellation).await?;
        total += count as u64;
        reporter.report(total, None);
    }
    flush(stream, timing.write_timeout, cancellation).await?;
    Ok(total)
}

async fn receive_raw<S>(
    stream: &mut S,
    path: &Path,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + Unpin,
{
    let mut file: Option<StagedReceiveFile> = None;
    let mut buffer = vec![0_u8; 32 * 1024];
    let result: Result<u64, String> = async {
        let mut total = 0_u64;
        loop {
            let count = tokio::select! {
                _ = cancellation.cancelled() => return Err("串口接收已取消".to_string()),
                result = timeout(timing.raw_idle_timeout, stream.read(&mut buffer)) => match result {
                    Ok(Ok(count)) => count,
                    Ok(Err(error)) => return Err(format!("串口接收失败：{error}")),
                    Err(_) => break,
                },
            };
            if count == 0 {
                break;
            }
            if file.is_none() {
                budget.begin_file(None)?;
                file = Some(StagedReceiveFile::create(path, budget.max_file_bytes()).await?);
            }
            let target = file.as_mut().expect("file was created above");
            target
                .write_all(&buffer[..count], cancellation, Some(budget))
                .await?;
            total += count as u64;
            reporter.report(total, None);
        }
        if let Some(file) = file.take() {
            file.commit().await?;
        }
        Ok(total)
    }
    .await;
    if result.is_err() {
        if let Some(file) = file.take() {
            file.cleanup().await;
        }
    }
    result
}
