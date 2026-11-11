## FileTerm 2.3.4-beta.5

FileTerm 2.3.4-beta.5 改进大文件传输校验与云端备份恢复，并完善界面动效。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                               |
| ------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.5 更新重点

- **大文件传输校验**：SHA-256 校验超时会根据文件大小估算，并可在高级设置中关闭默认 24 小时上限；传输取消和校验状态也更可靠。
- **云端备份恢复**：确认 WebDAV/S3 远端备份已不存在时可重新上传；远端仍存在时继续校验版本并使用条件写入，避免覆盖并发更新。
- **界面动效**：弹窗、菜单、侧栏和选择控件采用连贯过渡，并遵循系统的减弱动态效果设置。
- **诊断日志**：备份上传预检日志记录远端状态和采用的写入策略，不输出凭据或原始 ETag。

### 本版本包含的主要 PR 和问题修复

- [PR #388](https://github.com/St0ff3l/fileterm/pull/388)：加固传输校验与远端备份恢复，并完善动效、诊断日志和回归测试。
- [Issue #386](https://github.com/St0ff3l/fileterm/issues/386)：改善大文件 SFTP SHA-256 校验超时处理。
- [Issue #387](https://github.com/St0ff3l/fileterm/issues/387)：允许恢复已被删除的 WebDAV/S3 远端备份。

完整变更记录请查看 [v2.3.4-beta.4 与 v2.3.4-beta.5 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.4...v2.3.4-beta.5)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token.\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.5

FileTerm 2.3.4-beta.5 improves large-file transfer verification, cloud backup recovery, and interface motion.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                             |
| -------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.5/FileTerm-2.3.4-beta.5-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Large-file verification**: SHA-256 timeouts are estimated from file size, with an optional way to disable the default 24-hour cap; cancellation and verification status are more reliable.
- **Cloud backup recovery**: WebDAV/S3 backups can be uploaded again after confirmed remote deletion. Existing remote versions remain protected with version checks and conditional writes.
- **Interface motion**: Dialogs, menus, sidebars, and selection controls use continuous transitions and respect the system's reduced-motion setting.
- **Diagnostics**: Backup upload preflight logs show remote state and write strategy without exposing credentials or raw ETags.

### Main PRs and issues

- [PR #388](https://github.com/St0ff3l/fileterm/pull/388): Harden transfer verification and remote backup recovery, with motion, diagnostics, and regression improvements.
- [Issue #386](https://github.com/St0ff3l/fileterm/issues/386): Improve SHA-256 timeout handling for large SFTP transfers.
- [Issue #387](https://github.com/St0ff3l/fileterm/issues/387): Allow recovery of deleted WebDAV/S3 backups.

See the [comparison between v2.3.4-beta.4 and v2.3.4-beta.5](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.4...v2.3.4-beta.5) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
