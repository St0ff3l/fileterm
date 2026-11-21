## FileTerm 2.3.4-beta.11

FileTerm 2.3.4-beta.11 改进小屏和高 DPI 下的界面字号与布局，支持保存前测试 WebDAV/S3 备份配置，并统一设置操作反馈。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                                   |
| ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.11 更新重点

- **界面字号与布局**：新增独立的界面字号比例，并限制界面缩放与字号叠加后的综合倍率；终端字号保持独立，改善小屏高 DPI 和大字号下的控件布局。
- **备份连接测试**：可直接测试当前未保存的 WebDAV/S3 地址和配置；测试不会保存配置或修改远端备份，并会提示具体连接错误。
- **设置反馈**：设置操作的临时成功和错误提示改为显示在右下角气泡；配置加载失败和字段说明仍保留在设置页中。

### 本版本包含的主要 PR 和问题修复

- [PR #408](https://github.com/St0ff3l/fileterm/pull/408)（关联 [Issue #399](https://github.com/St0ff3l/fileterm/issues/399)）：新增界面字号调节与高倍率布局适配，支持保存前测试备份配置，并改进设置页反馈。

完整变更记录请查看 [v2.3.4-beta.10 与 v2.3.4-beta.11 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.10...v2.3.4-beta.11)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.11

FileTerm 2.3.4-beta.11 improves interface text scaling and layout on small high-DPI screens, enables testing WebDAV/S3 backup settings before saving, and unifies settings feedback.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                                 |
| -------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.11/FileTerm-2.3.4-beta.11-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Interface text and layout**: Adds an independent interface font scale and caps its combined effect with UI zoom; terminal font scaling stays independent, with improved control layout on small high-DPI screens and at larger text sizes.
- **Backup connection tests**: Tests the current unsaved WebDAV/S3 addresses and settings without saving configuration or changing remote backups, and reports the connection failure stage.
- **Settings feedback**: Temporary success and error messages from settings actions now appear in bottom-right bubbles; configuration load failures and field guidance remain visible in the settings page.

### Main PRs and issues

- [PR #408](https://github.com/St0ff3l/fileterm/pull/408) (related to [Issue #399](https://github.com/St0ff3l/fileterm/issues/399)): Adds interface text scaling and high-scale layout improvements, pre-save backup testing, and clearer Settings feedback.

See the [comparison between v2.3.4-beta.10 and v2.3.4-beta.11](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.10...v2.3.4-beta.11) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
