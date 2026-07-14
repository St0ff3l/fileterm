# Tauri 迁移进度与 Electron 功能差距

| 项目     | 值                                                                                         |
| -------- | ------------------------------------------------------------------------------------------ |
| 文档版本 | v1.1                                                                                       |
| 更新日期 | 2026-07-14                                                                                 |
| 状态     | Phase 4 实现已完成；macOS 真实协议与生产打包已验收，Windows/Linux socket CI 矩阵待推送触发 |
| 关联文档 | `russh-migration.md`、`rust-backend-migration-plan.md`                                     |

---

## 1. 各阶段执行进度

### Phase 0：Tauri 直连骨架与基础能力 ✅ 已完成

- ✅ `apps/desktop/src/bridge/tauri-api.ts` 建立，renderer 不再直接 import Electron 类型
- ✅ Tauri 基础 commands：平台信息、剪贴板、UI preferences/state
- ✅ React bridge 接入，renderer 通过 `tauri-api.ts` 初始化
- ✅ Contract test 建立（`tests/contract.rs`，9 项断言）
- ✅ 命令命名 `app_` 前缀、事件命名 `namespace:name` 格式冻结

### Phase 1：Tauri 桌面壳垂直切片 ✅ 已完成

- ✅ Tauri 壳加载 React renderer
- ✅ macOS Overlay titleBar + trafficLightPosition(20,18)
- ✅ Windows 无边框 + Linux 原生 decorations
- ✅ 窗口尺寸对齐 Electron 默认值（main 1280×820，子窗口 860×680）
- ✅ 菜单 + 托盘 + macOS dock reopen
- ✅ 平台/剪贴板/UI prefs/文件选择器通过 contract test

### Phase 2：Rust 存储与 Workspace ✅ 已完成

- ✅ JSON 存储读写（profiles.json / profile-secrets.json / folders.json / command-folders.json / commands.json / ui-preferences.json / ui-state.json / webdav-sync.json）
- ✅ Profile/Folder/Command CRUD（`services/profile_ops.rs`）
- ✅ group/parentId 双向自愈（5 个单元测试覆盖）
- ✅ Secrets stripping + 持久化（contract test 专项断言）
- ✅ Ordering（profile/folder/command/command-folder）
- ✅ 旧 Electron userData 兼容（按 id 去重合并 + secrets 回填）
- ✅ Workspace snapshot 广播

### Phase 3：SSH 工作区主链路 ✅ 实现完成，待真实服务验收（russh 迁移 + 补齐）

- ✅ M3.1 russh 0.62.2 锁定：password / privateKey / agent / keyboard-interactive 四种认证
- ✅ M3.2 SSH shell + 终端：write/resize/data/state，16ms batcher
- ✅ M3.3 SFTP 文件操作：list/read/write/mkdir/rename/delete/permissions，含 root 模式（`sudo -S`/`sudo -n`）
- ✅ M3.4 CWD 跟随：OSC 7 + RemoteUser 1337 解析与广播
- ✅ M3.5 系统指标：Linux/BusyBox POSIX + Windows PowerShell/CIM + CRLF 归一化 + 平台探测
- ✅ M3.6 host verification + MFA：in-handshake 异步弹窗 + 多轮 OTP
- ✅ `app_resolve_ssh_interaction` 真实异步接通
- ✅ 单 SSH session 复用 shell + SFTP + metrics（避免 MaxSessions 限制）
- ✅ echo 重复 bug 修复（worker recv None 退出 + StrictMode 双挂载防护）
- ✅ Shell setup 注入：POSIX CWD 脚本双重门控（`shell_cwd_setup_for_platform` + `SHELL_CWD_SETUP`/`BUSYBOX_SHELL_CWD_SETUP`）
- ✅ Transcript 水化：reconnect 保留终端历史（追加分隔符而非重置）
- ✅ Auto-reconnect 2000ms 延迟（`reconnectMode === 'auto'` 触发 + 三重 guard）
- ✅ 远程文件多编码：UTF-8/UTF-16/GBK/Big5/EUC-JP/Shift-JIS/EUC-KR 等 16 种（`encoding_rs`）
- ✅ chmod 递归：`-R` + `applyTo` (all/files/directories) + `find -exec {} +`
- ✅ JumpHost 跳板机：`jumpProfileId` → `channel_open_direct_tcpip` → `connect_stream`
- ✅ SOCKS5 / HTTP CONNECT 出站代理：认证、IPv6 authority 与 HTTP 头注入防护（`tokio-socks` + `connect_http_proxy`）
- ✅ M3.7 SSH `-L/-R/-D` 隧道：Tauri bridge + command + SSH worker，`TcpListener` / `tcpip-forward` / SOCKS5 listener，断线/重连/关闭 tab 自动回收

