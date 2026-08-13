# MCP / CLI 安全交互式远程执行计划

状态：进行中（交互式 SSH exec、最小审计与文档边界已落地；待完成真实客户端与三端手工验证）  
关联：[本地终端与 Agent MCP 接入](./local-terminal-mcp.md)、[AI Copilot 功能集成计划](./ai-copilot-integration.md)、[架构地图](../../architecture.md)

## 1. 结论

保留现有的独立 `exec` 作为 MCP / CLI 的默认远程执行方式；新增一个**安全交互式远程执行**模式，处理 `sudo` 密码、MFA、一次性确认回答等需要用户输入、但 Agent 又必须等待结果的场景。

关键不是把 Agent 的命令重新写进用户正在看的终端，而是：

- 当前普通 `exec` 已经复用 SSH worker 持有的 `Handle` 开启独立 exec channel；它不是让 renderer 另建 SSH client，但该 channel 没有受控的交互输入回路。
- 现有底层 helper 虽已支持“请求 PTY 后一次性写入 stdin”，但写完会立刻送 EOF；它适合固定数据，不适合等待 `sudo` 密码提示后再接收用户的下一次回答。
- 新模式继续复用**同一已认证 SSH transport**，但为本次任务开一个短生命周期、带 PTY / stdin 的隔离 session channel；不新建 SSH 登录连接，也绝不抢占或混入可见终端 PTY。
- 当该任务等待输入时，FileTerm 在桌面端弹出安全输入框。用户在这个框里输入密码、OTP 或确认值，renderer 通过 Rust command 直接把值写回**同一个任务 channel**。
- 外部 Agent 只等待任务完成并得到经过脱敏、截断的结果；它永远收不到用户输入的原文，也不能要求用户“去终端里输入密码”。

这解决了当前的断链：用户在可见终端输入密码，无法被后台独立 exec 接收；而把密码交给 Agent / MCP 参数又会暴露给模型、日志或调用记录。

## 2. 双执行模式与使用规则

| 场景                               | 模式                | SSH transport                                         | 用户输入去向                       | Agent 可获得的结果                       |
| ---------------------------------- | ------------------- | ----------------------------------------------------- | ---------------------------------- | ---------------------------------------- |
| 查询、脚本化检查、非交互命令       | 现有 `exec`（默认） | 当前 SSH worker 的独立 exec channel                   | 无                                 | 受限输出、退出码、超时、截断状态         |
| 明确预期 `sudo` / MFA / Y-N 等输入 | 新增安全交互式 exec | 同一 worker / 同一已认证 transport 的临时 PTY channel | FileTerm 安全输入框 → 此任务 stdin | 完成后的脱敏输出、退出码、超时、取消状态 |
| 全屏 TUI、pager、连续键盘控制      | 不纳入首版          | —                                                     | —                                  | 明确报不支持，不静默改写可见终端 PTY     |

外部 Agent 必须遵守以下规则：

1. 默认调用非交互 `exec`；不要因为命令“可能”需要权限就无条件进入交互模式。
2. 已知需要人工输入时，先告知用户 FileTerm 会安全地请求输入，再调用交互式工具。若普通 `exec` 意外发现需要输入，Agent 可在原操作已经获授权且不会造成未知重复副作用时，立即新建一次安全交互式任务；否则先解释重试风险并取得确认。
3. **禁止**让用户在 Agent 聊天框中粘贴密码、OTP、私钥口令或其他 secret；也禁止说“请到 FileTerm 终端输入密码后告诉我结果”。可见终端是不同 channel，不能回答后台任务。
4. 密码、MFA、Y/N 和其他 stdin 必须由 FileTerm 的任务专属安全弹窗采集并直接写回该任务。Agent、MCP 参数、CLI stdin 和可见终端均不得作为输入通道，Agent 只等待最终脱敏结果。
5. Agent 不可发送 stdin 内容，也不可控制后续按键、Ctrl+C、方向键或终端 resize。它只可以发起初始命令，随后等待 FileTerm 的用户交互和最终结果。

