async fn send_xmodem<S>(
    stream: &mut S,
    path: &Path,
    use_crc: bool,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let negotiated_crc = wait_for_sender_start(stream, timing, cancellation).await?;
    let use_crc = use_crc || negotiated_crc;
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件信息：{error}"))?;
    budget.begin_file(Some(metadata.len()))?;
    reporter.set_total(Some(metadata.len()));
    send_blocks(
        stream,
        path,
        BlockTransferOptions {
            block_size: 128,
            use_crc,
            offset: 0,
        },
        timing,
        reporter,
        cancellation,
    )
    .await
}

async fn send_ymodem<S>(
    stream: &mut S,
    paths: &[String],
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if paths.is_empty() {
        return Err("串口 YMODEM 至少需要一个发送文件".to_string());
    }
    let mut total_size = 0_u64;
    let mut seen_names = HashSet::new();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let path = Path::new(path);
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|error| format!("无法读取串口发送文件信息：{error}"))?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "串口 YMODEM 文件名无效".to_string())?;
        if !is_safe_transfer_file_name(file_name) {
            return Err("串口 YMODEM 文件名无效".to_string());
        }
        let header_text = format!("{}\0{}\0", file_name, metadata.len());
        if header_text.len() > 128 {
            return Err("串口 YMODEM 文件头过长，请缩短文件名".to_string());
        }
        if !seen_names.insert(file_name.to_lowercase()) {
            return Err("串口 YMODEM 文件名重复，无法安全接收".to_string());
        }
        budget.begin_file(Some(metadata.len()))?;
        total_size = total_size.saturating_add(metadata.len());
        files.push((path.to_path_buf(), metadata.len(), file_name.to_string()));
    }
    reporter.set_total(Some(total_size));

    let mut total = 0_u64;
    for (path, size, file_name) in files {
        let header_crc = wait_for_sender_start(stream, timing, cancellation).await?;
        let mut header = vec![0_u8; 128];
        let header_text = format!("{}\0{}\0", file_name, size);
        let header_bytes = header_text.as_bytes();
        header[..header_bytes.len()].copy_from_slice(header_bytes);
        send_packet(stream, 0, &header, header_crc, timing, cancellation).await?;
        // The receiver sends another C after ACKing the block-0 metadata.
        let data_crc = wait_for_sender_start(stream, timing, cancellation).await?;
        total = total.saturating_add(
            send_blocks(
                stream,
                &path,
                BlockTransferOptions {
                    block_size: 1024,
                    use_crc: data_crc,
                    offset: total,
                },
                timing,
                reporter,
                cancellation,
            )
            .await?,
        );
    }
    // Standard YMODEM terminates with an empty block-0 after the receiver
    // acknowledges EOT. Older peers may stop after EOT, so the final header
    // is negotiated with the same bounded timeout used at startup.
    let final_crc = wait_for_sender_start(stream, timing, cancellation).await?;
    send_packet(stream, 0, &[0_u8; 128], final_crc, timing, cancellation).await?;
    Ok(total)
}

async fn send_blocks<S>(
    stream: &mut S,
    path: &Path,
    options: BlockTransferOptions,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut file = File::open(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件：{error}"))?;
    let mut block = vec![PAD; options.block_size];
    let mut sequence = 1_u8;
    let mut total = 0_u64;
    loop {
        let count = read_file(&mut file, &mut block, cancellation).await?;
        if count == 0 {
            break;
        }
        block[count..].fill(PAD);
        send_packet(
            stream,
            sequence,
            &block,
            options.use_crc,
            timing,
            cancellation,
        )
        .await?;
        total += count as u64;
        reporter.report(
            options.offset.saturating_add(total),
            Some(u64::from(sequence)),
        );
        sequence = sequence.wrapping_add(1);
        block.fill(PAD);
    }
    send_eot(stream, timing, cancellation).await?;
    Ok(total)
}

async fn receive_xmodem<S>(
    stream: &mut S,
    path: &Path,
    timing: SerialTransferTiming,
    preserve_padding: bool,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut control, use_crc) = receive_protocol_start(stream, timing, cancellation).await?;
    budget.begin_file(None)?;
    let mut file = Some(StagedReceiveFile::create(path, budget.max_file_bytes()).await?);
    let result: Result<u64, String> = async {
        let mut expected = 1_u8;
        let mut pending_last: Option<Vec<u8>> = None;
        let mut total = 0_u64;
        let mut eot_seen = false;
        loop {
            match control {
                EOT => {
                    if !eot_seen {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        eot_seen = true;
                        control = read_next_protocol_byte(stream, timing, cancellation).await?;
                        continue;
                    }
                    write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    if let Some(mut last) = pending_last.take() {
                        last = finalize_xmodem_payload(last, preserve_padding);
                        file.as_mut()
                            .expect("XMODEM receive file exists")
                            .write_all(&last, cancellation, Some(budget))
                            .await?;
                        total += last.len() as u64;
                    }
                    break;
                }
                SOH | STX => {
                    let block_size = if control == SOH { 128 } else { 1024 };
                    let Some((sequence, payload)) =
                        read_packet_tail(stream, block_size, use_crc, timing, cancellation).await?
                    else {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        control = read_next_protocol_byte(stream, timing, cancellation).await?;
                        continue;
                    };
                    if sequence == expected {
                        if let Some(previous) = pending_last.replace(payload) {
                            file.as_mut()
                                .expect("XMODEM receive file exists")
                                .write_all(&previous, cancellation, Some(budget))
                                .await?;
                            total += previous.len() as u64;
                            reporter.report(total, Some(u64::from(sequence.wrapping_sub(1))));
                        }
                        expected = expected.wrapping_add(1);
                        write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    } else if sequence == expected.wrapping_sub(1) {
                        // The ACK may have been lost; acknowledge a duplicate
                        // without writing it twice.
                        write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    } else {
                        write_all(stream, &[CAN, CAN], timing.write_timeout, cancellation).await?;
                        return Err("XMODEM 数据块序号不连续".to_string());
                    }
                }
                CAN => return Err("对端取消了 XMODEM 传输".to_string()),
                _ => {}
            }
            control = read_next_protocol_byte(stream, timing, cancellation).await?;
        }
        let file = file.take().expect("XMODEM receive file exists");
        file.commit().await?;
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

fn finalize_xmodem_payload(mut payload: Vec<u8>, preserve_padding: bool) -> Vec<u8> {
    if !preserve_padding {
        while payload.last() == Some(&PAD) {
            payload.pop();
        }
    }
    payload
}
