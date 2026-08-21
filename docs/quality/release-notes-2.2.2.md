## FileTerm 2.2.2

FileTerm 2.2.2 聚焦 macOS 退出确认交互与 SSH root 大文件上传稳定性。

### 2.2.2 更新重点

- **macOS 退出确认**：通过 `Cmd+Q` 触发确认弹窗时，将焦点落在弹窗本身，避免部分场景下默认高亮“取消”按钮。
- **SSH root 大文件上传**：root 模式的上传临时文件从 `/tmp` 调整到 `/var/tmp`，避免 `/tmp` 为 `tmpfs` 时大文件 staging 消耗服务器内存；历史 `/tmp` 临时任务仍兼容断点恢复。
- **稳定性验证**：补充 root 上传 staging 路径兼容性测试，并继续覆盖 SSH、传输服务和 Tauri 合约测试。

### 本版本包含的主要 PR 和问题修复

- [PR #208](https://github.com/St0ff3l/fileterm/pull/208)：稳定退出确认焦点与 root 大文件上传 staging。

完整变更记录请查看 [v2.2.1 与 v2.2.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.1...v2.2.2)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.2

FileTerm 2.2.2 focuses on macOS quit confirmation behavior and SSH root-mode large-file upload stability.

### Highlights

- **macOS quit confirmation**: When `Cmd+Q` opens the confirmation dialog, focus is placed on the dialog itself instead of leaving the Cancel button highlighted in some cases.
- **SSH root large-file uploads**: Root-mode upload staging now uses `/var/tmp` instead of `/tmp`, avoiding memory-backed staging when `/tmp` is a `tmpfs`; existing `/tmp` staging tasks remain compatible with resume.
- **Stability validation**: Added compatibility coverage for root upload staging paths while retaining SSH, transfer-service, and Tauri contract coverage.

### Main PRs and issues

- [PR #208](https://github.com/St0ff3l/fileterm/pull/208): Stabilize quit confirmation focus and root large-file upload staging.

See the [comparison between v2.2.1 and v2.2.2](https://github.com/St0ff3l/fileterm/compare/v2.2.1...v2.2.2) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
