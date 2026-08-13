# 简化远程 exec 与 sudo 凭据自动化

状态：规划确认，待开工
关联：[MCP / CLI 安全交互式远程执行计划](./mcp-cli-interactive-exec.md)、[本机凭据字段加密](./secret-storage-encryption.md)、[本地终端与 Agent MCP 接入](./local-terminal-mcp.md)、[架构地图](../../architecture.md)

## 0. 审查结论与实施修正

目标合理：把非交互式的 `sudo` / `su` 执行收敛到独立 exec channel，并复用现有 profile secret storage，能减少重复的交互路径。不过原方案中有几项必须先修正，以下约束优先于后文示例：

- **公开 profile 不携带密码**。`SshProfile` 只增加非敏感的 `sudoSameAsLogin` 和 `hasSavedSudoPassword` / `hasSavedSuPassword` 标记；密码只允许出现在 transient 的 `CreateProfileInput`、Rust 内存和 `profile-secrets.json` 解密路径中。
- **Agent 不得被引导索取密码**。`sudo_password` / `su_password` 只接受用户已经明确提供的一次性参数；MCP tool description、CLI 帮助和错误提示不得要求 Agent 在聊天里询问或复述密码，也不得把密码写入日志、上下文或审计记录。优先使用 profile 加密存储，其次才是安全的本地输入；没有凭据时 fail-closed 返回稳定错误。
- **密码绝不拼入远端命令文本**。不能使用 `echo '<pw>' | ...`、`-c '<pw>'` 或其他会进入 shell history / `ps` / 审计文本的方式。密码通过 SSH exec channel 的 stdin 发送；`sudo` 使用 `-S`，`su` 按远端要求使用 stdin/PTY，并对输入通道做最小化生命周期管理。
- **旧交互式 exec 先保留**。它仍承担 MFA、一次性验证码和通用交互式程序的安全输入。在 sudo/su 的安全 stdin 路径和替代 UX 经过真实回归前，不删除 `interactive-exec`、`interactive_exec_audit` 或相关 renderer；后续只能按兼容迁移单独收敛。
- **分阶段验收**。第一阶段只实现 secret schema、普通 exec 的结构化 sudo/su 输入和 MCP/CLI 契约；本地密码弹窗、连接表单和内置 Copilot 衔接分别完成并验收，不能用“接口已增加”代替端到端能力。

## 1. 结论

保留现有通用交互式 exec 路径，同时把可识别的 `sudo` / `su` 命令新增为普通 exec 的安全 stdin 分支：

- 普通远程命令继续走 exec channel（无 PTY、无人值守、可拿到精确退出码）。
- `sudo` / `su` 类命令由 Rust 服务层仅调整命令参数，并通过 exec channel stdin 发送凭据；密码不进入命令字符串。
- 提权密码采用分层来源，覆盖无人值守和可见窗口场景，但不要求 Agent 代用户索取密码：

```text
1. 用户已明确提供的一次性参数（不主动索取、不回显、不持久化）
2. profile 加密存储（ftsec:v1:）             ← 用户在连接管理器预存（首选，无人值守）
3. 主窗口安全输入（含“保存到连接管理器”选项）← 主窗口可见时使用
4. 三者都不可用 → 返回 SUDO_PASSWORD_NEEDED / SU_PASSWORD_NEEDED
```

借鉴 `mcp-sudo` / `@htmitech/mcp-ssh-executor` 等工具的 stdin 注入方式，但保留 FileTerm 现有的加密存储、审批和通用交互式 exec 安全边界。

## 2. 覆盖范围

