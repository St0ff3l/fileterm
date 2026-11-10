## FileTerm 2.3.4-beta.4

FileTerm 2.3.4-beta.4 完善 SFTP 权限受限时的上传保护、续传和错误提示。

**下载地址**

|  平台   |   类型    |                                                                                                                                        下载                                                                                                                                        |
| :-----: | :-------: | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS  |    DMG    |                   [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-macos-x64.dmg)                    |
| Windows | Installer |                [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-arm64-setup.exe)                 |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
|  Linux  |    DEB    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.deb)                      |
|  Linux  |    RPM    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.rpm)                      |
|  Linux  | AppImage  |                [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.AppImage)                 |
|  Linux  |   Arch    |             [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.pkg.tar.zst)              |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.4 更新重点

- **SFTP 权限兼容**：无删除权限时可选择保护已有目标；新文件仍可上传，续传失败会保留断点并指出具体操作、路径和权限提示。
- **中文文件名**：SFTP 优先按 UTF-8 解码，无法无损解码时自动回退 GBK。
- **安全覆盖**：遇到已有目标且无法安全替换时保留原文件，不静默删除或覆盖。

### 本版本包含的主要 PR 和问题修复

- [PR #385](https://github.com/St0ff3l/fileterm/pull/385)：改进 Xlight FTP/SFTP 兼容性、字符编码、传输诊断和管理界面。

完整变更记录请查看 [v2.3.4-beta.3 与 v2.3.4-beta.4 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.3...v2.3.4-beta.4)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.4

FileTerm 2.3.4-beta.4 improves Xlight FTP/SFTP compatibility, transfer diagnostics, and manager interfaces.

**Downloads**

| Platform |   Type    |                                                                                                                                       Downloads                                                                                                                                       |
| :------: | :-------: | :-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS   |    DMG    |                     [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-macos-x64.dmg)                     |
| Windows  | Installer |                  [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-arm64-setup.exe)                  |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-windows-arm64-portable.exe) (requires WebView2 Runtime) |
|  Linux   |    DEB    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.deb)                       |
|  Linux   |    RPM    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.rpm)                       |
|  Linux   | AppImage  |                  [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.AppImage)                  |
|  Linux   |   Arch    |               [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.4/FileTerm-2.3.4-beta.4-linux-arm64.pkg.tar.zst)               |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.3.4-beta.4 Highlights

- **SFTP permissions**: An optional protection mode preserves existing targets when the account cannot delete; failed resume operations retain checkpoints and report the operation, path, and likely permission.
- **Chinese filenames**: SFTP prefers UTF-8 and falls back to GBK when names cannot be decoded losslessly.
- **Safe replacement**: When an existing target cannot be replaced safely, FileTerm preserves it instead of silently deleting or overwriting it.

### Main PRs and issues

- [PR #385](https://github.com/St0ff3l/fileterm/pull/385): Improve SFTP permission handling, safe uploads, resume diagnostics, and filename decoding.

See the [comparison between v2.3.4-beta.3 and v2.3.4-beta.4](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.3...v2.3.4-beta.4) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
