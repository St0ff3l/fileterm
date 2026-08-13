# AI Copilot 三模式与上下文级别简化计划

状态：规划确认，待开工
关联：[AI Copilot 功能集成计划](./ai-copilot-integration.md)、[简化远程 exec 与 sudo 凭据自动化](./simplify-exec-sudo-credentials.md)、[MCP / CLI 安全交互式远程执行计划](./mcp-cli-interactive-exec.md)、[架构地图](../../architecture.md)

## 1. 结论

把内置 Copilot 的能力边界从「单层保守助手 + 独立 Review Mode」收敛为**三种用户可选模式**，并把上下文级别从 L0/L1/L2/L3 简化为 **L0/L2 两档**：

- **纯对话模式（Pure Conversation）**：默认 L0，可由用户主动开启 L2；模型只生成可复制 / 可写入输入区的命令卡，不发起任何远端执行。
- **半自动模式（Semi-Automatic）**：强制 L2；模型可发起工具调用，每次执行必须经用户**逐次审批**后才走独立 SSH exec channel。
- **全自动模式（Fully Automatic）**：强制 L2；模型可发起工具调用，**无需逐次审批**直接走独立 SSH exec channel，但受危险命令黑名单、操作阈值、不可逆动作白名单和会话级累计上限约束。

关键决策：

1. **移除 L1（仅元数据）级别**。L1 在实际使用中辨识度低、收益有限：用户要么只想要纯对话（L0），要么希望模型看到完整上下文（L2）。保留 L1 会让模式矩阵和 UI 开关复杂化，且容易被用户误开成"半开不开"状态。
2. **半自动和全自动模式强制 L2**。模型若看不到终端 transcript，就无法在执行链里做正确判断；允许 L0 + 自动执行等于让 Agent 盲操作远端主机，违背 [ai-copilot-integration.md](./ai-copilot-integration.md) 的"保守助手"定位。
3. **纯对话模式 L2 改为可开关**。用户在排障、解释、学习场景下可能希望临时让模型看到终端输出，但不应被强制。开关默认关闭，开启后每条消息自动刷新一次性快照（沿用现有 `app_create_ai_context_preview` 行为）。
4. **L3 Review Mode 不再单列**。它的能力被半自动模式完全吸收：半自动模式的每次工具调用都走 `ActionReviewService` 的一次性审批 + 独立 SSH exec，等价于原 L3 的语义但不再作为一种"上下文级别"。
5. **全自动模式的护栏不是可选的**。模式切换到全自动时，护栏（黑名单、阈值、白名单）默认全部启用；用户不能在 UI 上关闭单条护栏，只能整体降级回半自动。

## 2. 模式 × 上下文矩阵

| 模式   | 默认上下文 | 用户可调     | 工具调用  | 执行边界                                 | 适用场景                       |
| ------ | ---------- | ------------ | --------- | ---------------------------------------- | ------------------------------ |
| 纯对话 | L0         | ✅ 可开关 L2 | ❌ 不允许 | 仅生成命令卡，复制或写入输入区（不回车） | 学习、解释、排障、生成命令     |
| 半自动 | L2         | ❌ 锁定 L2   | ✅ 允许   | 每次执行必须用户审批，独立 SSH exec      | 已知目标、可控范围内的运维任务 |
| 全自动 | L2         | ❌ 锁定 L2   | ✅ 允许   | 无需逐次审批，独立 SSH exec，受护栏约束  | 重复性查询、批处理、链式排障   |

UI 约束：

- 模式选择器位于 Copilot 面板顶部，三选一单选按钮组。
- 上下文开关仅在纯对话模式下可交互；半自动 / 全自动模式下开关被**禁用并置灰**，旁边提示"该模式强制附带完整终端上下文"。
- 全自动模式首次启用时弹 `<ConfirmActionDialog>` 警告："全自动模式允许 AI 不经审批直接在远端主机执行命令。请确认你信任当前 Provider 与目标主机。"

## 3. 上下文级别定义（简化后）

### L0：纯对话

- 仅发送用户输入和必要的会话历史。
- 不附带主机、路径、平台或终端内容。
- Provider 配置完成后默认进入该级别（仅在纯对话模式下生效）。
- 适用于纯对话模式；半自动 / 全自动模式**不允许**该级别。

### L2：完整终端上下文