## 3. 用户流程

```text
Claude Code / Codex CLI / FileTerm CLI
          │
          │  交互式远程执行请求（无 secret 参数）
          ▼
FileTerm MCP runtime
          │
          ├─ 校验 tab / SSH 状态 / target binding / 命令限制
          ├─ MCP：显示一次性执行审批；CLI：保留显式调用语义
          └─ SSH worker：在既有 Handle 上创建任务专属 PTY channel
                                      │
                                      │  远端任务等待输入
                                      ▼
                          `remote-exec:interaction-request`
                                      │
                                      ▼
                    FileTerm renderer 的安全输入弹窗
                    显示主机、用户、目录、完整命令与远端提示
                                      │
                   用户输入 / 取消（值不回传给 Agent）
                                      │
                                      ▼
              `app_resolve_remote_exec_interaction` → Rust task stdin
                                      │
                                      ▼
                     任务完成 / 取消 / 超时 / 断线
                                      │
                                      ▼
                   脱敏后的结果 → 原 MCP / CLI 调用者
```

FileTerm 的 renderer 只承担输入体验，**不持有 SSH client，也不直接访问协议连接**。SSH transport、channel、stdin、超时、取消和输出清洗都仍在 Rust workspace runtime 内完成。

## 4. 目标绑定与连接复用

每次交互式执行都必须绑定到一个已打开、已连接的 SSH tab，而不是根据“当前终端”猜测目标：

```ts
interface InteractiveRemoteExecTarget {
  tabId: string
  rootTabId: string
  sessionType: 'ssh'
  sessionRevision: string
  displayHost: string
  user?: string
  cwd?: string
}

interface InteractiveRemoteExecRequest {
  target: InteractiveRemoteExecTarget
  command: string
  cwd?: string
  timeoutMs?: number
}
```

运行前的 runtime 校验顺序：

1. `tabId` 存在、属于 SSH、已连接，且 SSH worker 仍保有可打开 channel 的 authenticated `Handle`。
2. `rootTabId`、分屏 leaf、公开主机、shell 用户、工作目录与 `sessionRevision` 与请求一致；目标切换、重连、用户切换或 CWD 变化后必须重新读取上下文再请求。
3. 命令、CWD、tab ID 都采用现有长度和控制字符限制；交互式模式不因此放宽输入上限。
4. 创建任务时记录 runtime transport / session identity。该 identity 不匹配、tab 关闭、断线或 worker 重启时，任务立即取消并清理等待输入。

实现前必须做一个 `russh` 能力验证：在现有 SSH worker 的同一 `Handle` 上，主交互 shell 存在时仍能安全创建一个独立的 PTY session channel，并可独立读取 / 写入 / 关闭。

- 验证成功：复用该 transport，不调用新的 `Client::connect`，不申请新的 profile credential。
- 验证失败或服务器拒绝附加 channel：返回 `INTERACTIVE_REMOTE_EXEC_UNAVAILABLE`；**禁止**悄悄新建后台 SSH 连接，也禁止回退写入可见终端 PTY。
- 首版每个 SSH tab 最多允许一个活动交互式任务，避免同一用户难以分辨“密码正在给哪个命令”。可见终端仍可正常使用。

任务启动后，可见终端产生的普通输出或用户 `cd` 不会把已启动任务重定向到别处；任务使用启动时已核对的连接身份和 CWD。普通输出不应导致任务失效。

## 5. 安全输入契约

### 用户看到什么

交互请求必须在 FileTerm 主窗口中以专用弹窗展示，不使用浏览器 `prompt()`、原生 `window.confirm()` 或外部 Agent 的聊天输入：

- 来源：`MCP（Claude Code / Codex 等）` 或 `FileTerm CLI`。
- 目标：SSH tab 名称、公开主机、shell 用户、启动工作目录。
- 完整初始命令，以及该任务正在等待的远端提示（转义、长度受限，仅本地展示）。
- 输入类型：密码 / MFA 使用 masked 的“安全输入”；确认回答使用普通文本输入，例如 `yes`。
- 操作：`发送到此远端任务`、`取消任务`。每一次输入前都让用户确认目标和命令，不能静默自动填入已保存的密码。

