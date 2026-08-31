# FileTerm 源码大文件职责拆分总计划

状态：active（盘点已完成，分阶段实施尚未完成）
创建日期：2026-08-31

本计划是源码规模治理的总入口，覆盖当前 Tauri Rust、bridge 和 renderer
中已经识别出的 40 个候选文件。它不替代已有的功能计划：SSH 的具体实施以
[`ssh-module-split.md`](./ssh-module-split.md) 为准，MCP/CLI、Serial、AI 附件和
网络设备能力仍分别以 [`mcp-cli-agent-access-policy.md`](./mcp-cli-agent-access-policy.md)、
[`serial-capability-gap.md`](./serial-capability-gap.md)、
[`ai-copilot-attachments.md`](./ai-copilot-attachments.md) 和
[`network-device-compatibility.md`](./network-device-compatibility.md) 为准。

## 1. 结论与范围

这 40 个文件不应全部用同一种方式处理：

- 7 个超过 3000 行的文件是第一优先级，必须按职责拆分，并保留原有对外
  facade 或 re-export 入口。
- 25 个 1001–3000 行的文件进入后续阶段；每次只处理一个清晰的职责边界，
  业务代码目标控制在 800 行以内。数据中心型文件允许保留超过 800 行，
  但不能继续把不同领域堆在同一文件中。
- 8 个 801–1000 行的文件作为机会项。先判断生产代码是否因 inline tests
  或类型聚合而超长，职责边界不明确时不做机械拆分。
- CSS、i18n 字典、类型/常量聚合、生成代码、vendor、测试文件和文档属于
  `AGENTS.md` 规定的豁免范围，不进入本计划的拆分目标。

当前实施顺序：

```text
盘点与契约 → SSH 专项收口 → Rust 数据中心文件 → 其他 Rust 边界
          → bridge / renderer → 机会项审计 → 质量门禁与文档归档
```

本计划只描述结构治理。拆分过程中不得顺手改变协议语义、权限策略、IPC
协议或 UI 行为；行为变更必须进入对应功能计划或独立 issue。

## 2. 硬性约束

执行时遵守 [`AGENTS.md`](../../../AGENTS.md) 与
[`architecture.md`](../../architecture.md)：

1. `packages/core` 继续作为领域类型的 single source of truth。
2. Renderer 继续使用 `Rust commands/events → tauri-api.ts → renderer`，
   不直接访问 SSH、SFTP、FTP 或其他 protocol client。
3. SSH/SFTP、FTP、Telnet、Serial 维持独立的 controller/protocol 边界，
   不为了降低行数强行抽成伪通用层。
4. 每个大文件先定义新模块的职责和依赖，再移动实现；不得按固定行数
   机械均分。
5. 原入口保留为 facade，调用方不因物理拆分而扩大可见性；只为搬文件把
   私有类型改成 `pub` 是禁止的。
6. 同一大模块的拆分文件必须进入模块目录，采用 `mod.rs` facade 加职责文件
   的结构，参照 `apps/tauri/src-tauri/src/sessions/ssh/`；不得在父目录用
   模块名前缀堆放同类文件。为保持私有作用域而使用 `include!` 时，引用路径
   也必须位于该模块目录内。
7. 修改任何超过 1000 行的源文件前，先按 `AGENTS.md` 报告当前行数并向
   用户确认是否顺带拆分。计划文档本身不触发这一限制。
8. 每个阶段都要有 focused tests；最终阶段执行完整质量门禁。

## 3. 第一优先级：超过 3000 行