- 包含 L1 原有的目标元数据（协议、公开主机标签、登录用户、远端平台、shell CWD、连接状态）**和**最近终端 transcript 快照。
- Rust 从当前 tab 的 runtime transcript 生成一次性快照，CRLF 归一化、ANSI 清理、best-effort 遮盖、末尾 120 行 / 16 KiB UTF-8 截断。
- 纯对话模式下由用户主动开启；半自动 / 全自动模式下每次发送自动生成。
- 快照仍按 [ai-copilot-integration.md](./ai-copilot-integration.md) §6.1 的契约执行：5 分钟 TTL、一次性消费、绑定窗口 / leaf tab / root pane / sessionRevision / Provider / mode、跨窗口或 revision 变化时拒绝发送。
- 不持续同步，不在后台上传，不默认写入对话历史。

被移除的级别：

- **L1（仅元数据）**：移除。原 L1 的元数据字段（host / user / cwd / platform）合并进 L2 的元数据头，不再单独成档。
- **L3（Review Mode）**：作为上下文级别移除。其执行能力（一次性审批 + 独立 SSH exec）被半自动模式吸收；保留 `ActionReviewService` 作为半自动 / 全自动共用的执行后端。

## 4. 模式定义与执行边界

### 4.1 纯对话模式（Pure Conversation）

```text
用户输入
  ↓
Rust 组装 prompt（L0 或 L2，按开关）
  ↓
Provider 流式回答（文本 + 可选命令卡）
  ↓
UI 渲染：Markdown + 命令卡
  ↓
命令卡动作：
  ├─ 复制命令（始终可用）
  └─ 写入当前终端输入区（不回车，仅 SSH/local 交互终端可用）
  ↓
不发起任何远端执行
```

约束：

- 模型不获得任何工具调用能力；Provider 请求里不携带 `tools` 字段。
- 命令卡仍按 [ai-copilot-integration.md](./ai-copilot-integration.md) §7 走严格 JSON schema 校验 + 本地风险升级。
- `app_insert_ai_command` 拒绝 `\r`、`\n`、NUL 和控制序列，且永不追加 Enter。
- 多行命令只允许复制，不允许一键写入。
- L2 开关开启时，每条消息发送前自动生成并消费一份新的 Rust 快照；关闭后回到 L0。

### 4.2 半自动模式（Semi-Automatic）

```text
用户输入
  ↓
Rust 强制 L2：自动生成 transcript 快照 + 元数据
  ↓
Provider 流式回答 + 工具调用提案（structured output）
  ↓
每个工具调用 → ActionReviewService 一次性审批队列
  ↓
用户审批弹窗（<ActionReviewDialog>）：
  显示目标主机、CWD、完整命令、风险、超时
  ├─ 拒绝 / 关闭 / 超时 → 不执行，结果回传给模型
  └─ 批准 → 独立 SSH exec channel 执行
  ↓
执行结果（脱敏、截断、退出码）→ 模型
  ↓
模型可继续提出下一步工具调用（仍需审批）
```

约束：

- 每次工具调用都走 `ActionReviewService`，沿用 [ai-copilot-integration.md](./ai-copilot-integration.md) §5 L3 的全部执行约束（独立 exec channel、不劫持 PTY、不提供"本会话始终允许"、不自动多步循环）。
- 工具调用提案必须经过本地风险分类器升级；`destructive` / `privileged` 风险在审批弹窗里**红色高亮**并要求二次确认（点击"我已知晓风险"复选框后才能点批准）。
- 审批弹窗的目标绑定沿用 L3 的 sessionRevision 校验：tab 关闭、root/pane 归属变化、CWD 或 user 变化时拒绝执行。
- 模型可以提出多步调用，但每步独立审批；不允许"批准链"或"批量批准"。

### 4.3 全自动模式（Fully Automatic）

```text
用户输入
  ↓
Rust 强制 L2：自动生成 transcript 快照 + 元数据
  ↓
Provider 流式回答 + 工具调用提案
  ↓
护栏检查（每次调用前）：
  ├─ 黑名单命中 → 拒绝执行，结果回传给模型并附带拒绝原因
  ├─ 阈值触发（如本会话累计 N 次执行） → 暂停自动执行，提示用户切回半自动
  ├─ 不可逆动作未在白名单 → 拒绝执行，提示用户切回半自动或手动审批
  └─ 全部通过 → 独立 SSH exec channel 直接执行（不弹审批）
  ↓
执行结果（脱敏、截断、退出码）→ 模型
  ↓
模型可继续提出下一步工具调用（仍受护栏约束）
```

约束：

- **不弹审批弹窗**；用户授权由模式切换时的 `<ConfirmActionDialog>` 一次性给出。
- 护栏不可在 UI 上单独关闭；用户只能整体降级回半自动。
- 执行结果仍走独立 SSH exec channel，不劫持交互式 PTY，不写入可见终端 transcript。
- 会话级累计上限：默认 20 次工具调用 / 会话，达到上限后自动暂停并提示用户切回半自动或新建会话。
- 超时、断线、sudo 密码错误等异常**不自动重试**；直接把错误回传给模型，由模型决定是否在聊天里向用户说明。

