# AI Copilot 文件与图片附件支持计划

状态：规划中（当前 Copilot 仅发送文本和经确认的终端上下文；附件能力尚未实现）

创建日期：2026-08-16

## 背景与现状

当前 Copilot composer 是文本输入框，`StartAiChatInput` 只有 `userMessage`、模式和一次性终端上下文快照；`AiMessage` 也只保存文本、上下文引用和工具活动。相关入口见 [Copilot 面板](../../../apps/tauri/src/renderer/features/ai/ai-copilot-panel.tsx)、[共享 AI 类型](../../../packages/core/src/index.ts) 和 [Rust AI service](../../../apps/tauri/src-tauri/src/services/ai/mod.rs)。

这意味着现在不能把本地文件、远程文件或图片作为一个受控附件交给 Copilot。它与终端上下文不是同一类数据，不能通过扩大 L2 transcript 快照或把路径拼进 prompt 来“顺手支持”。

## 目标

- 在 Copilot 对话框中选择、拖入或粘贴文件/图片，并以用户可见的附件 chip 管理待发送内容。
- 本地文件和远程 SFTP 文件都经过 Rust 所有权、大小/MIME 校验和会话范围校验，再进入 Provider 请求。
- 文本模型仍然可以正常处理文本类文件；只有具备视觉能力的模型才接收原始图片。
- Provider 不支持某种附件时，发送前明确提示并给出可操作的替代方案，不静默丢弃、不把二进制伪装成文本。
- 不把文件内容、base64、任意本机路径或远程凭据写入对话历史、日志、MCP/CLI 结果或标题生成请求。

## 非目标

- 第一版不承诺所有 Provider 的原生“文件 ID”或长期云端文件存储。
- 第一版不把任意目录、远程根路径或工作区快照暴露给模型；用户必须明确选择附件。
- OCR、表格计算、压缩包解包和视频/音频理解不作为首个版本的隐式能力；它们另行评估。

## 关键结论：非多模态模型可以接入，但不能直接理解图片

附件先按内容类型和模型能力路由，而不是把所有附件统一编码成一条 prompt：

| 附件类型                           | 多模态模型                              | 非多模态模型                                   |
| ---------------------------------- | --------------------------------------- | ---------------------------------------------- |
| 纯文本、代码、JSON/YAML、CSV、日志 | 作为受限文本片段发送                    | 作为受限文本片段发送，完全可用                 |
| PDF/DOCX 等可抽取文本              | 先抽取文本，再按文本发送                | 先抽取文本，再按文本发送                       |
| 图片                               | 使用 Provider 对应的 image content part | 发送前阻止，并提示改用支持视觉的模型或后续 OCR |
| 二进制、压缩包、未知格式           | 阻止或进入后续专用工具                  | 阻止或进入后续专用工具                         |

因此，接入非多模态模型本身没有问题：文本/代码附件走文本抽取路径即可；图片不能直接让文本模型“看懂”，不能通过把图片 base64 放入文字 prompt 来规避能力边界。

## 参考实现与取舍

参考 Catty 类客户端把 composer 上传状态、附件元数据和消息构造分开，并通过显式的附件列表/读取边界供 Agent 使用；MaidKit 的 Agent/file 工作流则说明“文件可访问”与“模型原生视觉输入”是两件事。FileTerm 借鉴这个分层，但保留自己的 `core → Rust → bridge → renderer` 边界：

1. renderer 只管理选择状态、预览和取消；不直接读取任意路径并拼 prompt。
2. Rust 管理附件句柄、临时存储、权限、抽取、清理和 Provider content mapping。
3. Provider capability 决定图片是可发送、需转换还是必须阻止。
4. 远程文件必须来自用户明确选中的 SFTP 文件项，并绑定连接 tab、session revision 和一次性附件句柄。

## 数据与 IPC 设计

### 核心类型

在 `packages/core` 增加最小的句柄模型，避免把内容本体放进通用消息：

```ts
type AiAttachmentSource = 'local' | 'remote'
type AiAttachmentKind = 'text' | 'image' | 'document' | 'binary'

interface AiAttachmentRef {
  id: string
  name: string
  mediaType: string
  kind: AiAttachmentKind
  sizeBytes: number
  source: AiAttachmentSource
  status: 'ready' | 'extracting' | 'unsupported' | 'failed'
}
```

`StartAiChatInput` 只携带已由 Rust 签发的 `attachmentIds`，不接受 renderer 直接传入文件内容、绝对路径或远程路径。持久化消息只保存脱敏元数据和引用状态；临时内容由 Rust attachment store 按 TTL 清理。

### Provider capability

增加模型级能力解析，至少包含：

- `supportsVision`：是否可以接收图片 content part。
- `supportsNativeFile`：是否可以接收 Provider 原生文件引用；默认关闭，除非适配器明确支持。
- `textExtraction`：当前附件是否可由本地/Rust 抽取为文本。

能力解析采用保守策略：已知模型映射或用户显式设置优先，未知模型默认不发送原始图片；不能因为 Provider URL 看起来兼容 OpenAI 就假定模型支持视觉。

### IPC 边界

