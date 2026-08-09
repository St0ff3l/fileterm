# AI Copilot 功能集成计划

状态：进行中（Phase 0–4 的代码实现已完成；Review Mode 与跨平台发行验收待收口）
前置工作：[AI Copilot 同窗 UI 草案](../completed/ai-copilot-companion-window.md)

## 1. 结论

FileTerm 内置 AI 首先定位为**保守的终端助手**，不是无人值守 Agent：

- 支持用户自带 Provider、本地对话历史和流式回答。
- 默认不读取终端输出；每次提问是否附带上下文由用户决定，并在发送前展示准确预览。
- 模型可以生成结构化命令卡；用户可以复制，或把单行命令写入当前终端输入区，但 FileTerm 不自动按回车。
- 第一阶段不开放模型工具调用、自动执行、循环修复、文件修改和后台任务。
- 已有 MCP/CLI 继续服务 Claude Code、Codex CLI 等外部 Agent。内置 AI 不通过本机 MCP 回环调用自己，后续只复用 MCP 已有的 action、审批和独立 SSH exec 边界。

这使两套入口保持清晰：

| 入口             | 面向用户              | 首要能力                   | 执行边界                                 |
| ---------------- | --------------------- | -------------------------- | ---------------------------------------- |
| 内置 Copilot     | 普通终端用户          | 解释、排障、生成可检查命令 | MVP 不执行，只复制或写入输入区           |
| FileTerm MCP/CLI | 熟悉 Agent 的高级用户 | 远程执行、文件、传输、隧道 | MCP 修改操作逐次审批，CLI 为用户显式调用 |

## 2. 产品权限级别

### L0：纯对话

- 仅发送用户输入和必要的会话历史。
- 不附带主机、路径或终端内容。
- Provider 配置完成后默认进入该级别。

### L1：目标元数据

- 用户按次开启后，可附带当前会话的协议、公开主机标签、登录用户、远端平台、shell CWD 和连接状态。
- 不包含连接凭据、IP 之外的 secret、文件内容或终端 transcript。
- 发送前显示上下文摘要。

### L2：最近终端上下文

- 用户按次开启“附带最近终端”后，Rust 从当前 tab 的 runtime transcript 生成一次性快照。
- UI 展示将发送的准确文本，用户确认后才随本次消息发送。
- 不持续同步，不在后台上传，不默认写入对话历史。

### L3：Review Mode（后续独立阶段）

- 模型只能提出结构化 action proposal。
- 用户点击“审核并运行”后，FileTerm 展示目标主机、CWD、完整命令、风险提示和超时，再进入一次性审批。
- 执行使用独立 SSH exec channel，不劫持交互式 PTY；结果作为工具结果回到对话。
- 不提供“本会话始终允许”、自动批准或无人值守循环。

MVP 只交付 L0–L2。

## 3. 总体架构

```text
AiCopilotPanel / AI Settings
  -> tauri-api.ts
    -> Rust AI commands
      -> AiProviderService
        -> OpenAI-compatible Chat adapter
        -> OpenAI Responses adapter
        -> Anthropic Messages adapter
      -> AiContextService
        -> workspace runtime transcript
        -> ANSI/control cleanup + limits + redaction
      -> AiConversationStore
        -> local conversation metadata/messages
```

约束：

- `packages/core` 是公开模型和 IPC 类型的 single source of truth。
- API Key 只在用户录入/替换时短暂存在于 renderer 表单内存，并通过一次保存 IPC 交给 Rust；保存后的读取、持久化、Provider 请求、流式解析、上下文快照与历史全部留在 Rust，renderer 不直接调用模型服务，也不能回读 Key。
- 每次生成使用独立 Tauri `Channel<AiStreamEvent>`，避免用全局 event 混合多个窗口或多个请求。
- Provider adapter 只负责外部协议差异，统一输出 FileTerm 自有的文本增量、命令卡、usage、完成和错误事件。

## 4. Provider 设计

### 4.1 首批 Provider

1. `openai-compatible-chat`
   - 作为第一个实现切片，支持可配置 Base URL、Model 和 API Key。
   - 覆盖仍提供 Chat Completions 兼容接口的云服务和本地模型网关。