## 5. 全自动模式护栏

护栏是全自动模式的核心安全机制，分四类：

### 5.1 危险命令黑名单

命中即拒绝，不可绕过：

```rust
const DANGEROUS_COMMAND_PATTERNS: &[&str] = &[
    // 文件系统毁灭
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm -rf ~/*",
    "mkfs",
    "dd if=/dev/zero of=/dev/",
    "dd if=/dev/random of=/dev/",
    // 权限提升逃逸
    "chmod -R 777 /",
    "chown -R ",
    // 进程失控
    ":(){ :|:& };:",            // fork bomb
    "kill -9 -1",
    // 网络后门
    "nc -l -p ",                // 反弹 shell 风险
    "bash -i >& /dev/tcp/",
    "sh -i >& /dev/tcp/",
    // 包管理破坏
    "apt remove --purge ",
    "yum remove -y ",
    "dnf remove -y ",
    // 内核模块
    "rmmod ",
    "modprobe -r ",
    // 系统关机 / 重启
    "shutdown",
    "reboot",
    "init 0",
    "init 6",
    "halt",
    "poweroff",
];
```

匹配规则：

- 命令 trim 后与黑名单字符串**前缀匹配**（`starts_with`）。
- 大小写敏感（避免 `RM` 误判，但 `shutdown` 类命令无大小写歧义）。
- 命中后返回 `AUTO_MODE_BLOCKED_COMMAND` 错误，附带命中模式串供模型理解为什么被拒。

### 5.2 不可逆动作白名单

未在白名单内的不可逆动作（`destructive` / `privileged` 风险）一律拒绝，要求用户切回半自动：

```rust
const IRREVERSIBLE_WHITELIST: &[&str] = &[
    // 文件删除（限单目录、非根）
    "rm ",                       // 允许，但 rm -rf / 等已被黑名单拦截
    // 文件移动 / 重命名
    "mv ",
    // 文件复制
    "cp ",
    // 文件权限
    "chmod ",                    // 允许，但 chmod -R 777 / 已被黑名单拦截
    "chown ",                    // 允许，但 chown -R 已被黑名单拦截
    // 包管理安装（不含卸载）
    "apt install",
    "apt-get install",
    "yum install",
    "dnf install",
    "pip install",
    "npm install",
    // 服务重启（限单服务）
    "systemctl restart ",
    "systemctl stop ",
    "systemctl start ",
    // sudo 提权（依赖三层密码源）
    "sudo ",
    // 文件写入（重定向）
    ">",                         // 允许覆盖写入
    ">>",                        // 允许追加写入
];
```

匹配规则：

- 风险分类器把命令标为 `destructive` / `privileged` 时，检查命令 trim 后是否以白名单内任一项开头。
- 命中白名单 → 允许自动执行。
- 未命中 → 返回 `AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED` 错误，提示模型建议用户切回半自动。

### 5.3 操作阈值

会话级累计约束：

```rust
struct AutoModeThresholds {
    max_tool_calls_per_session: usize,       // 默认 20
    max_destructive_calls_per_session: usize, // 默认 5
    max_privileged_calls_per_session: usize,  // 默认 3
    max_total_exec_duration_secs: u64,        // 默认 600（10 分钟）
}
```

触发后：

- `max_tool_calls_per_session`：暂停自动执行，返回 `AUTO_MODE_SESSION_LIMIT_REACHED`，提示用户切回半自动或新建会话。
- `max_destructive_calls_per_session` / `max_privileged_calls_per_session`：暂停自动执行，返回 `AUTO_MODE_RISK_LIMIT_REACHED`，提示用户切回半自动。
- `max_total_exec_duration_secs`：暂停自动执行，返回 `AUTO_MODE_DURATION_LIMIT_REACHED`。

阈值在设置页可调（高级设置区，默认折叠），但下限不可低于默认值。

### 5.4 目标绑定与 sessionRevision

沿用 [ai-copilot-integration.md](./ai-copilot-integration.md) §6 的 sessionRevision 校验：

- 每次工具调用绑定生成时的 leaf tab、root/pane 关系、`sessionRevision`、用户、CWD。
- tab 关闭、root/pane 归属变化、CWD 或 user 变化时拒绝执行，返回 `AUTO_MODE_TARGET_CHANGED`。
- 这与半自动模式的目标绑定逻辑完全一致，由 `ActionReviewService` 共用。

## 6. 三层 sudo 凭据与模式的衔接

[simplify-exec-sudo-credentials.md](./simplify-exec-sudo-credentials.md) 定义的三层密码源（Agent 参数 > profile 加密存储 > 主窗口弹窗 > 聊天问）与三模式的衔接：

