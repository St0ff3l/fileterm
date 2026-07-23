> 归档状态（2026-07-23）：P0/P1/P2 自动化回归全部通过，renderer 交互与后端契约对齐已完成。打包态 macOS/Windows/Linux 人工验收因资源/产品决策暂停。本文档移至 `docs/plans/completed/`。

# Tauri / Electron 细节功能对齐计划

## 背景

本计划用于收口 Tauri renderer 与 Rust backend 相比 Electron 版本仍存在的细节行为差异。
重点不是补齐 API 数量，而是修复同一 API 在数据结构、窗口生命周期、会话状态、异步反馈和
多窗口同步上的语义偏差。

本计划基于 2026-07-17 当前未提交工作区继续推进；既有
`tauri-code-audit-remediation-2026-07-17.md` 负责后端完整性与迁移，本计划不重复覆盖其改动。

## 产品确认边界

- 连接配置和凭据继续按本地明文文件策略保存，不接入 safeStorage、系统钥匙串或额外密文层。
- 现有 profile/secrets 文件拆分只承担数据结构兼容、导入导出边界和避免编辑时误覆盖，
  不作为本计划的安全存储专项或发布阻塞项。

## 已确认当前成立的能力

- [x] 连接管理器、命令管理器与 SSH Key 管理器已接入 pointer sort fallback。
- [x] 终端断开后按 Enter 重连链路已闭环到 Rust worker。
- [x] Monaco 保存按钮已有 spinner、disabled、read-only 与真实写入等待。
- [x] 连接/命令删除确认框已有提交态。
- [x] `sync:version` 已覆盖 Cargo.toml、Cargo.lock 与 tauri.conf.json。
- [x] Tauri 原生文件拖放路径 fallback 已改为消费后清理，不再派发后立即丢失。

## P0：崩溃与数据丢失

### A. 创建数据契约

- [x] 新建连接文件夹必须写入 `type: "folder"`、`order` 和规范化 `parentId`。
- [x] 新建命令文件夹必须写入 `type: "command-folder"`、`order` 和规范化 `parentId`。
- [x] 新建命令必须写入 `type: "command-template"`、`order`、`command` 与
      `appendCarriageReturn` 默认值。
- [x] 读取旧数据时自愈上述缺失字段，避免已经落盘的坏数据继续导致 renderer 崩溃。
- [x] 为创建与旧数据自愈补 Rust contract/unit tests。
- [x] 独立连接管理器/表单使用的 connection library 与完整 snapshot 一样剥离密码、
      私钥口令和代理密码，不把 main-side secret 暴露给 renderer。
- [x] 编辑脱敏 profile 时，空白密码/口令占位不会覆盖原凭据；表单 `proxyPassword`
      在 main-side 规范化为嵌套 secret，并从公开 profile 数据中清除。

### B. 全应用退出与独立 Monaco 编辑器

- [x] Cmd+Q、Alt+F4、托盘退出和原生菜单退出必须在 shutdown 前逐个请求独立编辑器关闭。
- [x] 任一编辑器取消丢弃时必须中止整个退出，不停止 session/transfer worker。
- [x] 全局退出等待期间要防止重复退出请求和重复关闭弹窗。
- [x] 隐藏的脏编辑器收到退出询问时必须恢复可见并聚焦。
- [x] 为关闭请求去重、同意与取消结果补 Rust tests。

## P1：行为契约对齐

### C. 会话状态

- [x] Rust 只发布 core `TabStatus` 允许的 `idle/connecting/connected/error/closed`。
- [x] 主动断开、正常远端断开映射为 `closed`；连接/worker 失败映射为 `error`。
- [x] 修复右键 Connect、标签状态点和 System Info 对断开状态的判断。
- [x] 为 SSH/FTP/Telnet/Serial 状态映射补回归测试。

### D. SSH 认证与初始路径

- [x] password 模式密码为空时回退到 system authentication。
- [x] system authentication 在 Agent 之外尝试默认私钥：`id_ed25519`、`id_ecdsa`、
      `id_rsa`、`id_dsa`。