| 阶段 | 文件                                                                                                    | 盘点行数 | 目标职责边界                                                                                                                                              |
| ---- | ------------------------------------------------------------------------------------------------------- | -------: | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | [`commands/mod.rs`](../../../apps/tauri/src-tauri/src/commands/mod.rs)                                  |     8101 | 按 `preferences`、`workspace/session`、`files`、`transfers`、`ai`、`backup`、`window`、`platform` 分组；`commands/mod.rs` 只负责模块注册和兼容 facade。   |
| 2    | [`services/ai/mod.rs`](../../../apps/tauri/src-tauri/src/services/ai/mod.rs)                            |     7547 | 拆成 provider store、conversation、context/attachments、chat loop、provider adapters、stream parser；保留服务层公开入口和敏感信息只在 Rust 侧处理的边界。 |
| 3    | [`services/mcp/mod.rs`](../../../apps/tauri/src-tauri/src/services/mcp/mod.rs)                          |     4737 | 拆成 runtime/bridge、MCP protocol、policy/approval、desktop actions、CLI/JSONL；与 MCP 功能计划正交，先做物理边界，再按功能阶段演进。                     |
| 4    | [`SettingsModal/index.tsx`](../../../apps/tauri/src/renderer/features/settings/SettingsModal/index.tsx) |     4606 | 按设置域拆为 `panels/` 与 `controller/`；根 facade 保持原 import，`index.tsx` 只保留导航、provider 装配和 Modal 边界。                                    |
| 5    | [`services/transfers/mod.rs`](../../../apps/tauri/src-tauri/src/services/transfers/mod.rs)              |     3266 | 保留 `TransferService` facade，拆出 model/journal、manifest/planning、upload/download execution、cleanup/recovery；传输状态仍统一由 Rust service 管理。   |
| 6    | [`sessions/ssh/worker/mod.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/worker/mod.rs)            |     3261 | 不另起计划；按现有 SSH 计划继续拆为 worker loop、dispatch、terminal、output，并完成 context 收口。                                                        |
| 7    | [`sessions/ftp/mod.rs`](../../../apps/tauri/src-tauri/src/sessions/ftp/mod.rs)                          |     3060 | 保留 FTP controller facade，拆出 worker、TLS/proxy transport、capabilities/checksum、listing、file operations；不与 SSH/SFTP 合并。                       |

### 3.1 SSH 处理方式

SSH 已经在工作树中开始迁移，当前状态是“目录 facade 和职责片段已经建立，
独立 Rust module 与 worker context 尚未全部收口”。本总计划只追踪它，不复制
具体 checklist：

- [`ssh-module-split.md`](./ssh-module-split.md) Stage 0 的测试抽取已完成。
- 该计划 Stage 1 的目录 facade、职责片段和 contract test 已完成；leaf
  modules、认证/ shell state owner、`SshSessionContext`、worker dispatch
  仍待完成。

归入 SSH 专项但仍计入本次盘点的文件：

| 文件                                                                                                 | 当前行数 | 处理方式                                                                    |
| ---------------------------------------------------------------------------------------------------- | -------: | --------------------------------------------------------------------------- |
| [`sessions/ssh/files.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/files.rs)                   |     1881 | 由 SSH 计划拆为 `sftp_files`、`transfer_io`、root/shell exec 相关职责。     |
| [`sessions/ssh/shell.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/shell.rs)                   |     1230 | 由 SSH 计划拆为 cwd/path、root auth、setup suppression、encoding 相关职责。 |
| [`sessions/ssh/authentication.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/authentication.rs) |      891 | 由 SSH 计划拆普通认证与 keyboard-interactive owner，但保持同一认证链。      |
| [`sessions/ssh/transport.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/transport.rs)           |      884 | 由 SSH 计划拆 jump、proxy、credential/host verification 相关职责。          |

新目录下的 worker 片段也归 SSH 专项计划管理，不在本计划重复 checklist。

## 4. 第二优先级：1001–3000 行 Rust

### 4.1 Session / protocol 边界

| 阶段 | 文件                                                                                           | 当前行数 | 拆分方向                                                                                                        |
| ---- | ---------------------------------------------------------------------------------------------- | -------: | --------------------------------------------------------------------------------------------------------------- |
| 3A   | [`sessions/local_terminal.rs`](../../../apps/tauri/src-tauri/src/sessions/local_terminal.rs)   |     2953 | shell discovery/platform、输出扫描器、worker/process；保留 local terminal session facade。                      |
| 3A   | [`sessions/system_metrics.rs`](../../../apps/tauri/src-tauri/src/sessions/system_metrics.rs)   |     2937 | exec orchestration、normalized parser、POSIX/FreeBSD/Windows command builders；平台采集继续在 Rust session 层。 |
| 3A   | [`sessions/local_files.rs`](../../../apps/tauri/src-tauri/src/sessions/local_files.rs)         |     1615 | SMB/network share、directory/file operations、permissions；保持本地文件与远程协议分离。                         |
| 3A   | [`sessions/serial/transfer.rs`](../../../apps/tauri/src-tauri/src/sessions/serial/transfer.rs) |     1248 | raw、XMODEM、YMODEM；优先沿用 Serial 计划已经定义的协议边界。                                                   |
| 3A   | [`sessions/serial/mod.rs`](../../../apps/tauri/src-tauri/src/sessions/serial/mod.rs)           |     1175 | port/device lifecycle、worker、reconnect；`mod.rs` 保留生命周期和路由 facade。                                  |
| 3A   | [`sessions/telnet.rs`](../../../apps/tauri/src-tauri/src/sessions/telnet.rs)                   |     1109 | parser、transport/proxy、worker；不增加 SSH 的文件或 exec 能力。                                                |

`serial-capability-gap.md` 已经定义了 Serial 的长期模块边界，但它主要是
功能 parity 计划；本阶段补充的是当这些边界尚未完全落成时的物理拆分和验证，
不改变 Serial 的能力范围。

### 4.2 Service / storage / application boundary

| 阶段 | 文件                                                                                       | 当前行数 | 拆分方向                                                                                                           |
| ---- | ------------------------------------------------------------------------------------------ | -------: | ------------------------------------------------------------------------------------------------------------------ |
| 3B   | [`lib.rs`](../../../apps/tauri/src-tauri/src/lib.rs)                                       |     2385 | error types、menu/tray、window platform、application bootstrap/runtime；入口只保留组装和启动顺序。                 |
| 3B   | [`services/profile_ops.rs`](../../../apps/tauri/src-tauri/src/services/profile_ops.rs)     |     1930 | profile healing、profile CRUD、folder/command CRUD、secret boundary；保持 workspace lock 和公开脱敏 profile 语义。 |
| 3B   | [`storage/mod.rs`](../../../apps/tauri/src-tauri/src/storage/mod.rs)                       |     1868 | paths、portable migration、JSON atomic I/O；`storage/mod.rs` 保留稳定存储 facade。                                 |
| 3B   | [`services/action_review.rs`](../../../apps/tauri/src-tauri/src/services/action_review.rs) |     1629 | approval queue、remote exec、privileged exec；不绕过统一策略和主窗口审批。                                         |
| 3B   | [`services/webdav.rs`](../../../apps/tauri/src-tauri/src/services/webdav.rs)               |     1211 | config/bundle、HTTP sync、ETag/conflict handling；敏感 payload 继续只在 Rust 服务层处理。                          |
| 3B   | [`services/workspace.rs`](../../../apps/tauri/src-tauri/src/services/workspace.rs)         |     1116 | model/capabilities、state、events；workspace snapshot 与 runtime event 边界不变。                                  |
| 3B   | [`services/s3_backup.rs`](../../../apps/tauri/src-tauri/src/services/s3_backup.rs)         |     1006 | config、SigV4 signing、sync/conflict handling；保持 R2 `region=auto` 和 path-style 约束。                          |

`webdav.rs`、`workspace.rs`、`s3_backup.rs` 和部分 session 文件的总行数包含
较多 inline tests；实施前要重新统计生产代码，若实际职责已经收敛，则只拆
明确的域，不为达到数字目标而拆测试。

## 5. 第二优先级：1001–3000 行 Renderer / bridge

### 5.1 桌面壳与连接表单

| 阶段 | 文件                                                                                               | 当前行数 | 拆分方向                                                                                                          |
| ---- | -------------------------------------------------------------------------------------------------- | -------: | ----------------------------------------------------------------------------------------------------------------- |
| 4A   | [`App.tsx`](../../../apps/tauri/src/renderer/App.tsx)                                              |     2493 | 继续收敛 window modes、workspace shell、resize、approval/modal orchestration；已完成的 hooks 和 host 不重新实现。 |
| 4A   | [`ConnectionModal.tsx`](../../../apps/tauri/src/renderer/features/connections/ConnectionModal.tsx) |     2307 | SSH、terminal/serial、session log、proxy、tunnel sections 物理分 panel；Modal 只负责表单状态、验证和提交。        |

`SettingsModal.tsx` 已在第一优先级单列；它与 `App.tsx` 的 modal orchestration
拆分必须分开提交。

### 5.2 Hook、文件和功能组件

| 阶段 | 文件                                                                                                                  | 当前行数 | 拆分方向                                                                                                          |
| ---- | --------------------------------------------------------------------------------------------------------------------- | -------: | ----------------------------------------------------------------------------------------------------------------- |
| 4B   | [`components/useTerminalLifecycle.ts`](../../../apps/tauri/src/renderer/components/useTerminalLifecycle.ts)           |     1898 | xterm lifecycle、IME/input、resize/zoom、events/gestures；保留 hook 的稳定调用契约。                              |
| 4B   | [`hooks/useWorkspaceTabs.ts`](../../../apps/tauri/src/renderer/hooks/useWorkspaceTabs.ts)                             |     1793 | persisted UI state、tab lifecycle、pane tree、context actions；状态模型仍由 workspace snapshot/React hooks 驱动。 |
| 4B   | [`hooks/useFileOperations.ts`](../../../apps/tauri/src/renderer/hooks/useFileOperations.ts)                           |     1638 | navigation、clipboard、file actions、transfer、credential dialogs；文件操作继续通过 bridge。                      |
| 4B   | [`components/useTerminalView.ts`](../../../apps/tauri/src/renderer/components/useTerminalView.ts)                     |     1097 | xterm setup、transcript、search、font/resize actions；不把终端输出业务塞回组件。                                  |
| 4C   | [`features/files/FileManager.tsx`](../../../apps/tauri/src/renderer/features/files/FileManager.tsx)                   |     1383 | dual-pane shell、selection/drag、context menu/keyboard、resize；共用纵向滚动条和文件操作 hook。                   |
| 4C   | [`features/ai/AiCopilotPanel.tsx`](../../../apps/tauri/src/renderer/features/ai/AiCopilotPanel.tsx)                   |     1234 | conversation list、composer、mode controls、tool activity；附件能力继续受 AI 功能计划约束。                       |
| 4C   | [`features/system/SystemSidebar.tsx`](../../../apps/tauri/src/renderer/features/system/SystemSidebar.tsx)             |     1143 | memory/disk/process/network cards、graphs、summary shell；平台采集逻辑不得下沉到 renderer。                       |
| 4C   | [`features/ssh-keys/SshKeyManagerPage.tsx`](../../../apps/tauri/src/renderer/features/ssh-keys/SshKeyManagerPage.tsx) |     1001 | key/folder tree、drag、CRUD/import、row rendering；敏感私钥文本仍只在允许的导入交互中短暂存在。                   |

## 6. 第三优先级：801–1000 行机会项

这些文件全部进入清单，但不承诺为了行数立即切分：

| 文件                                                                                                                          | 当前行数 | 处理策略                                                                                        |
| ----------------------------------------------------------------------------------------------------------------------------- | -------: | ----------------------------------------------------------------------------------------------- |
| [`features/ai/useAiCopilot.ts`](../../../apps/tauri/src/renderer/features/ai/useAiCopilot.ts)                                 |      953 | 有清晰边界时拆 conversation、stream request、mode/context state；否则随 AI 功能改动处理。       |
| [`features/commands/CommandCenter.tsx`](../../../apps/tauri/src/renderer/features/commands/CommandCenter.tsx)                 |      929 | 评估 command tree/editor、temporary history、execution 是否形成独立 feature；不为降行数拆 JSX。 |
| [`services/connections.rs`](../../../apps/tauri/src-tauri/src/services/connections.rs)                                        |      902 | 生产代码约 687 行，主要超出部分是 inline tests；当前明确暂不机械拆分。                          |
| [`sessions/ssh/authentication.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/authentication.rs)                          |      891 | 归入 SSH 专项，普通认证与 keyboard-interactive 的 owner 分离，但保持同一认证链。                |
| [`sessions/ssh/transport.rs`](../../../apps/tauri/src-tauri/src/sessions/ssh/transport.rs)                                    |      884 | 归入 SSH 专项，jump、proxy、credential/host verification 按责任拆分。                           |
| [`bridge/tauri-api.ts`](../../../apps/tauri/src/bridge/tauri-api.ts)                                                          |      859 | 与 Stage 4A 一起评估；只能做 facade/domain API 拆分，不改变 IPC 方向。                          |
| [`features/security/SecuritySettingsPanel.tsx`](../../../apps/tauri/src/renderer/features/security/SecuritySettingsPanel.tsx) |      835 | session lock 与 backup password 分成独立 panel/section，必须保持安全设置提交顺序。              |
| [`hooks/useWorkspaceIpcSync.ts`](../../../apps/tauri/src/renderer/hooks/useWorkspaceIpcSync.ts)                               |      833 | snapshot/events 与 preferences/window events 分开时再拆；先保持 listener lifecycle 一致。       |