| 模式   | sudo 密码首选源   | 主窗口隐藏时                          | 密码进 LLM 上下文             |
| ------ | ----------------- | ------------------------------------- | ----------------------------- |
| 纯对话 | N/A（不发起执行） | N/A                                   | N/A                           |
| 半自动 | profile 加密存储  | 弹审批弹窗时同步弹密码弹窗            | ❌ 不进（除非用户在聊天里给） |
| 全自动 | profile 加密存储  | 聊天问（返回 `SUDO_PASSWORD_NEEDED`） | ❌ 不进（首选 profile 存储）  |

关键约束：

- **全自动模式下，sudo 密码必须优先来自 profile 加密存储**；存储无密码且主窗口不可见时，返回 `SUDO_PASSWORD_NEEDED` 让模型在聊天里向用户索取，**不自动降级到弹窗**（因为全自动模式的语义是无人值守）。
- 半自动模式下，审批弹窗和密码弹窗可以合并显示（如果用户已预存密码，则审批弹窗只显示命令；未预存则同时显示密码输入框）。
- 全自动模式下首次需要 sudo 密码且未预存时，建议模型在聊天里直接告诉用户："请在 FileTerm 连接管理器预存 sudo 密码，或在此处提供（会进入我的上下文一次）。"

## 7. 领域模型变更

### 7.1 新增类型（`packages/core`）

```ts
type AiCopilotMode = 'pure-conversation' | 'semi-automatic' | 'fully-automatic'

type AiContextLevel = 'L0' | 'L2' // 移除 L1

interface AiCopilotModeState {
  mode: AiCopilotMode
  // 仅 pure-conversation 模式下可调；其他模式下锁定为 true
  attachTerminalContext: boolean
  // 全自动模式护栏状态（只读，由 Rust 派生）
  autoModeGuardrails: {
    sessionToolCallCount: number
    sessionDestructiveCount: number
    sessionPrivilegedCount: number
    sessionTotalExecDurationSecs: number
    thresholds: AiAutoModeThresholds
  }
}

interface AiAutoModeThresholds {
  maxToolCallsPerSession: number
  maxDestructiveCallsPerSession: number
  maxPrivilegedCallsPerSession: number
  maxTotalExecDurationSecs: number
}

// 工具调用提案（半自动 / 全自动模式用）
interface AiToolCallProposal {
  id: string
  toolName: 'execute_remote_command'
  command: string
  risk: 'read-only' | 'mutating' | 'destructive' | 'privileged' | 'unknown'
  target: AiContextTarget
  explanation?: string
}

// 工具调用结果（回传给模型）
interface AiToolCallResult {
  proposalId: string
  status: 'approved' | 'rejected' | 'auto-blocked' | 'executed' | 'failed' | 'timeout' | 'target-changed'
  exitCode?: number
  stdout?: string
  stderr?: string
  durationMs?: number
  reason?: string // 拒绝 / 阻断原因
}
```

### 7.2 修改现有类型

```ts
// StartAiChatInput 增加 mode 字段
interface StartAiChatInput {
  conversationId: string
  providerId: string
  userMessage: string
  contextSnapshotId?: string  // L0 省略；L2 必填
  mode: AiCopilotMode         // 新增
}

// CreateAiContextPreviewInput 的 mode 字段简化
interface CreateAiContextPreviewInput {
  tabId: string
  rootTabId?: string
  providerId: string
  mode: 'L0' | 'L2'  // 从 'metadata' | 'recent-terminal' 改为 'L0' | 'L2'
}

// AiStreamEvent 增加工具调用事件
type AiStreamEvent =
  | { type: 'started'; requestId: string; messageId: string }
  | { type: 'text-delta'; text: string }
  | { type: 'command'; command: AiCommandSuggestion }
  | { type: 'tool-call'; proposal: AiToolCallProposal }  // 新增
  | { type: 'tool-result'; result: AiToolCallResult }     // 新增
  | { type: 'usage'; inputTokens?: number; outputTokens?: number }
  | { type: 'completed'; finishReason?: string }
  | { type: 'error'; code: AiErrorCode; message: string; retryable: boolean }

// AiErrorCode 增加
type AiErrorCode =
  | ...  // 现有错误码
  | 'AI_AUTO_MODE_BLOCKED_COMMAND'           // 黑名单命中
  | 'AI_AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED'
  | 'AI_AUTO_MODE_SESSION_LIMIT_REACHED'
  | 'AI_AUTO_MODE_RISK_LIMIT_REACHED'
  | 'AI_AUTO_MODE_DURATION_LIMIT_REACHED'
  | 'AI_AUTO_MODE_TARGET_CHANGED'
  | 'AI_TOOL_CALL_REJECTED'                  // 半自动用户拒绝
```

