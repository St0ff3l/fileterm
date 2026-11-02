## FileTerm 2.3.3

FileTerm 2.3.3 改进 MCP 远程目录访问的路径提示，帮助 Agent 区分 SFTP 用户视图与提权 Shell 视图。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                   |
| ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.3 更新重点

- **MCP 远程目录**：明确目录路径按当前文件视图解释；SSH 用户模式使用 SFTP 路径，root 模式使用提权 Shell 路径。群晖的两种路径根目录可能不同。
- **目录错误提示**：列目录失败时显示请求路径和当时的文件视图模式，帮助 Agent 判断路径是否属于当前命名空间。
- **Agent 排查指引**：说明 `command -v` 只检查命令搜索路径，避免仅凭该结果判断服务或数据目录不存在。
- **更新器安全修复**：升级更新器依赖，修复下载更新包解压时使用的 ZIP 库安全问题。

### 本版本包含的主要 PR 和问题修复

- [PR #373](https://github.com/St0ff3l/fileterm/pull/373)：说明 MCP 远程目录路径随文件视图模式变化，并在错误中提供路径上下文。

完整变更记录请查看 [v2.3.2 与 v2.3.3 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.2...v2.3.3)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.3

FileTerm 2.3.3 clarifies remote directory paths in MCP so Agents can distinguish the SFTP user view from the privileged shell view.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                 |
| -------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.3/FileTerm-2.3.3-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.3.3 Highlights

- **MCP remote directories**: Explain that paths follow the active file view. SSH user mode uses SFTP paths, while root mode uses privileged shell paths; these roots may differ on Synology.
- **Directory error context**: Include the requested path and active file view mode when listing fails, helping Agents determine whether the path belongs to the current namespace.
- **Agent troubleshooting guidance**: Clarify that `command -v` only checks the command search path, so its result alone does not establish that a service or data directory is absent.
- **Updater security fix**: Update the updater dependency to fix a security issue in the ZIP library used to extract downloaded updates.

### Main PRs and issues

- [PR #373](https://github.com/St0ff3l/fileterm/pull/373): Clarify MCP remote directory paths by file view mode and add path context to listing errors.

See the [comparison between v2.3.2 and v2.3.3](https://github.com/St0ff3l/fileterm/compare/v2.3.2...v2.3.3) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