行数以当前工作树盘点为基线，下一阶段开始前必须重新运行统计。若行数变化，
不得因此把豁免文件或测试文件加入业务拆分范围。

## 7. 阶段执行规则

每个文件独立完成以下闭环，避免把多个大文件绑定成一个不可回退的重构：

1. **基线**：记录公开函数、调用方、事件名、序列化结构和现有 focused tests。
2. **边界**：先画出职责依赖，决定 facade、子模块名和 visibility；不先移动
   代码再倒推边界。
3. **提取**：优先移动自包含 leaf；再提取需要共享 context 的逻辑；最后
   处理主循环、命令分发和 UI orchestration。
4. **兼容**：保持原 import、Tauri command 名称、event 名称、core JSON 结构
   和 renderer 行为；必要时通过 re-export 保持旧入口。
5. **验证**：先跑 focused tests，再跑本阶段相关的 contract tests；跨层改动
   最后跑完整门禁。
6. **记录**：在本计划勾选具体项并追加进度记录；功能计划只记录功能变化，
   不把结构拆分伪装成已完成的功能阶段。

单个阶段不得同时改动 SSH、FTP、Serial 等不同 protocol controller 的公共
语义；跨协议的抽象如果确有必要，应另立架构决策。

## 8. 验收门禁

每个结构拆分阶段至少满足：