### 7.3 移除的类型

- `AiContextPreview.mode` 的 `'metadata'` 取值（L1 移除）。
- 现有 L3 Review Mode 相关的独立类型（合并进半自动模式）。

## 8. IPC 与代码落点

### `packages/core`

- 新增 `AiCopilotMode` / `AiContextLevel` / `AiCopilotModeState` / `AiAutoModeThresholds` / `AiToolCallProposal` / `AiToolCallResult` 类型。
- 修改 `StartAiChatInput` / `CreateAiContextPreviewInput` / `AiStreamEvent` / `AiErrorCode`。
- 移除 L1 相关字段和 L3 独立类型。

### `apps/tauri/src-tauri/src/services/ai/`

- `mod.rs`：增加 mode 状态管理、护栏状态、会话级累计计数。
- `context.rs`：移除 L1 路径，`mode` 字段简化为 `'L0' | 'L2'`。
- `command.rs`：风险分类器复用，新增黑名单 / 白名单 / 阈值检查。
- 新增 `auto_mode_guardrails.rs`：黑名单匹配、白名单匹配、阈值累计、sessionRevision 校验（复用 `ActionReviewService` 的逻辑）。
- `action_review.rs`：保持现有的一次性审批队列 + 独立 SSH exec；半自动模式直接走该路径，全自动模式跳过审批步骤但复用 exec channel。

### `apps/tauri/src-tauri/src/commands/ai.rs`

- `app_get_ai_copilot_mode_state`：返回当前模式 + 上下文开关 + 护栏状态。
- `app_set_ai_copilot_mode`：切换模式（切到全自动时返回 `RequiresUserConfirmation` 让 renderer 弹确认框）。
- `app_set_ai_context_attach`：仅 pure-conversation 模式下可调；其他模式返回 `ContextLockedByMode` 错误。
- `app_get_ai_auto_mode_thresholds` / `app_set_ai_auto_mode_thresholds`：读写护栏阈值（带下限校验）。
- `app_reset_ai_auto_mode_session_counts`：手动重置会话级累计（用户主动操作）。
- 现有 `app_start_ai_chat` / `app_create_ai_context_preview` / `app_run_ai_review` 按 §7 类型变更调整。

### `apps/tauri/src/bridge/tauri-api.ts`

- 暴露上述类型安全 API。
- 新增 `onAiToolCallApproval`（半自动模式审批弹窗触发）/ `resolveAiToolCallApproval`。
- 现有 `onAiContextPreview` 等保留。

### Renderer

- `features/ai/AiCopilotPanel.tsx`：顶部增加三模式选择器（单选按钮组，复用 `<DropdownSelect>` 或专用 segmented control）。
- `features/ai/ContextToggle.tsx`：上下文开关，pure-conversation 模式下可交互，其他模式禁用并置灰。
- `features/ai/AutoModeGuardrailIndicator.tsx`：全自动模式下显示会话级累计计数和剩余配额。
- `features/ai/ToolCallApprovalDialog.tsx`：半自动模式审批弹窗，复用 `<ConfirmActionDialog>` 样式，显示目标 / CWD / 命令 / 风险 / 超时。
- `features/ai/ToolCallResultCard.tsx`：工具调用结果展示卡（成功 / 失败 / 超时 / 被拒）。
- `features/ai/useAiCopilot.ts`：增加 mode 状态、护栏状态、工具调用流处理。
- 设置页高级设置区增加全自动模式阈值配置（默认折叠）。

### UI 公用组件边界

- 模式选择器必须使用项目内统一组件，禁止直写 `<select>`。
- 上下文开关的禁用置灰态必须配合 `--focus-outline` 等色彩 token，不直写硬编码颜色。
- 审批弹窗、结果卡、护栏指示器全部复用 `<ConfirmActionDialog>` / `<AppIcon>` / `<VerticalScrollbar>` 等公用组件，遵循 [AGENTS.md](../../../AGENTS.md) §3 UI 边界。

## 9. 实施步骤

按 8 个 commit 推进，每个 commit 必须通过 typecheck + lint + clippy + test + prettier。

### Commit 1: `refactor(core): 简化上下文级别为 L0/L2，移除 L1`

- `packages/core` 修改 `AiContextLevel` 为 `'L0' | 'L2'`。
- `CreateAiContextPreviewInput.mode` 改为 `'L0' | 'L2'`。
- 移除 L1 相关字段、注释、测试 fixture。
- typecheck + prettier 通过。
- 不动 Rust 和 renderer 业务逻辑（仅类型层）。