远端输出的 `Password:` 等文本只能作为提示检测的 UX 信号，不能当作可信身份声明。FileTerm 必须让用户看到命令和目标后自行决定是否发送 secret；不能因为远端输出看起来像密码提示就自动填充 / 发送任意凭据。

### secret 的数据路径

```text
renderer 受控输入 state
  → Tauri invoke `app_resolve_remote_exec_interaction`
  → WorkspaceState 的 task 专属 one-shot / stdin sender
  → 同一 interactive exec channel
```

安全约束：

- 不将输入值放入 MCP tool result、CLI stdout/stderr、Tauri event payload、action approval 内容、审计记录、终端 transcript、日志、错误文本或持久化状态。
- renderer 提交成功、取消、超时、组件卸载后立即清空输入 state；禁止 `console.log`、自动填充缓存或历史记录。
- Rust 只在内存中持有输入直到写入 task stdin；任务结束后删除 task state 和所有 pending sender。
- 所有提交过的值都登记进该任务的内存 redaction set。若远端错误地回显了用户输入，结果在返回给 MCP/CLI、写审计或发 renderer event 前必须精确替换为 `[REDACTED]`。
- 不复用 profile 中可能保存的密码、root 访问密码或先前 MFA 值；每个交互回答都由当前用户主动输入。
- 连接认证阶段既有的 `ssh:interaction`（host key、登录凭据、keyboard-interactive）保持独立。命令运行时的输入走新的 `remote-exec:interaction-*` 事件，不能把两类 prompt 混为同一请求。

首版受控支持的是密码、OTP 和短文本确认。全屏安装器、菜单选择、pager、无限对话、文件上传等连续终端协议不在范围内；达到交互次数上限或无法可靠判断等待输入时，任务取消并返回明确错误，不能改为控制可见终端。

## 6. MCP 与 CLI 契约

### MCP

新增单独的 mutation tool：

```text
fileterm_execute_interactive_remote_command
```

请求只包含 target、command、cwd、timeout 等非敏感参数；**没有** `stdin`、`password`、`otp`、`answers` 或“自动从 profile 读取凭据”的参数。

- 标注 `readOnlyHint=false`、`idempotentHint=false`。工具描述明确：会执行命令、可请求 FileTerm 用户输入、用户输入不会暴露给 Agent。
- MCP 调用仍使用现有 `<ConfirmActionDialog>` 一次性审批队列。审批框显示完整命令和 target，明确说明“若远端请求输入，FileTerm 会另行询问你；不会把值交给 Agent”。
- 调用保持 pending，直到命令完成、取消、超时、断线或错误；完成时返回与普通 exec 一致的、已脱敏结构化结果。
- 普通 `fileterm_execute_remote_command` 保持非交互独立 exec；Agent 预判 `sudo`、`su`、`passwd`、安装器、MFA 或确认输入时，应直接选择交互工具，而不是先让用户在可见终端输入。可见终端与 MCP 的独立 exec 不是同一输入通道，输入不会转发给后台任务。
- 普通 exec 的结果额外提供脱敏路由提示：`inputRequired=true` 且 `inputKind` 为 `secret` 或 `text` 时，表示有限提示检测发现命令可能正在等待人工输入；它不包含用户输入，也不代表命令已成功。Agent 应依据该提示改用交互工具，不应让用户把回答写入可见终端或聊天。
- 普通 exec 意外报告需要输入时，任务视为未完成；若用户已经授权该操作，Agent 应立即以交互工具新建任务，让 FileTerm 安全弹窗索取所需输入。只有第一次命令可能已产生未知副作用、重试会扩大风险时，才先解释并请求重试确认。不得要求用户把密码、验证码或确认值输到终端或聊天。

