## FileTerm 2.3.4-beta.7

FileTerm 2.3.4-beta.7 新增网络连接工作区，可查看主机监听端口、进程和活动连接，并按当前用户或 root 权限采集信息。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                               |
| ------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.7 更新重点

- **网络连接**：新增网络连接页面，查看监听端口、进程和活动连接；支持筛选、排序、拖动调整列宽，并可在启用布局记忆时恢复列宽。
- **权限与流量**：可切换当前用户或 root 视角。Linux 优先使用系统 `ss`，也会查找 Entware 常见的 `/opt/bin/ss` 和 `/opt/sbin/ss` 路径；旧版 `ss` 的上传统计使用对端确认字节，可能略低于实际发送量。
- **刷新与数据范围**：每 3 秒刷新，离开标签页后暂停。上传/下载显示活动连接的内核累计字节，不代表网卡总流量；系统不支持的计数显示 `—`。
- **工作区体验**：改进系统信息表格列宽与文本选择；隐藏会话后直接打开后台会话列表。SSH 断开时监控开关保留显示，重连后可继续操作。

### 本版本包含的主要 PR 和问题修复

- [PR #394](https://github.com/St0ff3l/fileterm/pull/394)：新增网络连接工作区、root 采集、旧版 `ss` 兼容和表格布局优化。

完整变更记录请查看 [v2.3.4-beta.6 与 v2.3.4-beta.7 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.6...v2.3.4-beta.7)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.7

FileTerm 2.3.4-beta.7 adds a Network Connections workspace for inspecting listening ports, processes, and active connections, with collection under the current user or root privileges.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                             |
| -------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.7/FileTerm-2.3.4-beta.7-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Network connections**: Inspect listening ports, processes, and active connections. Filter and sort results, resize table columns, and restore column widths when layout memory is enabled.
- **Permissions and traffic**: Switch between the current user and root views. On Linux, FileTerm looks for `ss` in the system path and common Entware locations `/opt/bin/ss` and `/opt/sbin/ss`. Older `ss` versions report acknowledged bytes for upload, which may be slightly lower than bytes actually sent.
- **Refresh and data scope**: Data refreshes every three seconds and pauses when its tab is inactive. Upload and download show kernel byte counters for active connections, not total network-interface traffic; unsupported counters appear as `—`.
- **Workspace improvements**: Refines system-information table resizing and text selection, opens the background-session list after hiding a session, and keeps the monitoring control visible while SSH is disconnected.

### Main PRs and issues

- [PR #394](https://github.com/St0ff3l/fileterm/pull/394): Add the Network Connections workspace, root collection, legacy `ss` compatibility, and table-layout improvements.

See the [comparison between v2.3.4-beta.6 and v2.3.4-beta.7](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.6...v2.3.4-beta.7) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