### Commit 2: `feat(core): 增加 AiCopilotMode 类型与工具调用提案`

- 新增 `AiCopilotMode` / `AiCopilotModeState` / `AiAutoModeThresholds` / `AiToolCallProposal` / `AiToolCallResult`。
- `StartAiChatInput` 加 `mode` 字段。
- `AiStreamEvent` 加 `tool-call` / `tool-result` 变体。
- `AiErrorCode` 加 6 个新错误码。
- typecheck 通过。

### Commit 3: `feat(services): Rust 实现 mode 状态与 L2 强制`

- `services/ai/mod.rs` 增加 mode 状态管理（默认 `pure-conversation`）。
- `services/ai/context.rs` 移除 L1 路径，`semi-automatic` / `fully-automatic` 模式下强制生成 L2 快照。
- 新增 `app_get_ai_copilot_mode_state` / `app_set_ai_copilot_mode` / `app_set_ai_context_attach` command。
- `app_set_ai_context_attach` 在非 pure-conversation 模式下返回 `ContextLockedByMode`。
- 单元测试：mode 切换、L2 强制、上下文开关锁定逻辑。

### Commit 4: `feat(services): 全自动模式护栏（黑名单 + 白名单 + 阈值）`

- 新增 `services/ai/auto_mode_guardrails.rs`。
- 实现黑名单匹配（前缀匹配）、白名单匹配（前缀匹配）、阈值累计、sessionRevision 校验。
- 新增 `app_get_ai_auto_mode_thresholds` / `app_set_ai_auto_mode_thresholds` / `app_reset_ai_auto_mode_session_counts` command。
- 阈值下限校验（不低于默认值）。
- 单元测试：黑名单命中、白名单命中与未命中、阈值触发、sessionRevision 变化。

### Commit 5: `feat(services): 工具调用提案与执行后端`

- `services/ai/action_review.rs` 扩展：半自动模式走现有审批队列；全自动模式跳过审批但复用独立 SSH exec channel。
- `AiStreamEvent` 发 `tool-call` / `tool-result` 事件给 renderer。
- 半自动模式下 `onAiToolCallApproval` 触发 renderer 审批弹窗；`resolveAiToolCallApproval` 回传决策。
- 全自动模式下护栏通过后直接执行；护栏未通过返回对应错误码。
- sudo / su 包装复用 [simplify-exec-sudo-credentials.md](./simplify-exec-sudo-credentials.md) 的三层密码源。
- 单元测试：半自动审批通过 / 拒绝 / 超时、全自动护栏通过 / 命中黑名单 / 命中阈值、sudo 密码源衔接。

### Commit 6: `feat(renderer): 模式选择器与上下文开关`

- `AiCopilotPanel.tsx` 顶部增加三模式选择器（segmented control）。
- `ContextToggle.tsx` 上下文开关，pure-conversation 可交互，其他模式禁用并置灰 + 提示。
- 切到全自动模式时弹 `<ConfirmActionDialog>` 警告。
- `useAiCopilot.ts` 接通 mode 状态、上下文开关锁定逻辑。
- 复用 `<DropdownSelect>` / `<AppIcon>` / `--focus-outline` 等 token 与公用组件。
- typecheck + prettier 通过。

### Commit 7: `feat(renderer): 半自动审批弹窗与全自动护栏指示器`

- `ToolCallApprovalDialog.tsx` 半自动审批弹窗：显示目标 / CWD / 命令 / 风险 / 超时；`destructive` / `privileged` 红色高亮 + 二次确认复选框。
- `ToolCallResultCard.tsx` 工具调用结果展示卡。
- `AutoModeGuardrailIndicator.tsx` 全自动模式下显示会话级累计计数和剩余配额。
- `onAiToolCallApproval` / `resolveAiToolCallApproval` bridge 接通。
- typecheck + prettier 通过。

### Commit 8: `refactor: 移除 L3 Review Mode 独立入口，文档收口`

- 移除 [ai-copilot-integration.md](./ai-copilot-integration.md) §2 L3 章节、§5 L3 相关类型（已被 §7 新类型替代）。
- 移除 `app_run_ai_review` command（能力已被半自动模式吸收）。
- 更新 `docs/architecture.md` 移除 L3 独立章节，新增三模式章节。
- 更新 `docs/hidden-features.md` 如有相关条目同步。
- clippy 通过。
- 全部验收通过后，将本计划移至 `docs/plans/completed/`。

## 10. 测试策略

### 10.1 自动化测试

**Rust 单元测试**：