### CLI

保留 `fileterm exec` 的默认非交互语义，新增单独命令：

```sh
fileterm interactive-exec \
  --tab-id TAB_ID \
  --expected-session-revision REVISION \
  --command 'sudo apt update'
```

- `interactive-exec` 必须显式调用；不可由 CLI 在普通 exec 失败后自动切换。
- CLI 进程在等待 FileTerm 的输入弹窗和命令结果期间保持连接；用户不会在 shell 的 Agent 对话或 FileTerm 终端中输入密码。
- 维持当前“用户显式发起 CLI 调用不重复弹应用内执行审批”的规则，但每次 secret / 文本回应都必须经过 FileTerm 的可见交互弹窗，不能由 CLI 管道或 stdin 注入。
- 当主窗口 / renderer 不可用或尚未订阅安全输入事件时，CLI 返回 `INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE`，而不是回退为后台连接、TTY 劫持或把 secret 读自标准输入；任务不会启动隔离 PTY。主窗口仅被隐藏到托盘时会先恢复显示本地提示。

## 7. Agent 行为指令

MCP tool description 和 FileTerm 给外部 Agent 的接入说明中都需写入以下规则：

> 如果远端命令需要密码、MFA、确认回答或其他人工输入，不要要求用户把值输入到 FileTerm 可见终端，也不要在聊天中索取 secret。可见终端与独立 exec 是不同通道，终端输入不会到达后台任务。对预期交互命令直接改用 `fileterm_execute_interactive_remote_command`；FileTerm 会在本地安全地询问用户并把回答发送到正在等待的同一远端任务；你只能等待最终状态和经过脱敏的结果。

具体行为：

1. Agent 可以根据命令语义预判 `sudo`、`su`、`ssh`、安装器或 MFA 场景；用户已要求执行时，直接通过交互式工具发起任务即可，常规应用内审批仍是执行前确认。不得引导用户改在可见终端输入。
2. 当命令语义或普通 exec 的无交互结果表明需要人工输入时，Agent 应说明该命令尚未成功，不应假设用户在终端已输入什么；用户已经授权该操作时，直接另起一次交互式执行请求。只有重试可能扩大未知副作用时，才再询问用户是否继续。密码、MFA 和确认值只由后续 FileTerm 安全弹窗向用户采集，不进入 Agent 对话。
3. 任务处于 `awaiting-user-input` 时，Agent 等待，不催促用户复制密码、不伪造完成结果，也不把此前输出误说成最终成功。
4. 用户取消或超时后，Agent 仅报告任务未完成，并可给出无 secret 的替代方案。

## 8. Runtime、bridge 与 renderer 设计

### Core 类型

在 `packages/core` 定义中性、非 AI 私有的类型：

```ts
type RemoteExecMode = 'non-interactive' | 'interactive'
type RemoteExecInteractionKind = 'secret' | 'text'

interface RemoteExecInteractionRequest {
  requestId: string
  executionId: string
  target: InteractiveRemoteExecTarget
  commandSummary: string
  prompt: string
  suggestedKind: RemoteExecInteractionKind
  attempt: number
  maxAttempts: number
}

interface RemoteExecInteractionResponse {
  requestId: string
  cancelled: boolean
  value?: string
  kind?: RemoteExecInteractionKind
}
```

`value` 只能作为 renderer → Rust 的瞬态 command 参数使用；它不得进入 snapshot、MCP schema、审计模型或事件回放模型。

### Rust

