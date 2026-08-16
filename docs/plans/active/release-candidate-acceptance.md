# FileTerm 发行候选跨平台验收计划

状态：进行中（功能实现与自动化回归已完成；待 macOS、Windows、Linux 打包应用和真实环境验收）

创建日期：2026-08-16

## 目标

把已经完成实现、只剩发行候选和真实环境验收的事项集中到一个入口，避免每个功能计划重复维护平台清单。这个计划只负责验收和证据收集；发现代码缺陷时，另开修复任务，不在本计划中直接扩展功能范围。

## 来源计划

以下计划的实现部分已经归档，原有的剩余验收范围统一转移到本页：

| 来源计划                                                                         | 转移的验收范围                                                                                         |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| [本地终端跨平台兼容性与稳定性适配](../completed/local-terminal-compatibility.md) | 三平台打包应用中的默认 shell、Claude/Codex 启动、输入法、复制粘贴、字体、快捷键、resize 和独立分屏 PTY |
| [本地终端与 Agent MCP 接入](../completed/local-terminal-mcp.md)                  | `fileterm mcp`、已打开 SSH 会话可见性、sudo/su 凭据边界、MFA/确认/REPL 的交互输入错误边界              |
| [WebDAV/S3 远程备份加密计划](../completed/remote-backup-encryption.md)           | v3/v2 兼容、请求体脱敏、一次性密码交互、主动 JSON 导出兼容和三平台恢复流程                             |
| [本机凭据字段加密](../completed/secret-storage-encryption.md)                    | Windows/Linux 设备标识、三平台实际包中的旧明文迁移、重启读取和公开 bridge 脱敏                         |

## 验收原则

- 以实际打包产物为准；CI 中“能生成包”不能替代打开包后的功能验收。
- 使用隔离的测试 profile、临时目录、非生产 SSH/WebDAV/S3 目标和测试凭据；截图、日志和 issue 必须脱敏。
- 任何密码、API key、私钥口令、备份主密码、终端输出和文件内容都不得进入提交记录或普通验收日志。
- `N/A` 只能用于平台确实不支持或当前没有对应外设/服务的项目，并在备注中说明原因；不能用 `N/A` 代替未执行。

## 1. 自动化质量门禁

在开始手工验收前，由同一提交执行并保存输出：

- [ ] `npm run typecheck -w @fileterm/tauri`
- [ ] `npm run lint`
- [ ] `npx prettier --check apps/tauri packages/core packages/shared packages/storage`
- [ ] `npm run test:tauri`
- [ ] `cargo clippy --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings`
- [ ] `npm run qa:ai-copilot-fixture-smoke`
- [ ] `npm run qa:mcp-stdio-smoke`
- [ ] CI 已产出 macOS、Windows、Linux 无签名 Tauri 包，并保存对应 job、commit 和 artifact 链接。

## 2. 本地终端与 MCP 验收

在每个实际平台包中使用独立测试数据完成：

