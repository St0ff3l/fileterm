## FileTerm 2.3.4-beta.13

FileTerm 2.3.4-beta.13 adds shared SSH proxy assignment and a Windows preview for choosing an outbound network interface per SSH connection.

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                                   |
| ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.13 更新重点

- **SSH 代理集中管理**：在代理管理页面保存 SOCKS5 或 HTTP CONNECT 端点，并把一台或多台 SSH 连接分配给指定代理。FileTerm 不会启动或控制本机代理软件；用户需要先运行代理并录入端点。
- **Windows SSH 指定出站网卡（预览）**：普通 SSH 直连可以选择系统默认或指定网卡。首版要求目标使用 IP 字面量；指定网卡不可用时明确失败，不会静默改走其他出口。代理、跳板和部分不支持的配置组合会提示冲突。
- **自动探测范围**：当前不会自动探测网络并选择出口，也不会扫描或自动选择本机代理软件。代理端点与网卡由用户手动添加或选择。
- **兼容性提示**：跨设备导入网卡偏好，以及旧配置重启读取的 Windows 手工验收尚未完成；网卡选择功能仍处于预览阶段。

### 本版本包含的主要 PR 和问题修复

- [PR #417](https://github.com/St0ff3l/fileterm/pull/417)：集中管理 SSH 代理分配，并增加 Windows SSH 指定出站网卡预览。
- [Issue #409](https://github.com/St0ff3l/fileterm/issues/409)：关联的网络出口管理需求讨论；自动探测与自动选择暂未包含。

完整变更记录请查看 [v2.3.4-beta.12 与 v2.3.4-beta.13 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.12...v2.3.4-beta.13)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.13

FileTerm 2.3.4-beta.13 adds shared SSH proxy assignment and a Windows preview for choosing an outbound network interface per SSH connection.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                                 |
| -------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.13/FileTerm-2.3.4-beta.13-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Shared SSH proxy management**: Save SOCKS5 or HTTP CONNECT endpoints in the proxy manager and assign one or more SSH connections to each endpoint. FileTerm does not start or control local proxy software; users must run a proxy and enter its endpoint.
- **Windows SSH outbound interface preview**: Choose the system default or a specific adapter for ordinary direct SSH connections. The first version requires an IP-literal target. If the selected adapter is unavailable, the connection fails explicitly without silently using another route. Proxy, jump-host, and other unsupported combinations show a conflict.
- **Automatic discovery scope**: FileTerm does not yet detect the current network and select an outbound route, nor scan for or choose local proxy software. Users add endpoints and choose adapters manually.
- **Compatibility note**: Manual Windows verification of importing adapter preferences across devices and reopening legacy configurations is incomplete; treat adapter selection as a preview.

### Main PRs and issues

- [PR #417](https://github.com/St0ff3l/fileterm/pull/417): Adds shared SSH proxy assignment management and a Windows SSH outbound interface preview.
- [Issue #409](https://github.com/St0ff3l/fileterm/issues/409): Related network egress management discussion; automatic discovery and selection are not included.

See the [comparison between v2.3.4-beta.12 and v2.3.4-beta.13](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.12...v2.3.4-beta.13) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
