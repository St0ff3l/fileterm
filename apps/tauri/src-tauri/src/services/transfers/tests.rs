#[cfg(test)]
mod tests {
    use super::{
        can_resume_from, collect_local_tree, failure_status, interrupt_status,
        is_permanent_transfer_error, is_root_upload_staging_path, join_remote_path,
        manifest_totals, normalize_root_upload_staging, partial_path, progress_event_due,
        relative_remote_path, root_staging_path, select_journal_tasks, transient_retry_backoff,
        TransferFileIdentity, TransferManifest, TransferManifestEntry, TransferTask,
        JOURNAL_MAX_TASKS, TRANSIENT_MAX_ATTEMPTS, UPDATE_INTERVAL,
    };
    use std::time::Duration;

    #[tokio::test]
    async fn local_directory_scan_includes_nested_and_hidden_files() {
        let root =
            std::env::temp_dir().join(format!("fileterm-transfer-scan-{}", rand::random::<u64>()));
        let nested = root.join("backend").join("src").join("nested");
        tokio::fs::create_dir_all(&nested).await.unwrap();
        tokio::fs::write(root.join(".env"), b"root").await.unwrap();
        tokio::fs::write(root.join("backend").join("README.md"), b"readme")
            .await
            .unwrap();
        tokio::fs::write(nested.join("main.rs"), b"main")
            .await
            .unwrap();

        let result = collect_local_tree(&root).await;

        let _ = tokio::fs::remove_dir_all(&root).await;
        let (directories, files) = result.unwrap();
        assert_eq!(directories.len(), 3);
        assert_eq!(files.len(), 3);
        assert!(files.iter().any(|(path, _)| path.ends_with(".env")));
        assert!(files
            .iter()
            .any(|(path, _)| path.ends_with("backend/src/nested/main.rs")));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_directory_scan_rejects_symlinks_instead_of_dropping_them() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "fileterm-transfer-symlink-{}",
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(root.join("real")).await.unwrap();
        symlink(root.join("real"), root.join("linked")).expect("create test symlink");

        let result = collect_local_tree(&root).await;

        let _ = tokio::fs::remove_dir_all(&root).await;
        let error = result.expect_err("symlinks must not be silently skipped");
        assert!(error.to_string().contains("linked"));
    }

    #[test]
    fn creates_posix_paths_without_double_slashes() {
        assert_eq!(join_remote_path("/", "file.txt"), "/file.txt");
        assert_eq!(
            join_remote_path("/var/tmp/", "file.txt"),
            "/var/tmp/file.txt"
        );
        assert_eq!(
            partial_path("/var/tmp/file.txt"),
            "/var/tmp/file.txt.fileterm-part"
        );
    }

    #[test]
    fn keeps_directory_download_entries_inside_the_selected_remote_root() {
        assert_eq!(
            relative_remote_path("/srv/releases", "/srv/releases/app/config.json").unwrap(),
            "app/config.json"
        );
        assert_eq!(
            relative_remote_path("/", "/var/log/app.log").unwrap(),
            "var/log/app.log"
        );
        assert!(relative_remote_path("/srv/releases", "/srv/private/key").is_err());
        assert!(relative_remote_path("/srv/releases", "/srv/releases/../../etc/passwd").is_err());
        assert!(relative_remote_path("/srv/releases", "/srv/releases").is_err());
    }

    #[test]
    fn cleanup_tracking_round_trips_with_the_core_contract() {
        let mut task: TransferTask = serde_json::from_value(serde_json::json!({
            "id": "transfer-cleanup",
            "direction": "upload",
            "name": "payload.bin",
            "progress": 25,
            "status": "canceled"
        }))
        .unwrap();
        assert!(!task.cleanup_pending);
        assert_eq!(task.retry_attempt, None);

        task.cleanup_pending = true;
        task.retry_attempt = Some(2);
        let value = serde_json::to_value(task).unwrap();
        assert_eq!(value["cleanupPending"], true);
        assert_eq!(value["retryAttempt"], 2);
    }

    #[test]
    fn progress_events_are_coalesced_to_the_configured_interval() {
        let now = std::time::Instant::now();
        assert!(progress_event_due(None, now));
        assert!(!progress_event_due(Some(now - UPDATE_INTERVAL / 2), now));
        assert!(progress_event_due(Some(now - UPDATE_INTERVAL), now));
    }

