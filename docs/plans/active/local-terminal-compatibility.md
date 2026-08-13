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
- [x] 为批量输出设置 16ms 时间窗口和 32KiB 单批硬上限，并在丢帧恢复时保留可见的 resync/diagnostic 语义。丢帧时使用跨 read 的状态机扫描被丢数据里的 DECSET/DECRST alt screen 切换序列（`\x1b[?47h/l`、`1047h/l`、`1049h/l`），在下一次恢复发送的帧里标注「终端状态可能不一致，建议 `reset` 或 Ctrl+L 重新同步」（参考 Netcatty `ptyOutputBuffer` 的 `droppedOutputMayAffectTerminalState` 语义）。
- [x] renderer 负责实际网格的首次 resize，后端对重复尺寸去重，并避免关闭后的迟到 resize。

### P1：产品化与回归

- [x] 为本地 tab 增加 shell、CWD、启动参数和环境变量的可扩展模型；默认打开路径保持不变，环境覆盖不进入工作区快照；同一 tab 重连时保留启动配置，关闭 tab 后清理。
- [x] 解析本地 PTY 的 OSC 7 CWD 并更新 `shellCwd`；bash 通过 `PROMPT_COMMAND` 自动注入 OSC 7 emit，zsh 在没有自定义 prompt 时通过 `promptsubst` 注入（均不覆盖用户显式设置），fish/sh 依赖用户 rc（可检测 `TERM_PROGRAM=FileTerm`）；复用 follow 开关更新本地 session 元数据，等本地文件面板启用后再消费该路径。
- [x] 增加 UTF-8 中文边界、shell 启动参数、退出码、resize 去重、高输出丢帧提示等逻辑回归测试。
- [x] 在真实 PTY 中补充 UTF-8 输出、shell 退出码和进程组终止集成测试。
- [x] 在真实 PTY 中补充重启/重连、Ctrl+C 输入路径集成测试；CI 的 macOS、Windows、Linux 矩阵都会运行本地 PTY 测试集。
- [ ] 在 macOS、Windows、Linux 打包产物中验收默认 shell、Claude/Codex 启动、复制粘贴、字体、快捷键和本地终端分屏；拆分出的每个 pane 必须拥有独立 PTY，关闭或重启一个 pane 不得影响其余 pane。
- [x] PR CI 在 macOS、Windows、Linux 生成无签名 Tauri 包并检查对应产物（`.app/.dmg`、NSIS installer、`.deb/.AppImage`）；默认 shell、输入法、字体、快捷键和真实客户端仍需打包应用手工验收。

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

## 实现说明与已知限制

### OSC 7 CWD 注入

- **bash**：在 POSIX `configure_shell_command` 里通过 `PROMPT_COMMAND` 环境变量注入：`printf '\033]7;file://%s\007' "${PWD//%/%25}"`，每次显示 prompt 前执行。字面量 `%` 会先编码，避免目录名中的 `%20` 被后端误解为一个空格。
  - 用户显式传入 `PROMPT_COMMAND`（通过 `LocalTerminalLaunchOptions.env`）时不覆盖。
  - 父进程已有 `PROMPT_COMMAND` 时 prepend 我们的 hook，保留用户原有逻辑。
- **已知限制**：用户 `.bashrc`、starship 或 oh-my-bash 如果在启动过程中重新赋值 `PROMPT_COMMAND`，环境注入的 hook 可能被覆盖；本地终端不会用 DEBUG trap 强行覆盖用户配置，因为它会在每条命令前产生额外输出并放大丢帧压力。需要强制覆盖时，应由用户在自己的 rc 中检测 `TERM_PROGRAM=FileTerm` 后追加 hook。
- **zsh**：没有自定义 `PROMPT` / `PS1` 时，开启 `promptsubst`，注入保持默认 `%m%#` 样式的 prompt；每次显示 prompt 时通过 `printf` 和 `${PWD//%/%25}` 输出经过编码的 OSC 7 绝对路径。如果 `.zshrc`、主题或用户环境覆盖 prompt，则在 rc 中检测 `TERM_PROGRAM=FileTerm` 后用 `add-zsh-hook` 追加 hook：
  ```zsh
  # ~/.zshrc
  if [ "$TERM_PROGRAM" = "FileTerm" ]; then
    autoload -Uz add-zsh-hook
    fileterm_precmd() { printf '\033]7;file://%s\007' "${PWD//%/%25}" }
    add-zsh-hook precmd fileterm_precmd
  fi
  ```
- **fish / sh / dash**：不自动注入，用户可在 `config.fish` 或对应 rc 中检测 `TERM_PROGRAM=FileTerm` 并加同等 hook。
- **Windows PowerShell**：默认 `prompt` 函数不 emit OSC 7。用户可在 `$PROFILE` 里覆盖 `prompt`，或后续单独评估自动注入。

### Windows 默认 shell fallback