2. `openai-responses`
   - 面向 OpenAI 原生 Responses API，单独处理其事件和结构化输出。
   - 不与兼容接口共用响应解析器。
3. `anthropic-messages`
   - 面向 Anthropic Messages API，独立解析 SSE content block 事件。

不做一个“任意 JSON API”万能适配器。自定义 Provider 必须选择已知协议族，再填写 Base URL；否则错误处理、流式事件和结构化输出无法形成可靠契约。

Base URL 表示协议的 API root，adapter 负责追加自己的 endpoint；保存和测试时显示最终请求地址的脱敏预览，避免用户误填 `/chat/completions` 后被重复拼接。每个 adapter 还需声明 streaming、structured output 和 usage reporting 能力，不支持的参数不得盲目透传。

首版 URL 契约：去除末尾 `/`，允许用户填写包含 `/v1` 的 API root，但拒绝以 `/chat/completions`、`/responses` 或 `/messages` 结尾；各 adapter 分别追加自己的 endpoint。连接测试发送固定的小请求（输出上限不超过 8 tokens，不附带历史或终端），HTTP 2xx 且能解析协议顶层对象才算成功，界面必须提前提示测试可能产生极少量费用。

OpenAI 当前官方建议在需要多轮、推理和工具调用的工作流中使用 Responses API；其流式响应和结构化输出都有独立事件/格式。Anthropic Messages 流同样基于 SSE，但事件按 message/content block 分层，因此需要独立 adapter，不能只替换 URL。

### 4.2 配置与凭据

公开配置保存在 `ai-providers.json`：

- `id`
- `name`
- `kind`
- `baseUrl`
- `model`
- `enabled`
- `isDefault`
- `allowNoAuth`
- `allowInsecureHttp`
- 可选的非敏感推理、温度和输出长度设置

API Key 与敏感自定义 header 保存在 `ai-provider-secrets.json`：

- 只由 Rust 读取。
- Unix 创建与自愈时使用 owner-only `0600`。
- 公开 snapshot 和 bridge 返回 `hasApiKey`，不返回原文。
- 空字符串表示保留旧 Key，显式 `null` 表示清除，沿用 profile secret 的更新语义。
- 允许无 Key 的 Provider 仅限可信 loopback endpoint，并要求用户显式勾选 `allowNoAuth`；首版可信 loopback 只认精确 `localhost`、IPv4 `127.0.0.0/8` 和 IPv6 `::1`，不通过 DNS 解析推断。
- `allowInsecureHttp` 是独立持久化开关。HTTP loopback 仍需用户确认；非 loopback HTTP 可以在用户显式开启后使用，但设置页持续显示传输可能被窃听的警告。HTTP 与无鉴权不能互相隐式开启。
- `usable` 是 Rust 派生字段，不落盘：`enabled && URL/协议合法 && model 非空 && (hasApiKey || allowNoAuth 合法)`。
- Provider 必须显式选择一个 default，存储中最多一个 `isDefault=true`。保存新 default 时在同一把锁内清除其他 default；读取发现多个时按稳定顺序自愈。默认 Provider 被禁用或删除后，原子回退到第一个 enabled 且 usable 的 Provider；没有候选时清空 default 并恢复未配置空状态。
- secret 保存使用专门的 patch DTO；保存成功或失败后都清空 renderer 中的 Key 输入，重新打开设置时永不回填。

Rust 读取两个文件后派生公开 `AiProviderSummary`，其中 `hasApiKey` 和 `usable` 只存在于返回值，不写回 `ai-providers.json`。

FileTerm 当前明确不接 macOS Keychain/safeStorage，因此 AI 设置页必须说明：凭据保存在本机应用数据目录，并依赖操作系统用户目录权限隔离。

### 4.3 Endpoint 安全

- 拒绝 URL 中内嵌用户名或密码。
- 默认要求 HTTPS；HTTP 仅允许 loopback 地址，其他地址需要明确的“不安全连接”开关和警告。
- 禁止把 Authorization/API Key 转发到跨 origin 重定向；优先关闭自动重定向，或只允许同 origin 跳转。
- 请求设置连接、首包、总时长和空闲流超时，并支持用户取消。
- 日志只记录 provider kind、model、request ID、耗时、状态和 token/字节统计，不记录 URL query、header、prompt、回答、终端或 Key。

