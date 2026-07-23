> 归档状态（2026-07-23）：russh 主链路、代理和 `-L/-R/-D` 隧道实现已完成。三平台构建验证和与历史 Electron 的真实服务 parity 因外部依赖暂停。本文档移至 `docs/plans/completed/`。

# russh 迁移评估与计划

> 状态（2026-07-15）：russh 主链路、代理和 `-L/-R/-D` 隧道实现已完成；本文剩余内容只描述三平台构建和历史 Electron 42.4.0 parity 的真实服务验收。

## 1. 背景

Tauri 迁移 Phase 3 曾以 `ssh2`（libssh2 C 绑定 + vendored-openssl）作为过渡方案；当前已完成到 `russh`（纯 Rust SSH 实现）的切换。本页保留选型依据、已完成迁移记录和剩余验收项。

## 2. 版本与安全状态（2026-07-13 核对 crates.io）

| Crate        | 最新稳定版 | 发布日期   | 说明                          |
| ------------ | ---------- | ---------- | ----------------------------- |
| `russh`      | `0.62.2`   | 2026-07-06 | 纯 Rust，RustCrypto + ssh-key |
| `russh-sftp` | `2.3.0`    | 2026-05-23 | SFTP subsystem client/server  |

**已知安全公告**（均已在 0.61.1+ 修复，0.62.2 已包含）：

- CVE-2026-46673：compression ZIP bomb（fixed in 0.61.1）
- GHSA-f5v4-2wr6-hqmg：keyboard-interactive OOM（fixed in 0.61.1）
- 早期 RsaSha2 公钥认证签名 bug（fixed in 0.60.1）

**结论**：`russh 0.62.2` 无已知未修复漏洞，可作为目标版本。低于 0.61.1 不得使用。

## 3. ssh2 vs russh 能力对照

| 维度                  | ssh2 (libssh2)                                            | russh 0.62.2                                                         |
| --------------------- | --------------------------------------------------------- | -------------------------------------------------------------------- |
| 实现语言              | C（libssh2）+ Rust FFI                                    | 纯 Rust（RustCrypto + ssh-key）                                      |
| Tauri 打包            | 需 vendored-openssl，Windows 构建复杂                     | 无 C 依赖，三平台一致                                                |
| 异步模型              | 同步阻塞 API                                              | 原生 async（tokio）                                                  |
| Host key 验证         | 握手后 `host_key()` 同步查，无法中断握手                  | `check_server_key` async handler，可在握手期间弹窗                   |
| MFA 多 prompt         | `KeyboardInteractivePrompt` 同步返回                      | `authenticate_keyboard_interactive_start` + `respond` 循环，支持多轮 |
| 本地转发 (-L)         | `channel_open_direct_tcpip`                               | `channel_open_forwarded_tcpip`（v0.62.0 重命名）                     |
| 远程转发 (-R)         | `channel_open_forwarded_tcpip` + listen                   | 同上 + `tcpip-forward` global request                                |
| 动态转发 (-D, SOCKS5) | 需自写 SOCKS5 listener                                    | 同上，需自写 SOCKS5 listener                                         |
| ProxyJump (跳板机)    | 手动链式：jump session → `forwardOut` socket → 主 session | 同上，但 async 链式更清晰                                            |
| SFTP                  | 内置 `Sftp`                                               | `russh-sftp` 2.3.0（独立 crate）                                     |
| SSH agent             | `userauth_agent`                                          | `authenticate_future_with_agent`                                     |
| 公钥认证              | `userauth_pubkey_file/memory`                             | `authenticate_future_with_key`                                       |
| 错误信息              | libssh2 错误字符串较粗糙                                  | 结构化 `russh::Error` 枚举                                           |

## 4. 强烈建议迁移的核心理由

1. **Tauri 打包友好**：russh 纯 Rust，无 C 依赖，Windows 不需要 vendored-openssl，CI 构建时间和体积都更优。
2. **异步原生**：Tauri IPC 本身基于 async，russh 的 async API 能直接 `await`，不需要 `blocking_write` / `set_blocking(true/false)` 来回切换。
3. **Host key 异步弹窗**：russh 的 `check_server_key` 是 async handler，可以在握手期间 `await` 用户决策，实现真正的 in-handshake host verification 弹窗。ssh2 只能在握手后比对指纹，无法中途等待用户。
4. **MFA 多 prompt**：russh 的 `authenticate_keyboard_interactive_start` + `respond` 循环天然支持多轮 OTP/MFA。ssh2 的 `KeyboardInteractivePrompt` 是同步返回，无法 mid-handshake 等待用户输入。
5. **安全审计友好**：纯 Rust 实现，无 C 内存安全风险，RustCrypto 经过多轮审计。

## 5. 不建议立即迁移的考量