- [ ] 默认 shell 可以启动；shell 缺失时有明确错误或按平台 fallback，不出现空白终端。
- [ ] UTF-8 中文/emoji、输入法、复制粘贴、字体、快捷键和 PTY resize 正常。
- [ ] Claude Code / Codex CLI 能在可用的平台环境中启动；若平台没有安装对应客户端，记录 `N/A` 和原因，不伪造通过。
- [ ] 本地终端分屏后每个 pane 拥有独立 PTY；关闭或重启一个 pane 不影响其他 pane 的输出、输入和进程。
- [ ] 保持一个非敏感 SSH tab 已连接，`fileterm mcp` 可以初始化、列出工具并看到该会话；MCP/CLI 不返回凭据或可见 terminal transcript。
- [ ] 普通远程 exec 使用独立 channel，不混入当前可见 SSH tab；切换 tab、分屏、CWD、登录用户、重连或关闭窗口后，旧 tool call fail closed。
- [ ] sudo/su 分别验证加密 profile、主窗口安全输入和明确的一次性字段；密码不进入命令文本、tool result、日志或截图。
- [ ] MFA、验证码、确认、安装器和 REPL 返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`，不创建隐藏 PTY，也不污染可见终端。

Copilot 的 Provider、模式、审批和重试细项沿用[跨平台发行验收清单](../../quality/ai-copilot-platform-regression.md)，不在本页复制 Provider 矩阵。

## 3. 远程备份加密验收

在 WebDAV 和已配置的 S3 测试端点分别验证；若某个端点在当前发行候选没有可用测试服务，记录阻塞原因和后续执行环境：

- [ ] v3 加密包可上传、下载和往返解密；错误密码、篡改 hash、坏密文和未知版本都会拒绝导入且不改动本地连接。
- [ ] Rust 回归覆盖 v3 往返、错误密码、密文不含 profile secret、v2 导入和 PBKDF2 兼容参数。
- [ ] 通过请求体 spy 或隔离服务确认上传 body 不出现 profile password、私钥口令、代理密码或 `profiles` 明文字段。
- [ ] v3 下载显示一次性密码框；取消、超时、窗口卸载和恢复流程都能收敛 pending 请求；v2/旧明文包可以导入并显示升级提示。
- [ ] 用户主动导出 JSON 的格式、文件路径和命令行为保持不变，不被远程备份加密流程改写。

## 4. 本机凭据字段加密验收

使用临时应用数据目录和专门的 legacy fixture，不操作真实用户凭据：

- [ ] 在 macOS、Windows、Linux 打包应用中验证设备标识读取、seed/密钥权限和字段加解密往返。
- [ ] 启动时读取旧版明文 fixture 后，保存一次并确认文件已迁移为 `ftsec:v1:` 密文；磁盘文件、日志和公开 bridge 不出现凭据明文。
- [ ] 重启应用后原凭据仍可用；公开 workspace/provider/profile snapshot 只返回 `has*` 标记，不回填 secret。
- [ ] scope 篡改、密文复制到其他 profile/字段、seed 缺失或设备标识不匹配时 fail closed，原文件不被覆盖。

## 5. 三平台证据矩阵

每个平台至少记录一个实际包和一组脱敏测试数据。结果列只允许 `pass`、`fail` 或带原因的 `N/A`。

| 平台    | 发行产物             | 应用版本/提交 | 终端与 MCP | 备份加密 | 本机凭据 | 证据位置 |
| ------- | -------------------- | ------------- | ---------- | -------- | -------- | -------- |
| macOS   | `.app` / `.dmg`      |               |            |          |          |          |
| Windows | NSIS / `.msi`        |               |            |          |          |          |
| Linux   | `.deb` / `.AppImage` |               |            |          |          |          |

证据记录模板：

```text
platform: macOS 15 / Windows 11 / Ubuntu 24.04
fileterm commit: <commit>
version: <version>
artifact: <signed-or-unsigned artifact label>
provider/target: <non-sensitive label>
result: pass | fail | N/A
notes: <mode, approval, credential, migration, retry and recovery result>
evidence: <redacted screenshot/log/CI URL>
```

相关质量清单：[AI Copilot 跨平台发行验收](../../quality/ai-copilot-platform-regression.md)、[桌面 UI 回归清单](../../quality/desktop-ui-regression-checklist.md)、[连接协议本机验证](../../quality/connection-protocol-local-testing.md)、[Tauri 发行候选协议验收清单](../../quality/tauri-rc-protocol-checklist.md)。

## 6. 退出条件

- [ ] 自动化门禁全部通过，或每个例外都有独立 issue、风险说明和明确的发布决策。
- [ ] macOS、Windows、Linux 的实际包都有结果和脱敏证据；不能用单一平台结果替代其他平台。
- [ ] 终端/MCP、远程备份和本机凭据三组验收均达到发布决定要求，没有未解释的 `fail`。
- [ ] 失败项已经创建独立修复任务；修复后只重新执行受影响的矩阵项，并补回证据。

全部退出条件满足后，将本计划移动到 `docs/plans/completed/`，并在 `docs/plans/README.md` 和发布记录中保留归档链接。