## 5. 领域模型

优先在 `packages/core` 增加：

```ts
type AiProviderKind = 'openai-compatible-chat' | 'openai-responses' | 'anthropic-messages'

interface AiProviderSummary {
  id: string
  name: string
  kind: AiProviderKind
  baseUrl: string
  model: string
  enabled: boolean
  hasApiKey: boolean
  usable: boolean
  isDefault: boolean
  allowNoAuth: boolean
  allowInsecureHttp: boolean
}

interface AiProviderDraft {
  id?: string
  name: string
  kind: AiProviderKind
  baseUrl: string
  model: string
  enabled: boolean
  isDefault: boolean
  allowNoAuth: boolean
  allowInsecureHttp: boolean
}

interface AiProviderSecretPatch {
  // undefined: reuse saved value; string: replace; null: clear
  apiKey?: string | null
}

interface SaveAiProviderInput {
  provider: AiProviderDraft
  secrets?: AiProviderSecretPatch
}

interface TestAiProviderInput {
  provider: AiProviderDraft
  // Existing provider + undefined reuses the Rust-side saved Key.
  // A new provider must send a Key or explicitly use valid allowNoAuth.
  secrets?: AiProviderSecretPatch
}

interface AiConversation {
  id: string
  title: string
  providerId: string
  createdAt: string
  updatedAt: string
  messages: AiMessage[]
}

interface AiMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  commands?: AiCommandSuggestion[]
  context?: { attached: boolean; target?: AiContextTarget; truncated?: boolean; redactions?: number }
  createdAt: string
}

interface AiContextTarget {
  tabId: string
  rootTabId?: string
  sessionType: 'ssh' | 'local'
  sessionRevision: string
  displayHost?: string
  user?: string
  cwd?: string
  platform?: string
}

interface AiContextPreview {
  snapshotId: string
  expiresAt: string
  target: AiContextTarget
  transcript?: string
  redactions: number
  truncated: boolean
}

interface AiCommandSuggestion {
  id: string
  command: string
  explanation?: string
  risk: 'read-only' | 'mutating' | 'destructive' | 'privileged' | 'unknown'
  multiline: boolean
  target: AiContextTarget
}

interface StartAiChatInput {
  conversationId: string
  providerId: string
  userMessage: string
  contextSnapshotId?: string
}

interface CreateAiContextPreviewInput {
  tabId: string
  rootTabId?: string
  providerId: string
  mode: 'metadata' | 'recent-terminal'
}

type AiStreamEvent =
  | { type: 'started'; requestId: string; messageId: string }
  | { type: 'text-delta'; text: string }
  | { type: 'command'; command: AiCommandSuggestion }
  | { type: 'usage'; inputTokens?: number; outputTokens?: number }
  | { type: 'completed'; finishReason?: string }
  | { type: 'error'; code: AiErrorCode; message: string; retryable: boolean }

type AiErrorCode =
  | 'AI_PROVIDER_NOT_FOUND'
  | 'AI_PROVIDER_INVALID_CONFIG'
  | 'AI_PROVIDER_INVALID_URL'
  | 'AI_PROVIDER_INSECURE_HTTP'
  | 'AI_PROVIDER_AUTH_REQUIRED'
  | 'AI_PROVIDER_CONNECTION_FAILED'
  | 'AI_PROVIDER_HTTP_ERROR'
  | 'AI_PROVIDER_RESPONSE_INVALID'
  | 'AI_PROVIDER_TIMEOUT'
  | 'AI_REQUEST_CANCELLED'
  | 'AI_CONTEXT_NOT_FOUND'
  | 'AI_CONTEXT_EXPIRED'
  | 'AI_CONTEXT_ALREADY_USED'
  | 'AI_CONTEXT_TARGET_CHANGED'
  | 'AI_COMMAND_UNSAFE_INPUT'
  | 'AI_CONVERSATION_LIMIT'

interface AiCommandError {
  code: AiErrorCode
  message: string
  retryable: boolean
  httpStatus?: number
}
```

实际实现时 TypeScript 与 Rust serde 类型必须通过 contract test 对齐；renderer 不自行发明额外状态。

