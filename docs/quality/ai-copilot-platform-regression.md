# AI Copilot 跨平台发行验收

本清单记录 AI Copilot 不能由当前开发机或纯 Rust fixture 伪造的发行前验证。勾选前需保存平台、FileTerm commit、Provider 协议族/版本、网络环境和失败日志。现有 GitHub Actions 的 macOS、Windows、Linux 生命周期矩阵仍是基础回归，但不等同于打包桌面应用的 AI 验收。

## 准备

- 使用打包后的 Tauri 应用，而非仅运行 `cargo test` 或开发服务器。
- 在三个平台分别配置一个可控的 OpenAI-compatible Provider；如使用本地 mock，必须确认 Base URL、模型名和 API Key 均为测试值。
- 为代理与断网用例准备可开关的 HTTP CONNECT 或 SOCKS5 代理，并确保不会在测试日志中保存真实 API Key 或终端输出。
- 准备一个非敏感 SSH 测试目标和可重复的短命令；不要在生产主机上测试 Review Mode 或上下文上传。

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
