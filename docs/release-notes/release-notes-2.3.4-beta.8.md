## FileTerm 2.3.4-beta.8

FileTerm 2.3.4-beta.8 改进文件夹传输卡顿诊断，并优化设置页面、工作区按钮和网络连接列表的交互细节。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                               |
| ------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.8 更新重点

- **文件夹传输诊断**：目录扫描增加超时保护；日志记录准备阶段的目录数、文件数和总字节数，以及传输汇总与耗时，便于定位大文件夹上传停顿的位置。
- **设置与日志**：将诊断日志和关于页面分开；关于页面集中展示应用信息与第三方开源许可。
- **界面交互**：统一按钮 hover 和边框反馈，调整常用图标与表单圆角；网络连接列表的选中行在 hover 时继续保持选中状态。

### 本版本包含的主要 PR 和问题修复

- [PR #395](https://github.com/St0ff3l/fileterm/pull/395)：增强文件夹传输诊断，拆分日志与关于页面，并优化按钮、图标、表单和网络连接列表交互。

完整变更记录请查看 [v2.3.4-beta.7 与 v2.3.4-beta.8 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.7...v2.3.4-beta.8)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.8

FileTerm 2.3.4-beta.8 improves folder-transfer stall diagnostics and refines settings, workspace controls, and Network Connections list interactions.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                             |
| -------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.8/FileTerm-2.3.4-beta.8-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Folder-transfer diagnostics**: Adds timeouts to directory scanning and logs directory/file counts, total bytes, and elapsed time for preparation and transfer, helping identify where a large folder upload stalls.
- **Settings and logs**: Separates diagnostic logs from About; About now groups application information and third-party open-source licenses.
- **Interface interactions**: Aligns button hover and border feedback, refines common icons and form-control radii, and keeps selected Network Connections rows visibly selected on hover.

### Main PRs and issues

- [PR #395](https://github.com/St0ff3l/fileterm/pull/395): Improve folder-transfer diagnostics, separate Logs and About, and refine button, icon, form, and Network Connections list interactions.

See the [comparison between v2.3.4-beta.7 and v2.3.4-beta.8](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.7...v2.3.4-beta.8) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
