# MCP / CLI 外部客户端访问控制与连接凭据闭环计划

状态：阶段 0-6 的代码、自动化回归与文档已完成；打包人工验收与真实设备验收待进行

关联 Issue：#224

创建日期：2026-08-29

关联文档：

- [架构地图](../../architecture.md)
- [本地终端与 Agent MCP 接入](../completed/local-terminal-mcp.md)
- [MCP / CLI interactive-exec 迁移记录](../completed/mcp-cli-interactive-exec.md)
- [简化远程 exec 与 sudo/su 凭据自动化](../completed/simplify-exec-sudo-credentials.md)
- [发行候选统一验收计划](./release-candidate-acceptance.md)

## 1. 一句话结论

Issue #224 需要解决的不是“给 MCP 再配置一个密码”，而是建立一条完整、可等待、可审计的 Agent 访问边界：

```text
只能访问 FileTerm 已保存且被用户允许的连接
        ↓
按全局策略限制操作范围和危险等级
        ↓
缺少 SSH 登录凭据时由 FileTerm 主窗口安全输入
        ↓
原始 MCP/CLI 调用保持等待并收到最终 Connected/Error 结果
        ↓
多个并行调用复用连接任务，避免重复建连和重复启动 GUI 进程
```

本计划同时处理四个相关但独立的问题：

1. 按连接配置访问范围：从“全部 / 当前 / 默认”扩展到“用户选中的已保存连接”。
2. 全局操作等级：所有 MCP 客户端共享同一份 FileTerm 策略。
3. SSH 登录凭据闭环：缺少登录密码、私钥口令或 MFA 时，用户在 FileTerm 界面输入后，外部调用能拿到最终连接结果。
4. CLI 进程模型：AI 并行调用 CLI 时，减少一次调用一个进程带来的多个 FileTerm 图标和重复连接任务。

## 2. 背景与现状

### 2.1 用户场景

当前用户可能让 AI 通过 CLI 或 MCP 操作 FileTerm：

```text
AI 并行发出四个 CLI 命令
        ↓
启动四个一次性 FileTerm CLI 进程
        ↓
macOS Dock / 应用切换器显示多个 FileTerm 图标
```

当目标 SSH 连接没有保存登录密码时，当前行为又分成两段：

```text
fileterm open
        ↓
立即返回 Connecting
        ↓
CLI 进程结束
        ↓
FileTerm 主窗口才弹出 SSH 登录密码框
        ↓
用户输入后，后台连接继续
```

桌面连接可以继续，但最初那次 CLI 调用已经结束，因此它拿不到后续的 Connected 结果。这与当前已经实现的 sudo/su 前台密码等待不同。

### 2.2 当前运行时边界

同一份 Tauri 可执行文件按参数分为三种角色：

```text
argv[1] = mcp       → 常驻 stdio JSON-RPC MCP server
argv[1] = CLI 命令  → 一次性 CLI bridge client
其他                  → FileTerm GUI
```

MCP/CLI 都通过本地 loopback bridge 请求已经运行的 FileTerm 主进程；MCP/CLI 子进程不直接持有 SSH 密码。

当前 bridge 的安全边界包括：

- 仅允许 loopback 地址。
- 使用运行时 descriptor 中的 token 认证。
- MCP/CLI 不返回连接凭据和 terminal transcript。
- action route 前执行连接范围和只读策略。
- MCP 写操作、会话状态变化、文件/传输变更、隧道和 sudo/su 默认经过桌面审批；普通远程命令自动执行。
- CLI 作为显式启动的接口仍受共用策略约束；基础安全操作中的查询和普通远程命令自动执行，其它变更同样回 FileTerm 主窗口审批。

代码入口：

- [main.rs](../../../apps/tauri/src-tauri/src/main.rs)

* [mcp/mod.rs](../../../apps/tauri/src-tauri/src/services/mcp/mod.rs)

### 2.3 当前权限策略

当前 MCP / CLI 偏好包含连接白名单和操作等级两个维度；设置页按两张独立策略卡片展示：

| 维度       | 当前值                                        |
| ---------- | --------------------------------------------- |
| 连接白名单 | all-saved-connections、selected-connections   |
| 操作策略   | read-only、basic-safe-operations、full-access |

第一张卡片是 MCP / CLI 共用执行权限和能力对照，第二张卡片是“所有连接 / 指定连接”的全局白名单。当前已有“只能访问保存连接”的基础：打开连接使用 profile_id，由 Rust 从保存的 profile 中查找；列表和会话信息也会按白名单过滤。连接自身的协议能力和 FileTerm 安全校验不在共用策略卡片中重复配置，而是继续作为硬上限。

当前实现已经补齐逐个连接选择、基础安全操作和第三档“完全访问”策略；仍然不区分具体客户端，MCP、一次性 CLI 和 CLI JSONL 共享 FileTerm 全局设置与 Rust policy evaluator。基础安全操作下查询和普通远程命令自动执行，会话/文件/传输变更、隧道、提权和未知操作统一回 FileTerm 主窗口确认。

### 2.4 当前凭据处理

连接 profile 的密码、私钥口令、代理密码、sudo 密码和 su 密码由 Rust backend-side 读取和使用。公开 profile 只带非敏感存在标记，例如 hasSavedPassword，不会跨公开 bridge 返回明文。

| 凭据                           | 当前处理方式                                                                 | 当前外部调用结果                                         |
| ------------------------------ | ---------------------------------------------------------------------------- | -------------------------------------------------------- |
| SSH 登录密码                   | 保存的密码由 SSH worker 使用；缺失时发出 ssh:interaction，主窗口弹凭据框     | open 先返回 Connecting，原 CLI/MCP 调用不等待认证结束    |
| 私钥 passphrase                | 缺失时主窗口弹 key-passphrase 交互                                           | 与 SSH 登录密码相同，外部 open 不等待最终状态            |
| SSH keyboard-interactive / MFA | 非密码类 prompt 交给主窗口                                                   | 外部 open 不形成完整的可等待凭据协议                     |
| sudo 密码                      | 一次性字段 → 加密 profile → 主窗口安全输入框                                 | CLI/MCP 的 exec 可以等待，输入后返回最终命令结果         |
| su / su - 密码                 | 一次性字段 → 加密 profile → 主窗口安全输入框，独立 exec channel 使用受控 PTY | CLI/MCP 的 exec 可以等待，输入后返回最终命令结果         |
| 普通命令交互输入               | 不接管 MFA、确认、安装器、passwd、REPL 等输入                                | 返回 REMOTE_INTERACTIVE_INPUT_REQUIRED                   |
| FTP 登录密码                   | profile 缺失字段时使用协议层默认值，不弹 FileTerm SSH 凭据框                 | 登录失败在后台变为连接错误；open 仍可能先返回 Connecting |

