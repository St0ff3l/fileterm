async fn receive_ymodem<S>(
    stream: &mut S,
    directory: &Path,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !directory.is_dir() {
        return Err("YMODEM 接收目录不存在".to_string());
    }
    let (mut control, use_crc) = receive_protocol_start(stream, timing, cancellation).await?;
    if control != SOH {
        return Err("YMODEM 文件头格式无效".to_string());
    }

    let mut total = 0_u64;
    loop {
        let Some((sequence, header)) =
            read_packet_tail(stream, 128, use_crc, timing, cancellation).await?
        else {
            // The complete frame was consumed (or its partial tail was
            // drained), so NAK is safe and the sender can retransmit a fresh
            // block-0 without leaving bytes from the old frame in front of it.
            write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
            control = read_next_protocol_byte(stream, timing, cancellation).await?;
            if control == CAN {
                return Err("对端取消了 YMODEM 传输".to_string());
            }
            continue;
        };
        if sequence != 0 {
            return Err("YMODEM 文件头序号无效".to_string());
        }
        let Some((file_name, size)) = parse_ymodem_header(&header)? else {
            // An empty block-0 is the standard end-of-batch marker.
            write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
            break;
        };
        let target = ymodem_target(directory, &file_name)?;
        let file_result = receive_ymodem_file(
            stream,
            &target,
            YmodemFileOptions {
                size,
                use_crc,
                offset: total,
            },
            timing,
            budget,
            reporter,
            cancellation,
        )
        .await;
        let received = match file_result {
            Ok(received) => received,
            Err(error) => return Err(error),
        };
        total = total.saturating_add(received);

        // The receiver announces that it is ready for the next block-0. A
        // legacy single-file sender may stop after the EOT ACK, so a timeout
        // here is treated as a successful end of the batch.
        write_all(
            stream,
            &[ACK, if use_crc { CRC_REQUEST } else { NAK }],
            timing.write_timeout,
            cancellation,
        )
        .await?;
        control = match read_byte(stream, timing.control_timeout(), cancellation).await? {
            Some(next) => next,
            None => break,
        };
        if control != SOH {
            return Err("YMODEM 下一个文件头格式无效".to_string());
        }
    }
    Ok(total)
}

async fn receive_ymodem_file<S>(
    stream: &mut S,
    path: &Path,
    options: YmodemFileOptions,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    budget.begin_file(Some(options.size))?;
    let mut file = Some(StagedReceiveFile::create(path, budget.max_file_bytes()).await?);
    let result: Result<u64, String> = async {
        write_all(
            stream,
            &[ACK, if options.use_crc { CRC_REQUEST } else { NAK }],
            timing.write_timeout,
            cancellation,
        )
        .await?;
        let mut expected = 1_u8;
        let mut remaining = options.size;
        let mut total = 0_u64;
        let mut eot_seen = false;
        loop {
            let Some(control) = read_byte(stream, timing.control_timeout(), cancellation).await?
            else {
                return Err("等待 YMODEM 数据超时".to_string());
            };
            match control {
                EOT => {
                    if !eot_seen {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        eot_seen = true;
                        continue;
                    }
                    write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    break;
                }
                SOH | STX => {
                    let block_size = if control == SOH { 128 } else { 1024 };
                    let Some((sequence, payload)) =
                        read_packet_tail(stream, block_size, options.use_crc, timing, cancellation)
                            .await?
                    else {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        continue;
                    };
                    if sequence != expected {
                        if sequence == expected.wrapping_sub(1) {
                            write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                            continue;
                        }
                        write_all(stream, &[CAN, CAN], timing.write_timeout, cancellation).await?;
                        return Err("YMODEM 数据块序号不连续".to_string());
                    }
                    if remaining == 0 {
                        write_all(stream, &[CAN, CAN], timing.write_timeout, cancellation).await?;
                        return Err("YMODEM 接收数据超过文件头声明的大小".to_string());
                    }
                    let count = remaining.min(payload.len() as u64) as usize;
                    file.as_mut()
                        .expect("YMODEM receive file exists")
                        .write_all(&payload[..count], cancellation, None)
                        .await?;
                    remaining -= count as u64;
                    total += count as u64;
                    reporter.report(
                        options.offset.saturating_add(total),
                        Some(u64::from(sequence)),
                    );
                    expected = expected.wrapping_add(1);
                    write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                }
                CAN => return Err("对端取消了 YMODEM 传输".to_string()),
                _ => {}
            }
        }
        if remaining != 0 {
            return Err("YMODEM 文件大小与接收数据不一致".to_string());
        }
        let file = file.take().expect("YMODEM receive file exists");
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