| 维度            | 改动                                                                                                                                                                                                           |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `packages/core` | `SshProfile` 加 `sudoPassword` / `suPassword` / `sudoSameAsLogin` 三个可选字段                                                                                                                                 |
| Rust services   | `profile_ops` 加 `ftsec:v1:` 加密读写；`ssh.rs` 普通 exec 加 sudo/su 包装；删除 `run_interaction_capable_command` 与 `interactive_exec_audit`                                                                  |
| Rust commands   | 删除 `app_resolve_remote_exec_interaction`；保留 `app_execute_remote_command`（接口扩展）                                                                                                                      |
| MCP             | 删除 `fileterm_execute_interactive_remote_command` 工具；`fileterm_execute_remote_command` 加 `sudo_password` / `su_password` / `save_sudo_password` / `save_su_password` 可选参数；更新 tool description 引导 |
| CLI             | 删除 `fileterm interactive-exec` 子命令；`fileterm exec` 加 `--sudo-password` / `--su-password` / `--save-sudo-password` / `--save-su-password` flag                                                           |
| Bridge          | 删除旧 `onRemoteExecInteraction` / `resolveRemoteExecInteraction`；新增 `onSudoPasswordPrompt` / `resolveSudoPasswordPrompt`                                                                                   |
| Renderer        | 删除 `RemoteExecInteractionModal` + `useRemoteExecInteractions`；新增 `SudoPasswordPromptModal` + `useSudoPasswordPrompt`；连接编辑表单加 sudo/su 密码字段 + “sudo 密码与登录密码相同”复选框                   |
| 测试            | 删除交互式 exec 全套测试；新增 sudo/su 包装、三层优先级、错误路径、加密往返、转义、弹窗三按钮测试                                                                                                              |

> 以上旧表中的“删除 interactive-exec / 将密码交给 Agent / 命令文本包装”均由第 0 节的实施修正覆盖。当前交付范围是：公开 profile 脱敏、加密 sudo/su secret、普通 exec 的 stdin/PTY 分支、MCP/CLI 参数契约；通用交互式 exec 暂不删除。

不在本计划范围：

- 不动 `ftsec:v1:` 加密层本身（复用 [secret-storage-encryption.md](./secret-storage-encryption.md) 已有能力）。
- 不动 MCP 审批队列（其他 mutation 仍需审批）。
- 不动 AI Copilot 内置聊天（独立路径，不走 MCP/CLI）。
- 不动 SFTP 文件操作、传输管理、SSH 隧道工具。
- 不动本机凭据加密的其他 4 个文件（AI Provider / SSH 私钥 / WebDAV / S3）。
- 不做输出 redaction（用户自己在可见终端或 Agent 聊天里输的密码，由用户自负其责；应用日志层面只做命令基本脱敏）。
- 不做跨平台系统通知（X1 思路不在本计划）。

## 3. 用户流程

### 3.1 已预存密码（首选场景）

```text
Agent 调用 execute_remote_command("sudo apt update")
  ↓
Rust 检测 sudo 前缀
  ↓
从 profile 加密存储取 sudo 密码
  ↓
包装为 `sudo -S -p '' apt update`，密码通过 SSH exec channel stdin 发送
  ↓
exec channel 跑完
  ↓
返回完整结果给 Agent（退出码、stdout、stderr、超时状态）
```

无打扰，密码不进 Agent 上下文，主窗口状态无关。

### 3.2 未预存 + 主窗口可见

```text
Agent 调用 execute_remote_command("sudo apt update")
  ↓
Rust 检测 sudo + 存储无密码 + 主窗口可见
  ↓
弹窗 SudoPasswordPromptModal（含三按钮：取消 / 仅本次 / 保存并执行）
  ↓
用户输入密码 + 选择按钮
  ├─ 取消         → 返回 SUDO_PASSWORD_CANCELLED
  ├─ 仅本次       → 用此密码跑，不存
  └─ 保存并执行   → ftsec:v1: 加密存进 profile + 用此密码跑
  ↓
返回结果给 Agent
```

无打扰（弹窗在 FileTerm 主窗口），密码不进 Agent 上下文。

### 3.3 未预存 + 主窗口隐藏/关闭

```text
Agent 调用 execute_remote_command("sudo apt update")
  ↓
Rust 检测 sudo + 存储无密码 + 主窗口不可见
  ↓
返回 SUDO_PASSWORD_NEEDED（不携带密码，不要求 Agent 索取密码）
  ↓
用户在 FileTerm 连接管理器预存密码，或通过受信任的本地调用方明确提供一次性参数
  ↓
重新执行
```

密码不进入 FileTerm 的 Agent 上下文；如果外部调用方自行把密码放入模型上下文，属于调用方边界，不由 FileTerm tool description 鼓励或扩大。

