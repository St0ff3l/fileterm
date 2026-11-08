## FileTerm 2.3.4-beta.2

FileTerm 2.3.4-beta.2 增加 MCP/CLI 调用审计记录，并优化设置页与常用对话框的交互体验。

**下载地址**

|  平台   |   类型    |                                                                                                                                        下载                                                                                                                                        |
| :-----: | :-------: | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS  |    DMG    |                   [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-macos-x64.dmg)                    |
| Windows | Installer |                [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-arm64-setup.exe)                 |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
|  Linux  |    DEB    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.deb)                      |
|  Linux  |    RPM    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.rpm)                      |
|  Linux  | AppImage  |                [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.AppImage)                 |
|  Linux  |   Arch    |             [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.pkg.tar.zst)              |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.2 更新重点

- **MCP / CLI 调用审计**：新增调用记录页，查看调用来源、操作、审批方式、结果和耗时等信息。
- **本地记录与隐私**：调用记录以 JSONL 保存并支持轮转和分页查看；凭据、文件正文等敏感内容按规则省略或脱敏。
- **设置与对话框**：统一部分表单对话框和操作反馈的视觉表现，普通确认弹窗保留原有样式。
- **主题自定义**：移除用途有限的导入与复制入口，保留主题预设、自定义编辑和保存管理。

### 本版本包含的主要 PR 和问题修复

- [PR #380](https://github.com/St0ff3l/fileterm/pull/380)：增加 MCP / CLI 调用审计，并优化设置页和对话框体验。

完整变更记录请查看 [v2.3.4-beta.1 与 v2.3.4-beta.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.1...v2.3.4-beta.2)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.2

FileTerm 2.3.4-beta.2 adds MCP/CLI call auditing and improves settings and dialog interactions.

**Downloads**

| Platform |   Type    |                                                                                                                                       Downloads                                                                                                                                       |
| :------: | :-------: | :-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS   |    DMG    |                     [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-macos-x64.dmg)                     |
| Windows  | Installer |                  [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-arm64-setup.exe)                  |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-windows-arm64-portable.exe) (requires WebView2 Runtime) |
|  Linux   |    DEB    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.deb)                       |
|  Linux   |    RPM    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.rpm)                       |
|  Linux   | AppImage  |                  [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.AppImage)                  |
|  Linux   |   Arch    |               [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.2/FileTerm-2.3.4-beta.2-linux-arm64.pkg.tar.zst)               |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.3.4-beta.2 Highlights

- **MCP / CLI call auditing**: A new call history page shows the source, action, approval mode, result, and duration of calls.
- **Local records and privacy**: Call records are stored as rotating JSONL history with paginated viewing; credentials, file contents, and other sensitive values are omitted or redacted according to policy.
- **Settings and dialogs**: Improve the visual consistency of selected form dialogs and operation feedback while retaining the existing style of standard confirmation dialogs.
- **Custom themes**: Remove the low-value import and copy actions while keeping theme presets, editing, and saved-theme management.

### Main PRs and issues

- [PR #380](https://github.com/St0ff3l/fileterm/pull/380): Add MCP / CLI call auditing and improve settings and dialog interactions.

See the [comparison between v2.3.4-beta.1 and v2.3.4-beta.2](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.1...v2.3.4-beta.2) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