- [x] Windows 无 Agent 时仍允许默认私钥认证，不直接返回平台不支持。
- [x] SSH session、首次 SFTP 列表与 shell CWD 使用 profile `remotePath`，不硬编码 `/`。
- [x] 为认证候选顺序和 remotePath 归一化补单元测试。
- [ ] 用外部 SSH fixture 单独覆盖“空 password → system default key”完整握手；现有真实
      OpenSSH fixture 已覆盖 auth/exec/SFTP/platform probe，但未精确钉住该 fallback 分支。

### E. 多窗口一致性

- [x] profile/folder/command 的全部增删改排序在持久化成功后广播 `workspace:snapshot`。
- [x] `openProfile` 立即广播 connecting snapshot，独立管理器和主窗口同步显示。
- [x] 统一 mutation command 的 snapshot 返回与广播逻辑，避免漏发或重复发。
- [x] Tauri 多窗口同时执行 profile/folder/command 读改写时由 Rust mutation lock 串行化，
      避免后写窗口覆盖先写窗口；snapshot 不读取跨文件级联写入的中间态。
- [x] 补 command contract 与多窗口事件测试。

### F. 批量命令错误与重复弹窗

- [x] 检查 `Promise.allSettled` 的 rejected 结果；保留成功目标，同时汇报失败目标数量与原因。
- [x] 删除重复渲染的 SSH keyboard-interactive modal。
- [x] 批量普通命令与命令模板使用同一错误汇总策略。

## P2：异步反馈与小交互

### G. Loading 与防重入

- [x] ConnectionModal 保存期间禁用关闭、取消、字段与重复提交，并展示 spinner。
- [x] CommandEditorModal 等待持久化成功后再关闭，失败时保留表单和错误。
- [x] FilePermissionModal 在递归 chmod 期间禁用关闭与重复应用。
- [x] SSH credentials、host verification、keyboard-interactive、key passphrase 在提交期间防重入。
- [x] WebDAV 保存/上传/下载分别展示操作态并阻止重复请求。
- [x] 导入 commit 期间禁止 backdrop、关闭与取消绕过。
- [x] 文件新建/重命名/删除、root 授权、导入与 WebDAV 使用同步 ref guard，
      消除 React 下一帧禁用按钮前的快速双击窗口。
- [x] 删除 profile/command 失败时不得因上层吞错而自动关闭确认框。
- [x] Monaco 对 Cmd/Ctrl+S 增加同步 guard，保存期间关闭先等待或明确确认。

### H. 右键菜单、拖拽与平台菜单

- [x] 终端读取不到文本剪贴板时安静返回并恢复终端焦点。
- [x] 通用 ContextMenu 增加 menu/menuitem 语义及方向键、Home/End 导航。
- [x] 原生菜单和自定义窗口菜单统一命令管理器快捷键。
- [x] 原生菜单标题跟随已持久化 locale 构建，并在语言切换后即时刷新。
- [x] `setUiPreferences` 的 Tauri 返回值对齐共享 API/Electron，不再静默返回空 IPC payload。
- [x] 更新检查按 Electron 的 single-flight 语义去重，异常路径不会把状态永久留在 checking。
- [x] pointer sort 在 window blur、pointer capture 丢失时可靠清理 ghost/state。
- [x] TabBar 从高风险 HTML5 DnD 迁移到 pointer sort，保留键盘操作并抑制拖拽后的误点击。
- [ ] macOS/Windows/Linux 打包态分别验证标签排序和外部文件拖放。

### I. 第二轮细节扫描

- [x] 主窗口 ModalPortal 内的连接 CRUD 必须应用 command 返回快照，不能只依赖跨窗口广播。
- [x] 连接/命令文件夹新建与重命名等待持久化结果；失败时保留输入，不制造成功假象。
- [x] SSH Key 删除、分类和备注持久化失败时在当前弹窗显示错误，并增加同步防重入。
- [x] Transfer pause/resume/discard/清理历史增加同步 guard、loading 和失败后的可靠复位。
- [x] SSH 隧道刷新、新建、启动、停止、删除增加逐操作 loading、防重复提交和弹窗内错误。
- [x] 通用确认弹窗补 dialog 语义、初始焦点、Escape 关闭和 busy 状态焦点保护。
- [x] Tauri 托盘、Windows 自绘 menubar 和原生 App/Window 菜单随 locale 刷新；macOS 补齐
      About/Services/Hide/Bring All to Front 等标准菜单角色，同时保留统一退出确认链路。