1. 扩展 `WorkerCmd`，增加 `ExecuteInteractiveRemoteCommand` 和任务专属输入 / 取消控制；保留 `ExecuteRemoteCommand` 的现有语义和实现不变。
2. 在 `services/action_review.rs` 旁建立专用 `interactive_remote_exec` service，负责 target 验证、审批整合、任务 ID、超时、pending input、取消与输出脱敏；MCP JSON-RPC adapter 只做参数转换。
3. 在 `sessions/ssh.rs` 中以同一个 `Arc<Handle<ClientHandler>>` 启动独立、带 PTY 的 task channel。它不得使用 `terminal_inputs`、`send_terminal_input`、`app_write_terminal` 或当前 shell writer。
4. 任务检测到候选输入提示时，创建 request ID，写入 task 专属 pending map，发射 `remote-exec:interaction-request`，并暂停只该任务的 stdin 等待。接受 response 后只把该值加换行写入这一 task channel。
5. 管理命令运行超时、单次用户输入等待超时、最大交互轮次和连接关闭。命令运行预算只计算隔离 channel 等待远端运行的时间；安全输入弹窗等待使用独立的有限 deadline，任务仍受总生命周期上限保护。取消、断线、worker 重启、窗口不可达和 response 过期均关闭 task channel，并清空状态。
6. 返回结果前执行控制字符清理、输出上限和 secret redaction；最终仅返回结构化、无 secret 的 `RemoteExecResult`。

### Bridge 与 renderer

1. `tauri-api.ts` 新增 `resolveRemoteExecInteraction` 与 `onRemoteExecInteraction`；按现有 `ssh:interaction` 的 one-shot 生命周期实现，但保持独立事件命名和类型。
2. 新建 `useRemoteExecInteractions`，队列化同一窗口内的 prompt；不与 SSH 登录认证 hook 共用状态，避免连接认证与命令输入串线。
3. 使用项目受控 dialog 组件实现安全输入，不使用原生 `prompt()`。密码 / MFA 提示使用 `type=password`，短文本确认使用普通文本框。
4. 关闭窗口、切换页面、tab 断线和组件卸载都显式 resolve `cancelled`；不让 Rust 任务无限等待。
5. 对话框只显示本地必要信息，不写入 AI Copilot 对话、命令卡、终端输出或浏览器控制台。

## 9. 生命周期与错误语义

```text
requested
  ├─ MCP: approval-pending → approved
  └─ CLI: explicit-call-validated
          ↓
running-on-shared-ssh-transport
  ├─ awaiting-user-input → running-on-shared-ssh-transport
  ├─ completed
  ├─ user-cancelled
  ├─ interaction-timeout
  ├─ execution-timeout
  ├─ target-disconnected
  └─ unavailable / rejected
```

建议错误码：

| 错误码                                         | 含义                                                             | Agent 处理方式                                 |
| ---------------------------------------------- | ---------------------------------------------------------------- | ---------------------------------------------- |
| `INTERACTIVE_REMOTE_EXEC_UNAVAILABLE`          | 当前 SSH transport / server 不能安全创建附加 interactive channel | 不新建连接、不写入可见终端；请用户选择替代方式 |
| `INTERACTIVE_REMOTE_EXEC_TARGET_CHANGED`       | tab、transport、用户、CWD、revision 或连接状态已变化             | 重新读取会话上下文后再请求                     |
| `INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE` | 无可用 FileTerm 窗口来安全收集输入                               | 不要求用户在 Agent 聊天或终端给 secret         |
| `INTERACTIVE_REMOTE_EXEC_USER_CANCELLED`       | 用户拒绝发送本次输入                                             | 报告命令未完成；不猜测执行结果                 |
| `INTERACTIVE_REMOTE_EXEC_INPUT_TIMEOUT`        | 用户输入等待超时                                                 | 关闭 task，清理 pending state，不重试          |
| `INTERACTIVE_REMOTE_EXEC_TOO_MANY_PROMPTS`     | 超过首版允许的交互轮次                                           | 关闭 task，提示该工作流超出受控范围            |

命令运行超时与用户输入等待超时已经分开：任务运行期间采用请求的命令预算；处于 `awaiting-user-input` 时采用 5 分钟的交互 deadline，并仍保留“命令预算 + 最大交互等待”总生命周期上限。这样用户输入密码时不会被普通 60 秒 exec timeout 错误打断，同时远端命令也不会无限运行。

## 10. 首版范围与非目标

### 纳入