## 3. 目标

### 3.1 功能目标

- AI 只能访问 FileTerm 已保存的连接，不能通过 MCP/CLI 临时构造任意主机、用户名或凭据。
- 用户可以从已保存连接中选择允许 Agent 访问的服务器。
- 所有 MCP 客户端共享 FileTerm 的全局连接范围和操作等级策略。
- MCP 和 CLI JSONL 在缺少 SSH 登录凭据时，可以等待 FileTerm 主窗口的安全输入，并收到最终连接结果。
- 已保存密码时不弹框；明确配置空密码时按空密码策略处理。
- sudo/su 继续使用现有的独立、安全凭据链路，不复用 SSH 登录密码。
- 同一 profile 的并行打开请求复用同一个连接任务、tab 和凭据 prompt。
- AI 常驻使用 MCP 或 CLI JSONL bridge 时，不因每个请求都创建一个新的 GUI 进程。
- 部署、镜像构建、迁移和微服务编排等长任务通过稳定的 command ID 启动一次、增量读取、显式终止，不因单次 MCP 请求结束或 SSH 重连而重复提交。

### 3.2 安全目标

- SSH 登录密码不新增到 MCP tool result、CLI stdout、命令行参数或对话历史中。
- 私钥口令、sudo/su 密码、代理密码和 session transcript 不进入外部 Agent 可见结果。
- 连接范围策略必须在 Rust bridge route 前执行，不能只依赖 Renderer 隐藏。
- 操作等级策略必须在所有外部 Agent 入口保持一致；策略不允许时 fail closed。
- 密码 prompt 必须绑定 profile、tab、session revision 和当前连接任务，防止输入被错投到其他会话。
- 所有等待都有明确 deadline、取消路径和稳定错误码。
- 后台命令拥有有界输出缓存、固定 tab 作用域和 INT → TERM → KILL → close 的 best-effort 终止路径。

## 4. 非目标

- 不新增让 MCP/CLI 直接传入 SSH 登录密码的公开字段。
- 不允许 Agent 向可见 SSH terminal 注入任意连续键盘输入。
- 不把普通 MFA、验证码、安装器确认或 REPL 变成后台自动化输入。
- 不在第一版引入每个 MCP client 独立的权限配置；第一版所有 MCP 客户端共用全局策略。
- 不强制把用户在 SSH 登录 prompt 中输入的密码永久保存；默认仍按一次性内存凭据处理。
- 不把 FTP、Telnet、Serial 伪装成支持 SSH 远程 exec 或 sudo/su。
- 不要求一次性 CLI 完全消失；保留它作为脚本入口，外部 Agent 优先使用 MCP 或 CLI JSONL bridge。

## 5. 关键设计原则

### 5.1 凭据归 FileTerm 主窗口所有

AI 只知道：

```text
连接是否允许
连接是否已保存
连接是否已连接
是否等待用户输入
最终成功或失败状态
```

AI 不知道：

```text
SSH 密码
私钥 passphrase
sudo/su 密码
代理密码
terminal transcript 中的敏感内容
```

用户在 FileTerm 主窗口中输入密码后，Rust 通过一次性 channel 把结果交给等待中的连接任务；外部 Agent 只收到脱敏状态。

### 5.2 连接授权与凭据配置是两件事

用户可以允许一个已经保存、但尚未保存密码的 profile：

```text
允许访问 production
        ↓
production 没有保存 SSH 密码
        ↓
FileTerm 主窗口请求用户输入
        ↓
连接任务继续
```

“允许访问”不等于“密码已经配置好”。没有权限的 profile 即使用户输入了密码，也必须在 bridge route 前被拒绝。

### 5.3 连接建立、普通 exec 和 sudo/su 保持分层

```text
SSH 建连：login password / key passphrase / host key / MFA
普通 exec：独立 SSH exec channel，不接管通用交互
sudo/su：普通 exec 的受控特例，stdin / PTY 传递凭据
```

不能为了让 CLI 等待 SSH 密码，就把普通 exec 改造成可以向远程 shell 注入任意输入。

## 6. 目标用户流程

### 6.1 已保存 SSH 密码

```text
MCP / Agent 调用 open
        ↓
策略检查通过
        ↓
读取加密 profile secret
        ↓
SSH 建连成功
        ↓
返回 Connected 结果
```

没有 UI 密码框，也不产生 credential progress。

### 6.2 缺少 SSH 登录密码：CLI

目标行为：

```text
fileterm open --profile-id PROFILE_ID
        ↓
CLI 请求进入 FileTerm desktop bridge
        ↓
FileTerm 创建 connection operation
        ↓
SSH worker 发出 ssh:interaction
        ↓
FileTerm 恢复并聚焦主窗口，弹 SSH 凭据框
        ↓
CLI stderr：等待用户在 FileTerm 输入 SSH 凭据
        ↓
用户输入密码
        ↓
SSH worker 完成认证
        ↓
CLI stdout：Connected JSON
```

如果用户取消、超时或认证失败，CLI 仍在同一个进程中收到非零退出结果和稳定错误信息。

### 6.3 缺少 SSH 登录密码：MCP

目标行为：

```text
fileterm_open_connection
        ↓
MCP 操作审批
        ↓
FileTerm 创建 connection operation
        ↓
notifications/progress：等待 FileTerm 前台输入
        ↓
主窗口弹 SSH 凭据框
        ↓
用户输入密码
        ↓
MCP tool call 返回 Connected JSON
```

MCP 客户端不接收密码内容，只接收等待状态和最终连接状态。

### 6.4 缺少 sudo/su 密码

保留现有行为：

```text
显式一次性密码
        ↓
加密 profile 中已保存密码
        ↓
FileTerm 主窗口安全 prompt
        ↓
主窗口不可用时返回 SUDO_PASSWORD_NEEDED / SU_PASSWORD_NEEDED
```

基础安全操作下，MCP/CLI/CLI JSONL 的查询和由内置 Copilot 规则判定为只读的普通远程命令自动执行；变更、破坏性、提权或未知命令，以及会话/文件/传输变更、隧道和 sudo/su 操作进入同一 FileTerm 主窗口审批。三者都能在主窗口 prompt 可用时保持原调用等待，并在用户输入后拿到最终命令结果。

