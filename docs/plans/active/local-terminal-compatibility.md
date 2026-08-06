# 本地终端跨平台兼容性与稳定性适配

## 目标

让 FileTerm 的本地终端稳定运行普通 shell、Claude Code、Codex CLI 等交互式 TUI，并在 macOS、Windows、Linux 上保持统一的 PTY、输入、resize、输出和关闭语义。

本计划只增强本地 PTY 链路，不改变现有的 CLI/MCP 边界：Agent 继续在本地终端内运行，通过 FileTerm MCP 操作已经打开的远程连接；不新增通过 MCP 注入本地 PTY 按键的接口。

## 实施顺序

### P0：数据与生命周期正确性

- [x] 采用跨 read 的 UTF-8 流式解码，避免中文、emoji 和 ANSI 数据在字节边界处损坏。
- [x] 将 local PTY runtime generation 传入输出路径，旧 shell 的迟到输出不能写入重连后的新 tab；旧代际清理按 runtime id 校验。
- [x] 退出时先完成 reader/output drain，再发送一次 Closed 状态；记录退出码或 signal，并对卡住的输出 drain 设置上限。
- [x] 关闭和重连时清理完整的本地进程树：POSIX session/process group，Windows Job Object。

### P0：启动环境兼容性

- [x] POSIX 默认 shell 支持 login shell，兼容 macOS GUI 启动时缺失的 Homebrew、npm、cargo、nvm PATH。
- [x] 统一设置 `TERM`、`COLORTERM` 和可用的 UTF-8 locale，同时保留用户显式环境。
- [x] Windows 支持 PowerShell/pwsh UTF-8 初始化、cmd UTF-8 code page，并为 Git Bash 等 POSIX shell 保留 login 参数入口。
- [x] shell/CWD 不可用时给出明确错误，不让终端表现为空白。

### P1：输出与尺寸稳定性

- [x] 本地 PTY 使用独立 bounded output pump，不能因为 Tauri Channel 或 WebView 变慢而阻塞输入和 Ctrl+C；队列饱和只限频记录并丢弃输出帧。
- [x] 为批量输出设置 16ms 时间窗口和 32KiB 单批硬上限，并在丢帧恢复时保留可见的 resync/diagnostic 语义。
- [x] renderer 负责实际网格的首次 resize，后端对重复尺寸去重，并避免关闭后的迟到 resize。

### P1：产品化与回归

- [x] 为本地 tab 增加 shell、CWD、启动参数和环境变量的可扩展模型；默认打开路径保持不变，环境覆盖不进入工作区快照；同一 tab 重连时保留启动配置，关闭 tab 后清理。
- [x] 解析本地 PTY 的 OSC 7 CWD 并更新 `shellCwd`；复用 follow 开关更新本地 session 元数据，等本地文件面板启用后再消费该路径。
- [x] 增加 UTF-8 中文边界、shell 启动参数、退出码、resize 去重、高输出丢帧提示等逻辑回归测试。
- [x] 在真实 PTY 中补充 UTF-8 输出、shell 退出码和进程组终止集成测试。
- [x] 在真实 PTY 中补充重启/重连、Ctrl+C 输入路径集成测试；CI 的 macOS、Windows、Linux 矩阵都会运行本地 PTY 测试集。
- [ ] 在 macOS、Windows、Linux 打包产物中验收默认 shell、Claude/Codex 启动、复制粘贴、字体和快捷键。

## 当前不做

- 不把本地终端伪装成远程 ConnectionProfile。
- 不通过 CLI/MCP 读取或注入本地终端 transcript/input。

## 验收命令

```bash
npm run typecheck -w @fileterm/tauri
npm run lint
npx prettier --check apps/tauri packages/core packages/shared packages/storage
npm run test:tauri
cargo clippy --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
```

## 本地终端启动参数示例

桌面 bridge 支持按 tab 传入一次性的启动覆盖；不传参数时仍使用平台默认 shell：

```ts
await window.fileterm.openLocalTerminal({
  shell: '/bin/zsh',
  cwd: '/Users/stoffel/CodeFile/fileterm',
  args: ['-i'],
  env: { FILETERM_WORKSPACE: 'fileterm' }
})
```

`env` 只作用于新建的本地 PTY，不写入 profile，也不会出现在 workspace snapshot 中。

Windows/Linux/macOS 的打包产物验收仍需要对应实体机或 CI runner；当前开发机是 macOS，Windows 交叉检查还会被本机缺少 MSVC C 工具链阻断。