`default_shell()` 在 Windows 上按 `powershell.exe` → `pwsh.exe` → `cmd.exe` 顺序查找。`shell_available_in_path` 先查 `PATH`，PATH 异常（如被清理过的服务进程）时查 `%SystemRoot%\System32`，并额外检查 PowerShell 7 的标准安装目录 `%ProgramFiles%\PowerShell\7` / `7-preview`。Server Core / 精简镜像缺 PowerShell 时自动回退 `cmd.exe`。

### PowerShell `-Command` 互斥参数检测

`configure_shell_command` 在 Windows PowerShell 分支检测 `extra_args` 是否包含 `-Command` / `-CommandWithArgs` / `-File` / `-EncodedCommand`、官方短写 `-c` / `-cwa` / `-f` / `-e` / `-ec`，或对应的参数前缀缩写。`-ConfigurationFile` / `-ConfigurationName` 只是会话配置参数，不会阻止 UTF-8 setup；命中真正的命令模式时才不再追加 `-Command`，避免 PowerShell 因参数互斥直接报错；用户需在自己的脚本/命令里设置 UTF-8 编码。`cmd.exe` 传入 `/C` 或 `/K` 时也不会再追加 FileTerm 自己的 `/K chcp`，避免覆盖用户命令模式。

### 进程组终止覆盖孙进程

`local_process_tree_terminates_grandchild_process` 测试验证 shell 派生的后台 `sleep`（grandchild）在 `LocalProcessTree::terminate` 后被一起收掉，回归会显式失败而不是只断言直接子 shell 退出。

### 丢帧时的 alt screen 状态检测

`AltScreenTransitionScanner` 在每个 PTY reader chunk 上运行，并保留未完成的 CSI 状态，因此 `ESC [`、参数和 `h/l` 即使跨 read 或跨“正常发送/丢帧”边界也能识别。它检测 DECSET/DECRST 风格的 alternate screen 切换序列（`\x1b[?47h/l`、`1047h/l`、`1049h/l`，含组合模式如 `\x1b[?1;1049h`）。丢帧期间命中时设置 `LocalOutputDropState.saw_alt_screen_change`，在下一次成功发送的 `LocalOutputChunk.dropped_alt_screen_change` 里携带。`append_local_output_chunk` 在丢帧提示后追加一行「dropped output may include alternate screen transitions; terminal state may be inconsistent — run `reset` or Ctrl+L to resync」。这对 vim/less/nano 等 TUI 程序在输出爆发时丢帧的恢复很重要：如果丢了 enter/leave alt screen 序列，renderer 的终端网格状态会跟实际不一致。

## 与 Netcatty 的对比

参考 [binaricat/Netcatty](https://github.com/binaricat/Netcatty)（Electron + React + node-pty）的本地 PTY 实现，对比 FileTerm 当前方案：

| 维度                 | Netcatty                                                                                | FileTerm                                                                             | 评估                                            |
| -------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ----------------------------------------------- |
| 丢帧 alt screen 检测 | `ptyOutputBuffer` 扫描 alt screen 序列，meta 标记 `droppedOutputMayAffectTerminalState` | `AltScreenTransitionScanner` 扫描，`dropped_alt_screen_change` 标记，文本提示 resync | 已对齐                                          |
| 进程树终止           | `ptyProcessTree` 先 `ps`/`@vscode/windows-process-tree` list 再 kill（异步，有竞态）    | POSIX `kill(-pgid)` 进程组 / Windows Job Object（原子，无竞态）                      | FileTerm 更优                                   |
| 紧急输入             | `terminalUrgentInputChannel` 独立 MessagePort                                           | 输入与输出分离，输入直接 write PTY master                                            | 已对齐                                          |
| OSC 7 解析           | 前端 xterm.js 解析                                                                      | 后端 `LocalOsc7CwdTracker`，BEL/ST 双终止符 + percent-decode + 跨 chunk + 16KB 上限  | FileTerm 更健壮                                 |
| Shell 发现           | `shellDiscovery.cjs` 跨平台发现（/etc/shells、Windows 注册表）                          | `default_shell()` 仅返回默认 shell                                                   | FileTerm 暂不做（产品功能，非稳定性）           |
| flush 策略           | `setImmediate` 事件循环回合 + 软上限切换短定时器                                        | 16ms 时间窗口 + 32KiB 硬上限                                                         | 可接受（Tauri Channel 同步 send，无跨进程开销） |
| session generation   | `sessionOutputGenerations` + tombstone TTL 60s                                          | `LocalTerminalRuntimeGate` runtime_id + gate 双校验                                  | 已对齐                                          |

**不需要借鉴的**：

- 进程树 list-then-kill：竞态窗口比进程组方案大。
- `setImmediate` flush：Tauri Channel 不像 Electron MessagePort 有跨进程开销，16ms 延迟对交互式回显可接受。

**暂不做的**：

- Shell 发现（`shellDiscovery`）：当前 plan 不要求 UI shell 选择器，用户可通过 `openLocalTerminal({ shell })` 传入。未来本地终端配置面板可考虑。