> 注：Phase 3 的已完成项包含当前未提交工作树中的实现；提交前仍需完成 Rust 编译、contract test、Electron parity 回归和必要的手工 SSH 验收。

> 2026-07-14 回归修复：POSIX CWD hook 现在以交互 shell 的 CR 提交执行，并以 CR/LF 兼容的状态机抑制内部命令回显；CWD 事件会在“跟随终端”开启时发布 `remoteFilesLoading`、异步刷新相同路径的 SFTP 文件列表，并在成功或失败后结束 loading。SFTP `read_dir` 不再依赖服务端返回 `..`，会按 Electron 语义为非根目录生成父目录行。POSIX 指标脚本也移除了从 TypeScript 模板误带入 Rust raw string 的双重转义，恢复磁盘、进程和网络行的真实换行解析。

### Phase 4：其他协议与 Transfer ✅ 实现完成

- ✅ `suppaftp` FTP/FTPS：plain、显式 TLS 与隐式 TLS，文件 CRUD、REST 断点上传/下载、原子 rename 与取消。
- ✅ Telnet：`tokio::net` transport、RFC 854 IAC/NAWS/TERMINAL-TYPE、SOCKS5/HTTP CONNECT、resize 与退出关闭 socket。
- ✅ Serial：`tokio-serial` 的波特率/数据位/停止位/奇偶校验/硬件流控映射和设备断开处理。`mark/space` parity 受 `serialport` 跨平台 API 限制，会返回明确错误而不静默降级。
- ✅ 统一 TransferService：持久 journal、单文件/目录 manifest、上传/下载、hash/identity 校验、暂停/继续/取消/丢弃、tab 关闭与应用退出清理。
- ✅ WebDAV：HTTPS 默认、Basic Auth、ETag 前置条件冲突检测、SHA-256 bundle 校验、秘密字段剥离与 5 MB 输入上限。
- ✅ Electron parity：SSH Config/外部 JSON profile 导入导出、命令历史/发送偏好、文件编辑器关闭确认、跨窗口 UI/最大化事件、CSP、应用/SSH/协议错误本地日志。
- ✅ 更新检查：GitHub Release 的版本检查与安全发布页交接。签名的应用内静默安装依赖未提供的 Tauri updater 公钥、更新清单与公证资产，保留在 Phase 5 发布前置，不能伪造为已启用。

质量记录见 [`../../quality/tauri-phase4-validation.md`](../../quality/tauri-phase4-validation.md)。

### Phase 5：发行与切换 🔲 未开始

- 🔲 Tauri updater + 签名公证
- 🔲 三平台安装包
- 🔲 性能对比
- 🔲 迁移工具 + 回滚
- 🔲 正式发布

---

## 2. 与 Electron 原版功能差距

### 2.1 完全缺失（优先级高）

| 功能                       | Electron 源                                                    | 说明                                                                                                           |
| -------------------------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **Transfer 系统**          | `services/transfers/`                                          | ✅ `services/transfers.rs`：journal、目录 manifest、断点、暂停/继续/取消/丢弃、退出/关闭清理与 snapshot 事件。 |
| **SSH -L 本地转发**        | `services/sessions/ssh-tunnel-service.ts`                      | ✅ 已补齐：`TcpListener` → `channel_open_direct_tcpip`                                                         |
| **SSH -R 远程转发**        | 同上                                                           | ✅ 已补齐：`tcpip_forward` / `cancel_tcpip_forward` + `forwarded-tcpip` 回调                                   |
| **SSH -D 动态 SOCKS5**     | 同上                                                           | ✅ 已补齐：本地 SOCKS5 CONNECT listener → `channel_open_direct_tcpip`                                          |
| **SOCKS5 代理**            | `services/network/proxy-socket-factory.ts`                     | ✅ 已补齐：`tokio-socks`，支持无认证或 username/password                                                       |
| **HTTP CONNECT 代理**      | 同上                                                           | ✅ 已补齐：CONNECT + Basic 认证 + IPv6 authority + 响应边界限制                                                |
| **Jump Host / ProxyJump**  | `services/sessions/ssh-session-controller.ts::connectJumpHost` | ✅ 已补齐：`jumpProfileId` → `channel_open_direct_tcpip` → `connect_stream`                                    |
| **FTP/FTPS**               | `services/sessions/ftp-session-controller.ts`                  | ✅ `sessions/ftp.rs`，plain/显式/隐式 FTPS 和传输操作均已接入。                                                |
| **Telnet**                 | `services/sessions/telnet-session-controller.ts`               | ✅ `sessions/telnet.rs`，RFC 854 IAC/NAWS/TERMINAL-TYPE 实现完成。                                             |
| **Serial**                 | `services/sessions/serial-session-controller.ts`               | ✅ `sessions/serial.rs`；`mark/space` parity 显式拒绝（上游跨平台限制）。                                      |
| **Auto-update**            | `services/app-update-service.ts`                               | ✅ GitHub Release check + 发布页模式；签名 in-app updater 是 Phase 5 发布资产前置。                            |
| **Profile import/export**  | `services/connection-config-codec.ts`                          | ✅ SSH config/JSON preview + commit、fileterm/compatible 导出。                                                |
| **Command history**        | `services/file-profile-repository.ts`                          | ✅ 每 profile 历史与命令发送偏好已持久化。                                                                     |
| **openLogsDirectory**      | `apps/desktop/src/main/main.ts`                                | ✅ Rust command 打开应用日志目录。                                                                             |
| **App logger**             | `services/app-logger.ts`                                       | ✅ 轮转本地 app/SSH/协议错误日志，秘密字段脱敏。                                                               |
| **SSH debug logger**       | `services/sessions/ssh-debug-logger.ts`                        | ✅ SSH worker 生命周期/错误写入本地日志；不暴露凭据。                                                          |
| **真实 sshd/FTP 集成测试** | `test/protocol/sftp-resume.test.mjs` 等                        | ✅ macOS 本机 OpenSSH SFTP、FTP、显式/隐式 FTPS 7 项通过；Tauri socket matrix 已加入 CI。                      |

