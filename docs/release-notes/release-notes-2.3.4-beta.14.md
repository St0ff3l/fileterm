## FileTerm 2.3.4-beta.14

FileTerm 2.3.4-beta.14 adds SSH route policies that choose and fail over among configured routes per server, tag, or default.

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                                                   |
| ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.14 更新重点

- **SSH 自动选路**：在「代理与隧道 → SSH 选路与故障切换」为单台服务器、标签或全局默认设置自动择优、首选或固定线路。每条规则最多 4 条候选；自动模式按目标近期连接成功率、平均延迟和延迟波动排序，不推断 IP 跳数。
- **故障切换与记录**：连接拨号或 SSH 握手遇到可归因于线路的故障时，会尝试规则内的其他候选，并记录线路、故障阶段、结果和是否使用备用线路。认证失败、主机密钥错误等不会通过切线隐藏。
- **本地线路客户端**：可登记客户端路径并从 FileTerm 启动受支持的客户端。V2Ray Core 配置中的受支持回环 SOCKS5/HTTP 端点可添加到代理库；Clash Verge 和 Amnezia 的端点需在客户端查看后手动登记；EasyTier 线路使用其虚拟网卡或系统路由。FileTerm 不扫描任意本机端口，也不修改系统代理或路由表。
- **适用范围与验证状态**：选路目前适用于普通 SSH Shell 建连和重连；现有 SSH Shell 断线后无法透明迁移，重连会建立新会话。跳板机和 OpenSSH 配置别名不适用。Amnezia Local Proxy 与 EasyTier 的真实 Windows 双线路验收仍待完成。

### 本版本包含的主要 PR 和问题修复

- [PR #418](https://github.com/St0ff3l/fileterm/pull/418)：增加 SSH 自动选路、按服务器/标签配置、目标探测、健康排序、故障切换日志和本地线路客户端管理。
- [Issue #409](https://github.com/St0ff3l/fileterm/issues/409)：复杂网络出口与代理线路管理需求；本版本实现了已配置线路内的 SSH 选路，限制见上文。

完整变更记录请查看 [v2.3.4-beta.13 与 v2.3.4-beta.14 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.13...v2.3.4-beta.14)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.14

FileTerm 2.3.4-beta.14 adds SSH route policies that choose and fail over among configured routes per server, tag, or default.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                                                 |
| -------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.14/FileTerm-2.3.4-beta.14-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Automatic SSH route selection**: Configure automatic, preferred, or fixed routes for one server, a tag, or the global default under “Proxies & Tunnels → SSH Route Selection and Failover.” Each rule supports up to four candidates. Automatic mode ranks configured routes using recent per-target connection success, average latency, and latency variation; it does not infer IP hop counts.
- **Failover and logs**: When a route-attributable dial or SSH handshake failure occurs, FileTerm tries other candidates in the rule and records the route, failure stage, outcome, and whether a fallback was used. Authentication failures and host-key errors are not hidden by switching routes.
- **Local route clients**: Save client paths and start supported clients from FileTerm. Supported loopback SOCKS5/HTTP endpoints in a V2Ray Core config can be added to the proxy library. Endpoints from Clash Verge and Amnezia must be checked in the client and registered manually. EasyTier routes use its virtual adapter or the system route. FileTerm does not scan arbitrary local ports or change the system proxy or routing table.
- **Scope and verification**: Route policies currently apply to ordinary SSH shell connections and reconnects. A disconnected SSH shell cannot migrate transparently; reconnecting creates a new session. Jump hosts and OpenSSH config aliases are outside this scope. Real Windows dual-route validation for Amnezia Local Proxy and EasyTier remains pending.

### Main PRs and issues

- [PR #418](https://github.com/St0ff3l/fileterm/pull/418): Adds automatic SSH route selection, per-server/tag policies, target probing, route health ranking, failover logs, and local route client management.
- [Issue #409](https://github.com/St0ff3l/fileterm/issues/409): Request for managing network egress and proxy routes in complex environments; this release adds SSH selection among configured routes, with the scope described above.

See the [comparison between v2.3.4-beta.13 and v2.3.4-beta.14](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.13...v2.3.4-beta.14) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
