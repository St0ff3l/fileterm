## FileTerm 2.3.4-beta.10

FileTerm 2.3.4-beta.10 新增多服务器管理工作区与连接地址遮罩，并改善工作区切换和发布构建可靠性。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                                   |
| ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.10 更新重点

- **服务器管理**：新增多服务器工作区，支持连接筛选、保存视图、拓扑关系和批量执行命令模板，并查看各主机结果与执行历史。
- **连接隐私**：可遮罩系统侧栏中的 IP 和访问地址；悬停或键盘聚焦时显示，点击仍复制真实地址。
- **工作区体验**：切换工作区模式时重新播放页面入场动画，并保留设置页导航状态。
- **构建稳定性**：改进发布构建的 Rust 缓存预热，生成自动补全规格以避免缓存构建缺少输入文件。
- **开发工具安全**：更新开发依赖并修复 npm 审计告警；不改变应用运行时依赖。

### 本版本包含的主要 PR 和问题修复

- [PR #402](https://github.com/St0ff3l/fileterm/pull/402)：新增多服务器管理工作区、拓扑和批量命令执行。
- [PR #401](https://github.com/St0ff3l/fileterm/pull/401)：增加连接地址遮罩和按需显示。
- [PR #403](https://github.com/St0ff3l/fileterm/pull/403)：修复工作区模式切换时的页面过渡动画。
- [PR #398](https://github.com/St0ff3l/fileterm/pull/398) 与 [PR #400](https://github.com/St0ff3l/fileterm/pull/400)：完善多平台 Rust 缓存预热流程。

完整变更记录请查看 [v2.3.4-beta.9 与 v2.3.4-beta.10 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.9...v2.3.4-beta.10)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.10

FileTerm 2.3.4-beta.10 adds a multi-server management workspace and connection address masking, while improving workspace transitions and release build reliability.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                                 |
| -------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.10/FileTerm-2.3.4-beta.10-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Server management**: Adds a multi-server workspace with connection filters, saved views, topology, batch command templates, per-host results, and execution history.
- **Connection privacy**: Masks IP and access addresses in the system sidebar, revealing them on hover or keyboard focus while preserving copy of the real value.
- **Workspace experience**: Replays page entrance animations when switching workspace modes and preserves navigation on the Settings page.
- **Build reliability**: Improves Rust cache warmup for release builds and generates completion specs to prevent missing inputs in cache builds.
- **Developer tooling security**: Updates development dependencies to resolve npm audit findings without changing runtime dependencies.

### Main PRs and issues

- [PR #402](https://github.com/St0ff3l/fileterm/pull/402): Adds the multi-server management workspace, topology, and batch command execution.
- [PR #401](https://github.com/St0ff3l/fileterm/pull/401): Adds connection address masking and on-demand reveal.
- [PR #403](https://github.com/St0ff3l/fileterm/pull/403): Fixes page transition animations when switching workspace modes.
- [PR #398](https://github.com/St0ff3l/fileterm/pull/398) and [PR #400](https://github.com/St0ff3l/fileterm/pull/400): Improve multi-platform Rust cache warmup.

See the [comparison between v2.3.4-beta.9 and v2.3.4-beta.10](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.9...v2.3.4-beta.10) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
