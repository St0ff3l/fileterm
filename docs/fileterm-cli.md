---
name: fileterm-cli
description: Use FileTerm's one-shot CLI, persistent CLI JSONL bridge, or MCP bridge from an external AI Agent while the FileTerm desktop app is running. Read this Skill whenever an AI Agent or automation needs to discover connections, execute remote actions, transfer files, or use the FileTerm bridge safely.
---

# FileTerm CLI Skill

本文档是 FileTerm 的用户侧接入说明，适用于 2.2.7 及之后的版本。命令的完整参数以当前安装版本的 `--help` 输出为准。

## 选择调用方式

| 使用场景                  | 启动方式                 | 进程行为                                      |
| ------------------------- | ------------------------ | --------------------------------------------- |
| 外部 AI Agent 默认接入    | `fileterm cli --jsonl`   | 启动一次，保持 stdin/stdout，连续处理多个请求 |
| 支持 MCP 的客户端         | `fileterm mcp`           | 启动一次，保持 MCP stdio 会话                 |
| 用户手动操作或 shell 脚本 | `fileterm cli <command>` | 每次调用一个短生命周期 CLI 进程               |

不存在单独的 `fileterm agent` 命令。AI Agent 使用 `fileterm cli --jsonl`，不要为每个动作重新启动一次性 CLI。

FileTerm 桌面应用需要先运行。CLI 和 MCP 是连接到正在运行的 FileTerm 的本机桥接客户端，不是独立的 SSH 客户端，也不会导出连接凭据或自动修改外部客户端配置。CLI 参数会在 Tauri GUI 初始化前处理，不会因为 CLI 调用额外打开 FileTerm 窗口；一次性 CLI 仍然会创建自己的短生命周期操作系统进程，只有 JSONL 模式复用同一个 CLI 进程。

CLI/MCP 打开的连接仍由 FileTerm App 持有 SSH/SFTP worker。后台模式不会把连接放进顶部标签栏，而是显示在 GUI 的“后台会话”页面；列表中的会话 ID 就是后续 CLI/MCP 请求使用的 `tabId`。点击“打开会话”或调用 `fileterm_activate_session` 会复用原 worker，把它挂回正常的可见标签，不会重新建立连接。

## AI Agent：持久在线 JSONL

启动一次：

```text
fileterm cli --jsonl
```

之后向 stdin 每行发送一个 JSON 对象，并从 stdout 逐行读取结果：

```json
{"id":"request-1","action":"list_connections","params":{}}
{"id":"request-2","action":"get_session_context","params":{"profile_id":"PROFILE_ID"}}
```

成功结果示例：

```json
{ "id": "request-1", "ok": true, "result": { "connections": [] } }
```

等待用户在 FileTerm 主窗口确认或输入密码时，可能先收到带有相同请求 ID 的 progress 行，最后再收到结果行。取消仍在等待的请求：

```json
{ "id": "cancel-1", "action": "cancel_request", "params": { "request_id": "request-1" } }
```

取消只停止 CLI JSONL 的等待和后续输出，不会回滚 FileTerm 已经接受或开始执行的远程操作。请求 ID 必须是非空字符串或数字，同一活动请求不能重复使用。单条输入最大 2 MiB，最多同时处理 8 个请求；stdin 关闭后桥接进程退出。

## 手动 CLI

查看帮助：

```text
fileterm cli --help
```

常见的一次性调用：

```text
fileterm cli connections
fileterm cli open --profile-id PROFILE_ID --wait-for-ready true
fileterm cli sessions --profile-id PROFILE_ID
fileterm cli directory --tab-id TAB_ID --path /
fileterm cli read --tab-id TAB_ID --path /etc/hostname
fileterm cli exec --tab-id TAB_ID --command "uname -a"
```

CLI `open` 默认创建后台会话，并在结果的 `sessionId` 中返回可复用的会话 ID（同时保留 `tabId` 字段兼容现有请求）。如果需要在顶部标签栏中查看终端，可在 GUI 的“后台会话”页面打开它，或通过 MCP 调用 `fileterm_activate_session`。

一次性 CLI 适合用户手动调试和 shell 脚本。每次调用都会创建一个新的 CLI 进程，调用完成后退出；它不会复用 CLI 进程，也不应作为 AI Agent 的逐动作调用方式。

## 权限与确认

MCP、一次性 CLI 和 CLI JSONL 共用 FileTerm 设置中的连接范围、操作权限和安全校验：

- **只读**：只允许查询连接、会话、目录、文件和传输状态，不执行命令或变更。
- **基础安全操作**：查询和被 FileTerm 判定为只读的普通命令自动执行；变更、危险/提权命令、会话变更、文件操作、传输、隧道和未知操作回到 FileTerm 主窗口确认。
- **完全访问**：跳过逐次操作确认，包括 `sudo`/`su`；连接白名单、协议能力、安全校验和可能需要的密码输入仍然有效。

CLI JSONL 请求中的 `requiresApproval` 不能关闭桌面端审批。密码不会放入命令文本、日志或结果；一次性 CLI 需要提供密码时，优先使用 stdin 选项，例如 `--sudo-password-stdin` 或 `--su-password-stdin`。

## 远程命令边界

- 普通 SSH 服务器命令使用独立的非交互 exec channel，不会写入可见终端。
- 网络设备命令通过可见的原始终端发送单行命令，结果可能包含命令回显和提示符，不提供后台 exec、`cwd`、`sudo` 或 `su` 能力。
- MFA、安装器确认、REPL 和其他需要连续交互输入的操作必须在可见 SSH 终端中完成；先打开对应后台会话或调用 `fileterm_activate_session`，再继续交互。FileTerm 返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED` 时，不要自动重复执行。
- `sudo`/`su` 可能等待 FileTerm 主窗口中的安全密码输入；用户完成输入后，原请求会继续返回结果。

## 客户端配置

外部客户端应注册 `fileterm mcp` 或 `fileterm cli --jsonl`，并复用启动后的 stdio 进程。FileTerm 只在设置页提供可复制的注册命令，不会自动运行客户端或改写其配置文件。
