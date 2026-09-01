#[cfg(test)]
mod tests {
    use super::{
        can_resume_from, failure_status, interrupt_status, is_root_upload_staging_path,
        join_remote_path, manifest_totals, normalize_root_upload_staging, partial_path,
        progress_event_due, relative_remote_path, root_staging_path, select_journal_tasks,
        TransferFileIdentity, TransferManifest, TransferManifestEntry, TransferTask,
        JOURNAL_MAX_TASKS, UPDATE_INTERVAL,
    };

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
}