### 6.5 普通命令需要通用交互输入

以下情况仍不由 MCP/CLI 自动回答：

- MFA / OTP。
- 安装器确认。
- passwd。
- 交互式 REPL。
- 需要用户选择的远程程序。

普通 exec 返回 REMOTE_INTERACTIVE_INPUT_REQUIRED，用户在可见 SSH tab 完成操作后再重试。

### 6.6 长部署：后台命令会话

长命令不使用同步的 `fileterm_execute_remote_command`。外部 Agent 应按下面的协议操作：

```text
fileterm_start_remote_command(tab_id, command)
        ↓ 返回 commandId
循环：fileterm_read_remote_command(tab_id, commandId, offset, wait_ms)
        ↓ 只消费 output，并把 nextOffset 作为下一次 offset
running=false
        ↓
必要时 fileterm_terminate_remote_command
        ↓
fileterm_close_remote_command
```

启动阶段打开一个独立 SSH exec channel，成功后把 channel 和有界输出缓存交给桌面 workspace registry；后续短 MCP 请求只读取这个 registry，不会重新提交命令。worker 断线时后台命令随原 channel 结束并报告终态，重连 worker 不会自动重跑。

## 7. 连接任务与凭据等待设计

### 7.1 内部连接任务模型

在 Rust workspace runtime 增加内部 connection operation registry。它不是连接 profile，也不是公开密码对象，只描述一次连接尝试：

```rust
struct ConnectionOperation {
    operation_id: String,
    profile_id: String,
    tab_id: String,
    state: ConnectionOperationState,
    created_at: Instant,
    deadline: Instant,
}

enum ConnectionOperationState {
    Starting,
    WaitingForCredentials,
    WaitingForKeyPassphrase,
    WaitingForKeyboardInteractive,
    Connecting,
    Connected,
    Failed,
    Cancelled,
}
```

ConnectionOperation 不保存密码文本。密码仍只存在当前 worker 的短生命周期对象和一次性 response channel 中。

### 7.2 连接完成通知

当前 app_open_profile 创建 tab 后立即返回 workspace snapshot。计划增加一个可等待的完成通知边界：

- worker 完成 SSH authentication 后发送 Connected。
- worker 进入最终错误后发送 Failed。
- worker 因用户取消、窗口关闭或 deadline 到期后发送 Cancelled / TimedOut。
- 任何等待者都通过同一个 operation 获得结果。

GUI 内部继续可以使用立即返回的 open 行为；MCP 和 CLI JSONL 使用可等待的 bridge action，避免阻塞整个桌面 UI command。

### 7.3 外部 API 建议

#### MCP

扩展 fileterm_open_connection：

```json
{
  "profile_id": "profile-1",
  "execution_mode": "background",
  "wait_for_ready": true,
  "timeout_ms": 120000
}
```

`execution_mode` 必须由 Agent 在第一次打开连接前向用户询问，取值为 `background` 或 `visible-terminal`。`open_connection` 默认等待连接完成；`background` 创建的 session 保留在 FileTerm worker 中，但不进入顶部标签栏，而是显示在 GUI 的“后台会话”页面，结果返回 `sessionId`（同时保留 `tabId`）。GUI 打开该会话或调用 `fileterm_activate_session` 会复用原 session 并 attach 到可见标签。`background` 后续只能配合独立 SSH exec 的 `fileterm_execute_remote_command`；`visible-terminal` 后续必须先调用 `fileterm_activate_session`，再调用只向可见终端写入单行命令的 `fileterm_execute_visible_command`。网络设备没有后台 exec 能力，只能使用可见终端路径。保留 wait_for_ready 为 false 给只需要创建 session 的客户端。

外部来源会记录在 session 的 `source` 字段中：CLI 请求为 `cli`，MCP 请求为 `mcp`。GUI 的后台会话页面以及会话 attach 后底部的会话 ID 都显示对应的 `CLI` / `MCP` 来源标签；普通 GUI 会话不设置来源。

返回值中的 `session` 会包含会话标识和后台可见性：

```json
{
  "operation": "open_connection",
  "activeTabId": null,
  "session": {
    "sessionId": "tab-1",
    "tabId": "tab-1",
    "background": true,
    "source": "mcp",
    "profileId": "profile-1",
    "status": "connected",
    "connected": true
  },
  "connectionOperationId": "connection-op-1",
  "connectionStatus": "connected",
  "executionMode": "background",
  "timedOut": false
}
```

如果一次调用已经超时但 connection operation 仍可查询，增加 fileterm_wait_for_connection。它只接收 operation_id 和等待超时，不接收密码。

#### CLI

建议把 CLI JSONL 的 open 默认改为等待，并提供显式的快速返回选项：

```bash
fileterm open --profile-id PROFILE_ID
fileterm open --profile-id PROFILE_ID --no-wait
fileterm wait-connection --operation-id OPERATION_ID
```

如果需要兼容既有脚本，也可以把默认行为暂时保持为快速返回，但所有 Agent 注册命令必须使用 wait；最终以实现阶段的兼容性验证决定。无论采用哪种默认值，都必须存在显式的 wait-connection 恢复路径。

### 7.4 进度事件

MCP 使用 notifications/progress 或 notifications/message；CLI 使用 stderr；二者共享脱敏消息：

```text
connection-started
waiting-for-ssh-credentials
waiting-for-key-passphrase
waiting-for-keyboard-interactive
connection-established
connection-failed
```

消息中可以包含脱敏的 profile 名称和 operation ID，但不能包含密码、prompt 原文中的敏感内容、私钥内容或 terminal transcript。

### 7.5 稳定错误码

建议增加以下外部错误码；具体命名可在实现时和现有 mcp_error_code 统一：

| 错误码                         | 含义                                       | 是否可自动重试              |
| ------------------------------ | ------------------------------------------ | --------------------------- |
| SSH_CREDENTIALS_NEEDED         | 主窗口/Renderer 不可用，无法展示登录凭据框 | 否；应提示用户打开 FileTerm |
| SSH_CREDENTIALS_CANCELLED      | 用户取消了登录凭据输入                     | 否                          |
| SSH_CREDENTIALS_TIMEOUT        | 登录凭据等待超时                           | 否；可由用户重新发起        |
| SSH_AUTH_FAILURE               | SSH 认证失败                               | 否；需用户检查凭据          |
| SSH_KEY_PASSPHRASE_NEEDED      | 私钥口令需要前台输入但输入通道不可用       | 否                          |
| SSH_INTERACTIVE_INPUT_REQUIRED | 建连阶段需要通用交互输入                   | 否；回到可见 SSH tab        |
| FILETERM_SESSION_DISCONNECTED  | exec 请求时会话尚未连接或已断开            | 可在连接成功后重试          |