### 3.4 密码错误

```text
任意路径跑 sudo 后，远端返回 "Sorry, try again." 或 "authentication failure"
  ↓
Rust 立即返回 SUDO_AUTH_FAILURE（不重试，不耗尽 sudo 3 次重试）
  ↓
调用方收到错误，由用户通过安全凭据入口更新密码后再重试
```

### 3.5 su 命令同路径

`su -c 'cmd'` 包装，同样三层优先级 + `SU_PASSWORD_NEEDED` 错误码 + `save_su_password` 参数。适用于没有 sudo 权限的服务器（老旧 CentOS、嵌入式设备）。

## 4. 三层密码源契约

### 4.1 优先级

```rust
fn resolve_sudo_password(
    profile_id: &str,
    explicit_password: Option<&str>,
    saved_password: Option<String>,
    login_password_if_enabled: Option<String>,
) -> Result<String, AppError> {
    // 一次性参数只接受调用方已经明确提供的值；FileTerm 不引导 Agent
    // 在聊天里索取它，也不把它写入日志或上下文。
    explicit_password
        .map(str::to_owned)
        .or(saved_password)
        .or(login_password_if_enabled)
        .ok_or_else(|| AppError::Command("SUDO_PASSWORD_NEEDED".to_string()))
}
```

### 4.2 命令包装

```rust
// 返回远端命令和独立 stdin/PTY 输入配置；password 绝不进入 command。
fn wrap_sudo_command(command: &str, password: &str) -> Result<ExecInput, AppError> {
    let trimmed = command.trim_start();
    let cmd_after_sudo = trimmed.strip_prefix("sudo")
        .ok_or_else(|| AppError::NotSudoCommand)?
        .trim_start();

    Ok(ExecInput {
        command: format!("sudo -S -p '' {}", cmd_after_sudo),
        stdin: Some(password.to_owned() + "\n"),
        pty: false,
    })
}

fn wrap_su_command(command: &str, password: &str) -> Result<ExecInput, AppError> {
    let trimmed = command.trim_start();
    let cmd_after_su = trimmed.strip_prefix("su")
        .ok_or_else(|| AppError::NotSuCommand)?
        .trim_start();

    // su 的命令参数保持独立；是否需要 PTY 由 SSH exec primitive 决定。
    Ok(ExecInput {
        command: format!("su {}", cmd_after_su),
        stdin: Some(password.to_owned() + "\n"),
        pty: true,
    })
}
```

### 4.3 sudo / su 检测

```rust
fn is_sudo_command(command: &str) -> bool {
    let trimmed = command.trim_start();
    trimmed.starts_with("sudo ") || trimmed == "sudo"
}

fn is_su_command(command: &str) -> bool {
    let trimmed = command.trim_start();
    trimmed.starts_with("su ") || trimmed == "su"
}
```

复杂包装（`bash -c "sudo xxx"`、`time sudo xxx`）不识别，按普通命令走 exec channel（sudo 会因非 TTY 拒绝，Agent 收到错误后自行处理）。

### 4.4 密码错误检测

```rust
const SUDO_AUTH_FAILURE_PATTERNS: &[&str] = &[
    "Sorry, try again.",
    "Sorry, try again",
    "authentication failure",
    "Authentication failure",
    "sudo: incorrect password",
];

fn detect_sudo_auth_failure(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{}\n{}", stdout, stderr);
    SUDO_AUTH_FAILURE_PATTERNS.iter().any(|p| combined.contains(p))
}
```

检测到立即返回 `SudoAuthFailure`，不让 sudo 跑完 3 次重试。

## 5. 存储与迁移契约

### 5.1 profile-secrets.json schema v3

实现时以仓库当前的 `{ "version": 1, "profiles": { ... } }` secret envelope 为兼容基线，先向现有 profile record 增加加密字段，不直接把 envelope 改名为 `schemaVersion: 3`。只有完成迁移脚本和回滚测试后，才单独提升 schema 版本；`sudoSameAsLogin` 属于公开 profile 元数据，不放进 secret record。