`app_test_ai_provider(input: TestAiProviderInput)` 始终测试当前表单草稿，不要求先保存。已存在 Provider 且 `secrets.apiKey === undefined` 时由 Rust 复用已保存 Key；新 Provider 必须随本次 IPC 提供 Key，或满足显式 loopback `allowNoAuth`。测试不会持久化草稿或 secret。

`sessionRevision` 由 Rust AI context service 维护：leaf tab 重连/替换、root/pane 归属变化、shell user 或 CWD 变化时递增；普通 transcript 追加不递增，因为发送的仍是用户已经预览过的不可变快照。命令卡和 context snapshot 都保存生成时 revision，写入或发送前再与 runtime 比较。

## 6. 终端上下文授权链路

### 6.1 生成预览

`app_create_ai_context_preview(input: CreateAiContextPreviewInput)` 在 Rust 中完成；来源窗口 label 由 Tauri command context 注入，不接受 renderer 伪造：

1. 校验 tab 仍存在且目标能力符合 `mode`。
2. 按 `mode` 从 workspace runtime 读取数据：L1 只读取目标元数据，且不得调用 transcript accessor；L2 才读取 transcript。
3. CRLF 归一化，移除 ANSI escape、不可见控制字符和异常长行。
4. 仅截取末尾有限行/字符；初始上限建议 120 行且不超过 16 KiB UTF-8。
5. 对 Bearer token、常见 API Key、密码赋值、私钥块等做 best-effort 遮盖。
6. 返回准确预览、截断标记、遮盖数量和短时有效的 `snapshotId`。

遮盖不是安全保证。UI 必须明确告诉用户“请检查即将发送的内容”。预览保持只读；用户可以取消整个 transcript 附件或切换为 L1 后重新生成快照，不能在 renderer 中编辑一份与 Rust 快照不一致的隐藏副本。用户若要修改文本，应把需要的内容手动粘贴进普通消息。

### 6.2 发送

- `app_start_ai_chat(input: StartAiChatInput)` 接受用户消息、会话和 Provider；L0 省略 `contextSnapshotId`，L1/L2 只能引用 Rust 生成的 `snapshotId`，不能携带 renderer 自行拼出的隐藏 transcript。
- 快照仅存内存，建议 TTL 5 分钟，并绑定创建它的窗口、leaf tab、root/pane 关系、`sessionRevision`、Provider 和 context mode。
- 发送时原子消费快照；过期、重放、跨窗口使用、tab 已关闭、Provider/目标/CWD/用户或连接 generation 变化时拒绝发送并要求重新预览。
- Rust 使用快照对应的准确内容组装 prompt，保证预览内容与发送内容一致。
- 终端文本使用明确的 data delimiter，并在 system prompt 中声明它是不可信数据，不能覆盖 FileTerm 的安全规则。

### 6.3 历史

- 默认持久化用户消息、助手回答和命令卡。
- 默认不持久化原始终端 excerpt，只记录 `contextAttached`、目标摘要、截断和遮盖计数。
- `ai-conversations.json` 只保存索引；每个会话写入 `ai-conversations/{id}.json`，使用原子替换并按 owner-only 权限创建，避免一个不断膨胀的全局 JSON。
- 首版限制最多 50 个会话、每个会话最多 200 条消息和 1 MiB；达到上限时提示用户清理，不静默删除历史。
- 向 Provider 发送历史时设置总字符/token 预算，优先保留 system policy、最近消息和本次上下文，超限时从最旧普通消息开始裁剪。
- 删除会话必须在本地立即删除；Provider 侧的数据处理仍受用户选择的 Provider 政策约束，FileTerm 不宣称能控制第三方保留。

## 7. 回答与命令卡

- 普通解释使用 Markdown 文本流式显示。
- 命令不能依靠扫描 Markdown code fence 推断；Provider 支持结构化输出时使用 schema，否则要求严格 JSON envelope，并在 Rust 中校验。
- “不开放工具调用”指不向模型提供任何可产生副作用的 FileTerm action；Provider 原生 structured output 或仅用于返回 `AiCommandSuggestion` 的 output-only schema 不属于执行工具。
- 普通解释回合可以直接流式显示；需要返回命令卡的结构化回合先在 Rust 缓冲并完成 schema 校验，再一次性提交回答和命令卡，不能把未闭合的 JSON 增量直接渲染为可信命令。
- 结构化解析失败时降级为普通文本，不自动提取或执行疑似命令。
- FileTerm 本地风险分类器可以提高模型给出的风险等级，不能降低；出现 `rm`、重定向覆盖、磁盘/分区、账号权限、包管理、服务重启、sudo/su 等模式时至少标记为 mutating/privileged/unknown。

