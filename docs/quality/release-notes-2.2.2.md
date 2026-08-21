## FileTerm 2.2.2

FileTerm 2.2.2 聚焦 macOS 退出确认交互与 SSH root 大文件上传稳定性。

### 2.2.2 更新重点

- **macOS 退出确认**：通过 `Cmd+Q` 触发确认弹窗时，将焦点落在弹窗本身，避免部分场景下默认高亮“取消”按钮。
- **SSH root 大文件上传**：root 模式的上传临时文件从 `/tmp` 调整到 `/var/tmp`，避免 `/tmp` 为 `tmpfs` 时大文件 staging 消耗服务器内存；历史 `/tmp` 临时任务仍兼容断点恢复。
- **稳定性验证**：补充 root 上传 staging 路径兼容性测试，并继续覆盖 SSH、传输服务和 Tauri 合约测试。

### 反馈与支持

- [提交 Issue](https://github.com/St0ff3l/fileterm/issues/new)
- [查看完整文档](https://github.com/St0ff3l/fileterm#readme)
- [加入社区讨论](https://github.com/St0ff3l/fileterm/discussions)

---

## FileTerm 2.2.2

FileTerm 2.2.2 focuses on macOS quit confirmation behavior and SSH root-mode large-file upload stability.

### Highlights

- **macOS quit confirmation**: When `Cmd+Q` opens the confirmation dialog, focus is placed on the dialog itself instead of leaving the Cancel button highlighted in some cases.
- **SSH root large-file uploads**: Root-mode upload staging now uses `/var/tmp` instead of `/tmp`, avoiding memory-backed staging when `/tmp` is a `tmpfs`; existing `/tmp` staging tasks remain compatible with resume.
- **Stability validation**: Added compatibility coverage for root upload staging paths while retaining SSH, transfer-service, and Tauri contract coverage.

### Feedback & Support

- [Report an issue](https://github.com/St0ff3l/fileterm/issues/new)
- [Read the documentation](https://github.com/St0ff3l/fileterm#readme)
- [Join the community](https://github.com/St0ff3l/fileterm/discussions)