    #[test]
    fn new_root_upload_staging_uses_disk_backed_temp_path() {
        let path = root_staging_path("debian.iso");

        assert!(path.starts_with("/var/tmp/fileterm-root-upload-"));
        assert!(is_root_upload_staging_path(&path));
        assert!(is_root_upload_staging_path(
            "/tmp/fileterm-root-upload-legacy.part"
        ));
        assert!(!is_root_upload_staging_path("/home/user/debian.iso"));
    }

    #[test]
    fn migrates_legacy_root_uploads_to_two_stage_staging() {
        let mut task: TransferTask = serde_json::from_value(serde_json::json!({
            "id": "transfer-root",
            "direction": "upload",
            "name": "config.toml",
            "progress": 42,
            "status": "paused",
            "fileAccessMode": "root",
            "targetType": "file",
            "destinationPath": "/etc/fileterm/config.toml",
            "partialPath": "/tmp/fileterm-root-upload-legacy.part",
            "resumable": true
        }))
        .unwrap();

        normalize_root_upload_staging(&mut task);

        assert_eq!(
            task.staging_path.as_deref(),
            Some("/tmp/fileterm-root-upload-legacy.part")
        );
        assert_eq!(
            task.partial_path.as_deref(),
            Some("/etc/fileterm/config.toml.fileterm-part")
        );
    }

    #[test]
    fn migrates_legacy_root_directory_entries_independently() {
        let mut task: TransferTask = serde_json::from_value(serde_json::json!({
            "id": "transfer-root-folder",
            "direction": "upload",
            "name": "configs",
            "progress": 10,
            "status": "paused",
            "fileAccessMode": "root",
            "targetType": "folder",
            "manifest": {
                "version": 1,
                "directories": ["/etc/fileterm"],
                "files": [{
                    "relativePath": "app.toml",
                    "sourcePath": "/local/app.toml",
                    "destinationPath": "/etc/fileterm/app.toml",
                    "partialPath": "/tmp/fileterm-root-upload-entry.part",
                    "sourceIdentity": { "size": 12 },
                    "status": "pending",
                    "transferredBytes": 4
                }]
            },
            "resumable": true
        }))
        .unwrap();

        normalize_root_upload_staging(&mut task);
        let entry = &task.manifest.as_ref().unwrap().files[0];

        assert_eq!(
            entry.staging_path.as_deref(),
            Some("/tmp/fileterm-root-upload-entry.part")
        );
        assert_eq!(entry.partial_path, "/etc/fileterm/app.toml.fileterm-part");
    }

    #[test]
    fn manifest_totals_count_completed_and_partial_files_once() {
        let manifest = TransferManifest {
            version: 1,
            directories: vec!["/tmp/export".to_string()],
            files: vec![
                TransferManifestEntry {
                    relative_path: "done.txt".to_string(),
                    source_path: "/remote/done.txt".to_string(),
                    destination_path: "/tmp/export/done.txt".to_string(),
                    partial_path: "/tmp/export/done.txt.fileterm-part".to_string(),
                    staging_path: None,
                    source_identity: TransferFileIdentity {
                        size: 10,
                        modified_at: None,
                    },
                    status: "done".to_string(),
                    transferred_bytes: 10,
                },
                TransferManifestEntry {
                    relative_path: "partial.txt".to_string(),
                    source_path: "/remote/partial.txt".to_string(),
                    destination_path: "/tmp/export/partial.txt".to_string(),
                    partial_path: "/tmp/export/partial.txt.fileterm-part".to_string(),
                    staging_path: None,
                    source_identity: TransferFileIdentity {
                        size: 20,
                        modified_at: None,
                    },
                    status: "running".to_string(),
                    transferred_bytes: 7,
                },
            ],
        };
        assert_eq!(manifest_totals(&manifest), (17, 30));
    }