1. **迁移工作量**：ssh2 同步 API → russh async API 是全面重构，不是简单替换。Handler trait 模式、`check_server_key` 必须实现、`channel_open_*` v0.62.0 breaking change 都需要逐一处理。
2. **SFTP API 完全不同**：`russh-sftp` 的 API 与 libssh2 `Sftp` 差异很大，所有 SFTP 操作（list/read/write/mkdir/copy/move/rename/delete/permissions）都要重写。
3. **当前 ssh2 链路已基本可用**：Phase 3 主链路（shell + SFTP + metrics + CWD + tab 生命周期）已落地，metrics/echo 双修后体验可接受。
4. **russh-sftp 生态较新**：相比 libssh2 数十年的生产验证，russh-sftp 2.x 系列还在快速迭代，边角 case 可能未覆盖。

## 6. 推荐策略：分阶段迁移

### 阶段 A（历史）：ssh2 补齐 Phase 3 主链路

在 ssh2 基础上完成 Phase 3 缺失项中**可行性高**的部分：

- ✅ host key 指纹验证（同步阻塞式，mismatch 硬失败，first-connect 通知但不阻断）
- ✅ password / privateKey / agent + keyboard-interactive fallback
- ✅ shell + 16ms batcher + terminal:data/state
- ✅ SFTP list/read/write/mkdir/create/copy/move/rename/delete/permissions
- ✅ system_metrics Linux/BusyBox POSIX + Windows probe + CRLF 归一化
- ✅ OSC7/RemoteUser CWD 跟随 + 远端用户广播
- ✅ tab 生命周期（open/reconnect/disconnect/close/activate）
- 🔜 sudo/root 文件访问模式（exec channel + `sudo -S`/`sudo -n`，参考 Electron `execShellFileCommand`）
- 🔜 SSH -L/-R 隧道（ssh2 `channel_open_direct_tcpip` / `channel_open_forwarded_tcpip` + `tcpip-forward`）
- 🔜 SOCKS5/HTTP 代理（自实现协议握手，注入 ssh2 `set_tcp_stream`）
- 🔜 Jump Host（链式 SSH session，jump session `forwardOut` → 主 session `set_tcp_stream`）

### 阶段 B（已完成）：russh 迁移解决 ssh2 架构性瓶颈

下列功能在 ssh2 下**架构性不可行或极复杂**，必须等 russh 迁移：

- ⏳ 真·in-handshake host key 异步弹窗（需 `check_server_key` async handler）
- ⏳ MFA 多 prompt 异步弹窗（需 `authenticate_keyboard_interactive_start` + `respond` 循环）
- ⏳ SSH -D 动态转发（SOCKS5 server + async `channel_open_direct_tcpip`）
- ⏳ `app_resolve_ssh_interaction` 真实异步接通（pending Map + oneshot channel 唤醒）

### 阶段 C（当前）：russh 迁移后的外部验收

- `russh 0.62.2` + `russh-sftp 2.3.0` 已在 Cargo.lock 锁定并作为当前 SSH 主链路。
- `ssh2` 与 `vendored-openssl` 已不在当前 Cargo manifest；历史 Electron `ssh2` controller 仅保留作 parity 对照。
- 继续评估 `russh-sftp` 2.3.0 在真实服务和三平台发行场景的边角 case。

## 7. russh 迁移技术要点

### 7.1 依赖替换

```toml
# 移除
# ssh2 = { version = "0.9.6", features = ["vendored-openssl"] }

# 新增
russh = "0.62.2"
russh-sftp = "2.3.0"
russh-keys = "0.62"  # 若需要独立 key 解析（russh 已内嵌 ssh-key，通常不需要）
```

### 7.2 Handler trait 实现

russh 采用 Handler trait 模式，必须实现：

```rust
struct ClientHandler {
    tab_id: String,
    app: AppHandle,
    host_key_tx: oneshot::Sender<bool>,  // 用户是否接受 host key
    kbi_tx: mpsc::Sender<KbiPrompt>,     // keyboard-interactive prompts 通道
}

#[async_trait]
impl russh::client::Handler for ClientHandler {
    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fp = server_public_key.fingerprint(Sha256::default());
        let fp_str = format!("SHA256:{}", base64::engine::general_purpose::STANDARD.encode(fp.as_bytes()));
        // 发 ssh:interaction 事件，await 用户决策
        let accepted = self.host_key_tx.1.await?;
        Ok(accepted)
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        // terminal:data 事件
    }
    // ... 其他回调
}
```

### 7.3 v0.62.0 breaking change

`channel_open_direct_tcpip` → `channel_open_forwarded_tcpip`（命名统一）。迁移时注意 API 名称变更。

### 7.4 SFTP API 差异