- 已连接 SSH tab 上的单条命令，复用现有 authenticated `Handle`。
- 密码、OTP、短文本确认的受控输入，默认一次任务最多 3 轮。
- 交互式执行的 MCP tool 和 `fileterm interactive-exec`。
- MCP 一次性执行审批、敏感输入弹窗、取消 / 超时 / 断线清理、脱敏结果与最小审计元数据。
- 三端统一 renderer UI：macOS、Windows、Linux 都走同一个 React + Tauri command/event 路径，不依赖原生窗口 prompt 或模拟键盘。

### 不纳入

- 读取、复用、回放或注入可见交互式 PTY 的 transcript / 连续键盘输入。
- 将秘密显示给 Agent、写入 MCP/CLI 参数、从外部 CLI stdin 读取或存入任何 profile / 历史。
- 新建隐藏 SSH 登录连接作为 fallback。
- 全屏 TUI、shell REPL、vim、top、pager、expect 脚本或任意无限交互流程。
- FTP、Telnet、Serial 的伪交互 exec；先保持各协议已有能力边界。
- 改变内置 AI Copilot 的“仅写入终端输入、不自动回车”命令卡规则。此计划只服务外部 MCP / CLI 执行任务。

## 11. 实施阶段

### Phase 0：能力验证与契约

- [x] 复用当前 `russh::Handle` 开立临时 PTY channel；不调用新 SSH 登录连接，也不写入可见 terminal PTY。
- [x] 从一次性 stdin helper 旁拆出可持续写入的 task channel；运行时由用户 prompt 回送短文本，不在首个回答后发送 EOF。
- [x] 在 `packages/core` 定义 interaction request/response，并使用 `sessionRevision` 绑定目标。
- [x] 更新 MCP tool descriptions、CLI help 和 Agent 接入说明，写入“不能向聊天或终端索取 secret”的规则。

### Phase 1：Rust 任务状态机

- [x] interactive exec service 复用 workspace session / `ai_session_revision` 的 target identity 语义。
- [x] 加入 task ID、一个 tab 一个活跃任务、pending input map、worker cancel、连接断开清理、最大轮次、运行 / 输入双 timeout。
- [x] 实现专属 channel 的输出收集与 task-local secret redaction；普通 `ExecuteRemoteCommand` 保持非交互语义。
- [x] 在 MCP / CLI 路由中接入交互模式，不把 secret 放入 action approval request 或 MCP transport。

### Phase 2：安全交互 UI

- [x] 扩展 bridge，并以独立 hook / dialog 订阅 `remote-exec:interaction-request`。
- [x] 对话框显示目标、命令与远端提示；密码 / MFA 使用 masked 输入，确认回答使用文本输入，提交后按 request ID 精确回送。
- [x] 窗口卸载、tab 关闭、连接断开、dialog 取消或 resolver 超时均 fail closed；仅在主 renderer 已确认订阅任务安全输入事件后才启动隔离 PTY，避免后台任务白等不可见 UI。
- [ ] 完成 macOS、Windows、Linux 的焦点、中文输入法、密码掩码、键盘快捷键和高 DPI 手工验证。

### Phase 3：客户端、审计与文档

- [x] 为 MCP 注册 tool、为 CLI 添加 `interactive-exec`；结果沿用普通 exec 的脱敏结构化 schema。
- [x] 审计只记录来源、公开 target、命令摘要 / hash、开始 / 完成 / 取消、交互轮次与结果；不记录 prompt 原文、用户回答或输出中的 secret。审计文件 2 MiB 轮转，Unix 强制 `0600`、Windows 使用 per-user application-data ACL；建立审计失败时任务不启动。
- [x] macOS 正在运行的 FileTerm runtime 已通过真实 stdio `initialize`、`tools/list`、`fileterm_list_connections` 与 `fileterm_get_session_context` 冒烟；初始化指令和 tool descriptions 均明确要求任务专属安全输入，而非 Agent 聊天或可见终端。
- [x] 将 `fileterm interactive-exec` 纳入 macOS、Windows、Linux 的 non-GUI CLI dispatch smoke；`interactive-exec --help` 在不初始化 Tauri 窗口的情况下进入交互式 CLI 路由。
- [ ] 用真实 Claude Code、Codex CLI 与一个已连接 FileTerm runtime 的纯 `fileterm interactive-exec` 验证：Agent 等待 FileTerm prompt，用户输入 password 后 Agent 获得最终脱敏输出。
- [x] 更新 `docs/architecture.md`，只记录已落地的 transport、输入隔离、脱敏与审计边界；不把三端手工验证描述为已完成。