- 新旧 facade 的公开入口和调用方编译通过。
- 原有 focused unit/contract tests 通过，并补充新模块边界测试。
- Rust：`cargo fmt --check`、相关测试、`cargo clippy --locked --all-targets --all-features -- -D warnings`。
- Renderer/bridge：`npm run typecheck -w @fileterm/tauri`、`npm run lint`、
  `npx prettier --check apps/tauri packages/core packages/shared packages/storage`。
- 完整阶段收口：`npm run test:tauri`，并按改动范围执行三平台/协议夹具或
  发行候选验收。
- 对外 facade 不因拆分新增敏感数据、协议 client 或 renderer 直连。

全部阶段完成后，更新 `docs/architecture.md`、`docs/roadmap.md` 的当前
结构描述，将本计划移动到 `docs/plans/completed/`；在此之前保持 `active`。

## 9. 当前进度

- [x] 完成 40 个候选文件的行数与豁免类别盘点。
- [x] 确认 SSH 由 [`ssh-module-split.md`](./ssh-module-split.md) 作为专项
      计划管理，避免重复 checklist。
- [x] 完成第一优先级前 3 项的物理拆分：`commands`、`services/ai`、
      `services/mcp`；均保留目录内 `mod.rs` facade，职责文件不再堆放在父目录。
