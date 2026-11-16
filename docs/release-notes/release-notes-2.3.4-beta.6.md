## FileTerm 2.3.4-beta.6

FileTerm 2.3.4-beta.6 为 SSH 终端加入基于 Fig 规则的命令候选，并支持命令历史和远端路径建议。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                               |
| ------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.6 更新重点

- **终端命令候选**：输入命令、子命令、选项或参数时显示候选；规则默认离线可用，并结合当前连接的命令历史和现有文件通道提供远端路径建议。
- **键盘与设置**：高级设置中默认开启，可即时关闭；Tab 只填入候选、不执行命令，没有候选时仍可将 Tab 交给远端 shell。
- **交互安全**：每个终端窗格独立维护输入状态；密码提示、交互式程序或无法确认光标位置时暂停建议。远端目录只通过现有只读文件通道读取。
- **Fig 致谢**：明确标注 Fig 规则来源、固定版本和 MIT 许可，并在系统信息设置中提供完整许可文本。

### 本版本包含的主要 PR 和问题修复

- [PR #392](https://github.com/St0ff3l/fileterm/pull/392)：新增 SSH 终端命令候选、Fig 规则转换、设置项和许可致谢。
- [Issue #389](https://github.com/St0ff3l/fileterm/issues/389)：为操作台内终端增加 Tab 自动补全体验。

完整变更记录请查看 [v2.3.4-beta.5 与 v2.3.4-beta.6 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.5...v2.3.4-beta.6)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token.\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.6

FileTerm 2.3.4-beta.6 adds Fig-based command suggestions to SSH terminals, including command history and remote path suggestions.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                             |
| -------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.6/FileTerm-2.3.4-beta.6-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Terminal suggestions**: See command, subcommand, option, and argument candidates while typing. Rules work offline and combine with the current connection's command history and remote paths from the existing file channel.
- **Keyboard and settings**: Enabled by default and can be turned off immediately in Advanced Settings. Tab fills text without running it; when there are no candidates, Tab still reaches the remote shell.
- **Interactive safety**: Each terminal pane tracks its own input. Suggestions pause at password prompts, interactive programs, or uncertain cursor positions. Remote directories are read only through the existing file channel.
- **Fig attribution**: The app identifies the Fig rule source, pinned version, and MIT license, with the full license text available in System Information settings.

### Main PRs and issues

- [PR #392](https://github.com/St0ff3l/fileterm/pull/392): Add SSH terminal command suggestions, Fig rule conversion, settings, and license attribution.
- [Issue #389](https://github.com/St0ff3l/fileterm/issues/389): Add a Tab-completion experience to the workspace terminal.

See the [comparison between v2.3.4-beta.5 and v2.3.4-beta.6](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.5...v2.3.4-beta.6) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