对于“主窗口可用”的情况，不立即返回 *_NEEDED，而是发送 progress 并保持原调用等待。

### 7.6 并发与去重

同一 profile 的并行 open 必须通过 profile-scoped connection flight 去重：

```text
请求 A ─┐
请求 B ─┼─→ 同一个 profile connection operation
请求 C ─┘              ↓
              一个 tab / 一个 SSH worker / 一个密码框
```

规则：

- 已有 Connected session 时直接返回现有 session。
- 已有 Connecting operation 时加入等待者，不创建新 tab。
- 同一 operation 的多个等待者只共享状态，不共享密码对象。
- operation 失败后可以由用户明确发起新的尝试；不自动无限重试。
- tab 关闭、profile 删除或 session revision 改变时，所有等待者收到取消/失效结果。

## 8. MCP 全局权限模型

### 8.1 连接范围

连接策略收敛为“全部已保存连接”和“指定连接”两种模式：

```ts
type McpConnectionScope = 'all-saved-connections' | 'selected-connections'
```

偏好增加：

```ts
interface McpAgentPreferences {
  connectionScope: McpConnectionScope
  allowedProfileIds: string[]
  operationPolicy: McpOperationPolicy
}
```

allowedProfileIds 只保存稳定 profile ID，不保存主机密码或连接 secret。

策略规则：

- selected-connections 只允许 allowedProfileIds 中仍然存在的已保存 profile。
- profile 被删除后自动清理对应 ID。
- profile 改名不影响授权。
- profile 未选择时，不能通过 open_connection、tab、transfer、tunnel 或 call 间接访问。
- 连接列表、会话列表、传输列表和等待传输结果必须使用同一过滤器。
- 选择了 profile 但 profile 没有密码时，允许进入 SSH 凭据等待流程；“选中”不等于“已保存密码”。
- 新安装默认采用 fail-closed 的指定连接模式，初始没有允许连接；已有用户的显式旧策略不得被静默扩大。
- 旧的 `active-session` 配置迁移为空白指定连接白名单；旧的 `default-connection` 配置迁移为只包含原 `defaultProfileId` 的指定连接白名单。
- 连接编辑页不增加 MCP 专属权限字段；连接自身的协议能力和 FileTerm 安全校验继续限制 MCP/CLI 的有效能力。

### 8.2 操作等级

设置页提供三档执行权限和一张能力对照表：

| 等级         | 读取 | 写入/删除 | 远程命令                                                                 | 传输/隧道      | 是否审批                                                                                          |
| ------------ | ---- | --------- | ------------------------------------------------------------------------ | -------------- | ------------------------------------------------------------------------------------------------- |
| 只读         | 允许 | 拒绝      | 只允许明确只读查询                                                       | 只允许查询状态 | 不需要                                                                                            |
| 基础安全操作 | 允许 | 允许      | Copilot 判定的普通安全命令自动；变更、破坏性、提权或未知命令需主窗口确认 | 允许           | 会话/文件/传输变更、隧道和未知操作需要 FileTerm 主窗口审批                                        |
| 完全访问     | 允许 | 允许      | 允许                                                                     | 允许           | 跳过包括 sudo/su 操作在内的逐次审批，但仍受连接范围、session revision、输入校验和凭据安全边界约束 |

旧版 `approved-operations` 读取时迁移为 `basic-safe-operations`。full-access 只表示免除逐次操作审批（包括 sudo/su 操作确认），不表示绕过 sudo/su 密码输入或其它安全边界：

- 可以访问未保存或未选择的主机。
- 可以读取密码。
- 可以注入任意 terminal 输入。
- 可以绕过路径、tab、session revision 或传输校验。

### 8.3 MCP 客户端共享策略

第一版不区分 Claude、Codex、OpenCode 或其他 MCP client。所有 MCP client：

```text
读取同一份 UiPreferences.mcpAgent
        ↓
经过同一套 Rust policy evaluator
        ↓
进入同一套 action route / approval / credential boundary
```

BridgeRequest 使用内部 `source` 字段记录审计和 UI 展示来源，但不把它当作独立权限身份。当前只有 `Mcp` 与 `Cli` 两种值；一次性 CLI 与常驻 `fileterm cli --jsonl` 都记录为 `Cli`。未来如需 per-client 权限，另立安全设计，不在本计划中隐式加入。

### 8.4 CLI 审批语义

当前 CLI 被视为用户显式启动的接口，因此不弹 MCP 审批；但它仍受连接范围和只读策略影响。

推荐分两类入口：

1. 一次性 fileterm command：保留脚本兼容性；查询和普通远程命令自动执行，其它变更遵循基础安全操作的 FileTerm 主窗口审批。
2. 常驻 `fileterm cli --jsonl`：面向 AI，使用与 MCP 相同的全局策略、审批和等待语义。

这样既不增加新的顶层命令，也不会把“AI 调 CLI”继续当成一个没有来源信息的特殊旁路。外部 Agent 不得按每个动作启动一次性 CLI；它必须使用 MCP 或常驻 `fileterm cli --jsonl`。一次性 CLI 仍保留给用户显式调用和 shell 脚本，并继续使用同一套权限评估。

## 9. 凭据安全策略

### 9.1 SSH 登录密码

- 不新增 MCP ssh_password 字段。
- 不新增 CLI --ssh-password 参数。
- 缺失时只由 FileTerm 主窗口 ssh:interaction prompt 接收。
- 输入默认只用于当前连接 operation；是否保存由后续明确的连接编辑操作决定。
- 连接失败、取消和超时不把密码回显到错误文本。
- useEmptyPassword 明确启用时按空密码逻辑处理，不应被误判为缺失凭据。

### 9.2 私钥 passphrase 和 MFA

- 延续现有 key-passphrase 和 keyboard-interactive 事件。
- 外部等待者只收到“等待前台输入”的 progress，不收到 prompt 中可能包含的敏感文本。
- 存储的 profile password 只能自动回答明确识别为 password-like 的 keyboard-interactive prompt，不能自动填充 OTP/MFA。
- 私钥口令是否保存继续遵循现有用户明确选择规则。

