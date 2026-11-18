## FileTerm 2.3.4-beta.9

FileTerm 2.3.4-beta.9 优化 MCP/CLI 客户端接入体验，并让正式发布构建读取 Rust 编译缓存。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                               |
| ------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.9 更新重点

- **MCP 接入**：新增通用 FileTerm MCP 命令卡片；dsh 配置预览单行横向查看，复制时仍保留原始 YAML 换行。
- **CLI 说明**：去除过时版本号，改为描述当前的一次性 CLI 启动行为。
- **发布构建**：平台匹配时，各平台发布任务会读取 `main` 上的 Rust 缓存。

### 本版本包含的主要 PR 和问题修复

- [PR #397](https://github.com/St0ff3l/fileterm/pull/397)：优化 MCP/dsh 设置展示、更新 CLI 模式说明，并为正式发布构建接入 Rust 缓存。

完整变更记录请查看 [v2.3.4-beta.8 与 v2.3.4-beta.9 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.8...v2.3.4-beta.9)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.9

FileTerm 2.3.4-beta.9 streamlines MCP/CLI setup and lets release builds restore Rust build caches.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                             |
| -------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.9/FileTerm-2.3.4-beta.9-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **MCP setup**: Adds a generic FileTerm MCP card; dsh configuration is previewed on one horizontally scrollable line while copying preserves the original YAML line breaks.
- **CLI guidance**: Removes the obsolete version reference and describes current one-shot CLI startup behavior.
- **Release builds**: Platform jobs restore Rust caches from `main` when a matching target cache is available.

### Main PRs and issues

- [PR #397](https://github.com/St0ff3l/fileterm/pull/397): Polish MCP/dsh setup, update the CLI mode guidance, and connect Rust caches to official release builds.

See the [comparison between v2.3.4-beta.8 and v2.3.4-beta.9](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.8...v2.3.4-beta.9) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
