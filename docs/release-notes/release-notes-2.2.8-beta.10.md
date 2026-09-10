## FileTerm 2.2.8-beta.10

FileTerm 2.2.8-beta.10 improves folder upload reliability across Linux, macOS, and Windows with stronger diagnostics and complete traversal.

### 2.2.8-beta.10 更新重点

- **文件夹上传**：修复 Windows 重解析点目录、特殊文件名、空文件和空目录处理，确保单次上传完整遍历。
- **传输诊断**：补充请求、扫描、目录准备、远端临时文件校验和失败路径日志；单个文件失败时继续诊断其余文件。
- **兼容性**：覆盖 Linux、macOS、Windows 的文件夹上传流程，保持现有单一上传按钮。

### 本版本包含的主要 PR 和问题修复

- [PR #249](https://github.com/St0ff3l/fileterm/pull/249)：加固跨平台文件夹上传、Windows 重解析点处理、远端完整性校验和传输诊断日志。

完整变更记录请查看 [v2.2.8-beta.9 与 v2.2.8-beta.10 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.9...v2.2.8-beta.10)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.8-beta.10

FileTerm 2.2.8-beta.10 improves folder upload reliability across Linux, macOS, and Windows with stronger diagnostics and complete traversal.

### Highlights

- **Folder uploads**: Handle Windows reparse-point directories, special names, empty files, and empty directories while preserving complete traversal.
- **Transfer diagnostics**: Add request, scan, directory-preparation, remote temporary-file verification, and path-specific failure logs while continuing after individual file failures.
- **Compatibility**: Cover folder uploads on Linux, macOS, and Windows while keeping the existing single upload button.

### Main PRs and issues

- [PR #249](https://github.com/St0ff3l/fileterm/pull/249): Harden cross-platform folder uploads, Windows reparse-point handling, remote completeness checks, and transfer diagnostics.

See the [comparison between v2.2.8-beta.9 and v2.2.8-beta.10](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.9...v2.2.8-beta.10) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