    #[test]
    fn to_ui_task_strips_manifest_collections_while_preserving_truthiness() {
        let task = TransferTask {
            id: "transfer-large-folder".to_string(),
            direction: "upload".to_string(),
            name: "node_modules".to_string(),
            progress: 50.0,
            status: "running".to_string(),
            message: Some("node_modules/lodash/lodash.js".to_string()),
            speed: Some("15.2 MB/s".to_string()),
            transferred_bytes: Some(400_000_000),
            total_bytes: Some(800_000_000),
            tab_id: Some("tab-1".to_string()),
            profile_id: None,
            session_type: None,
            file_access_mode: None,
            target_type: Some("folder".to_string()),
            source_path: Some("/local/node_modules".to_string()),
            destination_path: Some("/remote/node_modules".to_string()),
            partial_path: None,
            staging_path: None,
            source_identity: None,
            manifest: Some(TransferManifest {
                version: 1,
                directories: vec![
                    "/remote/node_modules/a".to_string(),
                    "/remote/node_modules/b".to_string(),
                ],
                files: vec![
                    TransferManifestEntry {
                        relative_path: "a/1.js".to_string(),
                        source_path: "/local/node_modules/a/1.js".to_string(),
                        destination_path: "/remote/node_modules/a/1.js".to_string(),
                        partial_path: "/remote/node_modules/a/1.js.fileterm-part".to_string(),
                        staging_path: None,
                        source_identity: TransferFileIdentity {
                            size: 100,
                            modified_at: None,
                        },
                        status: "done".to_string(),
                        transferred_bytes: 100,
                    },
                    TransferManifestEntry {
                        relative_path: "b/2.js".to_string(),
                        source_path: "/local/node_modules/b/2.js".to_string(),
                        destination_path: "/remote/node_modules/b/2.js".to_string(),
                        partial_path: "/remote/node_modules/b/2.js.fileterm-part".to_string(),
                        staging_path: None,
                        source_identity: TransferFileIdentity {
                            size: 200,
                            modified_at: None,
                        },
                        status: "running".to_string(),
                        transferred_bytes: 50,
                    },
                ],
            }),
            resumable: true,
            retry_attempt: None,
            cleanup_pending: false,
            created_at: Some(1000),
            updated_at: Some(2000),
        };

        let ui_task = task.to_ui_task();
        let manifest = ui_task
            .manifest
            .as_ref()
            .expect("manifest must remain present for UI truthiness check");
        assert_eq!(manifest.version, 1);
        assert!(
            manifest.directories.is_empty(),
            "UI task directories must be empty to avoid IPC bloat"
        );
        assert!(
            manifest.files.is_empty(),
            "UI task files must be empty to avoid IPC bloat"
        );
        assert_eq!(
            ui_task.message.as_deref(),
            Some("node_modules/lodash/lodash.js")
        );
        assert_eq!(ui_task.name, "node_modules");

        // Verify JSON serialization contains manifest object with empty arrays
        let json = serde_json::to_value(&ui_task).unwrap();
        assert!(json.get("manifest").is_some());
        assert_eq!(json["manifest"]["directories"].as_array().unwrap().len(), 0);
        assert_eq!(json["manifest"]["files"].as_array().unwrap().len(), 0);
    }

    fn sample_task(id: &str, updated_at: u64) -> TransferTask {
        TransferTask {
            id: id.to_string(),
            direction: "download".to_string(),
            name: format!("file-{id}"),
            progress: 0.0,
            status: "queued".to_string(),
            message: None,
            speed: None,
            transferred_bytes: None,
            total_bytes: None,
            tab_id: None,
            profile_id: None,
            session_type: None,
            file_access_mode: None,
            target_type: None,
            source_path: None,
            destination_path: None,
            partial_path: None,
            staging_path: None,
            source_identity: None,
            manifest: None,
            resumable: false,
            retry_attempt: None,
            cleanup_pending: false,
            created_at: Some(updated_at),
            updated_at: Some(updated_at),
        }
    }

    #[test]
    fn journal_keeps_most_recent_tasks_when_over_limit() {
        // Regression for S2: the previous `take(200)` kept the oldest 200
        // entries from the append-only vector and silently dropped any
        // active/resumable task appended after the limit was reached. The
        // new selector must keep the most recently updated entries so
        // in-flight transfers survive a restart.
        let total = JOURNAL_MAX_TASKS + 50;
        let tasks: Vec<TransferTask> = (0..total)
            .map(|index| sample_task(&format!("task-{index}"), index as u64))
            .collect();

        let kept = select_journal_tasks(&tasks, JOURNAL_MAX_TASKS);
        assert_eq!(
            kept.len(),
            JOURNAL_MAX_TASKS,
            "selector must cap at the configured limit"
        );

        // The newest 200 tasks (indices 50..total) must survive — these are
        // the ones a `take(200)` from the front would have dropped.
        let kept_ids: std::collections::HashSet<&str> =
            kept.iter().map(|task| task.id.as_str()).collect();
        for index in 0..50 {
            assert!(
                !kept_ids.contains(format!("task-{index}").as_str()),
                "oldest task-{index} should have been evicted"
            );
        }
        for index in 50..total {
            assert!(
                kept_ids.contains(format!("task-{index}").as_str()),
                "newest task-{index} should have survived"
            );
        }
    }