```json
{
  "schemaVersion": 3,
  "profiles": {
    "uuid-xxx": {
      "passwordEncrypted": "ftsec:v1:...",
      "privateKeyPassphraseEncrypted": "ftsec:v1:...",
      "proxyPasswordEncrypted": "ftsec:v1:...",
      "sudoPasswordEncrypted": "ftsec:v1:...",
      "suPasswordEncrypted": "ftsec:v1:...",
      "sudoSameAsLogin": false
    }
  }
}
```

scope 字符串：

- `profile/<id>/sudo-password`
- `profile/<id>/su-password`

### 5.2 读写规则

1. 新值写入时，先 `secret_crypto::encrypt(scope, password)` 加密，再走现有 `write_restricted_file` 原子替换。
2. 读取 `ftsec:v1:` 密文时解密给 Rust 服务层；renderer、公开 workspace snapshot、日志始终只得到 `hasSudoPassword` 标记。
3. 读取旧版（无 sudo/su 字段）profile 时按 `None` 处理，不强制迁移。
4. `sudoSameAsLogin: true` 时，运行时复用连接密码（要求连接密码已配置）；如果连接用私钥无密码，返回错误引导用户单独配 sudo 密码。
5. 解密失败不覆盖原文件，返回"请在此设备重新配置该凭据"的通用错误。

## 6. 威胁模型

| 场景                              | 结果                                                                         |
| --------------------------------- | ---------------------------------------------------------------------------- |
| 用户预存 sudo 密码（场景 3.1）    | 密码不进 Agent 上下文、不进 LLM 日志；本机磁盘 `ftsec:v1:` 加密              |
| 主窗口可见 + 弹窗收集（场景 3.2） | 密码不进 Agent 上下文、不进 LLM 日志；可选保存后落盘加密                     |
| 主窗口隐藏 + 未配置凭据           | 返回稳定的 `*_PASSWORD_NEEDED`；不要求 Agent 在聊天里索取密码                |
| 密码进入远端 history / `ps aux`   | 命令文本不包含密码；通过 SSH exec channel stdin 发送，密码输入不写日志       |
| 密码错误 sudo 卡 3 次重试         | 检测 `Sorry, try again` 立即返回 `SudoAuthFailure`，不重试                   |
| Agent 重复跑 sudo 浪费            | `SUDO_AUTH_FAILURE` 不自动重试；调用方须先通过安全凭据入口更新密码           |
| Agent 把聊天密码回显给用户        | tool description 不索取、不回显密码；应用日志和审计记录不写入密码            |
| 提示注入攻击                      | FileTerm 不把密码放进 Agent 上下文；外部调用方提供的 secret 由调用方自行负责 |
| 旧 profile 兼容                   | 读取时无 sudo/su 字段按 None，不强制迁移                                     |

本计划保持 [mcp-cli-interactive-exec.md](./mcp-cli-interactive-exec.md) 的“不在 Agent 聊天索取 secret”边界；只新增普通 exec 的安全 stdin 分支，不以删除通用交互式 exec 为前提。

## 7. 实现位置

### 7.1 新增