- mode 状态切换：pure-conversation ↔ semi-automatic ↔ fully-automatic。
- L2 强制：semi-automatic / fully-automatic 模式下 `app_set_ai_context_attach(false)` 返回 `ContextLockedByMode`。
- 黑名单匹配：`rm -rf /` / `mkfs` / `shutdown` 等命中；`rm -rf /tmp/foo` 不命中（因为不是前缀完全匹配 `/`）。
- 白名单匹配：`rm /tmp/foo` 命中白名单；`rm -rf /` 已被黑名单先拦截。
- 阈值累计：20 次工具调用后触发 `AUTO_MODE_SESSION_LIMIT_REACHED`；5 次 destructive 后触发 `AUTO_MODE_RISK_LIMIT_REACHED`。
- sessionRevision 变化：tab 关闭 / CWD 变化后返回 `AUTO_MODE_TARGET_CHANGED`。
- 半自动审批：用户拒绝 / 超时 / 关闭弹窗均不执行；批准后执行结果正确回传。
- 全自动执行：护栏通过后直接执行，无审批弹窗。
- sudo 衔接：全自动模式下 profile 存储有密码 → 直接执行；存储无密码 + 主窗口不可见 → 返回 `SUDO_PASSWORD_NEEDED`。

**Rust 契约测试**：

- TypeScript / Rust 序列化对齐：`AiCopilotMode` / `AiToolCallProposal` / `AiToolCallResult` / 新错误码。
- 旧 schema 兼容：现有 `ai-conversations.json` 中无 `mode` 字段的消息按 `pure-conversation` 处理。

**三端 CI 矩阵**：

- macOS / Windows / Linux 都执行上述测试。

### 10.2 手工验证

- 纯对话模式：L0 默认、L2 开关可切换、命令卡仅复制 / 写入输入区不回车、不发起执行。
- 半自动模式：L2 强制、上下文开关置灰、每次工具调用弹审批弹窗、destructive 红色高亮 + 二次确认、拒绝 / 超时不执行。
- 全自动模式：L2 强制、首次启用弹警告、护栏通过直接执行、护栏命中返回错误码、阈值触发暂停。
- 模式切换：纯对话 → 半自动 → 全自动 → 半自动 → 纯对话，状态正确转换，上下文开关按模式禁用 / 启用。
- sudo 衔接：全自动模式下预存密码无人值守跑通；未预存 + 主窗口隐藏返回 `SUDO_PASSWORD_NEEDED` 引导聊天问。
- 跨设备迁移：旧 `ai-conversations.json` 在新设备读取，无 `mode` 字段按 `pure-conversation` 处理。
- UI 公用组件边界：模式选择器、上下文开关、审批弹窗、结果卡全部复用公用组件，无硬编码颜色 / `<select>` / `window.confirm`。

### 10.3 Provider 端到端

- OpenAI-compatible-chat：三模式 × L0/L2 组合，工具调用提案通过 structured output 返回。
- OpenAI-responses：同上。
- Anthropic-messages：同上。
- 真实 Claude Code / Codex CLI 不受影响（它们走 MCP/CLI，不走内置 Copilot）。

## 11. 威胁模型

| 场景                        | 结果                                                                              |
| --------------------------- | --------------------------------------------------------------------------------- |
| 纯对话模式 L0               | 模型看不到任何终端内容，只能基于用户输入回答                                      |
| 纯对话模式 L2（用户主动开） | 模型看到一次性 transcript 快照（已脱敏、截断）；不持续同步                        |
| 半自动模式 L2               | 每次发送自动生成快照；每次执行必须用户审批；destructive / privileged 二次确认     |
| 全自动模式 L2               | 每次发送自动生成快照；护栏未命中时直接执行；护栏命中返回错误码                    |
| 全自动模式被提示注入攻击    | 黑名单拦截 `rm -rf /` 等；阈值限制单会话最大破坏；不可逆动作白名单限制范围        |
| 全自动模式密码泄露          | sudo 密码首选 profile 加密存储，不进 LLM 上下文；未预存时聊天问，密码进上下文一次 |
| 半自动模式误批准            | destructive / privileged 红色高亮 + 二次确认降低误操作概率                        |
| 模式被用户误切到全自动      | 首次切换弹 `<ConfirmActionDialog>` 警告                                           |
| 旧会话无 mode 字段          | 按 `pure-conversation` 处理，不强制迁移                                           |

## 12. 与同行做法对比