## 12. 测试与验收

### 自动化

- 同一 transport 不变量：交互任务创建期间不调用 `Client::connect`，且与可见 terminal shell 共享同一 authenticated `Handle` / transport identity。
- channel 隔离：交互任务的 stdin 不会到达 `terminal_inputs`，可见终端键盘输入也不会到达任务 stdin；两侧输出互不混入。
- target 校验：tab / host / user / CWD / revision 变化、重连、断线、关闭 pane 或 worker 重启均拒绝或取消正确任务。
- secret 路径：密码、OTP、普通确认都不出现在 MCP result、CLI stdout/stderr、日志、审计、Tauri event、workspace snapshot 或 terminal transcript；模拟远端回显时会被 `[REDACTED]`。
- 生命周期：审批拒绝、用户取消、输入 timeout、执行 timeout、断线、重复 resolve、过期 request、并发第二任务均无残留 sender / task。
- 回归：普通 `fileterm_execute_remote_command` 仍是非交互独立 exec，不会显示输入弹窗，不会因普通 terminal 输出失效。

### 三端手工验收

| 场景                                                                  | macOS | Windows | Linux |
| --------------------------------------------------------------------- | ----- | ------- | ----- |
| MCP 发起交互式 `sudo`，审批框显示正确 host / user / CWD / 命令        | [ ]   | [ ]     | [ ]   |
| 用户在 FileTerm 安全输入框输入密码，Agent 不要求终端或聊天输入        | [ ]   | [ ]     | [ ]   |
| 密码提交到正在等待的同一 task，Agent 最终只看到脱敏输出               | [ ]   | [ ]     | [ ]   |
| 可见终端仍可输入且不会影响 task；task 输出不会混进可见终端            | [ ]   | [ ]     | [ ]   |
| MFA / Y-N / 取消 / 超时 / 断线 / 切换 tab 均安全结束                  | [ ]   | [ ]     | [ ]   |
| server 不支持附加 channel 时明确失败，未新建 SSH 连接也未写入可见终端 | [ ]   | [ ]     | [ ]   |

## 13. 关键决策

- **为什么不让用户在可见终端输入？** 当前后台 exec 与可见 terminal PTY 是不同 channel；用户输入会走错通道，Agent 永远等不到结果。安全输入必须回送到任务自己的 stdin。
- **为什么不把 password 交给 Agent 再由它调用工具？** 这样 secret 会穿过模型上下文、MCP 参数和潜在日志，违背最小暴露原则。Agent 只能知道“正在等待 / 已完成 / 已取消”。
- **为什么不让 renderer 直接管理 SSH？** Renderer 不能绕过 Rust 的 SSH / PTY 所有权。它只是本地安全交互 UI，协议能力仍严格走 Rust command/event → bridge → renderer。
- **为什么保留默认独立 exec？** 大多数命令不应阻塞等待用户，也需要稳定、机器可读的结果。交互式执行是明确选择的例外，而不是普通 exec 的隐式降级。
- **为什么不自动从非交互失败升级？** 自动重跑可能重复修改服务器状态；而是否输入密码、OTP 或确认本身必须由用户主动决定。
- **为什么不用隐藏的新 SSH 连接 fallback？** 它会重新引入“密码到底给哪条连接”的问题，也可能和当前会话用户 / 代理状态不一致。不能复用当前 transport 时应 fail closed。
