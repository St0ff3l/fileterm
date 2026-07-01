# 结构边界整理计划

## 背景

FileTerm 已经具备 SSH / SFTP / FTP MVP 雏形，但核心逻辑正在向少数大文件集中。继续加功能前，需要逐步稳住 `protocol / service / UI / type / theme` 的边界。

## 目标

- 让 IPC、服务层、协议控制器、renderer feature 之间的职责更清楚。
- 降低 `workspace-service.ts`、`session-controllers.ts`、`App.tsx` 的继续膨胀风险。
- 保持每一步都可运行、可回退、可验证。

## 影响范围

- `apps/desktop/src/main/ipc.ts`
- `apps/desktop/src/main/services/workspace-service.ts`
- `apps/desktop/src/main/services/session-controllers.ts`
- `apps/desktop/src/preload/preload.cts`
- `apps/desktop/src/renderer/App.tsx`
- `apps/desktop/src/renderer/components/`
- `apps/desktop/src/renderer/styles/themes/`
- `packages/core/src/index.ts`

## 推荐顺序

1. 拆分 `apps/desktop/src/main/ipc.ts`，按 `app / workspace / terminal / remote-files / transfers` 分文件。
2. 从 `workspace-service.ts` 中拆出 `tabs / sessions / transfers` 相关子模块或服务。
3. 将 `session-controllers.ts` 中 SSH 与 FTP controller 分离。
4. 将 `App.tsx` 中连接管理、文件面板、传输面板、顶部标签拆成 feature components 或 hooks。
5. 把新增或重复出现的共享类型收敛到 `packages/core`。
6. 主题相关改动继续走 `tokens -> theme vars -> component skins -> terminal colors`。

## 验收方式

- TypeScript 编译通过。
- Electron renderer 与 main 的 IPC 能力保持兼容。
- SSH 终端输入输出、resize、SFTP 文件浏览、FTP 文件浏览、传输任务状态不回退。
- 明暗主题和终端配色仍能联动。

## 进度记录

- 2026-05-19：建立记录系统骨架，先把 `AGENTS.md` 收束为入口地图，并将结构边界整理作为 active plan 记录。
- 2026-06-30：renderer 已继续下沉桌面壳 UI 到 `features/layout`、`features/workspace`、`features/system`、`features/files` 等 feature 组件；工作区焦点模式、文件面板抽屉、终端悬浮输入条和文件编辑器双栏布局仍属于 renderer UI 状态，不改变 main/preload 公开 API。