| 同行                   | 模式分级                   | 上下文级别      | 自动执行                | 护栏                                     |
| ---------------------- | -------------------------- | --------------- | ----------------------- | ---------------------------------------- |
| Claude Code (Composer) | 默认对话 + 可选 Agent 模式 | 全量上下文      | Agent 模式下自动        | 危险命令确认 + 工作目录限制              |
| Codex CLI              | 默认对话 + 可选 full-auto  | 全量上下文      | full-auto 下自动        | 沙箱 + 命令审批                          |
| OpenCode               | 默认对话 + 可选 Agent      | 全量上下文      | Agent 下自动            | 工作目录限制 + 命令黑名单                |
| Cline                  | 默认对话 + Auto Pilot      | 全量上下文      | Auto Pilot 下自动       | 命令黑名单 + 自动批准开关                |
| Roo Code               | 默认对话 + Auto            | 全量上下文      | Auto 下自动             | 命令黑名单 + 工作目录限制                |
| FileTerm（本计划）     | 纯对话 / 半自动 / 全自动   | L0 / L2（简化） | 半自动审批 / 全自动护栏 | 黑名单 + 白名单 + 阈值 + sessionRevision |

本计划落地后，FileTerm 在模式分级上与 Claude Code / Codex CLI 等同行对齐，在上下文级别上比同行更**简洁**（仅 L0/L2 两档，无中间态），在全自动模式护栏上比同行更**严格**（不可逆动作白名单 + 阈值累计，而非单纯黑名单）。

## 13. 风险与缓解

| 风险                                     | 缓解                                                                                                  |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| 全自动模式被提示注入攻击执行危险命令     | 黑名单 + 白名单 + 阈值三重护栏；不可逆动作未在白名单一律拒绝                                          |
| 用户误切全自动模式                       | 首次切换弹 `<ConfirmActionDialog>` 警告                                                               |
| 半自动模式审批疲劳（用户无脑点批准）     | destructive / privileged 红色高亮 + 二次确认复选框                                                    |
| 全自动模式 sudo 密码未预存导致卡住       | 三层密码源兜底：profile 存储 > 弹窗 > 聊天问                                                          |
| L1 移除影响已有用户                      | L1 在当前版本尚未广泛使用（Phase 3 刚落地）；迁移成本可控                                             |
| 旧会话无 mode 字段                       | 按 `pure-conversation` 处理，不强制迁移                                                               |
| 工具调用提案模型不支持 structured output | 降级为严格 JSON envelope，沿用 [ai-copilot-integration.md](./ai-copilot-integration.md) §7 的降级路径 |
| 全自动模式阈值默认值过严                 | 阈值在设置页可调（高级设置区），下限不低于默认值                                                      |
| 全自动模式阈值默认值过松                 | 默认值保守（20 次工具调用 / 5 次 destructive / 3 次 privileged / 10 分钟总执行时长）                  |

## 14. 不做的事

- 不动 Provider adapter（`openai-compatible-chat` / `openai-responses` / `anthropic-messages`）。
- 不动 API Key 存储与 secret 加密层。
- 不动 MCP / CLI 通道（外部 Agent 走自己的路径，不受三模式影响）。
- 不动 SFTP / 传输 / 隧道工具。
- 不动 [simplify-exec-sudo-credentials.md](./simplify-exec-sudo-credentials.md) 的三层密码源实现（本计划只衔接，不重写）。
- 不做"自动多步循环"（半自动模式每步独立审批；全自动模式由模型决定是否继续，但每步仍受护栏）。
- 不做"批量批准"或"会话级永久授权"（违背保守助手定位）。
- 不做跨会话的护栏状态持久化（护栏累计按会话计，新建会话自动重置）。
- 不做 L1 级别保留（明确移除，不提供兼容开关）。
- 不做全自动模式下的远端文件修改 / 传输自动执行（首版只开放 SSH exec 工具调用）。

## 15. 拍板记录

1. ✅ 三模式：纯对话 / 半自动 / 全自动
2. ✅ 移除 L1 上下文级别，简化为 L0 / L2 两档
3. ✅ 半自动 / 全自动模式强制 L2，不允许 L0
4. ✅ 纯对话模式 L2 可开关（默认关闭）
5. ✅ L3 Review Mode 作为独立级别移除，能力合并进半自动模式
6. ✅ 全自动模式护栏：黑名单 + 白名单 + 阈值 + sessionRevision
7. ✅ 护栏不可在 UI 上单独关闭，只能整体降级回半自动
8. ✅ 全自动模式首次启用弹 `<ConfirmActionDialog>` 警告
9. ✅ 半自动模式 destructive / privileged 红色高亮 + 二次确认
10. ✅ 全自动模式 sudo 密码首选 profile 加密存储；未预存 + 主窗口不可见时聊天问
11. ✅ 阈值默认保守（20 / 5 / 3 / 600s），高级设置可调，下限不低于默认
12. ✅ 不做自动多步循环、批量批准、会话级永久授权
13. ✅ 不做远端文件修改 / 传输自动执行（首版只开放 SSH exec）