### 9.3 sudo/su

延续现有三层来源：

```text
显式一次性 sudo_password / su_password
        ↓
加密 profile sudoPassword / suPassword
        ↓
FileTerm 主窗口安全 prompt
```

约束不变：

- SSH 登录密码不作为 sudo/su 密码 fallback。
- sudo 通过独立 exec channel 的 stdin 使用 sudo -S。
- su 使用独立 exec channel 的受控 PTY。
- 密码不进入命令文本、可见 terminal、日志或 tool result。
- save_* 只有在显式输入一次性密码时才生效。
- 错误密码返回 SUDO_AUTH_FAILURE / SU_AUTH_FAILURE，不自动无限重试。

### 9.4 CLI 明文参数风险

当前 CLI 支持 --sudo-password / --su-password。这不会进入命令文本和最终 JSON，但命令行参数可能被本机进程观察或写入 shell history。

已增加 stdin 输入方式：

```bash
printf '%s\n' "$SUDO_PASSWORD" | fileterm exec --tab-id TAB_ID --command 'sudo systemctl restart app' --sudo-password-stdin
printf '%s\n' "$SU_PASSWORD" | fileterm exec --tab-id TAB_ID --command 'su - deploy -c "./deploy.sh"' --su-password-stdin
```

`--sudo-password-stdin` 和 `--su-password-stdin` 都只读取 stdin 的第一行，保留密码中的空格，不把密码放进 argv、stdout、stderr 或最终 JSON；输入长度限制为 4 KiB。脚本和 Agent 生成命令应优先使用这种方式。两个 stdin 选项不能同时使用，也不能和对应的明文参数混用。

兼容性约束：

- --sudo-password / --su-password 仍保留兼容，但 CLI 会在 stderr 给出风险提示；密码不会被打印。
- 不在 Agent 生成的默认调用中使用明文 argv。

## 10. Agent / CLI 进程模型

### 10.1 当前问题

当前一次 CLI 调用对应一个 FileTerm 可执行进程。即使每个 CLI 最终都只是通过 loopback 请求同一个桌面 runtime，也会在操作系统层面出现多个进程；macOS 可能因此显示多个 FileTerm 图标。

SSH 密码闭环修复本身不能消除这个问题。它只能让每个 CLI 调用正确等待和返回。

### 10.2 推荐目标

在同一个 CLI 入口增加 JSONL 常驻模式：

```text
fileterm cli --jsonl
        ↓
一个常驻 headless 进程
        ↓
复用本地 desktop bridge 连接
        ↓
按 request_id 多路复用 MCP/CLI JSONL 请求
```

目标能力：

- 一个 CLI JSONL 进程处理多个请求。
- 请求之间通过 request ID 区分 progress 和最终结果。
- CLI JSONL 可发送 `cancel_request` 请求取消仍在等待中的 request ID；取消返回后，原请求以 `FILETERM_CLI_JSONL_REQUEST_CANCELLED` 结束。
- MCP stdio 可发送 `notifications/cancelled`；MCP/CLI 客户端断开或请求超时也会触发同一条请求取消边界。
- 仍然由 FileTerm GUI 主进程持有 profile secret、SSH session 和 approval queue。
- CLI JSONL 进程不得初始化 GUI 窗口或创建额外桌面 runtime。
- 普通 SSH exec 会在原 channel 上 best-effort 发送 INT → TERM → KILL 并关闭 channel；这不宣称远端进程已经退出，也不会在新 channel 上重跑已接受的命令。已进入桌面端的其它变更操作不因客户端取消而回滚。
- 如果 macOS 应用包仍把 headless 子进程展示为 GUI 图标，则再验证 application type / bundle 行为；当前不引入 daemon 或 sidecar。

### 10.3 一次性 CLI 的定位

一次性 CLI 继续保留：

- shell 脚本。
- 用户手动调试。
- 不支持常驻进程的外部调用方。

它不会被强行宣称为“零进程”接口；外部 Agent 的调用通道是 `fileterm cli --jsonl` 或 MCP。Agent 不得为每个动作重新 spawn 一次性 CLI。

### 10.4 进程模型与连接去重的关系

即使外部错误地启动多个一次性 CLI，桌面 runtime 仍应通过 profile-scoped connection flight 做连接去重：

```text
四个外部 CLI 进程
        ↓
四个 bridge request
        ↓
同一个 profile 的一个 connection operation
        ↓
一个 tab、一个 worker、一个凭据 prompt
```

这能降低重复建连和重复弹框，但不能完全避免错误启动四个一次性 CLI 进程；正确的做法是让 Agent 复用一个 `fileterm cli --jsonl` 进程。

## 11. Bridge 与数据契约

### 11.1 内部 BridgeRequest

建议补充但不公开 secret 的字段：

```rust
struct BridgeRequest {
    action: String,
    params: Value,
    source: BridgeSource,
    requires_approval: bool,
    progress_token: Option<Value>,
    request_id: Option<String>,
}

enum BridgeSource {
    Mcp,
    Cli,
}
```

source 用于审计、等待和审批语义，不用于绕过连接范围策略。

### 11.2 统一脱敏结果

连接相关外部结果使用统一状态字段：

```json
{
  "operationId": "connection-op-1",
  "profileId": "profile-1",
  "tabId": "tab-1",
  "status": "connected",
  "connected": true,
  "errorCode": null
}
```

失败结果：

```json
{
  "operationId": "connection-op-1",
  "profileId": "profile-1",
  "tabId": "tab-1",
  "status": "failed",
  "connected": false,
  "errorCode": "SSH_AUTH_FAILURE"
}
```

禁止出现：

- password。
- passphrase。
- sudoPassword / suPassword。
- 私钥内容。
- 完整 terminal transcript。
- 用于重放认证的隐藏 token。

### 11.3 CLI 输出约定

- stdout：成功的最终 JSON 结果。
- stderr：progress、等待前台输入、诊断信息和错误信息。
- 退出码 0：调用成功。
- 非零退出码：调用失败；错误文本包含稳定错误码，但不包含 secret。

CLI JSONL 的输出沿用同一边界：progress 和最终结果都带原 request ID；取消请求本身返回 `cancelled: true/false`，被取消的请求最终返回 `FILETERM_CLI_JSONL_REQUEST_CANCELLED`。

### 11.4 MCP 输出约定