```rust
// ssh2
let sftp = sess.sftp()?;
let entries = sftp.readdir(path)?;

// russh-sftp
let channel = sess.channel_open_session().await?;
let sftp = SftpSession::new(channel.into_stream()).await?;
let entries = sftp.read_dir(path).await?;
```

### 7.5 异步重构

所有 SSH 操作从同步线程模型迁移到 tokio async：

- `start_ssh_worker` 改为 `tokio::spawn` 而非 `thread::spawn`
- `cmd_rx: mpsc::Receiver<WorkerCmd>` 从 `tokio::sync::mpsc` 改为 `tokio::sync::mpsc`（已经是了，但 `blocking_write` 调用要改成 `.await`）
- `sess.set_blocking(true/false)` 全部移除，russh 没有 blocking 概念
- `shell_channel.read()` 改为 `channel.wait()` async 循环

### 7.6 迁移工作量估算

| 模块                         | 工作量 | 风险点                                  |
| ---------------------------- | ------ | --------------------------------------- |
| `sessions/ssh.rs`            | 高     | Handler trait + async 重构              |
| `sessions/system_metrics.rs` | 中     | `exec_command` async 化                 |
| `commands/mod.rs`            | 低     | WorkerCmd 处理改为 async                |
| `services/workspace.rs`      | 低     | 无变化                                  |
| 测试                         | 中     | contract test 不变，新增 russh 集成测试 |
| 文档                         | 低     | 更新 architecture.md                    |

## 8. 决策

**已完成 russh 迁移（2026-07-13）；当前仍在做 Tauri Phase 3 的功能收尾。**

用户明确要求"转 Rust 最主要的就是要用 russh"，已推翻"保留 ssh2 补齐 Phase 3"的过渡决策，直接完成 russh 迁移。

已完成项：

1. `ssh2` 依赖从 Cargo.toml 移除，`vendored-openssl` feature 移除。
2. `russh = "0.62.2"` + `russh-sftp = "2.3.0"` 作为 SSH 主链路。
3. `sessions/ssh.rs` 全面 async 重构：Handler trait + `check_server_key` async handler + in-handshake host key 异步弹窗（oneshot + `pending_interactions` Map）。
4. MFA 多 prompt 异步弹窗：`authenticate_keyboard_interactive_start` + `respond` 循环，第一轮用配置密码，后续轮次经 `ssh:interaction` 事件交由用户填写。
5. `app_resolve_ssh_interaction` 真实异步接通：从 `pending_interactions` 取出 oneshot sender，把 renderer 的 response 投回握手 worker。
6. 单 SSH session 复用 shell + SFTP + metrics（`Arc<Handle>` 共享），避免服务器 MaxSessions 限制导致 sidebar metrics 不显示。
7. `cmd_rx.recv()` 返回 None 时 worker 正确退出（修复 reconnect 后旧 worker 残留导致 echo 重复 "clear" → "clearclear" 和回车双行）。
8. 前端 `subscribe` 改用 `transformCallback` 直接注册 callback，cleanup 时同步注销 JS-side callback，关闭 StrictMode 双挂载下 listen/unlisten IPC 竞态窗口。
9. `system_metrics.rs` 全面 async 化：`probe_remote_platform` / `exec_command` / `exec_command_with_stdin` 接收 `&ClientHandle`，复用主 session。
10. cargo build/test（14 pass）+ npm typecheck/lint/format/test（16 pass）全绿。

已补齐：

- SSH `-L/-R/-D` 隧道（`TcpListener` / `tcpip-forward` / forwarded-tcpip callback / SOCKS5 listener）。
- SOCKS5/HTTP CONNECT 代理（认证、IPv6 authority、HTTP 响应边界与注入防护）。
- Jump Host（链式 SSH session，jump session 转发 → 主 session）。
- sudo/root 文件访问模式真正用 `sudo -S` / `sudo -n` 执行（以当前工作树实现和测试结果为准）。

## 9. 验收标准

russh 迁移完成的验收标准：

- [x] `ssh2` 依赖从 Cargo.toml 移除，`vendored-openssl` feature 移除
- [x] host key 验证改为 in-handshake 异步弹窗（用户可在握手期间 accept/reject）
- [x] MFA 多 prompt 异步弹窗（支持多轮 OTP）
- [x] `app_resolve_ssh_interaction` 真实异步接通
- [x] cargo build/test + npm typecheck/lint/format/test 全绿
- [x] SSH -L/-R/-D 隧道全部支持（待真实 SSH 服务验收）
- [x] SOCKS5/HTTP 代理全部支持（待真实代理服务验收）
- [ ] macOS / Windows / Linux 三平台构建验证（后续推进）
- [ ] 与历史 Electron 42.4.0 版本完成真实服务 parity：重点为三平台 socket 生命周期、SFTP 边角 case 与 Phase 4 协议/传输能力
