# AI Copilot 跨平台发行验收

本清单记录 AI Copilot 不能由当前开发机或纯 Rust fixture 伪造的发行前验证。勾选前需保存平台、FileTerm commit、Provider 协议族/版本、网络环境和失败日志。现有 GitHub Actions 的 macOS、Windows、Linux 生命周期矩阵仍是基础回归，但不等同于打包桌面应用的 AI 验收。

## 准备

- 使用打包后的 Tauri 应用，而非仅运行 `cargo test` 或开发服务器。
- 在三个平台分别配置一个可控的 OpenAI-compatible Provider；如使用本地 mock，必须确认 Base URL、模型名和 API Key 均为测试值。
- 为代理与断网用例准备可开关的 HTTP CONNECT 或 SOCKS5 代理，并确保不会在测试日志中保存真实 API Key 或终端输出。
- 准备一个非敏感 SSH 测试目标和可重复的短命令；不要在生产主机上测试 Review Mode 或上下文上传。

### 可控本地 Provider（推荐）

仓库内提供仅监听 loopback 的 OpenAI-compatible fixture，用于把 L0 流式、停止、断连重试、usage 和命令卡的验收变成可重复操作：

```bash
npm run qa:ai-copilot-fixture
```

在打包应用的 AI Provider 设置中填写以下 **测试专用** 值：

| 字段            | 值                         |
| --------------- | -------------------------- |
| Provider 类型   | `OpenAI-compatible Chat`   |
| Base URL        | `http://127.0.0.1:9419/v1` |
| 模型            | `fileterm-fixture`         |
| API Key         | `fileterm-fixture-key`     |
| 允许不安全 HTTP | 开启                       |
| 无 API Key      | 关闭                       |

fixture 只记录请求模式和长度，绝不记录 prompt 或 `Authorization` 内容。可用下列消息触发确定性行为：

- `fixture:hello`：普通 SSE 回答和 usage。
- `fixture:slow`：持续流式输出，便于验证“停止”与关闭面板/窗口后的取消。
- `fixture:fail-once`：首个请求返回 HTTP 503；对同一消息点击“重试”后成功。
- `fixture:disconnect-once`：首个请求在首个 SSE chunk 后断开；对同一消息重试后成功。
- 在已授权 L1/L2 上下文并打开“命令建议”时发送 `fixture:command` 或 `fixture:multiline`：分别返回只读 `pwd` 卡和多行卡。

该 fixture 故意绑定 `127.0.0.1`，而应用会对 loopback Provider 禁用系统代理，避免本机 API Key 被意外转发。因此它**不能**替代 HTTP CONNECT / SOCKS5 验收；代理项仍须使用一个受控的非 loopback Provider 或相应测试网络。

## 每个平台：macOS、Windows、Linux

- [ ] Provider 配置保存后重新打开设置：只显示 `hasApiKey`，不得回填 Key；默认 Provider、禁用和删除状态正确。
- [ ] L0 聊天连续发送两条消息，确认 Provider/model、usage、错误重试和本地历史搜索、重命名、删除均可用。
- [ ] 流式输出中点击“停止”，确认请求停止、对话可继续且没有残留忙碌状态；随后关闭 AI 面板或整个窗口，确认不会崩溃或继续向已关闭窗口写事件。
- [ ] 经 HTTP CONNECT 或 SOCKS5 代理完成一轮流式聊天；停止代理后确认出现可重试连接错误，恢复网络后重试成功。
- [ ] 设备睡眠后唤醒，再发送一条消息；确认明确失败并可重试，或直接恢复，不得静默卡在生成中。
- [ ] 在 SSH tab 预览 L1/L2 上下文后切换 tab、分屏、CWD 或身份；旧预览和旧命令卡必须显示“终端目标已变化”，且不能写入输入框。
- [ ] 命令卡只能复制或写入受控输入框，写入后不自动回车；多行、危险或已过期目标的命令不得一键写入。
- [ ] 对非敏感 SSH 测试命令点击“审核并运行”：确认框展示 host、CWD、完整命令、风险和超时；拒绝、关闭或超时都不启动远端 exec。批准后验证执行不写入交互式 PTY、结果/退出码/超时/截断显示为本地审核记录，且正在审核的对话不能被删除。

## 记录格式

每个平台至少附一条通过记录，包含：

```text
platform: macOS 15 / Windows 11 / Ubuntu 24.04
fileterm commit: <commit>
provider: <protocol + non-sensitive endpoint label>
network: direct / HTTP CONNECT / SOCKS5 / offline recovery
result: pass | fail
notes: <stream cancel, sleep recovery, close behavior, retry result>
```

任何失败都应保留脱敏日志，并在修复后重新跑该平台对应条目；不要用另一个平台的成功结果替代。

## 已执行记录（未完成发行签收）

### 2026-08-10 — macOS 本地包，部分通过

```text
platform: macOS 27.0 / arm64
fileterm commit: 86821841
provider: OpenAI-compatible Chat / loopback QA fixture
network: direct loopback（不经过系统代理）
result: pass（L0 + local L1 子集）
notes:
  - 用 npm run build -w @fileterm/tauri 生成 FileTerm.app，并使用隔离 HOME 启动，未触及日常应用数据。
  - Provider 保存后可被重启后的包体读取；Key 只显示“已保存”状态，不回填到设置表单。
  - L0 连续消息、usage、本地历史、503 错误重试均正常。
  - fixture:slow 的“停止生成”与关闭 AI 面板都会中断 SSE；服务端没有记录 stream-completed，面板可立刻继续发送且不显示连接失败。
  - 本地终端的 L1 元数据预览可生成；fixture:command 只产生可审查的 pwd 命令卡，本地目标的写入和审核动作保持禁用。
```

这不是完整发行签收：该产物由本机普通 build 生成，不是 release 配置下的签名/公证包；Windows、Linux、代理、睡眠恢复、SSH 目标变化和 SSH Review Mode 仍必须按上方清单在对应环境完成。

### 2026-08-10 — macOS 本地包，SSH host key 与 Review Mode

```text
platform: macOS 27.0 / arm64
source: 86821841 之后的 SSH host-key 修补工作树
target: disposable local Docker sshd (127.0.0.1:2222)
provider: OpenAI-compatible Chat / loopback QA fixture
result: pass（SSH host key + L1 + Review Mode）
notes:
  - 空 trustedHostFingerprint 会按“尚未信任”处理，不会被误判为已保存指纹不匹配。
  - 首次 host key 弹窗出现后，人工等待超过 35 秒再选择“只接受本次”，SSH 仍能成功认证；网络握手 30 秒与人工确认 300 秒分别计时。
  - 会话元数据预览、结构化 pwd 命令卡和风险标识均正常生成。
  - “写入当前终端”只留下 pwd 输入，不自动回车、没有产生交互终端输出。
  - “审核并运行”先显示目标、工作目录、风险、30 秒超时与完整命令；确认后经独立 SSH exec 通道执行一次，审计记录输出 /home/filetermqa。
```

本条仅覆盖本机普通 build + 一次性 localhost 容器，不等同于签名/公证发行包验证；Windows、Linux、代理、睡眠恢复、SSH 目标变更与真实远端环境仍需按清单验收。