- notifications/progress / notifications/message：等待和阶段状态。
- tools/call 最终返回脱敏结构化内容。
- 失败使用 isError: true，并包含稳定 error.code 和 retryable。
- SUDO_PASSWORD_NEEDED / SU_PASSWORD_NEEDED 仍属于可由用户提供一次性字段后重试的路径。
- SSH 登录凭据缺失优先等待主窗口，不通过 MCP 字段重试传密码。

## 12. Renderer 设计

### 12.1 MCP / CLI 设置

在现有 MCP / CLI 设置区域增加：

- “MCP / CLI 共用执行权限”策略卡：只读 / 基础安全操作 / 完全访问，以及查询、变更、传输、隧道和审批跳过能力对照。
- “允许 MCP / CLI 访问的连接”策略卡：所有连接 / 指定连接。
- 指定连接模式下的已保存连接搜索和多选列表。
- 每个连接展示名称、协议、host 脱敏摘要和凭据存在状态。
- “未选择连接”时的明确提示。
- 已删除 profile 的授权项自动清理或显示待清理状态。
- 连接自身协议能力和 FileTerm 安全校验的说明，不在 MCP 页面复制一套 per-connection MCP 权限。

不得展示：

- 明文密码。
- 私钥内容。
- sudo/su 密码。
- 外部 Agent 的完整命令历史。

### 12.2 连接凭据 prompt

继续复用主窗口交互体系：

- SSH 登录凭据使用现有 SSH credentials modal。
- 私钥口令使用现有 key-passphrase modal。
- keyboard-interactive 使用现有交互 prompt modal。
- sudo/su 使用 SudoPasswordPromptModal。

新建的连接等待状态只需要让 prompt 关联 operation_id / tab_id，不把密码传回 Renderer 以外的外部客户端。

## 13. 实施阶段

### P0：契约、策略和状态机设计

- [x] 确认 selected-connections、allowedProfileIds 和三档操作等级的数据模型。
- [x] 确认 ConnectionOperation、等待者、deadline、取消和去重状态机。
- [x] 确认 MCP、一次性 CLI、CLI JSONL 三种 source 的审批语义。
- [x] 定义稳定错误码、progress 消息和统一脱敏结果。
- [x] 明确 CLI open 默认等待还是显式 --wait 的兼容策略。
- [x] 更新 packages/core 类型草案和架构决策记录（ADR-0008）。

### P1：SSH 登录凭据等待闭环

- [x] 在 Rust workspace state 增加 connection operation registry。
- [x] 在 SSH worker 的 Connected / Failed / Cancelled 路径发送 operation 完成通知。
- [x] 将 SSH credentials、key passphrase、keyboard-interactive 的等待状态映射为外部 progress。
- [x] 主窗口可用时，保持原 CLI/MCP operation 等待到最终状态。
- [x] 主窗口不可用、用户取消、认证失败和超时返回稳定错误码。
- [x] 保持密码只在 Rust/main window 内流转，不新增 SSH 密码外部字段。
- [x] 增加 wait_for_connection MCP/CLI 恢复路径。

### P1：按连接白名单和全局操作等级

- [x] 扩展 McpAgentPreferences。
- [x] 对新旧配置做显式迁移，不静默扩大访问范围。
- [x] 在 Rust bridge route 前执行 selected profile 校验。
- [x] 对 connections、sessions、transfers、tunnels 和 wait 操作统一过滤。
- [x] 增加三档操作等级及对应的 action classification。
- [x] 让 MCP、CLI 和 CLI JSONL bridge 使用同一份 policy evaluator。
- [x] 增加 unselected profile、删除 profile、重命名 profile 和默认 profile 变化测试。

### P1：Renderer 设置和凭据交互状态

- [x] 增加已保存连接多选列表。
- [x] 展示非敏感凭据存在状态。
- [x] 增加只读 / 基础安全操作 / 完全访问说明。
- [x] 增加连接 operation 等待中的 UI 状态和错误提示。
- [x] 确保等待状态不泄漏 prompt 内容和密码。

### P2：CLI JSONL 常驻进程与连接去重

- [x] 设计 `fileterm cli --jsonl` 的 stdio multiplexing 契约。
- [x] 复用 desktop bridge 的认证和 policy evaluator。
- [x] 支持多个 request ID、独立 progress、取消和最终结果。
- [x] 完成 profile-scoped connection flight 去重。
- [x] 验证同一 profile 四个并发请求只创建一个 tab / worker / credential prompt。
- [ ] macOS 验证 headless agent 不产生额外 GUI 图标；必要时构建独立 sidecar。
- [x] 保留一次性 CLI，并在设置页和文档中推荐 MCP/CLI JSONL 常驻模式。

### P3：CLI 凭据输入硬化

- [x] 增加 sudo/su 密码 stdin 输入方式，限制为单行且有 4 KiB 上限。
- [x] 对 --sudo-password / --su-password argv 方式增加 stderr 安全提示和文档警告。
- [x] 不在 Agent 默认生成的命令中使用明文 argv。
- [x] 现有一次性字段和加密 profile 行为保持兼容。

### P4：回归、打包和文档

- [x] 完成 Rust unit/contract 测试。
- [x] 完成 CLI 参数、凭据边界和 bridge 等待单测，以及 CLI 子进程 stdout/stderr/exit code 回归。
- [x] 完成 MCP progress/tool result/error code 单测。
- [ ] 完成 macOS、Windows、Linux 打包应用的交互验证。
- [x] 更新 docs/architecture.md、本计划和 Issue #224 关联说明；不自动改写远端 Issue。
- [x] 通过项目现有 typecheck、lint、format、Tauri tests 和 clippy 门禁。

## 14. 验收标准

说明：本次执行已完成代码、单元/契约和本地子进程回归；涉及真实 SSH/FTP/网络设备、打包产物和 macOS Dock 的条目保留未勾选，按“除了实机测试”约定跳过。

### 14.1 SSH 登录凭据

- [ ] 已保存 SSH 密码时，CLI/MCP open 不弹密码框并返回 Connected。
- [ ] 缺少 SSH 密码时，CLI 原调用保持等待；用户在 FileTerm 主窗口输入后，CLI 返回 Connected JSON。
- [ ] 缺少 SSH 密码时，MCP tool call 发送 progress；用户输入后返回 Connected structuredContent。
- [ ] 用户取消或等待超时后，CLI/MCP 收到稳定错误码，不能得到假成功。
- [ ] 主窗口不可用时，不无限等待，不要求 AI 传递 SSH 密码字段。
- [ ] 私钥 passphrase 和 keyboard-interactive 的等待行为与连接 operation 一致。
- [ ] useEmptyPassword 明确启用时按空密码逻辑处理，不错误弹框。

