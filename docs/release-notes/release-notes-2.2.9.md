## FileTerm 2.2.9

FileTerm 2.2.9 聚焦文件管理与传输链路的安全性、可恢复性和发布前质量门禁。

### 2.2.9 更新重点

- **文件管理安全**：创建、重命名、复制、移动和下载统一校验路径边界，拒绝目录穿越、符号链接、硬链接和特殊文件带来的越权或误覆盖；跨本地与远端面板的剪切不再在传输入队后提前删除源文件。
- **传输可靠性**：完善传输日志恢复、历史保留、取消与超时传播，以及失败收尾和断点保护，避免任务丢失、状态滞留和外部文件被改写。
- **备份与协议并发**：WebDAV/S3 合并上传沿用读取时的 ETag 并在并发变化时报告冲突，下载响应使用有界流式读取；SMB 清理仅删除空挂载目录，降低误删共享数据的风险。
- **文件页操作体验**：文件夹内容占满列表时，底部补充约四行行高的可操作空白区域，方便用户在当前文件夹空白处打开右键菜单。
- **质量与依赖**：将 renderer 文件操作回归接入 CI，补充 Rust 后端边界测试，并升级锁定的 rustls 安全依赖。

### 本版本包含的主要 PR 和问题修复

- 文件与备份质量审查覆盖传输路径、文件操作、后端并发、取消传播、协议断点和 renderer 回归。

完整变更记录请查看 [v2.2.8 与 v2.2.9 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8...v2.2.9)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.9

FileTerm 2.2.9 focuses on safer file management, more recoverable transfers, and stronger pre-release quality gates.

### Highlights

- **File management safety**: Validate path boundaries for create, rename, copy, move, and download operations, rejecting traversal, symlink, hard-link, and special-file hazards; cross-pane local-to-remote cuts no longer delete the source after merely queueing a transfer.
- **Transfer reliability**: Improve transfer-log recovery, history retention, cancellation and timeout propagation, failure cleanup, and checkpoint protection so tasks do not disappear or remain stuck and external files are not overwritten through unsafe paths.
- **Backup and protocol concurrency**: WebDAV/S3 merges use the ETag observed during the read and report conflicts when the remote object changes; downloads use bounded streaming reads; SMB cleanup only removes empty mount directories to reduce the risk of deleting shared data.
- **File-pane workflow**: When a directory fills the list, add roughly four row-heights of usable space at the bottom so users can open the current-folder context menu from the blank area.
- **Quality and dependencies**: Add renderer file-operation regressions to CI, expand Rust backend boundary coverage, and update the locked rustls security dependency.

### Main PRs and issues

- The release includes the file and backup quality audit covering transfer paths, file operations, backend concurrency, cancellation, protocol checkpoints, and renderer regressions.

See the [comparison between v2.2.8 and v2.2.9](https://github.com/St0ff3l/fileterm/compare/v2.2.8...v2.2.9) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