命令卡动作：

- “复制命令”：始终可用。
- “写入当前终端”：仅 SSH/local 交互终端可用，调用专用 `app_insert_ai_command`。
- `app_insert_ai_command` 拒绝 `\r`、`\n`、NUL 和其他终端控制序列，且永不追加 Enter。
- 多行命令第一阶段只允许复制，不允许一键写入。
- 命令卡绑定生成时的 leaf tab、root/pane 关系、`sessionRevision`、用户和 CWD；任一项变化时显示目标已变化，并禁止直接写入，用户需基于新目标重新生成。

## 8. IPC 与代码落点

### `packages/core`

- Provider summary/input、conversation、message、context preview、command suggestion、stream event、稳定错误码。

### `apps/tauri/src-tauri/src/services/ai/`

- `mod.rs`：service facade、并发限制、请求生命周期。
- `providers/openai_compatible.rs`
- `providers/openai_responses.rs`
- `providers/anthropic.rs`
- `context.rs`：预览、清理、遮盖、预算与短时快照。
- `conversation.rs`：本地历史和裁剪。
- `command.rs`：结构化解析、风险升级、写入约束。

当前 Phase 0–2 暂时集中实现为 `apps/tauri/src-tauri/src/services/ai.rs`，以便在 context
与 command card 尚未落地时保持边界紧凑；进入 Phase 3 前再按上下文、命令和 Provider
职责拆分到上述目录。

### `apps/tauri/src-tauri/src/commands/ai.rs`

- `app_list_ai_providers`
- `app_save_ai_provider(input: SaveAiProviderInput)`
- `app_delete_ai_provider`
- `app_test_ai_provider(input: TestAiProviderInput)`
- `app_list_ai_conversations`
- `app_get_ai_conversation`
- `app_delete_ai_conversation`
- `app_create_ai_context_preview`
- `app_start_ai_chat`
- `app_cancel_ai_chat`
- `app_insert_ai_command`

### `apps/tauri/src/bridge/tauri-api.ts`

- 只暴露上述类型安全 API 和 per-request stream channel。
- 不暴露 API Key 读取接口，不把 provider HTTP client 放进 renderer。

### Renderer

- 将 `AiCopilotPanel.tsx` 拆成 provider empty state、conversation list、message list、context preview、composer 和 command card。
- 把 AI 状态收敛到 `features/ai/useAiCopilot.ts`；在没有明确跨 feature 高频共享之前不引入 Zustand。
- 设置页保存后应用 Rust 返回的 provider summary，不依赖广播更新当前窗口。

## 9. 与 MCP/CLI 的关系

- 内置 Copilot 不启动 `claude`/`codex` 子进程，也不通过 `fileterm mcp` loopback 调用自身服务。
- MCP 保持“外部 Agent 协议适配器”；内置 Copilot 保持“应用内 Provider 客户端”。
- 后续 Review Mode 不复制 MCP 的业务逻辑，应先把 `services/mcp.rs` 中 action dispatch 与 approval detail 抽成内部可复用 service，再由 MCP 和 AI 两个入口调用。
- AI 请求不能借 CLI 的“用户显式调用”语义绕过审批；任何 AI 发起的执行都按 MCP mutation 的审批强度处理。

## 10. 分阶段交付

### Phase 0：契约与安全基线

- [x] 增加 core Provider 类型、稳定错误码和 Rust serde contract fixture。
- [x] 建立公开配置/secret 分离存储与 Unix owner-only `0600` 测试。
- [x] 实现 endpoint 校验、HTTP/无鉴权显式门控、禁用重定向与不含请求内容的稳定错误码。
- [x] 流式请求的取消和并发限制（Phase 2）。

### Phase 1：Provider 配置