- [x] 完成第一优先级剩余 4 项的首轮目录化拆分：`SettingsModal/`、
      `services/transfers/`、`sessions/ftp/`、`sessions/ssh/worker/`；Settings
      面板按域放入 `SettingsModal/panels/`，状态、副作用和提交动作继续按域放入
      `SettingsModal/controller/`，Rust `include!` 片段均保留在所属模块目录。
- [ ] 收口 SSH Stage 1 leaf modules、Stage 2 auth/shell state、Stage 3
      worker context/dispatch。
- [ ] 完成 Rust 第一优先级：commands、AI、MCP、transfers、FTP。
- [ ] 完成剩余 Rust session、service、storage 边界。
- [ ] 完成 Settings、workspace shell、bridge、hooks 和 renderer features。
- [ ] 审计 801–1000 行机会项，确认哪些由测试行数或类型聚合造成。
- [ ] 执行完整质量门禁，更新架构/路线图并归档本计划。

## 10. 进度记录

- 2026-08-31：根据 `AGENTS.md` 的文件规模边界建立总计划；将原始“大文件
  列表”改为按优先级、职责、现有专项计划和验收门禁组织。
- 2026-08-31：补充模块目录结构约束；第一优先级前 3 个 Rust 模块统一采用
  `commands/mod.rs`、`services/ai/mod.rs`、`services/mcp/mod.rs` facade，
  职责文件放入各自目录，后续拆分按同一约定执行。
- 2026-08-31：完成第一优先级前 3 项的目录化物理拆分；函数实现保持原逻辑，
  并通过 Rust 编译、523 个单测、6 个 CLI 测试、20 个 contract 测试、Clippy、
  TypeScript 类型检查、Lint、Prettier 和 `npm run test:tauri`。
- 2026-08-31：开始并完成第一优先级剩余 4 项的首轮目录化拆分：传输服务、FTP
  controller、SSH worker 和 SettingsModal 均改为所属目录内的 facade 加职责文件；
  SettingsModal 的所有设置面板移入 `panels/`，并将原本 1740 行的 controller
  按 AI、主题、偏好、概览、Agent、同步、安全和公共副作用拆入 `controller/`；
  `SettingsModal/index.tsx` 收敛至导航、provider 装配和 Modal 边界，原有调用入口和
  IPC/协议边界保持不变。SSH worker context/dispatch 深层收口仍由
  `ssh-module-split.md` Stage 3 管理。
