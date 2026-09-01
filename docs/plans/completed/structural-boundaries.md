# 结构边界整理计划 (Completed)

## 背景

FileTerm 已经具备 SSH / SFTP / FTP MVP 雏形；重构前核心逻辑曾高度集中在少数超大文件中。当前实现已按职责拆到 `apps/tauri/src-tauri/src/services/`、`apps/tauri/src-tauri/src/sessions/` 和 renderer feature/hooks 目录。为了控制继续迭代时的代码复杂度，我们需要坚定推行职责重构与边界划分。

## 目标

- 明确 IPC、服务层、协议控制器与 Renderer Feature 之间的单向流职责。
- 降低核心逻辑的膨胀速度，实现小而精的模块开发。
- 保证每一步重构均可独立运行、可回退、可验证。

---

## 完成清单

### 1. 服务层解耦与重构

- [x] **继续拆分 Workspace service**（当前由 `apps/tauri/src-tauri/src/services/workspace/mod.rs` 及其同目录模块组成）：
  - [x] 将标签页生命周期相关逻辑收敛到 `apps/tauri/src-tauri/src/commands/tab_lifecycle.rs` 和 `apps/tauri/src-tauri/src/services/workspace/state.rs`。
  - [x] 将传输断点 staging、速度估算、目录进度等 runtime helper 收敛到 `apps/tauri/src-tauri/src/services/transfers/`。
  - [x] **建立独立的 `TransferService`**：
    - 将 `createUploadTask`、`createDownloadTask` 以及与 FTP/SFTP 传输流订阅（stream pipe）、速度计算、进度更新相关的执行编排彻底移入新建的 `apps/tauri/src-tauri/src/services/transfers/mod.rs`。
    - `WorkspaceService` 仅通过委托形式保留必要的外层调用，保持接口兼容。
  - [x] **剥离会话 Runtime 事件监听**：
    - 改造 workspace state 的事件转发机制，使用统一 runtime/channel 边界汇总 CWD 跟随和 Root 状态变更事件，避免 workspace service 分头监听 controller 实例。

### 2. 渲染端大文件解耦

- [x] **继续重构 `apps/tauri/src/renderer/app.tsx`**：
  - [x] 将连接编辑表单（Connection Edit Form）封装为 `features/connections/connection-form-host.tsx`。
  - [x] 将系统侧栏外壳封装为 `features/system/system-sidebar-shell.tsx`。
  - [x] 将 Windows 窗口菜单栏封装为 `features/layout/window-menubar.tsx`。
  - [x] 将传输管理器中心（Transfer Manager Modal）挂载层封装为 `features/transfers/transfer-center-host.tsx`。
  - [x] 远程文件面板抽屉主体已由 `features/workspace/session-workspace.tsx` 管理。
  - [x] **引入 `useWorkspaceTabs` 状态托管 Hook**：
    - 将 `tabs` 列表、`activeTabId` 状态移入 Hook，收纳 `handleOpenProfile`、`handleCloseTab` 等控制回调，将状态流与 UI 视图分离。
  - [x] **引入 `useWorkspaceModals` 弹窗控制 Hook**：
    - 统一托管连接管理器、命令编辑器、设置面板的打开/关闭/编辑状态，清空 `app.tsx` 顶层冗余的 modal 变量。
  - [x] **引入 `useFileOperations` 状态与剪贴板 Hook**：
    - 将复制、剪切、粘贴、新建、重命名、删除等文件操作，以及复杂的路径处理工具（如重命名冲突解决）从 app.tsx 中彻底抽离。
  - [x] **引入 `useSshInteractions` 会话认证交互 Hook**：
    - 托管键盘交互式凭据请求、主机指纹信任核对、交互式密码弹窗等状态及对应的 IPC 监听注册。
  - [x] **引入 `useFileEditor` 在线编辑器生命周期 Hook**：
    - 托管 Monaco Editor 编辑器的打开、保存、修改脏状态检查（Dirty Check）以及文件大小限制逻辑。
  - [x] **建立 `ModalPortalManager` 统一弹窗挂载组件**：
    - 解决 Modal 大杂烩（Modal Soup）问题，将设置、连接管理、命令管理、文件编辑器、交互认证弹窗等全局模态框集中至该组件内挂载，使 `app.tsx` 保持清爽。
  - [x] **引入 `useWorkspaceIpcSync` 数据同步 Hook**：
    - 将通过 `useEffect` 侦听主进程 IPC 并同步状态（会话变更、传输进度、日志打开事件）的逻辑抽离，通过 Hook 自动管理侦听器生命周期防内存泄漏。

---

## 已完成工作 (Completed)

- **[x] IPC 处理器拆分**：`apps/tauri/src-tauri/src/commands/mod.rs` 保留稳定 facade，通过 `include!` 装配 `platform_commands.rs`、`remote_file_commands.rs`、`terminal_commands.rs`、`transfer_commands.rs`、`workspace_commands.rs` 等领域命令文件。
- **[x] 协议控制器物理隔离**：SSH 终端/SFTP 和 FTP 会话分别位于 `apps/tauri/src-tauri/src/sessions/ssh/`、`apps/tauri/src-tauri/src/sessions/ftp/`，各自保留独立协议实现。
- **[x] 前端布局组件下沉**：Renderer 侧已经将桌面外壳、状态统计和设置下沉到 `features/layout`、`features/workspace` 等子组件。
- **[x] 工作区标签生命周期服务**：打开、重连、激活、关闭和断开标签页的生命周期由 `apps/tauri/src-tauri/src/commands/tab_lifecycle.rs`、`session_runtime.rs` 和 `services/workspace/state.rs` 协作编排。
- **[x] 传输 runtime helper 外移**：传输断点 staging、速度估算和目录进度计算已进入 `apps/tauri/src-tauri/src/services/transfers/`。
- **[x] App shell host 拆分**：连接表单、窗口菜单栏、系统侧栏外壳和传输中心挂载层已从 `apps/tauri/src/renderer/app.tsx` 拆到对应 feature/layout 组件。

---

## 进度记录

- 2026-05-19：建立记录系统骨架，先把 `AGENTS.md` 收束为入口地图，并将结构边界整理作为 active plan 记录。
- 2026-06-30：renderer 已继续下沉桌面壳 UI 到 feature组件。
- 2026-07-09：重整结构边界计划，归档已完成的 IPC 与协议控制器拆分工作，对齐最新 TODO。
- 2026-07-09：完成 `WorkspaceTabLifecycleService`、传输 runtime helper 和若干 renderer host 组件拆分；保留更层级 TransferService 与 tab state hook 化作为下一步小粒度重构。
- 2026-07-09：将文件传输调度解耦、事件监听中心化、以及 React hooks 数据流托管的详细规划合并入 active plan。
- 2026-07-09：扩展渲染端大文件解耦方案，补充 `useFileOperations`、`useSshInteractions`、`useFileEditor` 和 `ModalPortalManager` 规划细节。
- 2026-07-09：补充 `useWorkspaceIpcSync` 规划细节，将主进程高频数据变更在组件层防漏解耦。
- 2026-07-10：TransferService 抽离（commit d095f8c）与会话事件总线中心化（commit 163b3c9）已落地；app.tsx 从 3898→1337 行（-66%），7 个 hooks（含后续补充的 useWorkspaceDataOps）、ModalPortalManager 与 ErrorBoundary 全部接入并通过质量门禁。本计划全部完成并归档。
