## FileTerm 2.3.4-beta.1

FileTerm 2.3.4-beta.1 增加 SSH 动态验证码自动填写，并修复 SFTP 下载文件大小与实际内容不一致的问题。

**下载地址**

|  平台   |   类型    |                                                                                                                                        下载                                                                                                                                        |
| :-----: | :-------: | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS  |    DMG    |                   [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-macos-x64.dmg)                    |
| Windows | Installer |                [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-arm64-setup.exe)                 |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
|  Linux  |    DEB    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.deb)                      |
|  Linux  |    RPM    |                     [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.rpm)                      |
|  Linux  | AppImage  |                [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.AppImage)                 |
|  Linux  |   Arch    |             [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.pkg.tar.zst)              |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.1 更新重点

- **SSH 动态验证码**：可保存 OTP 密钥，在明确匹配的 keyboard-interactive 提示中自动生成并提交 TOTP。密码或私钥认证均可配合使用；其他或不明确的验证提示仍需手动输入。
- **OTP 安全**：OTP 密钥复用加密凭据存储，公开连接信息只返回是否已保存，不记录密钥内容。
- **SFTP 下载修复**：按服务器声明的文件大小完成下载，避免文件内容尚未接收完整时提前结束。
- **连接设置**：整理认证、二次验证和连接对象的表单布局，补充用户名、密码可留空的提示。

### 本版本包含的主要 PR 和问题修复

- [PR #377](https://github.com/St0ff3l/fileterm/pull/377)：增加 SSH OTP 自动填写并优化连接设置。
- [PR #376](https://github.com/St0ff3l/fileterm/pull/376)：修复 SFTP 下载未完整接收文件的问题。

完整变更记录请查看 [v2.3.3 与 v2.3.4-beta.1 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.3...v2.3.4-beta.1)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.1

FileTerm 2.3.4-beta.1 adds automatic SSH one-time password entry and fixes incomplete SFTP downloads.

**Downloads**

| Platform |   Type    |                                                                                                                                       Downloads                                                                                                                                       |
| :------: | :-------: | :-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  macOS   |    DMG    |                     [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-macos-x64.dmg)                     |
| Windows  | Installer |                  [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-arm64-setup.exe)                  |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-windows-arm64-portable.exe) (requires WebView2 Runtime) |
|  Linux   |    DEB    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.deb)                       |
|  Linux   |    RPM    |                       [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.rpm)                       |
|  Linux   | AppImage  |                  [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.AppImage)                  |
|  Linux   |   Arch    |               [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.1/FileTerm-2.3.4-beta.1-linux-arm64.pkg.tar.zst)               |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.3.4-beta.1 Highlights

- **SSH one-time passwords**: Save an OTP secret and automatically generate and submit a TOTP when a keyboard-interactive prompt clearly matches. It works with password or private-key authentication. Other or ambiguous verification prompts still require manual input.
- **OTP security**: OTP secrets use encrypted credential storage. Public connection data exposes only whether a secret is saved, and logs do not include the secret.
- **SFTP download fix**: Finish downloads according to the size advertised by the server, preventing early completion before all file contents arrive.
- **Connection settings**: Refine the authentication, second-factor, and connection-target forms, including concise optional username and password hints.

### Main PRs and issues

- [PR #377](https://github.com/St0ff3l/fileterm/pull/377): Add automatic SSH OTP entry and refine connection settings.
- [PR #376](https://github.com/St0ff3l/fileterm/pull/376): Fix incomplete SFTP downloads.

See the [comparison between v2.3.3 and v2.3.4-beta.1](https://github.com/St0ff3l/fileterm/compare/v2.3.3...v2.3.4-beta.1) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