### 2.2 部分实现（需补齐）

| 功能                           | Electron 源                                                                            | Tauri 现状                                                                                                     | 缺口 |
| ------------------------------ | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ---- |
| **远程文件 encoding**          | `services/text-encoding.ts`（iconv-lite + 16 种编码）                                  | ✅ 已补齐：`decode_bytes`/`encode_text` + `encoding_rs`，支持 UTF-8/UTF-16/GBK/Big5/EUC-JP/Shift-JIS/EUC-KR 等 | —    |
| **远程 chmod 递归**            | `services/sessions/ssh-session-controller.ts::changeRemotePermissions`                 | ✅ 已补齐：`recursive` + `applyTo` (all/files/directories) + `find -exec {} +`                                 | —    |
| **Shell setup injection**      | `services/sessions/shell-cwd-integration.ts`（bash/zsh/posix/busybox 脚本 + 双重门控） | ✅ 已补齐：`shell_cwd_setup_for_platform` + `SHELL_CWD_SETUP`/`BUSYBOX_SHELL_CWD_SETUP` + 平台门控             | —    |
| **Transcript hydration**       | `services/sessions/ssh-session-controller.ts::BoundedTextBuffer`                       | ✅ 已补齐：reconnect 追加分隔符 + 200k 截断                                                                    | —    |
| **Auto-reconnect 2000ms 延迟** | `services/workspace-service.ts::autoReconnectingTabs`                                  | ✅ 已补齐：`reconnectMode === 'auto'` + 2000ms 延迟 + 三重 guard                                               | —    |
| **JumpHost / ProxyJump**       | `services/sessions/ssh-session-controller.ts::connectJumpHost`                         | ✅ 已补齐：`jumpProfileId` → `channel_open_direct_tcpip` → `connect_stream`                                    | —    |
| **WebDAV upload/download**     | `services/webdav-sync-service.ts`                                                      | ✅ PUT/GET + ETag/If-Match + SHA-256 + secret stripping。                                                      | —    |
| **UI preferences 变更事件**    | `apps/desktop/src/main/main.ts`（广播到所有窗口）                                      | ✅ `app:ui-preferences-changed` 广播。                                                                         | —    |
| **窗口最大化事件**             | Electron 自动广播                                                                      | ✅ toggle 与 Resized 均广播 `app:window-maximized-change`。                                                    | —    |
| **文件编辑器关闭确认**         | `apps/desktop/src/main/main.ts::requestQuitConfirmation`                               | ✅ `CloseRequested` 防关闭并发事件，确认后 destroy，取消保持窗口。                                             | —    |
| **CSP 安装**                   | `apps/desktop/src/main/main.ts::installContentSecurityPolicy`                          | ✅ Tauri production CSP 已安装；打包验证通过。                                                                 | —    |
| **Command send preferences**   | `services/file-profile-repository.ts`                                                  | ✅ `command-send-preferences.json` get/set。                                                                   | —    |

### 2.3 已完整实现（无需补齐）