### 14.2 sudo/su

- [ ] 已保存 sudo/su 密码时，CLI/MCP 不弹框并返回命令结果。
- [ ] 主窗口可用且缺少 sudo/su 密码时，CLI/MCP 原调用等待并在用户输入后返回结果。
- [ ] MCP 在 sudo/su exec 前先经过操作审批。
- [ ] 主窗口不可用时返回 SUDO_PASSWORD_NEEDED / SU_PASSWORD_NEEDED。
- [ ] 取消、超时和错误密码分别返回对应稳定错误码。
- [ ] SSH 登录密码不会被当作 sudo/su 密码 fallback。
- [ ] 密码不会进入命令字符串、terminal transcript、日志或最终结果。

### 14.3 权限策略

- [x] 未保存 profile 无法被 open_connection 或 CLI open 访问。
- [x] 未选中的已保存 profile 无法被 MCP/CLI 访问。
- [x] selected profile 的连接列表、会话、传输和隧道结果均按同一白名单过滤。
- [x] 只读策略拒绝写入、删除、远程命令和危险传输操作。
- [x] 基础安全操作策略在 MCP/CLI/CLI JSONL bridge 对 Copilot 判定的普通安全命令自动执行；对变更、破坏性、提权或未知命令，以及会话/文件/传输变更、隧道、sudo/su 和未知操作触发 FileTerm 主窗口审批。
- [x] 完全访问只跳过包括 sudo/su 操作确认在内的逐次审批，不绕过 sudo/su 密码、连接范围、session revision、路径和凭据安全边界。
- [x] 两个不同 MCP client 看到并使用同一份全局策略。

### 14.4 并发与进程

- [x] 同一 profile 并发 open 只创建一个连接 operation、一个 tab 和一个密码 prompt。
- [x] 多个等待者能收到同一最终状态，但不会共享密码文本。
- [x] `fileterm cli --jsonl` 能在一个常驻进程中处理多个 request ID。
- [x] CLI JSONL 常驻模式不会为每个请求创建新的 GUI runtime。
- [x] 一次性 CLI 仍能独立运行并输出标准 JSON。
- [ ] macOS 不再因推荐的 CLI JSONL 常驻模式显示多个 FileTerm GUI 图标。

### 14.5 脱敏与质量门禁

- [x] MCP tool result、CLI stdout/stderr、workspace snapshot 和日志均无凭据明文。
- [x] 错误消息不包含密码、私钥内容、完整 terminal transcript 或敏感 prompt。
- [x] 并发、取消、窗口关闭、session revision 变化和 profile 删除均有回归覆盖。
- [x] 通过 npm run typecheck -w @fileterm/tauri。
- [x] 通过 npm run lint。
- [x] 通过 npx prettier --check apps/tauri packages/core packages/shared packages/storage。
- [x] 通过 npm run test:tauri。
- [x] 通过 cargo clippy --locked --all-targets --all-features -- -D warnings。

## 15. 风险与应对

| 风险                                        | 应对                                                                                            |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| SSH prompt 等待导致 CLI/MCP 长时间挂起      | 统一 deadline、progress、取消和 wait_for_connection 恢复路径                                    |
| 用户输入后 session 已重连或 revision 已变化 | prompt 绑定 tab、profile 和 session revision，失效时拒绝提交                                    |
| 并发 open 重复创建 tab                      | profile-scoped connection flight 和幂等结果                                                     |
| AI 绕过 MCP 改用一次性 CLI                  | Agent 接入契约禁止按请求调用一次性 CLI；CLI 仅保留给用户脚本，MCP/CLI JSONL bridge 复用常驻进程 |
| 选择列表和实际 route 不一致                 | Rust route、列表过滤和 UI 使用同一个 policy evaluator                                           |
| CLI 密码参数出现在 argv                     | 增加 stdin 输入方式，CLI JSONL 默认不使用明文 argv                                              |
| headless 子进程仍显示 FileTerm 图标         | 使用独立 sidecar 并对 macOS bundle/application type 做手工验收                                  |
| 连接等待改变现有 GUI open 语义              | 只在外部 bridge 增加 wait path，GUI 保留立即返回；提供 --no-wait                                |
| 密码缺失和普通交互输入混淆                  | SSH 建连凭据走 connection operation；普通 exec 继续返回 REMOTE_INTERACTIVE_INPUT_REQUIRED       |

## 16. 推荐 PR 拆分

### PR 1：SSH 连接等待闭环

建议分支：feat/mcp-cli-connection-wait

只处理 connection operation、SSH 凭据等待、CLI/MCP 最终结果和稳定错误码，不改变 selected profile UI。

### PR 2：MCP 全局连接白名单与操作等级

建议分支：feat/mcp-selected-connections-policy

处理 allowedProfileIds、三档操作等级、Renderer 设置、Rust route 统一策略和迁移测试。

### PR 3：CLI JSONL 常驻模式与并发去重

建议分支：feat/fileterm-cli-jsonl-runtime

处理 fileterm cli --jsonl、request multiplexing、connection flight 去重和 macOS headless 进程验证。

### PR 4：CLI 凭据输入硬化与发行验收

建议分支：chore/mcp-cli-credential-hardening

处理 stdin 密码输入、argv 警告、跨平台回归、文档同步和质量门禁。

每个 PR 不自动关闭 Issue #224；Issue 只有在代码合入、发布并由用户实际验证后，才由用户决定是否关闭。

## 17. 最终预期行为

完成后，用户看到的行为应是：

```text
AI 请求访问未配置密码但已获授权的服务器
        ↓
FileTerm 判断 profile 已保存且在允许列表内
        ↓
FileTerm 主窗口弹出 SSH 密码输入
        ↓
CLI/MCP 显示“等待前台输入”，原调用不丢失
        ↓
用户输入密码
        ↓
原 CLI/MCP 调用返回 Connected 结果
```

而当 AI 并行发出多个操作时：

```text
多个 MCP/CLI JSONL 请求
        ↓
一个 FileTerm desktop runtime
        ↓
一个常驻 CLI JSONL bridge
        ↓
按请求 ID 多路复用
        ↓
同一服务器复用连接任务，不重复弹密码框、不重复建连
```

