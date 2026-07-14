# Tauri Phase 4 验收记录

更新日期：2026-07-14（macOS arm64 本机）

## 已执行结果

| 项目                           | 结果        | 说明                                                                                                                                                       |
| ------------------------------ | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust 单元测试                  | 通过，26/26 | 覆盖 transfer manifest、WebDAV hash、SSH Config/JSON、Telnet RFC 854/直接 socket 释放，以及 Tauri `suppaftp` 对真实 TCP FTP server 的上传下载 round-trip。 |
| 真实协议测试                   | 通过，7/7   | 本机 `/usr/sbin/sshd` SFTP，FTP、显式 FTPS、隐式 FTPS；含断点双向传输和原子完成。                                                                          |
| Tauri production build         | 通过        | 产出 `FileTerm.app` 与 `FileTerm_1.1.1_aarch64.dmg`；CSP 与本地 `.icns/.ico/.png` 图标参与实际打包。                                                       |
| macOS socket lifecycle         | 通过        | Telnet 直接 transport drop 后服务端在 2 秒内收到 EOF。                                                                                                     |
| Windows/Linux socket lifecycle | 已纳入 CI   | `.github/workflows/ci.yml` 的 `tauri-socket-lifecycle` 在 macOS、Windows、Ubuntu 各运行同一测试；需要推送后由 GitHub Actions 给出外部结果。                |

## 性能基线

同一台 macOS arm64 机器、隔离临时 HOME、冷启动后 2 秒采样一次主进程 RSS：

| 指标            | Electron 1.2.1（`/Applications/FileTerm.app`） | Tauri 1.1.1 candidate | 结论                                 |
| --------------- | ---------------------------------------------: | --------------------: | ------------------------------------ |
| 进程可见时间    |                                        约 5 ms |               约 6 ms | 仅衡量 OS 创建进程，差异无统计意义。 |
| 主进程 RSS      |                                     约 228 MiB |            约 116 MiB | Tauri 低约 49%。                     |
| App bundle 体积 |                                     约 608 MiB |             约 40 MiB | Tauri 小约 93%。                     |

该基线不是交互就绪（TTI）或远程吞吐基准；两者版本也不同。发行候选必须在每个平台用同一 profile、同一连接和同一大文件重复采样，再决定是否满足发布阈值。

## 仍需外部发布条件

- Tauri signed updater 需要发布方提供更新 endpoint、Ed25519 公钥/私钥与 macOS/Windows 签名、公证资产；当前实现安全地使用 GitHub Release 检查及发布页安装，不伪造 silent install。
- macOS 以外的 CI 结果、真实代理服务和实体/虚拟串口设备需要在对应平台上运行；这些是外部运行环境，不应由 macOS 本机结果代替。
