## FileTerm 2.3.4-beta.12

FileTerm 2.3.4-beta.12 为多服务器拓扑增加可配置的节点尺寸与服务指标，并改进多服务器工作区和 AI Copilot 展开后的布局。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                                   |
| ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.12 更新重点

- **拓扑与服务状态**：可调整拓扑节点尺寸，并选择显示 CPU、内存、磁盘、GPU、IP、网络接口和忙碌进程等指标；服务探测间隔可配置，并仅在多服务器模式的设置中显示。
- **多服务器管理**：服务状态筛选改用统一的分段控件，节点显示设置会保存，并遵循连接地址遮罩偏好。
- **AI Copilot 与工作区布局**：展开 Copilot 时为其保留独立列，窗口操作工具仍显示在工作区一侧；优化多服务器模式下的主机、会话与 Copilot 布局。

### 本版本包含的主要 PR 和问题修复

- [PR #411](https://github.com/St0ff3l/fileterm/pull/411)（关联 [Issue #409](https://github.com/St0ff3l/fileterm/issues/409)）：增强拓扑节点和服务指标配置，并改进多服务器管理及 AI Copilot 布局。

完整变更记录请查看 [v2.3.4-beta.11 与 v2.3.4-beta.12 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.11...v2.3.4-beta.12)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.12

FileTerm 2.3.4-beta.12 adds configurable topology node sizes and service metrics, and improves the multi-server workspace and expanded AI Copilot layout.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                                 |
| -------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.12/FileTerm-2.3.4-beta.12-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Topology and service status**: Adjust topology node sizes and choose metrics such as CPU, memory, disk, GPU, IP, network interfaces, and busy processes. Configure the service probe interval from Settings in server-management mode.
- **Multi-server management**: Service status filters now use the shared segmented control. Node display settings persist and follow the connection-address masking preference.
- **AI Copilot and workspace layout**: Copilot keeps a dedicated column when expanded, leaving the workspace window tools visible beside it; host, session, and Copilot areas are better arranged in server-management mode.

### Main PRs and issues

- [PR #411](https://github.com/St0ff3l/fileterm/pull/411) (related to [Issue #409](https://github.com/St0ff3l/fileterm/issues/409)): Expands topology node and service metric settings, and improves multi-server management and AI Copilot layout.

See the [comparison between v2.3.4-beta.11 and v2.3.4-beta.12](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.11...v2.3.4-beta.12) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
