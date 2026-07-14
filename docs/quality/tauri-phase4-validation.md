# Tauri Phase 4 验收记录

更新日期：2026-07-14（macOS arm64 本机；Linux CI 计划已配置，尚未取得远端结果）

> 本记录区分“本机/夹具已验证”和“发行候选必须手测”。此前把 Electron 协议测试、Tauri 代码存在性或 CI 配置误写为 Tauri 跨平台验收的地方，均以本页的证据状态为准。

不能由本机自动化替代的发行候选步骤见 [Tauri 发行候选协议验收清单](tauri-rc-protocol-checklist.md)。

## 已执行结果

| 项目                           | 结果                          | 说明                                                                                                                                                                                     |
| ------------------------------ | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust 单元/协议夹具             | 通过，34 library + 9 contract | macOS 本机 `cargo test` 通过真实 OpenSSH 公钥/exec/SFTP/HTTP 代理/local direct-tcpip、SSH MFA、多模式 FTPS、WebDAV ETag、Telnet HTTP CONNECT；Linux PTY 用例已加入 CI，未在 macOS 运行。 |
| Electron 真实协议测试          | 通过，7/7                     | 本机 `/usr/sbin/sshd` SFTP，FTP、显式 FTPS、隐式 FTPS；这是 Electron controller 证据，不能替代 Tauri 验收。                                                                              |
| Tauri production build         | 通过                          | 产出 `FileTerm.app` 与 `FileTerm_1.1.1_aarch64.dmg`；CSP 与本地 `.icns/.ico/.png` 图标参与实际打包。                                                                                     |
| macOS socket lifecycle         | 通过                          | Telnet 直接 transport drop 后服务端在 2 秒内收到 EOF。                                                                                                                                   |
| Windows/Linux socket lifecycle | 已配置，未执行                | `.github/workflows/ci.yml` 的 `tauri-socket-lifecycle` 在 macOS、Windows、Ubuntu 各运行同一测试；需要推送后由 GitHub Actions 给出外部结果。                                              |

## 性能基线

同一台 macOS arm64 机器、隔离临时 HOME、冷启动后 2 秒采样一次主进程 RSS：

| 指标            | Electron 1.2.1（`/Applications/FileTerm.app`） | Tauri 1.1.1 candidate | 结论                                 |
| --------------- | ---------------------------------------------: | --------------------: | ------------------------------------ |
| 进程可见时间    |                                        约 5 ms |               约 6 ms | 仅衡量 OS 创建进程，差异无统计意义。 |
| 主进程 RSS      |                                     约 228 MiB |            约 116 MiB | Tauri 低约 49%。                     |
| App bundle 体积 |                                     约 608 MiB |             约 40 MiB | Tauri 小约 93%。                     |

该基线不是交互就绪（TTI）或远程吞吐基准；两者版本也不同。发行候选必须在每个平台用同一 profile、同一连接和同一大文件重复采样，再决定是否满足发布阈值。

## 仍需外部发布条件

- Tauri SSH：真实 OpenSSH/PAM MFA、多级提示词、Jump Host、SOCKS5/HTTP 代理、`-L/-R/-D`、sudo/root、CWD 事件与持续指标流必须在发行候选完整手测。当前自动化覆盖的是 MFA 协议夹具，以及本机 OpenSSH 的公钥/exec/SFTP/HTTP 代理/local direct-tcpip；macOS 远端指标当前不支持。
- Tauri FTPS：本地显式/隐式 TLS 夹具已验证控制与数据通道；仍需真实 FTPS 服务和受信任证书的发行候选验收。
- WebDAV：本地 HTTP 夹具已验证 HEAD/ETag/`If-Match`/412 冲突；仍需真实 WebDAV 服务、认证与 TLS 证书验收。
- Telnet：真实 TCP peer 和 HTTP CONNECT 代理夹具已验证；真实设备与 SOCKS5 代理仍待验收。
- Serial：Linux kernel PTY 已纳入 CI；macOS 和 Windows 需要实体设备或已知可靠的虚拟串口。`mark`/`space` parity 受上游 `serialport` API 限制，必须保持“明确不支持”，不能降级成 `none`。
- 三平台 socket lifecycle 与新增 Linux `tauri-real-protocols` workflow 仍只是配置，尚未推送运行；不能以 workflow 文件代替结果。
- Tauri signed updater 需要发布方提供更新 endpoint、Ed25519 公钥/私钥与 macOS/Windows 签名、公证资产；当前实现安全地使用 GitHub Release 检查及发布页安装，不伪造 silent install。
- macOS 以外的 CI 结果、真实代理服务和实体/虚拟串口设备需要在对应平台上运行；这些是外部运行环境，不应由 macOS 本机结果代替。