    #[test]
    fn journal_selector_preserves_input_order_under_limit() {
        // When the input is already under the limit the selector must return
        // every task in the original append order — the journal is append-only
        // and downstream code relies on stable ordering for display + cleanup.
        let tasks: Vec<TransferTask> = (0..10)
            .map(|index| sample_task(&format!("task-{index}"), index as u64))
            .collect();

        let kept = select_journal_tasks(&tasks, JOURNAL_MAX_TASKS);
        assert_eq!(kept.len(), tasks.len());
        assert_eq!(
            kept.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
            tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn journal_selector_breaks_ties_by_append_order() {
        // Two tasks with the same updated_at must not evict each other; the
        // selector keeps both (up to the limit) and breaks ties by the
        // original append index so older entries drop first.
        let tasks = vec![
            sample_task("old-same-ts", 1000),
            sample_task("new-same-ts", 1000),
        ];
        let kept = select_journal_tasks(&tasks, 1);
        assert_eq!(kept.len(), 1);
        // Tie on timestamp → append order decides → the first appended task
        // is treated as older and evicted, the second survives.
        assert_eq!(kept[0].id, "new-same-ts");
    }

    // ---- 状态机集成测试 ----
    //
    // 传输状态机的核心约束：
    //   - active 状态：queued / running / verifying / finalizing
    //     （worker 仍在跑或已排队）
    //   - terminal 状态：done / failed / canceled
    //     （不会再自动变化，journal 可以淘汰）
    //   - 中断态：paused / interrupted
    //     （保留断点，等待用户 resume）
    //
    // 任何新状态加入时，下面这组分类测试会先失败，强制开发者确认它属于
    // 哪一档，避免出现"既不 active 也不 terminal 也不可 resume"的孤儿状态
    // 导致 journal 永远保留 / UI 永远显示转圈。

    fn task_with_status(status: &str) -> TransferTask {
        let mut task = sample_task("state-machine", 0);
        task.status = status.to_string();
        task
    }

    #[test]
    fn active_classifies_in_flight_statuses_correctly() {
        for status in ["queued", "running", "verifying", "finalizing"] {
            let task = task_with_status(status);
            assert!(
                task.active(),
                "status `{status}` must be active (worker still running or queued)"
            );
            assert!(
                !task.terminal(),
                "active status `{status}` must not be terminal"
            );
        }
    }

    #[test]
    fn terminal_classifies_final_statuses_correctly() {
        for status in ["done", "failed", "canceled"] {
            let task = task_with_status(status);
            assert!(
                task.terminal(),
                "status `{status}` must be terminal (no further automatic transitions)"
            );
            assert!(
                !task.active(),
                "terminal status `{status}` must not be active"
            );
        }
    }

    #[test]
    fn paused_and_interrupted_are_neither_active_nor_terminal() {
        // paused / interrupted 是中断态：保留断点等待用户操作，既不属于
        // active（worker 已停止），也不属于 terminal（用户仍可 resume）。
        for status in ["paused", "interrupted"] {
            let task = task_with_status(status);
            assert!(
                !task.active(),
                "paused/interrupted status `{status}` must not be active"
            );
            assert!(
                !task.terminal(),
                "paused/interrupted status `{status}` must not be terminal"
            );
        }
    }

    #[test]
    fn interrupt_status_pauses_resumable_tasks_and_cancels_the_rest() {
        // 应用退出 / session 丢失时：可恢复任务保留断点（paused），
        // 不可恢复任务直接取消（canceled）—— 不能反过来，否则用户会丢失
        // 可以恢复的断点，或留下永远无法清理的 canceled-but-has-partial 任务。
        assert_eq!(interrupt_status(true), "paused");
        assert_eq!(interrupt_status(false), "canceled");
    }

    #[test]
    fn failure_status_pauses_resumable_tasks_and_fails_the_rest() {
        // 传输本身失败时：可恢复任务保留断点（paused），不可恢复任务
        // 进入 terminal failed —— 与 interrupt 不同，因为失败意味着数据
        // 可能已损坏，不可恢复时必须终态以便用户清理。
        assert_eq!(failure_status(true), "paused");
        assert_eq!(failure_status(false), "failed");
    }

    #[test]
    fn can_resume_from_only_accepts_paused_interrupted_failed() {
        // resume 入口的门控：只有 paused / interrupted / failed 三种状态
        // 保留足够信息可以断点续传。done/canceled 是终态不能复活，
        // queued/running/verifying/finalizing 是 active 态已有 worker 在跑。
        for status in ["paused", "interrupted", "failed"] {
            assert!(
                can_resume_from(status),
                "status `{status}` must be resumable"
            );
        }
        for status in [
            "queued",
            "running",
            "verifying",
            "finalizing",
            "done",
            "canceled",
        ] {
            assert!(
                !can_resume_from(status),
                "status `{status}` must not be resumable"
            );
        }
    }

    #[test]
    fn resume_rejects_active_and_terminal_statuses() {
        // 集成层面验证 resume 的完整门控：即使 resumable=true，状态不属于
        // 可恢复集合时也必须拒绝，避免出现"resume 一个正在跑的任务"导致
        // 两个 worker 同时写同一份 partial 文件。
        let resumable_active_task = TransferTask {
            resumable: true,
            ..task_with_status("running")
        };
        assert!(
            !can_resume_from(resumable_active_task.status.as_str()),
            "active task must not be resumable even if resumable flag is true"
        );

        let resumable_done_task = TransferTask {
            resumable: true,
            ..task_with_status("done")
        };
        assert!(
            !can_resume_from(resumable_done_task.status.as_str()),
            "terminal done task must not be resumable"
        );
    }

    #[test]
    fn journal_interrupt_round_trips_through_interrupt_status() {
        // 集成：journal 加载时会把所有 active 任务转换为中断态。
        // 验证 resumable=true 的 active 任务在 journal 重载后落到 paused，
        // resumable=false 的落到 canceled —— 与 interrupt_status 输出一致。
        let mut resumable_active = sample_task("resumable-active", 100);
        resumable_active.status = "running".to_string();
        resumable_active.resumable = true;
        resumable_active.status = interrupt_status(resumable_active.resumable).to_string();
        assert_eq!(resumable_active.status, "paused");

        let mut non_resumable_active = sample_task("non-resumable-active", 100);
        non_resumable_active.status = "queued".to_string();
        non_resumable_active.resumable = false;
        non_resumable_active.status = interrupt_status(non_resumable_active.resumable).to_string();
        assert_eq!(non_resumable_active.status, "canceled");

        // 转换后两者都不应再被 active() 判定为活跃，避免下次启动时
        // 重复触发 interrupt 转换。
        assert!(!resumable_active.active());
        assert!(!non_resumable_active.active());
    }

    #[test]
    fn failure_then_resume_round_trip_preserves_partial_state() {
        // 集成：失败 → paused（可恢复）→ resume 入口放行的完整链路。
        // 模拟一个可恢复的失败任务：先通过 failure_status 落到 paused，
        // 然后 can_resume_from 必须放行，确保用户点"继续"时不会被拒绝。
        let mut task = sample_task("failed-but-resumable", 100);
        task.status = "running".to_string();
        task.resumable = true;

        // 1) 失败时，因为 resumable=true，状态转为 paused 而非 failed
        task.status = failure_status(task.resumable).to_string();
        assert_eq!(task.status, "paused");
        assert!(!task.terminal(), "paused must not be terminal");

        // 2) 用户点继续 → resume 入口检查通过
        assert!(
            task.resumable && can_resume_from(task.status.as_str()),
            "resumable paused task must pass resume gate"
        );

        // 3) resume 内部会把状态重置为 queued（line 2503 附近），
        //    重新进入 active 集合
        task.status = "queued".to_string();
        assert!(task.active());
    }

    #[test]
    fn permanent_transfer_errors_are_recognized_across_protocols_and_locales() {
        // 权限/空间/路径类：重试不可能成功，必须跳过该文件。
        for message in [
            "command error: Permission denied (os error 13)",
            "SFTP failure: permission denied",
            "open failure: Access denied",
            "write error: no space left on device",
            "disk quota exceeded",
            "stat error: no such file",
            "550 File not found",
            "上传源文件不存在或无法读取：node_modules/.bin/cli",
            "源文件已发生变化，不能继续目录断点：a.js",
            "断点文件大于源文件：b.js",
            "Read-only file system (os error 30)",
        ] {
            assert!(
                is_permanent_transfer_error(message),
                "应判定为永久错误：{message}"
            );
        }
    }

    #[test]
    fn transient_transfer_errors_are_not_misclassified_as_permanent() {
        // 网络/超时类：重试可能恢复，绝不能被误判为永久而静默跳过。
        for message in [
            "command error: connection reset by peer (os error 54)",
            "io error: broken pipe",
            "channel closed",
            "session closed by peer",
            "operation timed out",
            "Timeout",
            "network unreachable",
            "resource temporarily unavailable",
            "传输操作响应超时，后台操作已取消",
            "未知错误应默认按瞬时处理",
            "",
        ] {
            assert!(
                !is_permanent_transfer_error(message),
                "不应判定为永久错误：{message}"
            );
        }
    }

    #[test]
    fn transient_retry_backoff_grows_geometrically() {
        assert_eq!(transient_retry_backoff(1), Duration::from_secs(1));
        assert_eq!(transient_retry_backoff(2), Duration::from_secs(4));
        // 超出次数上限后的退避值不应溢出或倒退。
        assert!(transient_retry_backoff(9) > transient_retry_backoff(2));
        assert_eq!(TRANSIENT_MAX_ATTEMPTS, 3);
    }
}

#[cfg(test)]
mod large_directory_regressions {
    use super::*;

    fn entry(index: usize) -> TransferManifestEntry {
        TransferManifestEntry {
            relative_path: format!("node_modules/package-{index}/index.js"),
            source_path: format!("/local/node_modules/package-{index}/index.js"),
            destination_path: format!("/remote/node_modules/package-{index}/index.js"),
            partial_path: format!("/remote/node_modules/package-{index}/index.js.fileterm-part"),
            staging_path: None,
            source_identity: TransferFileIdentity {
                size: 32,
                modified_at: None,
            },
            status: "pending".to_string(),
            transferred_bytes: 0,
        }
    }

    #[test]
    fn many_small_files_keep_completed_and_active_entries_without_reallocating_the_tree() {
        let mut manifest = TransferManifest {
            version: 1,
            directories: vec!["/remote".to_string()],
            files: (0..20_000).map(entry).collect(),
        };
        let mut stored = Some(manifest.clone());
        let files_allocation = stored.as_ref().unwrap().files.as_ptr();
        // A path in an untouched entry must keep its allocation: per-file
        // progress must not clone all 20,000 source/destination path strings.
        let untouched_path = stored.as_ref().unwrap().files[19_999].source_path.as_ptr();
        for index in 0..100 {
            manifest.files[index].status = "running".to_string();
            sync_directory_manifest(&mut stored, &manifest, Some(index));
            assert_eq!(
                stored
                    .as_ref()
                    .unwrap()
                    .files
                    .iter()
                    .position(|entry| entry.status == "running"),
                Some(index)
            );
            manifest.files[index].status = "done".to_string();
            manifest.files[index].transferred_bytes = 32;
            sync_directory_manifest(&mut stored, &manifest, Some(index));
        }
        let stored = stored.unwrap();
        assert_eq!(stored.files.as_ptr(), files_allocation);
        assert_eq!(stored.files[19_999].source_path.as_ptr(), untouched_path);
        assert_eq!(manifest_totals(&stored), (3200, 640_000));
        let restored: TransferManifest =
            serde_json::from_slice(&serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(restored.files[..100]
            .iter()
            .all(|entry| entry.status == "done"));
        assert!(restored.files[100..]
            .iter()
            .all(|entry| entry.status == "pending"));
    }

    #[test]
    fn ui_payload_stays_small_for_a_hundred_thousand_file_manifest() {
        let mut task: TransferTask = serde_json::from_value(serde_json::json!({
            "id":"large-directory", "direction":"upload", "name":"frontend", "status":"running", "progress":10,
            "transferredBytes":320_000, "totalBytes":3_200_000,
        })).unwrap();
        task.manifest = Some(TransferManifest {
            version: 1,
            directories: vec!["/remote".to_string(); 1000],
            files: (0..100_000).map(entry).collect(),
        });
        for _ in 0..10 {
            let ui = task.to_ui_task();
            assert!(serde_json::to_vec(&ui).unwrap().len() < 1024);
            assert_eq!(ui.transferred_bytes, Some(320_000));
        }
        assert_eq!(task.manifest.unwrap().files.len(), 100_000);
    }
}