- [x] 接通设置页的读取、保存、删除和“测试未保存草稿”，并验证 default/usable 状态转换。
- [x] 实现 `openai-compatible-chat` 的最小非流式测试；固定请求不含终端/历史，输出上限为 8 tokens。
- [x] UI 只显示 `hasApiKey/usable/isDefault`，重新打开设置时不回填 Key，并覆盖 loopback 无鉴权和 HTTP 风险确认。

### Phase 2：无终端上下文的流式对话

- [x] 实现 conversation CRUD、请求 channel、取消和重试。
- [x] AI 面板支持新建会话、切换 Provider、发送、停止和错误恢复。
- [x] 增加 `openai-responses` 与 `anthropic-messages` adapter fixture。

### Phase 3：按次上下文与命令卡

- [x] 接入 context preview、5 分钟 TTL、一次性消费、清理、截断和 best-effort 遮盖。
- [x] 支持 L1/L2 按次授权，不提供全局“永久读取终端”默认值；L1 不读取 runtime transcript，L2 才生成不可变预览。
- [x] 接入严格 JSON 命令卡、复制和单行“写入但不回车”；写入动作只更新受控终端输入框，不经过 PTY。

### Phase 4：本地历史与体验收口

- [x] 本地历史搜索、重命名、删除和容量上限。
- [x] 展示 Provider、model、usage、目标变化和 context attached 状态。
- [ ] macOS、Windows、Linux 真机验证流式取消、代理、睡眠恢复、窗口关闭和断网重试（见 `docs/quality/ai-copilot-platform-regression.md`；不能由当前单一开发机伪造）。

### Phase 5：Review Mode（单独评审后再做）

- [ ] 抽取 MCP action/approval service。
- [ ] 只开放独立 SSH exec 的单步命令 proposal。
- [ ] 完整审批、结果回传、超时/截断和审计元数据。
- [ ] 不做自动多步循环、交互式 PTY 注入或永久授权。

## 11. 测试与验收

### 自动化

- Provider adapter：SSE 分片、未知事件、错误事件、截断流、取消、usage 和结构化输出 fixtures。
- Context：L1 不调用 transcript accessor、ANSI/CRLF/control 清理、UTF-8 边界、长行、截断、secret patterns、目标 revision、一次性消费、重放/跨窗口拒绝和快照过期。
- Storage：Key 不进入公开配置/snapshot/log，Unix 权限收紧，空值保留与显式清除。
- Command：单行约束、控制字符拒绝、风险只升不降、目标变化。
- Contract：TypeScript/Rust 序列化一致，稳定错误码和 stream event 完整。

### 手工验收

- 未配置 Provider 时只有安全空状态。
- 多 Provider 场景下 default、disabled、delete 和 loopback 无鉴权回退符合 `usable` 规则。
- L0 提问不会读取或上传 transcript。
- L2 必须先看见准确预览；关闭开关后抓包确认无 transcript。
- API Key 在录入时只短暂存在于当前设置表单；保存/关闭后不进入 renderer snapshot、持久化 UI state、DevTools 可回读状态、日志和错误信息，Rust 也绝不把 Key 返回给前端。
- 模型给出危险或多行命令时不能一键写入，更不能自动执行。
- 写入单行命令后终端不自动回车，用户仍有最终执行权。
- 切换 tab、分屏 pane、CWD 或身份后，旧命令卡不会静默写入错误目标。

## 12. 已完成的前两阶段

Phase 0–2 已在不读取终端的前提下完成：

1. 在 core 定义 provider 类型和稳定错误码。
2. Rust 增加公开配置/secret 存储和 endpoint 校验。
3. 实现三个 Provider 协议族的最小非流式连接测试。
4. 经 commands 与 `tauri-api.ts` 接通设置页、会话持久化与 per-request 流式 channel。
5. 补齐 secret、URL、SSE、错误、取消和核心 stream-event contract 测试。

右侧 Copilot 现在会识别可用 Provider、保存本地会话并显示流式纯对话回答。当前仍为 L0：
它不会读取终端、不会上传上下文，也不会生成可执行动作或执行命令。

## 13. 参考

- [OpenAI Responses 流式响应](https://developers.openai.com/api/docs/guides/streaming-responses)
- [OpenAI Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- [Anthropic Messages 流式事件](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [本地终端与 Agent MCP 接入](./local-terminal-mcp.md)
