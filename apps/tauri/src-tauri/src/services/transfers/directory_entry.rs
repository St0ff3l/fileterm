// 目录传输中单个文件条目的一次性执行：源身份校验 → 已完成跳过探测 →
// 断点计划 → 数据传输 → 大小校验 → 原子提交。
// 重试策略（瞬时退避/永久跳过）由 directory_execution.rs 的编排层负责。

/// 单个目录条目一次尝试的结果。
enum DirectoryEntryAttempt {
    /// 本次尝试完成传输并提交（rename/finalize 已执行）。
    Transferred,
    /// 目标已存在且大小吻合，无需重传（断点恢复时的跳过路径）。
    AlreadyDone,
}

/// 执行一个目录条目的完整单次尝试：源身份校验 → 已完成跳过探测 →
/// 断点计划 → 数据传输 → 大小校验 → 原子提交。
///
/// 调用方负责瞬时/永久错误分类、重试退避与失败条目标记。
#[allow(clippy::too_many_arguments)]
async fn transfer_directory_entry_once(
    app: &AppHandle,
    transfer_id: &str,
    tab_id: &str,
    direction: &str,
    session_type: Option<&str>,
    cancel: &CancellationToken,
    resume_requested: bool,
    manifest: &mut TransferManifest,
    index: usize,
    current_transferred: u64,
    total_bytes: u64,
    throttler: &mut DirectoryTransferThrottler,
) -> Result<DirectoryEntryAttempt, AppError> {
    let entry = manifest.files[index].clone();
    let current_identity = if direction == "upload" {
        stat_local_transfer_file(&entry.source_path)
            .await
            .ok_or_else(|| {
                transfer_error(format!("上传源文件不存在或无法读取：{}", entry.relative_path))
            })?
    } else {
        let stat = worker_call_with_cancel(app, tab_id, cancel, |respond_to, token| {
            WorkerCmd::StatRemoteFile {
                path: entry.source_path.clone(),
                cancellation: token,
                respond_to,
            }
        })
        .await?
        .ok_or_else(|| {
            transfer_error(format!("下载源文件不存在或无法读取：{}", entry.relative_path))
        })?;
        TransferFileIdentity {
            size: stat.size,
            modified_at: stat.modified_at,
        }
    };
    if !same_transfer_identity(&current_identity, &entry.source_identity) {
        return Err(transfer_error(format!(
            "源文件已发生变化，不能继续目录断点：{}",
            entry.relative_path
        )));
    }

    if entry.status == "done" {
        let destination = if direction == "upload" {
            worker_call_with_cancel(app, tab_id, cancel, |respond_to, token| {
                WorkerCmd::StatRemoteFile {
                    path: entry.destination_path.clone(),
                    cancellation: token,
                    respond_to,
                }
            })
            .await?
            .map(|value| TransferFileIdentity {
                size: value.size,
                modified_at: value.modified_at,
            })
        } else {
            stat_local_transfer_file(&entry.destination_path).await
        };
        if destination
            .as_ref()
            .is_some_and(|value| value.size == entry.source_identity.size)
        {
            return Ok(DirectoryEntryAttempt::AlreadyDone);
        }
    }

    let is_fresh_entry =
        !resume_requested || (entry.status == "pending" && entry.transferred_bytes == 0);
    let (upload_plan, offset) = if direction == "upload" {
        if is_fresh_entry {
            let upload_path = entry
                .staging_path
                .clone()
                .unwrap_or_else(|| entry.partial_path.clone());
            (
                RemoteUploadPlan {
                    upload_path,
                    resume_offset: 0,
                    upload_needed: true,
                    partial_ready: false,
                },
                0,
            )
        } else {
            let plan = prepare_remote_upload(
                app,
                tab_id,
                &entry.partial_path,
                entry.staging_path.as_deref(),
                entry.source_identity.size,
                Some(cancel),
            )
            .await?;
            let offset = plan.resume_offset;
            (plan, offset)
        }
    } else {
        let offset = if is_fresh_entry {
            0
        } else {
            stat_local_transfer_file(&entry.partial_path)
                .await
                .map(|value| value.size)
                .unwrap_or(0)
        };
        (
            RemoteUploadPlan {
                upload_path: String::new(),
                resume_offset: offset,
                upload_needed: true,
                partial_ready: false,
            },
            offset,
        )
    };
    if offset > entry.source_identity.size {
        return Err(transfer_error(format!(
            "断点文件大于源文件：{}",
            entry.relative_path
        )));
    }

    manifest.files[index].status = "running".to_string();
    manifest.files[index].transferred_bytes = offset;

    let start_delivery = throttler.delivery_for_file_start();
    update_directory_manifest(
        app,
        transfer_id,
        manifest,
        DirectoryManifestPatch {
            status: "running",
            message: Some(if offset > 0 {
                format!("{}（从 {offset} bytes 继续）", entry.relative_path)
            } else {
                entry.relative_path.clone()
            }),
            transferred: current_transferred.saturating_add(offset),
            total: total_bytes,
            delivery: start_delivery,
        },
    )
    .await?;

    if direction == "upload" {
        if upload_plan.upload_needed {
            worker_data_call_with_cancel(app, tab_id, cancel, |respond_to, _token| {
                WorkerCmd::UploadLocalFile {
                    local_path: entry.source_path.clone(),
                    remote_path: upload_plan.upload_path.clone(),
                    resume_offset: offset,
                    transfer_id: transfer_id.to_string(),
                    cancel: cancel.clone(),
                    verify_checksum: false,
                    respond_to,
                }
            })
            .await?;
        }
        // FTP 没有类似 SSH MAC 的传输层完整性保证，上传完成后需一次
        // 远端大小比对兜底；SSH 传输层已保证完整性，跳过该往返。
        if session_type == Some("ftp") && upload_plan.upload_needed {
            let uploaded = stat_remote_transfer_size(
                app,
                tab_id,
                &upload_plan.upload_path,
                Some(cancel),
            )
            .await?
            .unwrap_or(0);
            if uploaded != entry.source_identity.size {
                return Err(transfer_error(format!(
                    "FTP 传输校验失败：{} 实际 {uploaded} 字节，期望 {}",
                    entry.relative_path, entry.source_identity.size
                )));
            }
        }
    } else {
        if let Some(parent) = Path::new(&entry.partial_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| transfer_error(error.to_string()))?;
        }
        worker_data_call_with_cancel(app, tab_id, cancel, |respond_to, _token| {
            WorkerCmd::DownloadRemoteFile {
                remote_path: entry.source_path.clone(),
                local_path: entry.partial_path.clone(),
                resume_offset: offset,
                transfer_id: transfer_id.to_string(),
                cancel: cancel.clone(),
                verify_checksum: false,
                respond_to,
            }
        })
        .await?;
    }
    if cancel.is_cancelled() {
        // 消息内容无关紧要：调用方在分类前先检查取消令牌并静默返回。
        return Err(transfer_error("transfer canceled"));
    }

    let completed_size = if direction == "upload" {
        entry.source_identity.size
    } else {
        stat_local_transfer_file(&entry.partial_path)
            .await
            .map(|value| value.size)
            .unwrap_or(0)
    };
    if completed_size != entry.source_identity.size {
        return Err(transfer_error(format!(
            "传输校验失败：{} 断点大小为 {completed_size}，期望 {}",
            entry.relative_path, entry.source_identity.size
        )));
    }

    if direction == "upload" {
        finalize_remote_upload(
            app,
            tab_id,
            RemoteUploadFinalize {
                partial_path: &entry.partial_path,
                staging_path: entry.staging_path.as_deref(),
                destination_path: &entry.destination_path,
                source_size: entry.source_identity.size,
                partial_ready: upload_plan.partial_ready,
            },
            Some(cancel),
        )
        .await?;
    } else {
        replace_local_file(
            Path::new(&entry.partial_path),
            Path::new(&entry.destination_path),
        )
        .await?;
    }

    Ok(DirectoryEntryAttempt::Transferred)
}