这条路径同时解决 Issue #224 的权限控制问题、SSH 登录凭据断链问题，以及 AI 并行调用 CLI 时的进程膨胀问题。

## 18. 实施进度

### 阶段 1：SSH 连接等待闭环

- [x] 增加 desktop runtime 内的短期 `ConnectionOperation` 注册表，只保存 operation ID、profile ID、tab ID 和非敏感状态。
- [x] 外部 `open_connection` 默认等待连接就绪；可用 `wait_for_ready=false` 立即返回 operation ID。
- [x] 增加 `wait_for_connection` MCP tool 与 `fileterm wait-connection` CLI 命令，等待超时后可继续等待。
- [x] SSH 登录凭据取消、超时、认证失败和一般连接失败映射为稳定错误码。
- [x] CLI 进度写入 stderr，最终 JSON 保持在 stdout；MCP 通过 progress/message 通知等待状态。
- [x] SSH、FTP、Telnet、Serial 和本地终端的就绪/失败状态接入连接操作通知。
- [x] 自动化质量门禁已通过；真实 SSH/FTP/设备连接测试按约定跳过，待实际环境验收。

### 阶段 2：全局连接白名单与操作等级

- [x] `packages/core` 与 Rust 偏好模型增加 `selected-connections`、`allowedProfileIds` 和 `full-access`。
- [x] Rust bridge route 在进入 action handler 前校验选中 profile；列表、会话、传输和等待结果使用同一范围过滤。
- [x] 删除 profile 时自动从 Agent 允许列表清理，旧配置不会因为缺失字段而扩大到全部连接。
- [x] 设置页增加搜索、多选、非敏感凭据存在状态和空选择提示。
- [x] 完全访问只跳过包括 sudo/su 操作确认在内的逐次审批，仍保留 sudo/su 密码、连接范围和其它安全边界。
- [x] 增加 selected visibility 与操作等级策略单元测试。
- [x] 自动化质量门禁已通过；真实 SSH/FTP/设备连接测试按约定跳过，待实际环境验收。

### 阶段 3：CLI JSONL 常驻与连接去重

- [x] `fileterm cli --jsonl` 使用有界 worker pool 常驻读取 JSONL，并复用同一个 desktop bridge。
- [x] request ID、独立 progress 和最终结果支持并行调用，输出按行原子化写出。
- [x] 同一 profile 的并发 `open_connection` 复用一个 connection operation、tab、worker 和凭据 prompt。
- [x] 连接失败、断开或关闭 tab 时清理 profile flight，并唤醒等待者。
- [x] Settings → CLI 提供 `fileterm cli --jsonl`，同时保留一次性 CLI。
- [ ] macOS 打包后的 headless/application type 与 Dock 图标行为待人工验证。
- [x] 自动化质量门禁已通过；真实 SSH/FTP/设备连接测试按约定跳过，待实际环境验收。

### 阶段 3 补充：CLI JSONL 请求生命周期

- [x] CLI JSONL 请求无论传入 `requiresApproval=false` 还是省略该字段，都强制进入桌面端审批策略。
- [x] 通过 `cancel_request` 按 request ID 设置取消标记；等待 desktop bridge 响应时以短轮询及时结束 CLI JSONL 等待。
- [x] 取消停止 CLI JSONL 等待和后续输出；对于普通 SSH exec 同步尝试终止原 channel 上的命令，不回滚桌面端已接受的其它操作。
- [x] 远程 exec 取消/超时在原 SSH exec channel 上执行 best-effort INT → TERM → KILL + close；MCP stdio 识别 `notifications/cancelled`，loopback 客户端 EOF/timeout 同样传播取消。
- [x] 拒绝重复 request ID，进度事件和最终结果均绑定原 request ID。
- [x] 增加 CLI JSONL 审批/取消/ID 校验单测，并通过 CLI 子进程回归。

### 阶段 4：CLI 凭据输入硬化

- [x] `--sudo-password-stdin` / `--su-password-stdin` 作为无值参数读取一行 stdin，并限制长度、编码和控制字符。
- [x] 拒绝明文 argv 与 stdin 两种来源混用；兼容旧参数并输出不含密码的安全提示。
- [x] 补充 stdin 解析、长度边界和来源冲突测试。
- [x] 自动化质量门禁已通过；真实 SSH/FTP/设备连接测试按约定跳过，待实际环境验收。

### 阶段 5：CLI 子进程回归与文档收口

- [x] 增加 `tests/cli.rs`，验证 `fileterm cli --help`、`fileterm cli --jsonl --help` 的 headless 分发和 stdout/stderr 边界。
- [x] 验证 CLI JSONL 无桌面 runtime 时仍输出单条最终 JSONL 错误，不启动 GUI。
- [x] 验证 CLI 明文密码参数冲突和 stdin 密码读取的非零退出、错误输出和密码脱敏。
- [x] 同步架构地图、ADR 和本计划中的 CLI JSONL 审批、取消、进程模型与验证状态。
- [x] 自动化质量门禁已通过；macOS/Windows/Linux 打包交互和真实 SSH/FTP/网络设备测试按约定跳过。

### 阶段 6：全局策略展示与模型收敛

- [x] 将设置页拆成“MCP / CLI 共用执行权限”和“允许 MCP / CLI 访问的连接”两张全局策略卡片，并将两张卡片置于 MCP / CLI 子标签页之上。
- [x] 执行权限提供只读、基础安全操作、完全访问三档，并展示能力对照与硬边界提示。
- [x] 连接访问提供所有连接 / 指定连接两档；指定模式保留搜索、多选、选中计数和空白拒绝提示。
- [x] 移除 active-session / default-connection 作为新的 MCP 权限选项，不增加 per-connection MCP 专属权限。
- [x] 旧配置安全迁移：default-connection 转为单 profile 白名单，active-session 转为空白白名单；迁移后不再序列化旧字段。
- [x] Rust route、列表可见性和 transfer 过滤统一使用两档连接策略。
- [ ] macOS/Windows/Linux 打包后的设置页视觉、键盘操作和真实连接策略仍待人工验收。

阶段 1-6 的实现均不把 SSH 登录密码新增到 CLI 参数、MCP 参数或结果中。sudo/su 仍支持明确的一次性凭据，但脚本和 Agent 应使用 stdin 方式；macOS 打包后的 headless/application type 与 Dock 图标行为、真实 SSH/FTP/设备连接仍待目标环境人工验证。