- SSH 会话管理（connect/reconnect/disconnect/tab lifecycle/host key/MFA/agent/privateKey/keyboard-interactive）
- SFTP 文件操作（list/read/write/mkdir/create/copy/move/rename/delete，含 root 模式）
- PTY + CWD 跟随（OSC7 + RemoteUser 1337）
- sudo/root 文件访问模式（`sudo -S`/`sudo -n` + 密码缓存 + 失败检测）
- 系统指标采集（Linux/BusyBox/Windows + CRLF 归一化 + 平台探测）
- 16ms 终端 batcher
- Profile/Folder/Command CRUD + group/parentId 自愈 + secrets stripping + ordering
- 窗口管理（main/connection-manager/command-manager/connection-form/command-form/file-editor）
- 菜单 + 托盘 + macOS dock reopen
- UI preferences + UI state KV
- 剪贴板 + openExternalUrl
- 本地文件操作（list/read/write/mkdir/create/copy/move/rename/delete/permissions/selectFiles/selectDirectory，含 EXDEV 回退 + 递归 chmod）
- macOS keychain 规避（plain-text-fallback）
- Legacy 数据迁移（旧 Electron userData 兼容）
- 命令模板级联删除（parentId 上移到祖父）

---

## 3. 后续推进优先级

### P0（阻塞日常使用）

1. ~~**Transfer 系统**：上传/下载是文件管理器的核心能力，当前完全缺失~~ ✅ 已完成
2. ~~**远程文件 encoding**：中文/日文环境远程文件乱码，影响可用性~~ ✅ 已完成
3. ~~**Shell setup injection per platform**：CWD 跟随依赖远端 shell 主动 emit OSC7，不注入脚本则 CWD 不更新~~ ✅ 已完成
4. ~~**Auto-reconnect 2000ms 延迟**：立即重连在网络抖动时加剧服务器负载~~ ✅ 已完成

### P1（功能对齐）

5. ~~**SSH -L/-R/-D 隧道**~~ ✅ 已完成
6. ~~**SOCKS5/HTTP CONNECT 代理**~~ ✅ 已完成
7. ~~**Jump Host / ProxyJump**~~ ✅ 已完成
8. ~~**WebDAV 同步真实实现**~~ ✅ 已完成
9. ~~**Profile import/export**（SSH config + 外部 JSON）~~ ✅ 已完成
10. ~~**Transcript hydration**（reconnect 后终端历史）~~ ✅ 已完成
11. ~~**远程 chmod 递归**~~ ✅ 已完成

### P2（生态完整）

12. ~~**FTP/FTPS**（suppaftp）~~ ✅ 已完成
13. ~~**Telnet**（tokio::net + RFC 854）~~ ✅ 已完成
14. ~~**Serial**（tokio-serial）~~ ✅ 已完成
15. **签名应用内更新**（需要 release endpoint、公钥和公证资产；Release-page check 已完成）

### P3（质量加固）

16. ~~**真实 sshd/FTP 集成测试**~~ ✅ macOS 本机通过；跨平台 socket CI 已配置
17. ~~**App logger + SSH debug logger**~~ ✅ 已完成
18. ~~**CSP 安装**~~ ✅ 已完成
19. ~~**文件编辑器关闭确认流程**~~ ✅ 已完成
20. ~~**UI preferences / window-maximized 变更事件补齐**~~ ✅ 已完成
21. ~~**Command history + send preferences**~~ ✅ 已完成

### P4（发行）

22. 三平台签名/公证 + 安装包
23. 性能对比 Electron
24. 用户数据迁移工具 + 回滚保障

---

## 4. 验收标准

Tauri 迁移整体完成的验收标准（与 Electron 原版功能对齐）：

- [x] Transfer 系统：upload/download queue + journal + pause/resume/cancel/discard + 断点续传
- [x] SSH 隧道：-L / -R / -D 全部支持
- [x] 代理：SOCKS5 + HTTP CONNECT + 鉴权
- [x] Jump Host：单级链式 SSH session（当前范围不支持嵌套跳板）
- [x] 协议补齐：FTP/FTPS + Telnet + Serial
- [x] 远程文件多编码：gbk/big5/euc-jp/shift_jis/euc-kr 等
- [x] Shell setup injection：POSIX 双重门控
- [x] Auto-reconnect：2000ms 延迟
- [x] WebDAV 同步：upload + download + ETag + content hash
- [x] Profile import/export：SSH config + 外部 JSON + fileterm/compatible
- [x] Auto-update：GitHub Release check + 安全发布页更新路径
- [ ] 签名的 Tauri in-app updater：待 release endpoint、公钥和公证资产
- [ ] 三平台签名/公证 + 安装包
- [x] 真实 sshd/FTP 集成测试（macOS 本机 OpenSSH/FTP/FTPS）
- [x] macOS 冷启动进程/RSS 基线：Tauri 约 116 MiB，Electron 约 228 MiB（同机隔离配置；详见质量记录）