- `packages/core`：`CreateProfileInput` 增加 transient 的 `sudoPassword?` / `suPassword?`；公开 `SshProfile` 只保留 `sudoSameAsLogin?` 与 `hasSaved*Password` 标记。
- `apps/tauri/src-tauri/src/services/profile_ops.rs`：sudo/su 密码的 `ftsec:v1:` 加密读写 + 旧 profile 兼容。
- `apps/tauri/src-tauri/src/sessions/ssh.rs`：`run_remote_command` 加 sudo/su 检测 + 凭据来源解析 + stdin/PTY 执行；新增命令结构化包装和密码错误检测 helper，禁止把密码拼入 command。
- `apps/tauri/src-tauri/src/services/mcp.rs`：`fileterm_execute_remote_command` 工具签名加 `sudo_password` / `su_password` / `save_sudo_password` / `save_su_password` 可选参数；tool description 引导文案。
- `apps/tauri/src-tauri/src/main.rs`：CLI `exec` 加 `--sudo-password` / `--su-password` / `--save-sudo-password` / `--save-su-password` flag。
- `apps/tauri/src-tauri/src/services/app_error.rs`（或现有错误类型位置）：加 `SudoPasswordNeeded` / `SuPasswordNeeded` / `SudoAuthFailure` / `SuAuthFailure` / `SudoPasswordCancelled` / `SuPasswordCancelled` 错误变体。
- `apps/tauri/src-tauri/src/commands/`：新增 `app_resolve_sudo_password_prompt` command；新增 `set_sudo_prompt_renderer_ready` / `has_sudo_prompt_renderer` 状态管理。
- `apps/tauri/src/bridge/tauri-api.ts`：新增 `onSudoPasswordPrompt` / `resolveSudoPasswordPrompt`。
- `apps/tauri/src/renderer/features/connections/SudoPasswordPromptModal.tsx`：新弹窗组件（三按钮：取消 / 仅本次 / 保存并执行）。
- `apps/tauri/src/renderer/hooks/useSudoPasswordPrompt.ts`：新 hook（弹窗触发 + 保存逻辑 + renderer ready 注册）。
- `apps/tauri/src/renderer/features/connections/ConnectionEditForm.tsx`（或类似表单组件）：加 sudo 密码 / su 密码字段 + “sudo 密码与登录密码相同”复选框。

### 7.2 后续收敛（不得作为第一阶段改动）

以下删除项只有在通用交互式程序（MFA、验证码、REPL 等）已有等价安全入口，并完成跨平台回归后才允许执行；第一阶段保留它们。

当前阶段不执行下列删除，也不把它们计入 sudo/su 能力的完成条件；它们是独立的后续迁移议题。

- `apps/tauri/src-tauri/src/sessions/ssh.rs`：`run_interaction_capable_command` 函数（~800 行）。
- `apps/tauri/src-tauri/src/services/interactive_exec_audit.rs`：整个文件（~300 行）。
- `apps/tauri/src-tauri/src/services/mcp.rs`：`fileterm_execute_interactive_remote_command` 工具注册与 handler（~250 行）。
- `apps/tauri/src-tauri/src/main.rs`：`fileterm interactive-exec` 子命令（~100 行）。
- `apps/tauri/src-tauri/src/services/action_review.rs`：`ActionApprovalSource::RemoteExecInteraction` 变体 + `has_remote_exec_interaction_renderer` / `set_remote_exec_interaction_renderer_ready` 相关代码（~150 行）。
- `apps/tauri/src-tauri/src/commands/`：`app_resolve_remote_exec_interaction` command（~80 行）。
- `apps/tauri/src/bridge/tauri-api.ts`：`onRemoteExecInteraction` / `resolveRemoteExecInteraction`（~150 行）。
- `apps/tauri/src/renderer/features/connections/RemoteExecInteractionModal.tsx`：整个文件（~400 行）。
- `apps/tauri/src/renderer/hooks/useRemoteExecInteractions.ts`：整个文件（~250 行）。
- 相关测试：交互式 exec PTY channel、安全输入弹窗 hook、Task 隔离 + redaction、Fail-closed renderer 检查、interactive_exec_audit 测试（~1230 行）。

### 7.3 估算

| 项         | 行数      |
| ---------- | --------- |
| 砍掉       | ~3310     |
| 新增       | ~2200     |
| **净减少** | **~1110** |

## 8. 实施步骤

按“secret schema → exec stdin → MCP/CLI 契约 → renderer UX”四个阶段推进；每个阶段必须通过 typecheck + lint + clippy + test + prettier。interactive-exec 的后续收敛另行验收。

### Commit 1: `feat(core): profile 加 sudo/su 密码字段`

- `packages/core` 加 `sudoPassword` / `suPassword` / `sudoSameAsLogin` 可选字段
- typecheck 通过
- 不动其他层

### Commit 2: `feat(services): sudo/su 密码加密存储 + 读写`

- `profile_ops.rs` 加 `ftsec:v1:` 加密读写（复用 `secret_crypto::encrypt` / `decrypt_or_migrate`）
- 旧 profile 兼容读取（无字段按 None）
- 单元测试：加密往返、scope 绑定、旧 profile 兼容、`sudoSameAsLogin` 复用连接密码

