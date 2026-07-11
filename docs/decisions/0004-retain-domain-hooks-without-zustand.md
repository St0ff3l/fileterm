# ADR-0004: 保留领域 hooks，当前不引入 Zustand

## 状态

Accepted（2026-07-11）

## 背景

早期架构约定在 `App.tsx` 拆分后重新评估是否需要引入全局 store。当前 `App.tsx` 已从单体组件拆出 7 个领域 hooks、`ModalPortalManager`、`ErrorBoundary` 与布局组件，具备进行评估的前提。

现有状态分为两类：

- Renderer 局部交互状态，由 React state 与领域 hooks 管理。
- 跨窗口、会话和协议状态，以 main process 为真理源，通过 workspace snapshot、领域事件和 preload IPC 同步。

目前没有稳定出现“多个非父子 feature 必须直接读写同一份 Renderer 可变状态”的场景。此时引入 Zustand 会增加第二套状态源，并带来 snapshot 与 store 双向同步、失效顺序和调试边界问题。

## 决策

当前不引入 Zustand，继续采用：

- `useWorkspaceTabs`、`useFileOperations`、`useFileEditor` 等领域 hooks 管理 Renderer 编排。
- `App.tsx` 作为组合根负责把领域能力传给布局与 feature 组件。
- main process 继续作为会话、传输和持久化 workspace 状态的唯一真理源。
- 跨窗口状态继续通过 `main -> preload -> renderer` 的 snapshot/事件边界同步，不为减少 props 而复制到全局 store。

只有同时满足以下条件时才重新评估 store：

1. 多个非父子 feature 需要直接订阅同一份高频可变状态。
2. props 传递已经跨越多个纯转发层，且组件边界无法合理调整。
3. 引入 store 不会与 main process snapshot 形成双真理源。

## 影响

- 不新增 Zustand 依赖，也不进行无收益的状态迁移。
- 保持当前 hooks 职责清晰、main process 状态权威的架构。
- 新增共享状态时先判断它属于 Renderer UI 状态还是 main 领域状态，再决定进入 hook、IPC snapshot 或未来的领域 store。
- “无 store”不再是待办债务，而是一项已完成评估的架构决策。
