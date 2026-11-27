## FileTerm 2.3.4-beta.14

FileTerm 2.3.4-beta.14 增加 SSH 自动选路与故障切换，并改进命令编辑器、工作模式设置和主题控件。

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

- **SSH 自动选路**：在「代理与隧道 → SSH 选路与故障切换」按单台服务器、标签或全局默认设置自动择优、首选或固定线路。每条规则最多 4 条候选；自动模式按目标近期连接成功率、平均延迟和延迟波动排序，不推断 IP 跳数。
- **探测、故障切换和日志**：可探测线路到目标的 TCP 可达性。连接阶段遇到可归因于线路的错误时，会尝试规则内其他候选，并在日志记录线路、失败阶段、错误分类和切换结果。认证失败和主机密钥错误不会通过切线隐藏。固定线路不会自动切换。
- **代理与组网工具**：用户自行启动 V2Ray、AmneziaVPN、Clash Verge 或 EasyTier，再将 SOCKS5/HTTP CONNECT 端点或虚拟网卡加入线路候选。FileTerm 不启动或管理这些客户端，不扫描本机端口，也不修改系统代理、路由表或 DNS。
- **界面调整**：命令编辑器改为紧凑的终端工作台；工作模式设置布局和保存图标更新；统一按钮圆角，并保存自定义主题中的操作按钮颜色。
- **范围与验证**：选路适用于普通 SSH Shell 建连和重连。已有 Shell 断线后不能透明迁移，重连会创建新会话；跳板机和 OpenSSH 配置别名不适用。Amnezia Local Proxy 与 EasyTier 的真实 Windows 双线路验收仍待完成。

### 本版本包含的主要 PR 和问题修复

- [PR #418](https://github.com/St0ff3l/fileterm/pull/418)：增加 SSH 自动线路策略、目标探测、健康排序、故障切换和记录。
- [Issue #409](https://github.com/St0ff3l/fileterm/issues/409)：复杂网络出口与代理线路管理需求；本版本支持在已配置的 SSH 候选线路中择优和切换，限制见上文。

完整变更记录请查看 [v2.3.4-beta.13 与 v2.3.4-beta.14 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.13...v2.3.4-beta.14)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.14

FileTerm 2.3.4-beta.14 adds SSH route selection and failover, with refinements to the command editor, workspace settings, and theme controls.

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

- **Automatic SSH route selection**: Under “Proxies & Tunnels → SSH Route Selection and Failover,” configure automatic, preferred, or fixed routes for one server, a tag, or the global default. Each rule supports up to four candidates. Automatic mode ranks configured routes using recent per-target connection success, average latency, and latency variation; it does not infer IP hop counts.
- **Probing, failover, and logs**: Probe TCP reachability from a route to its target. When a connection-stage error is attributable to a route, FileTerm tries other candidates in that rule and logs the route, failure stage, error category, and outcome. Authentication failures and host-key errors are not hidden by switching. Fixed routes do not fail over.
- **Proxy and mesh clients**: Start V2Ray, AmneziaVPN, Clash Verge, or EasyTier yourself, then add SOCKS5/HTTP CONNECT endpoints or a virtual adapter to route candidates. FileTerm does not start or manage these clients, scan local ports, or change the system proxy, routing table, or DNS.
- **Interface refinements**: The command editor is now a compact terminal workbench; workspace mode settings have a refreshed layout and save icon; button corners are consistent and custom theme action-button colors persist.
- **Scope and verification**: Route policies apply to ordinary SSH shell connections and reconnects. An established shell cannot migrate transparently after a disconnect; reconnecting creates a new session. Jump hosts and OpenSSH config aliases are outside this scope. Real Windows dual-route validation for Amnezia Local Proxy and EasyTier remains pending.

### Main PRs and issues

- [PR #418](https://github.com/St0ff3l/fileterm/pull/418): Adds SSH route policies, target probing, route health ranking, failover, and activity logs.
- [Issue #409](https://github.com/St0ff3l/fileterm/issues/409): Request for network egress and proxy route management; this release supports selection and failover among configured SSH candidates, with the scope described above.

See the [comparison between v2.3.4-beta.13 and v2.3.4-beta.14](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.13...v2.3.4-beta.14) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