### Commit 3: `feat(services): 普通 exec 加 sudo/su 包装 + 三层密码优先级`

- `ssh.rs` 修改 `run_remote_command`
- 检测 sudo/su + 三层优先级 + 命令包装 + 密码错误检测
- 错误类型：`SudoPasswordNeeded` / `SuPasswordNeeded` / `SudoAuthFailure` / `SuAuthFailure`
- 单元测试：包装正确性、三层优先级、错误路径、密码转义（单引号、双引号、`$`、反斜杠）、`sudoSameAsLogin` 但连接用私钥无密码

### Commit 4: `feat(mcp-cli): execute_remote_command 加 sudo_password / save_sudo_password`

- MCP 工具签名加可选参数 `sudo_password` / `su_password` / `save_sudo_password` / `save_su_password`
- CLI `exec` 加 `--sudo-password` / `--su-password` / `--save-sudo-password` / `--save-su-password` flag
- tool description 明确：`SUDO_PASSWORD_NEEDED` / `SU_PASSWORD_NEEDED` 只提示用户通过连接管理器配置凭据；不在 Agent 聊天里索取、转发或复述密码；`SUDO_AUTH_FAILURE` / `SU_AUTH_FAILURE` 不自动重试。
- 测试：MCP 参数解析、CLI flag 解析、错误码传递

### Commit 5: `feat(renderer): sudo 密码弹窗 + hook + bridge`

- 新弹窗组件 `SudoPasswordPromptModal.tsx`：
  - 显示目标主机、用户、CWD、完整命令
  - 密码输入框（type=password + 显示/隐藏切换）
  - 三按钮：取消 / 仅本次 / 保存并执行
  - 复用 `<ConfirmActionDialog>` 样式规范，遵循 UI 公用组件边界
- 新 hook `useSudoPasswordPrompt.ts`：弹窗触发 + 保存逻辑 + renderer ready 注册
- Bridge `onSudoPasswordPrompt` / `resolveSudoPasswordPrompt`
- `app_resolve_sudo_password_prompt` command
- `set_sudo_prompt_renderer_ready` / `has_sudo_prompt_renderer` 状态管理
- 主窗口不可见时 fail-closed（跳过弹窗走聊天问）
- 测试：弹窗三按钮行为、fail-closed、renderer ready 注册/注销

### Commit 6: `refactor(renderer): 连接编辑表单加提权凭据字段`

- 表单加 sudo 密码 / su 密码字段（type=password + 显示/隐藏 + 清空）
- 加“sudo 密码与登录密码相同”复选框（勾上后 sudo 密码字段禁用并提示"将使用登录密码"）
- 复用现有密码输入框组件，遵循 UI 公用组件边界
- typecheck + prettier 通过

### Commit 7: `refactor: 砍掉旧交互式 exec 全套`

- 删除 MCP `fileterm_execute_interactive_remote_command` 工具
- 删除 CLI `fileterm interactive-exec` 子命令
- 删除旧 `RemoteExecInteractionModal` / `useRemoteExecInteractions`
- 删除旧 bridge `onRemoteExecInteraction` / `resolveRemoteExecInteraction`
- 删除 `app_resolve_remote_exec_interaction` command
- 删除 `interactive_exec_audit.rs`
- 删除 `has_remote_exec_interaction_renderer` / `set_remote_exec_interaction_renderer_ready`
- 删除 `ActionApprovalSource::RemoteExecInteraction` 变体
- 删除 `run_interaction_capable_command`
- 删除相关测试
- 更新文档：
  - `docs/architecture.md` 移除交互式 exec 相关章节
  - `docs/plans/active/mcp-cli-interactive-exec.md` 移至 `docs/plans/completed/` 或标记 superseded
  - `docs/hidden-features.md` 如有相关条目同步更新
- clippy 通过

## 9. 测试策略

### 9.1 自动化测试

**Rust 单元测试**：

