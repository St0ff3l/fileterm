# Tauri 发行候选协议验收清单

本清单只记录不能由当前 macOS 本机自动化伪造的验收项。自动化覆盖见 `tauri-phase4-validation.md`；勾选本清单前，先保存对应平台、服务版本、FileTerm commit 和失败日志。

## SSH / SFTP

- [ ] 在真实 OpenSSH `sshd` 上使用 PAM/keyboard-interactive MFA：密码和 OTP 分两轮、同一轮混合提示、取消和错误 OTP 各跑一次。
- [ ] 经 HTTP CONNECT 和 SOCKS5（含用户名/密码）连接；记录代理拒绝、超时和正常断开后的 UI 状态。
- [ ] 用一台 bastion 和一台 target 执行单级 Jump Host；验证 target host key、SFTP、shell、断开重连。
- [ ] 对真实 target 验证 `-L`、`-R`、`-D`：本地/远端端口冲突、开始/停止、tab 关闭与应用退出回收。
- [ ] 在有 sudo 权限的真实账户验证 root 文件读写、暂停/继续 staging 文件及密码错误路径。
- [ ] Linux/BusyBox target 上验证 shell `cd` 后 OSC7/RemoteUser CWD 与文件面板同步；Windows target 确认不注入 POSIX hook。
- [ ] Linux/BusyBox/Windows target 各采集至少两个系统指标周期。macOS remote 当前不支持指标采集，必须明确保留为不支持，不得误报为 Linux。

## FTP / WebDAV / Telnet

- [ ] 对受信任 CA 证书的真实 FTPS 服务分别验证 explicit 与 implicit：登录、上传、下载、REST 续传、暂停、取消、重连及 TLS 证书失败提示。
- [ ] 对真实 WebDAV 服务验证 Basic Auth、HTTPS、上传后 ETag 保存、由第二客户端修改导致 `If-Match` 412、下载 hash 拒绝和 5 MB 限制。
- [ ] 对真实 Telnet 设备验证 IAC 协商、NAWS resize、编码和断线；分别走 HTTP CONNECT 与 SOCKS5 代理。

## Serial

- [ ] macOS：使用实际 `/dev/cu.*` 设备，验证打开、读写、断开、硬件与软件流控。
- [ ] Linux：使用实体设备或可信 PTY，验证与 CI 的 `tokio-serial` PTY 回环一致。
- [ ] Windows：使用实体 COM 或已记录的虚拟串口对，验证同一配置和拔插后的状态。
- [ ] 三个平台均确认 `mark`/`space` parity 返回明确“不支持”错误；不得降级为 `none`。

## 跨平台与性能

- [ ] 归档 GitHub Actions 的 macOS、Windows、Linux `tauri-socket-lifecycle` 和 Linux `tauri-real-protocols` 结果。
- [ ] 在三平台使用同一 profile、同一目标与同一大文件采样冷启动、RSS、连接时间和传输吞吐；Electron/Tauri 必须同版本、隔离用户目录、重复至少三次。
- [ ] 执行打包后的 Tauri 应用（不是 `cargo test`）的原生拖放、多文件路径、metadata、窗口菜单和文件编辑器关闭确认手测。