- [x] 密钥管理器与连接/命令管理器统一表体视觉：移除独有的逐行横隔线和嵌套底色，文件夹行仅保留
      展开箭头，密钥实体与“全部密钥”入口使用本地 key 图标。
- [x] 命令编辑器保存/取消按钮与确认弹窗统一动作规格；SSH 私钥导入弹窗拆出 `#242424` 紧凑标题表头
      并补本地 key 图标。
- [x] 系统资源侧边栏折叠偏好改为按会话标签保存；焦点模式和资源不可用只做临时强制折叠，
      不再覆盖用户偏好或串扰新标签。
- [x] SSH 私钥导入表头精确复用新建连接的 `48px / 13px / 500 / 16px icon` 规格，选择文件按钮补
      文件夹图标、选择中 spinner 与完整 hover/focus 状态；WebDAV 风险选项和操作区分组并拉开间距。
- [x] 远端目录下载接入右键菜单、顶部下载按钮和拖入本地窗格；文件与目录统一进入既有传输清单，
      保留进度、暂停、继续和取消，并校验清单相对路径不能逃逸用户选择的本地目标目录。
- [x] 文件权限弹窗表头对齐新建连接的 `#242424 / 48px / 13px / 500 / 16px icon` 规格，
      使用本地 lock 图标，并在 Electron/Tauri 保持一致。
- [x] Tauri root 文件视角的 `sudo find` 列表补齐 `..` 父目录行，与普通 SFTP 和 Electron root
      视角一致；远端 `/` 仍保持为唯一不显示返回入口的真实根目录。
- [x] Tauri 终端从 `sudo -i` 执行 `exit` 回到登录用户时，文件区立即切回 user 访问模式并
      刷新当前目录；sudo 凭据缓存仅用于下次复用，不再错误地锁住工具栏 root 状态。

## 验证门禁

- `npm run typecheck`
- `npm run lint -- --max-warnings=0`
- `npm run format:check`
- `npm run test:tauri`
- `cargo fmt --manifest-path apps/tauri/src-tauri/Cargo.toml --all -- --check`
- `cargo clippy --locked --all-targets --all-features --manifest-path apps/tauri/src-tauri/Cargo.toml -- -D warnings`
- 打包态手测：全局退出脏编辑器、管理器拖拽、标签拖拽、右键重连、剪贴板无文本、
  SSH 空密码/default key/remotePath、多窗口 CRUD 同步。

## 完成定义

- P0/P1 自动化回归全部通过，且 Electron 行为差异有明确测试或文档化例外。
- P2 中可以自动化的 renderer 交互进入测试；只能由原生 WebView 验证的项目保留三平台验收记录。
- 完成后将本文件移动到 `docs/plans/completed/`，不在 `AGENTS.md` 堆积过程信息。

## 2026-07-17 自动化结果

- [x] `npm run typecheck`
- [x] `npm run lint -- --max-warnings=0`
- [x] `npm run format:check`
- [x] `npm run test:tauri`：108 unit tests + 18 contract tests 全绿（含目录下载路径边界回归）。
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --locked --all-targets --all-features -- -D warnings`
- [x] `npm run test:electron`：65 unit tests + 21 controller tests 全绿。
- [x] `npm run build:renderer -w @fileterm/electron`：Vite production bundle 构建成功。
- [x] `npm run build:renderer -w @fileterm/tauri`：Vite production bundle 构建成功。
- [ ] macOS/Windows/Linux 打包态人工验收。

第二轮补扫已完成自动化覆盖范围内的管理器 snapshot 回填、文件夹异步提交、SSH Key、
Transfer、SSH Tunnel、通用确认弹窗、托盘/窗口菜单本地化与 macOS 标准菜单角色。连接信息明文
文件存储按产品确认边界保留，不列为遗留缺口。