- `wrap_sudo_command` / `wrap_su_command` 包装正确性
- 三层优先级（参数 > 存储 > 弹窗 > 报错）
- `SudoPasswordNeeded` / `SudoAuthFailure` / `SudoPasswordCancelled` 错误路径
- `save_sudo_password=true` 时正确存进 profile
- `ftsec:v1:` 加密 sudo/su 密码往返
- 旧 profile 兼容读取（无字段按 None）
- 密码含特殊字符（单引号、双引号、`$`、反斜杠、中文）转义
- 弹窗三按钮（取消 / 仅本次 / 保存）行为
- 弹窗 fail-closed（主窗口不可见 → 跳过弹窗走聊天问）
- `sudoSameAsLogin` 复用连接密码
- `sudoSameAsLogin` 但连接用私钥无密码时报错
- 密码错误检测（`Sorry, try again` / `authentication failure`）

**Rust 契约测试**：

- profile schema v3 兼容性
- 旧 schema v2（无 sudo/su 字段）兼容读取

**三端 CI 矩阵**：

- macOS / Windows / Linux 都执行上述测试
- 复用 [secret-storage-encryption.md](./secret-storage-encryption.md) 已有的三端 CI 矩阵

### 9.2 手工验证

- macOS 真机：连接一台 Linux 服务器，三层场景全跑通
- 真实 Claude Code 端到端：调用 `fileterm_execute_remote_command` 跑 sudo，验证无弹窗 + 拿到结果
- 真实 Codex CLI 端到端：同上
- 主窗口隐藏/关闭场景：验证聊天问路径
- su 命令：连一台只有 su 没有 sudo 的服务器，配 su 密码，Agent 跑 `su -c 'whoami'`
- 复选框：勾上"sudo 密码与登录密码相同"，验证不用重复填
- 密码含特殊字符（单引号、双引号、`$`）：验证转义正确
- 跨设备迁移：旧 `ftsec:v1:` 加密的 profile-secrets.json 在新设备读取，验证 fail-closed
- 密码错误：故意填错，验证返回 `SudoAuthFailure` 不重试

### 9.3 MCP/CLI 端到端

- Claude Code 调用 `fileterm_execute_remote_command` 跑 sudo（已配密码）：验证无弹窗 + 拿到结果
- Claude Code 调用跑 sudo（未配密码 + 主窗口可见）：验证弹窗 + 引导保存
- Claude Code 调用跑 sudo（未配密码 + 主窗口隐藏）：验证聊天问 + 引导保存
- CLI `fileterm exec --tab-id X --command 'sudo apt update'`：验证同上
- CLI `fileterm exec --tab-id X --command 'sudo apt update' --sudo-password pw --save-sudo-password`：验证存进 profile

## 10. 已覆盖回归

（待实施后填写）

## 11. 待完成

1. 按四个阶段推进实施，每阶段通过 typecheck + lint + clippy + test + prettier。
2. macOS arm64 release 构建通过。
3. Windows / Linux 打包环境验证 sudo/su 包装、加密读写、三层优先级在真实打包应用里跑得通。
4. 真实 Claude Code / Codex CLI 端到端验证三层兜底全跑通。
5. 更新 `docs/architecture.md`，补充 sudo/su stdin 执行边界；不移除通用 interactive exec 章节。
6. 另立兼容迁移任务，评估是否将 `docs/plans/active/mcp-cli-interactive-exec.md` 标记 superseded。
7. 全部验收通过后，将本计划移至 `docs/plans/completed/`。

## 12. 风险与缓解

| 风险                                       | 缓解                                                            |
| ------------------------------------------ | --------------------------------------------------------------- |
| 密码进 LLM 服务商日志（未配置凭据）        | FileTerm 返回 `*_PASSWORD_NEEDED`，不要求 Agent 在聊天索取密码  |
| 密码进入远端 history / `ps aux`            | 命令文本不包含密码；通过 SSH exec channel stdin 发送            |
| 密码错误 sudo 卡 3 次重试                  | 检测 `Sorry, try again` 立即返回 `SudoAuthFailure`              |
| Agent 重复跑 sudo 浪费                     | tool description 明确"密码错误不要重试"                         |
| Agent 把聊天密码回显                       | tool description 明确"不要在回复里复述密码"                     |
| 弹窗主窗口不可见 fail-closed               | 三层兜底里第三层自动降级到聊天问                                |
| 旧 profile 兼容                            | 读取时无字段按 None，不强制迁移                                 |
| `sudoSameAsLogin` 但连接用私钥无密码       | 运行时报错引导单独配 sudo 密码                                  |
| `save_sudo_password=true` 但密码错误       | 不存，直接返回 `SudoAuthFailure`                                |
| 复杂命令包装（`bash -c "sudo xxx"`）不识别 | 按普通命令走 exec，sudo 因非 TTY 拒绝，Agent 收到错误后自行处理 |
| 用户在 Agent 聊天里输密码后又被 Agent 复述 | tool description 明确禁止复述；密码错误时也只说"密码错误"不回显 |

