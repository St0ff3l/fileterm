## FileTerm 2.3.4-beta.3

FileTerm 2.3.4-beta.3 改进 Xlight FTP/SFTP 兼容性、文件传输诊断和管理界面。

**下载地址**

|  平台   |   类型    |                                                                                                                                        下载                                                                                                                                        |
| :-----: | :-------: | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS  |    DMG    |                   [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-macos-x64.dmg)                    |
| Windows | Installer |                [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-arm64-setup.exe)                 |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
|  Linux  |    DEB    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.deb)                      |
|  Linux  |    RPM    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.rpm)                      |
|  Linux  | AppImage  |                [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.AppImage)                 |
|  Linux  |   Arch    |             [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.pkg.tar.zst)              |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.3 更新重点

- **Xlight FTP/SFTP 兼容性**：处理目录创建和模糊服务器回复差异，完善 SFTP 上传阶段的错误信息与路径定位。
- **FTP 字符编码**：新增 UTF-8 和 GBK 选项，兼容使用旧编码的服务器；文件内容按原始字节传输。
- **传输与管理界面**：改进传输记录筛选与清理入口，统一分段控件样式，并让管理列表填满可用区域。
- **实机验证**：已通过 Xlight FTP 和 SFTP 的小文件上传、下载及字节比对。

### 本版本包含的主要 PR 和问题修复

- [PR #383](https://github.com/St0ff3l/fileterm/pull/383)：改进 Xlight FTP/SFTP 兼容性、字符编码、传输诊断和管理界面。

完整变更记录请查看 [v2.3.4-beta.2 与 v2.3.4-beta.3 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.2...v2.3.4-beta.3)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.3

FileTerm 2.3.4-beta.3 improves Xlight FTP/SFTP compatibility, transfer diagnostics, and manager interfaces.

**Downloads**

| Platform |   Type    |                                                                                                                                       Downloads                                                                                                                                       |
| :------: | :-------: | :-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS   |    DMG    |                     [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-macos-x64.dmg)                     |
| Windows  | Installer |                  [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-arm64-setup.exe)                  |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-windows-arm64-portable.exe) (requires WebView2 Runtime) |
|  Linux   |    DEB    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.deb)                       |
|  Linux   |    RPM    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.rpm)                       |
|  Linux   | AppImage  |                  [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.AppImage)                  |
|  Linux   |   Arch    |               [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.3/FileTerm-2.3.4-beta.3-linux-arm64.pkg.tar.zst)               |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.3.4-beta.3 Highlights

- **Xlight FTP/SFTP compatibility**: Handle directory-creation and ambiguous server-reply differences, and improve SFTP upload-stage errors with operation and path details.
- **FTP character encoding**: Add UTF-8 and GBK options for legacy servers while keeping file contents byte-for-byte unchanged.
- **Transfer and manager interfaces**: Improve transfer-history filtering and cleanup access, unify segmented controls, and let manager lists fill the available space.
- **Live verification**: Xlight FTP and SFTP small-file uploads and downloads passed byte-for-byte comparison.

### Main PRs and issues

- [PR #383](https://github.com/St0ff3l/fileterm/pull/383): Improve Xlight FTP/SFTP compatibility, character encoding, transfer diagnostics, and manager interfaces.

See the [comparison between v2.3.4-beta.2 and v2.3.4-beta.3](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.2...v2.3.4-beta.3) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
