# ADR-0008: MCP、CLI 与 Agent 共享访问策略和凭据边界

## 状态

Accepted（2026-08-29）

## 背景

FileTerm 同时提供桌面 GUI、本地 MCP 子进程和一次性 CLI。外部 Agent 需要访问已经保存的连接，但不能因为调用方是 AI 就获得所有主机、密码或可见终端输入能力。

并行调用还会带来两个独立问题：一次性 CLI 会为每个调用产生一个进程；同一 profile 的多个 open 请求可能重复创建 tab、SSH worker 和凭据弹窗。连接凭据输入也必须能在用户完成主窗口交互后把最终结果交回原始调用。

## 决策

### 1. 访问范围和操作等级由桌面端全局策略决定

策略持久化在 `UiPreferences.mcpAgent`，只包含非敏感数据：

- `connectionScope`：全部已保存连接、选中的已保存连接、当前活动会话或默认连接。
- `allowedProfileIds`：选中模式下允许访问的 profile ID。
- `defaultProfileId`：默认连接模式使用的 profile ID。
- `operationPolicy`：只读、受控操作或完全授权。

策略在 desktop bridge 的 action route 入口执行，而不是只在 renderer 隐藏按钮。MCP、一次性 CLI 和常驻 Agent 都使用同一个策略评估器；profile 删除后授权 ID 自动清理，旧配置缺失字段时采用 fail-closed 默认值。

完全授权只跳过逐次操作审批，不能绕过连接范围、session revision、路径校验、凭据边界或 terminal 输入限制。

### 2. 三种调用来源具有明确且不互相扩大权限的语义

| 来源       | 入口                 | 审批语义                                                      | 备注                               |
| ---------- | -------------------- | ------------------------------------------------------------- | ---------------------------------- |
| MCP        | `fileterm mcp`       | MCP mutation tool 在桌面端弹审批                              | 遵循 MCP JSON-RPC 和 progress 通知 |
| 一次性 CLI | `fileterm <command>` | 视为用户显式运行，不重复弹 MCP 审批                           | 仍受全局连接范围与只读策略约束     |
| Agent      | `fileterm agent`     | 面向 AI 的 Agent 请求在受控操作策略下强制进入同一桌面审批队列 | 不能通过请求字段关闭审批           |

调用来源只用于审计和选择正确的输出协议，不用于绕过连接范围。Agent 不区分具体客户端；第一版所有常驻 Agent 客户端共享同一份 FileTerm 全局策略。

### 3. SSH 登录凭据归 FileTerm 主窗口所有

`ConnectionOperation` 只保存 operation ID、profile ID、tab ID 和非敏感状态，并通过 profile-scoped single-flight 去重。MCP/Agent 的 `open_connection` 默认等待连接就绪，也可以返回 operation ID 后通过 `wait_for_connection` 继续等待。

当 SSH 登录密码、私钥口令或 keyboard-interactive 需要用户输入时：

1. SSH worker 通过现有 `ssh:interaction` 事件请求主窗口输入。
2. desktop bridge 向原始 CLI/MCP/Agent 调用发送脱敏 progress。
3. 用户完成或取消输入后，原始调用收到最终 Connected 或稳定错误码。

SSH 登录密码不新增到 MCP/Agent/CLI 的公开参数、stdout、stderr、结果或日志。普通 exec 仍不接管 MFA、确认、安装器、`passwd` 或 REPL 等通用交互输入。

sudo/su 是普通 exec 的受控特例：保存的凭据或主窗口安全 prompt 仍由 Rust 使用；确需一次性 CLI 脚本输入时，优先使用 `--sudo-password-stdin` 或 `--su-password-stdin`，而不是把密码放进 argv。

### 4. 常驻 Agent 解决重复 CLI 进程问题

桌面 GUI 进程负责持有连接、会话、秘密和审批队列。`fileterm agent` 是一个不启动 GUI 的常驻 JSONL 进程，通过同一个本地 authenticated desktop bridge 处理多个 request ID；每个请求拥有独立 progress 和最终结果，输出按行原子化写出。Agent 请求可通过 `cancel_request` 按 request ID 取消仍在等待的 Agent 结果，取消不回滚桌面端已经接受或开始执行的操作。

一次性 CLI 保留用于 shell 脚本和手动调试，不承诺“零进程”。设置页和 Agent 注册说明优先推荐常驻 Agent 或 MCP，以避免 AI 为每个动作重新 spawn FileTerm CLI。

无论外部客户端是否错误地并行启动多个一次性 CLI，桌面端仍对同一 profile 的连接建立做 single-flight：只复用一个 connection operation、tab、worker 和凭据 prompt。该去重不能消除已经启动的 CLI 进程本身。

### 5. 本地 bridge 的安全边界

- 仅监听 `127.0.0.1` 随机端口，并使用每次桌面启动生成的 owner-only runtime descriptor 和 token。
- token 使用常时比较；非 loopback peer、非法 descriptor、超大消息和超出并发上限的请求直接拒绝。
- bridge 结果、Agent progress、CLI 输出和 workspace snapshot 不包含 profile secret、私钥、密码或完整 terminal transcript。
- Agent 请求只能调用已注册 action；参数仍经过严格 schema、长度和路径校验。
- Agent 请求的 `requiresApproval` 仅为兼容字段，桌面端收到 Agent 请求时始终强制使用审批策略；`cancel_request` 只影响 Agent 等待与输出，不撤销桌面端已经开始的远程操作。

## 影响

### 正面影响

- 用户可以按连接和操作等级限制所有 MCP/Agent 客户端，而不需要为每个客户端维护一套权限。
- 缺少 SSH 登录密码时，原始调用不会在弹框后丢失；用户输入后能收到最终结果。
- AI 并行操作可以复用常驻 Agent 和同一 profile 的连接任务，减少重复 GUI 启动与重复凭据交互。
- 旧的 MCP JSON-RPC 和一次性 CLI 保持兼容；新 Agent 作为明确的新入口加入。

### 限制与非目标

- 一次性 CLI 本身仍是一个进程；要减少进程数量必须使用常驻 Agent 或 MCP。
- FileTerm 不通过后台 bridge 自动回答 MFA、验证码、安装器确认或任意终端输入。
- Agent 取消是 best-effort 的请求生命周期控制，不是远程命令回滚机制。
- macOS 打包产物的 application type、Dock 图标行为以及真实 SSH/FTP/网络设备连接仍需要目标环境人工验收。

## 实现位置

- `packages/core/src/index.ts`：共享的 MCP/Agent 偏好类型。
- `apps/tauri/src-tauri/src/commands/mod.rs`：偏好规范化、迁移和设置命令。
- `apps/tauri/src-tauri/src/services/mcp.rs`：MCP、CLI、Agent 协议、bridge route 和策略评估。
- `apps/tauri/src-tauri/src/services/connection_operations.rs`：连接 single-flight 与可等待状态。
- `apps/tauri/src-tauri/src/main.rs`：GUI、MCP、Agent 和一次性 CLI 入口分发。
- `apps/tauri/src/renderer/features/settings/SettingsModal.tsx`：策略和 Agent 设置界面。