## 13. 不做的事

- 不动 `ftsec:v1:` 加密层本身（复用已有能力）
- 不动 MCP 审批队列（其他 mutation 仍需审批）
- 不动 AI Copilot 内置聊天（独立路径）
- 不动 SFTP / 传输 / 隧道工具
- 不动本机凭据加密的其他 4 个文件
- 不做输出 redaction（用户自负其责）
- 不做跨平台系统通知（X1 思路不在本计划）
- 不使用 here-string 或命令文本注入；stdin 是当前实现的必要安全边界
- 不识别复杂命令包装（`bash -c "sudo xxx"` 等）

## 14. 与同行做法对比

| 同行                       | 加密算法            | 密钥来源             | 存储位置                             | 喂法                       | 本计划对比                                                          |
| -------------------------- | ------------------- | -------------------- | ------------------------------------ | -------------------------- | ------------------------------------------------------------------- |
| mcp-sudo                   | Fernet              | machine-id           | `~/.config/.../credential.enc` (600) | PTY + sudo -S + stdin 灌   | 本计划加密更严（AES-256-GCM + scope AAD），喂法相同                 |
| @htmitech/mcp-ssh-executor | AES-256-GCM         | 用户 masterKey (env) | `servers.json` 配置文件              | PTY + 提示检测 + 写 stdin  | 本计划加密密钥来源更便利（设备标识自动派生），喂法用 sudo -S 更简单 |
| mcp-interactive-bash       | 加密 secret storage | 配置文件             | `config.yaml`                        | PTY + 规则匹配             | 本计划更简单（不维护规则匹配，直接前缀检测）                        |
| mcp-ssh-session            | 无加密              | -                    | MCP 参数（明文进 Agent 上下文）      | 直接 sudo_password 参数    | 本计划三层兜底，密码不必然进 Agent 上下文                           |
| FileTerm 当前（旧架构）    | AES-256-GCM         | 设备标识 + seed      | `profile-secrets.json` (ftsec:v1:)   | 普通 exec + 通用交互式 PTY | 本阶段保留交互式 PTY，新增 sudo/su 安全 stdin 分支                  |

本计划落地后，FileTerm 在加密严格度上与 mcp-sudo / @htmitech 相当，在用户体验上（三层兜底 + 无人值守）超过所有同行，在代码复杂度上比当前架构简化 ~1110 行。

## 15. 拍板记录

1. ✅ 凭据来源（用户已明确提供的一次性参数 > profile 存储 > 安全本地输入）
2. ✅ 一次性参数可在明确保存标志下写入加密 profile；不从 Agent 聊天索取
3. ✅ 新弹窗组件（含三按钮：取消 / 仅本次 / 保存并执行）
4. ✅ 连接编辑表单加 sudo/su 密码字段 + “sudo 密码与登录密码相同”复选框
5. ⏳ 保留旧交互式 exec；是否收敛另立兼容迁移任务
6. ✅ su 命令同样支持
7. ✅ tool description 引导 Agent 行为
8. ✅ 主窗口可见时弹窗收集（密码不进 LLM）
9. ✅ 主窗口隐藏且未配置时 fail-closed 返回 `*_PASSWORD_NEEDED`
10. ✅ interactive exec 继续保持既有的本地输入脱敏和审计
11. ✅ 密码通过 SSH exec channel stdin 发送，不进入远端命令文本
12. ✅ 密码错误立即返回，不自动重试