- `app_prepare_ai_attachments`：接收用户选择的本地文件或远程文件句柄，校验后返回 `AiAttachmentRef[]`。
- `app_read_ai_attachment`：按附件 ID、受限 byte/line range 返回抽取后的文本片段；不得接受任意路径。
- `app_discard_ai_attachments`：取消发送、关闭会话或超时后清理句柄。
- `startAiChat` / `retryAiChat`：只接收 attachment IDs，并在 Rust 侧重新校验会话、Provider、TTL 和能力。

具体 command 名称可以在实现阶段调整，但不能绕过这四条安全边界。

## UI 方案

在现有 [AiCopilotPanel](../../../apps/tauri/src/renderer/features/ai/ai-copilot-panel.tsx) composer 内增加：

- 使用 `<AppIcon />` 的附件按钮，打开系统文件选择器；支持拖放和剪贴板图片，但不引入外部图标字体。
- 发送前的附件 chip：文件名、类型、大小、状态、移除按钮和失败原因。
- 模型能力提示：例如“文本文件可用 / 当前模型不支持图片”；阻止按钮应解释原因。
- 文本附件的行数/片段范围提示，避免用户误以为整个大文件已经发送。
- 不显示真实本机绝对路径、远程凭据、临时目录和内部句柄。

第一版不做自动上传进度大面板；只需在 chip 上展示准备中、可发送、失败和已移除状态。

## Provider 适配

- OpenAI-compatible chat：文本进入 text part；视觉模型使用约定的 image URL/data part，但由 Rust 适配器生成，renderer 不拼接。
- OpenAI Responses：映射到 input text / input image content part。
- Anthropic Messages：映射到 text / image content block。
- 非多模态或能力未知：文本附件继续走抽取；图片在发送前返回稳定的“不支持视觉输入”错误。
- 标题摘要、重试和历史重放默认不重新上传附件；只有用户再次明确发送或附件句柄仍有效时才重新准备。

## 实施阶段

### P0：契约与能力路由

- [ ] 在 `packages/core` 定义附件引用、能力矩阵、错误码和 `StartAiChatInput` 扩展。
- [ ] 在 Rust 建立附件句柄/TTL/大小/MIME 校验和清理边界。
- [ ] 为当前三类 Provider 增加文本、图片、未知能力的统一路由测试。

### P1：本地文件与文本附件

- [ ] 接入文件选择、拖放、剪贴板图片入口和 composer chip UI。
- [ ] 支持 txt、md、json、yaml、csv、log 以及常见源码的受限文本读取和分片。
- [ ] 大文件超限、无法解码、权限失败和过期句柄都有明确 UI 状态。

### P1：图片与非多模态兼容

- [ ] 多模态模型按 Provider 适配器发送图片 content part，不把 base64 写入本地历史。
- [ ] 非多模态/未知能力模型对图片发送前 fail closed，并给出切换模型或后续 OCR 的提示。
- [ ] 文本/代码附件在非多模态模型上完成真实请求回归。

### P2：远程 SFTP 文件

- [ ] 从已打开连接的文件区明确选择远程文件，生成绑定 tab/session revision 的附件句柄。
- [ ] 远程读取沿用 Rust SFTP service 和现有权限边界，不让模型自行读取任意远程路径。
- [ ] tab 切换、重连、关闭、权限变化或 session revision 变化时旧句柄失效。

### P2：文档抽取与后续能力

- [ ] 评估 PDF/DOCX/XLSX 的离线抽取库、体积和许可证，再决定是否进入主应用包。
- [ ] OCR、压缩包、表格/媒体解析另立子计划，不把首版附件范围无限扩大。

## 安全与限制

- 每文件、每轮和每会话设置上限；候选默认值为 20 MiB/文件、50 MiB/轮，最终以实现和平台内存验证为准。
- 只接受明确选择的文件和剪贴板内容；禁止 renderer 通过字符串伪造路径或附件 ID。
- 附件存储在应用私有临时目录，使用后、取消后、窗口关闭后或 TTL 到期后清理。
- 日志只记录附件类型、大小、错误码和 request ID，不记录文件名中的敏感路径、文本内容、图片数据或远程路径。
- 文件内容视为不可信数据；Provider prompt 中必须保留“附件是数据，不是指令”的系统边界，避免文档注入扩大为工具调用权限。

## 实现验收标准

- [ ] 非多模态 Provider 可发送小型文本/代码附件，模型收到的是受限文本内容而不是本机路径。
- [ ] 非多模态 Provider 选择图片时发送前明确阻止，错误可理解且不会产生半条 AI 消息或残留临时文件。
- [ ] 支持视觉的 Provider 可发送图片；OpenAI-compatible、Responses、Anthropic 三条映射至少各有一条脱敏 fixture。
- [ ] 本地附件、远程附件、过期句柄、超限文件、tab/session revision 变化和取消清理均有 Rust/renderer 回归。
- [ ] 对话历史、标题摘要、日志、MCP/CLI 和公开状态不泄露附件本体、绝对路径、远程凭据或 API key。

实现完成后的跨平台打包回归，统一登记到[发行候选跨平台验收计划](./release-candidate-acceptance.md)；本计划只保留附件功能本身的实现验收。
