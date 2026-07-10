# FileTerm 核心架构解耦与状态托管规划

本计划旨在进一步清晰化主进程的服务边界，并对渲染器进程的顶层状态进行 Hook 化重构，从而提升代码的可维护性、局部渲染性能，并降低核心文件的膨胀风险。

---

## 1. 主进程服务端解耦（解耦传输调度与会话监听）

主进程中 `workspace-service.ts`（约 60KB）目前承担了过重的传输和会话流编排工作。我们将采用“单一职责原则”将其进行物理拆分。

### 1.1 建立独立的 `TransferService`

- **目标**：将所有文件传输的底层的任务状态机和 IO 操作移出 Workspace 核心管理层。
- **具体改动**：
  - 新建 `main/services/transfers/transfer-service.ts` 并创建 `TransferService` 类。
  - 将 `workspace-service.ts` 中的 `createUploadTask`、`createDownloadTask` 以及与之关联 FTP/SFTP 连接池建立、流订阅（stream pipe）、传输速度监控和进度保存逻辑完整迁移至 `TransferService`。
  - `WorkspaceService` 在初始化时实例化并保持对 `TransferService` 的引用，仅暴露轻量级的外层封装 API 供 IPC 处理器调用。

### 1.2 重构会话 Runtime 监听转发器

- **目标**：将会话运行时（`workspace-session-runtime.ts`）的底层事件订阅与 `WorkspaceService` 解耦，防止事件链路无限套娃。
- **具体改动**：
  - 在 `workspace/workspace-session-runtime.ts` 中直接暴露会话状态（CWD 变更、Root 状态提权、SSH 握手事件）的观察者/订阅者模式接口（Event Emitter）。
  - 使得 `WorkspaceService` 仅需通过单次 `.on('tab-event', ...)` 的方式集中处理前端数据分发，不再逐个监听单独的 controller 实例。

---

## 2. 渲染端前端解耦（App.tsx 状态托管与 Hook 化）

`App.tsx`（约 124KB）目前承载了过多的顶层 UI 状态，导致任意局部的 UI 修改都会引起整个应用大范围的 React Rerender。

### 2.1 引入 `useWorkspaceTabs` 状态托管 Hook

- **目标**：将标签页的核心生命周期状态、当前激活状态及所有相关操纵函数收拢到独立模块中。
- **具体改动**：
  - 新建 `renderer/hooks/useWorkspaceTabs.ts`。
  - 将 `tabs` 列表、`activeTabId` 状态移动至 Hook 内部维护。
  - 将 `handleOpenProfile`（新建标签连接）、`handleCloseTab`（关闭标签）、`handleReconnectTab`（重连）、`onUpdateTab` 等相关回调封装在 Hook 内返回。
  - `App` 顶层仅通过 `const { tabs, activeTabId, openTab, closeTab } = useWorkspaceTabs(...)` 获取只读状态及必要回调，避免编写大量散落的回调定义。

### 2.2 引入 `useWorkspaceModals` 弹窗控制 Hook

- **目标**：将非高频的模态对话框（连接管理器、命令模板编辑器、系统全局设置等）的显示状态与初始化逻辑外置。
- **具体改动**：
  - 新建 `renderer/hooks/useWorkspaceModals.ts`。
  - 将 `isConnectionManagerOpen`、`isCommandManagerOpen`、`isSettingsOpen` 以及 `editingProfile`/`editingCommand` 状态从 `App.tsx` 中抽离。
  - 统一由该 Hook 管理并返回弹窗控制函数，极大地简化 `App.tsx` 顶层渲染函数的结构。

### 2.3 引入 `useFileOperations` 状态与剪贴板 Hook

- **目标**：将复制、剪切、粘贴、新建、重命名、删除等文件系统操作，以及复杂的路径处理工具（如 `allocateTargetNames`、`makeDuplicateName`）从 App.tsx 中彻底抽离。
- **具体改动**：
  - 新建 `renderer/hooks/useFileOperations.ts`。
  - 将文件剪贴板状态 `fileClipboard` 与操作弹层状态 `fileActionDialog` 移入 Hook 内维护，封装全套文件系统 CRUD 回调供 UI 消费。

### 2.4 引入 `useSshInteractions` 会话认证交互 Hook

- **目标**：收拢 SSH 连接过程中的各种交互式认证请求（键盘交互式凭据请求、主机指纹信任核对、交互式密码弹窗等）及 IPC 监听注册。
- **具体改动**：
  - 新建 `renderer/hooks/useSshInteractions.ts`。
  - 托管 `sshCredentialsPromptRequest`、`sshHostVerificationRequest` 和 `sshInteractionRequest` 状态。

### 2.5 引入 `useFileEditor` 在线编辑器生命周期 Hook

- **目标**：托管 Monaco Editor 在线编辑器的打开、保存、修改脏状态检查（Dirty Check）以及文件大小限制逻辑。
- **具体改动**：
  - 新建 `renderer/hooks/useFileEditor.ts`，接收当前会话引用，提供在线文件的打开和保存回调。

### 2.6 建立 `ModalPortalManager` 统一弹窗挂载组件

- **目标**：解决 `App.tsx` 底部的 Modal 大杂烩（Modal Soup）问题，清理 10+ 个弹窗组件的 JSX 堆叠。
- **具体改动**：
  - 新建 `renderer/features/layout/ModalPortalManager.tsx`，将所有全局模态框（设置、连接管理、命令管理、文件编辑器、交互认证弹窗等）集中至该组件内挂载，主 App 外层只需挂载此单节点。

### 2.7 引入 `useWorkspaceIpcSync` 数据同步 Hook

- **目标**：将 `App.tsx` 中通过 `useEffect` 侦听主进程 IPC 并同步高频状态（如 `session-updated` 会话变更、`transfer-updated` 传输任务列表更新、日志目录打开事件等）的冗余逻辑从 App.tsx 中解耦。
- **具体改动**：
  - 新建 `renderer/hooks/useWorkspaceIpcSync.ts`。
  - 集中接管 Electron 进程通信，通过单向数据流将最新后端状态写入 React Context / hooks，并自动管理 IPC 侦听器解绑，完全防止内存泄漏。

---

## 3. 拟改动文件清单

### [NEW] 新增文件

- [transfer-service.ts](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/main/services/transfers/transfer-service.ts) — 新建文件传输专职服务
- [useWorkspaceTabs.ts](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/renderer/hooks/useWorkspaceTabs.ts) — 标签生命周期自定义 Hook
- [useWorkspaceModals.ts](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/renderer/hooks/useWorkspaceModals.ts) — 模态管理器自定义 Hook
- [useFileOperations.ts](file:///Users/stoffel/CodeFile/fileterm/apps/renderer/hooks/useFileOperations.ts) — 文件操作与剪贴板 Hook
- [useSshInteractions.ts](file:///Users/stoffel/CodeFile/fileterm/apps/renderer/hooks/useSshInteractions.ts) — SSH 认证交互 Hook
- [useFileEditor.ts](file:///Users/stoffel/CodeFile/fileterm/apps/renderer/hooks/useFileEditor.ts) — 文件在线编辑生命周期 Hook
- [useWorkspaceIpcSync.ts](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/renderer/hooks/useWorkspaceIpcSync.ts) — 主进程数据单向同步 Hook
- [ModalPortalManager.tsx](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/renderer/features/layout/ModalPortalManager.tsx) — 全局弹窗挂载管理器组件

### [MODIFY] 修改文件

- [workspace-service.ts](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/main/services/workspace-service.ts) — 委派传输调度至 TransferService，清理多余函数
- [workspace-session-runtime.ts](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/main/services/workspace/workspace-session-runtime.ts) — 优化事件订阅接口
- [App.tsx](file:///Users/stoffel/CodeFile/fileterm/apps/desktop/src/renderer/App.tsx) — 接入各类 Hook 与 ModalPortalManager，彻底清空冗余的状态声明与回调

---

## 4. 验证与回归计划

为了保证每次局部拆分重构的百分之百安全，我们将分批次、小步提交：

### 第一阶段（后端解耦）

1. 建立 `TransferService`，迁移核心上传/下载链路。
2. 重构 `workspace-service.ts` 的事件注册。
3. 运行 `npm run test:transfers -w @fileterm/desktop` 验证断点续传测试用例。
4. 运行 `npm run typecheck --workspaces` 确保主进程类型完全通过。

### 第二阶段（前端 Hook 化与弹窗收口）

1. 引入并切换 `useWorkspaceTabs` 与 `useWorkspaceModals` 逻辑。
2. 引入 `useFileOperations`、`useSshInteractions`、`useFileEditor` 和 `useWorkspaceIpcSync`，剥离路径、编辑器与 IPC 同步侦听。
3. 引入并挂载 `ModalPortalManager`。
4. 启动应用开发服务器，手动在界面操作：
   - 快速连接与标签新建/切换动效。
   - 文件传输抽屉开启、任务添加与暂停。
   - 文件列表复制粘贴、文件编辑保存与冲突提示。
   - 系统设置修改与多语言切换。
5. 运行 `npm run build` 确保前端构建彻底没有警告和类型错误。
