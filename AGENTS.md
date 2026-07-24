# FileTerm Agent Guide

本文件是智能体进入 FileTerm 仓库时的入口地图，不是完整手册。详细事实以 `docs/` 为准；当代码、设计或计划变化时，优先更新对应文档，不要把所有知识继续堆进这里。

## 1. 项目定位

FileTerm 是面向开发者与运维场景的 Rust + Tauri 桌面远程工作台，围绕 `SSH / SFTP / FTP` 构建可日常使用的多标签桌面客户端。`apps/electron` 仅保留为历史代码参考，不再参与构建、测试或发布。

当前阶段：**Tauri 主链路稳定与发行收口**。质量门禁覆盖共享包与 Rust/Tauri，Windows 使用签名的 Tauri 应用内更新；macOS 继续检查后跳转 GitHub Release 下载。

## 2. 先读哪里

- 架构地图：`docs/architecture.md`
- 设计规范：`docs/design.md`
- 路线图：`docs/roadmap.md`
- 进行中计划：`docs/plans/active/`
- 已完成计划：`docs/plans/completed/`
- 架构决策：`docs/decisions/`
- 质量与回归：`docs/quality/`
- 已隐藏功能：`docs/hidden-features.md`
- 功能草案：`.agents/extensions/`
- 项目技能：`.agents/skills/`

如果任务只改一个小点，先读本文件和相关源码即可；如果任务跨 `Rust commands / bridge / renderer / packages` 多层，必须先看 `docs/architecture.md` 和 `docs/plans/active/`。

## 3. 硬性边界

### 架构边界

- `packages/core` 是领域模型的 single source of truth。
- Renderer 不直接访问 SSH / SFTP / FTP protocol clients。
- 所有系统能力必须走 `Rust commands/events -> tauri-api.ts -> renderer`。
- SSH/SFTP 与 FTP 在 controller/protocol 层保持分离，不做伪统一。
- Transfer 进度统一进入 Rust transfer service，不在组件里零散维护。
- 会话事件通过 Rust workspace runtime 的统一 event/channel 边界分发，不分散监听协议 worker。
- 新状态优先进入 `packages/core` 定义类型，再下沉到服务层和 UI。
- 新窗口能力先定义 IPC 边界，再做 renderer 交互。
- 主题样式优先走 `token -> theme vars -> component skins -> terminal colors`。

### 平台兼容边界

- **CWD 目录跟随**：终端工作目录 (CWD) 变化通过底层会话流安全捕获，经 runtime 广播同步给文件管理器，严禁 UI 层轮询或直接探测平台路径。
- **POSIX CWD 注入门控**：`supportsPosixShellSetup()` 仅对 `linux` / `busybox` 返回 true。Windows / unknown 平台**严禁注入** Linux shell CWD 脚本，采用 fail-closed 双重门控（`detectPlatformAndSetupShell` + `injectShellSetup` 各一道）。
- **CRLF 归一化**：系统指标解析入口必须对远端输出做 `replace(/\r\n?/g, '\n')` 归一化，避免 `'windows\r'` 等污染导致平台误判。
- **Sudo 与 Root 状态同步**：终端执行 `sudo` 或切换用户态需被底层 runtime 解析，双向同步到文件管理器权限模型。

### 资源与安全边界

- **离线资源就地化**：所有图标、字体与基础样式资源预置在代码库中打包输出，严禁运行时动态拉取外部 CDN 资源。
- **macOS 钥匙串规避**：禁用 safeStorage，用品牌重命名等替代机制存储凭据，避免触发 macOS 系统安全弹窗。
- 连接的 `group`（文件夹名）和 `parentId`（文件夹 ID）必须双向同步，存储层负责自愈。

## 4. 代码位置

- Tauri Rust backend：`apps/tauri/src-tauri/src/`
- Tauri bridge：`apps/tauri/src/bridge/tauri-api.ts`
- Tauri renderer：`apps/tauri/src/renderer`
- Electron 历史参考：`apps/electron/`（不得作为新实现、CI 或发行依赖）
- Renderer hooks：`apps/tauri/src/renderer/hooks/`
  - `useWorkspaceTabs.ts`、`useWorkspaceModals.ts`、`useFileOperations.ts`
  - `useSshInteractions.ts`、`useFileEditor.ts`、`useWorkspaceIpcSync.ts`
  - `useWorkspaceDataOps.ts`
- Layout、ErrorBoundary、工作区、终端和主题组件：位于 `apps/tauri/src/renderer/`，不得反向依赖 Electron 组件。
- 领域类型：`packages/core`
- 存储抽象：`packages/storage`
- 共享常量：`packages/shared`

## 5. 当前侧边栏布局

侧边栏导航顺序：**概览 → 连接管理器 → 命令管理器 → 设置**

已从 UI 隐藏但代码保留的功能见 `docs/hidden-features.md`，包括：

- 快速连接（Quick Connect）侧边栏入口
- Docs 侧边栏入口
- 页脚 Changelog / API Reference / Status 导航
- 页脚 System Latency 文字

## 6. 当前热点

这些文件功能集中，改动前要格外注意边界：

- `apps/tauri/src-tauri/src/`：Rust commands、services、sessions、transfers 与 storage。
- `apps/tauri/src/bridge/tauri-api.ts` 与 `apps/tauri/src/renderer/`：Tauri 专用 bridge 与 UI。

## 7. 质量门禁（已落地）

所有代码改动必须通过以下门禁，pre-push 自动阻断不通过项：

| 门禁          | 命令                                                                             | 状态                      |
| ------------- | -------------------------------------------------------------------------------- | ------------------------- |
| 类型检查      | `npm run typecheck -w @fileterm/tauri`                                           | Tauri renderer            |
| 静态检查      | `npm run lint`                                                                   | Tauri 与共享源码          |
| 格式检查      | `npx prettier --check apps/tauri packages/core packages/shared packages/storage` | Tauri 与共享源码          |
| Tauri 测试    | `npm run test:tauri`                                                             | Rust unit + contract 测试 |
| Rust 静态检查 | `cargo clippy --locked --all-targets --all-features -- -D warnings`              | Rust/Tauri crate          |

提交门禁：

- **pre-commit**（`.husky/pre-commit`）：`npx lint-staged` — 对暂存 `.ts/.tsx` 文件执行 prettier + eslint --fix
- **pre-push**（`.husky/pre-push`）：`npm run typecheck`（仅共享包与 Tauri）— 失败阻断 push

CI（`.github/workflows/ci.yml`）：push/PR 时只执行共享包与 Rust/Tauri 的 typecheck、lint、format、Rust tests、生产构建与协议夹具。

测试覆盖分布：

- `apps/tauri/src-tauri/src/`：Rust unit/contract、协议夹具与 socket lifecycle 测试。

## 8. 推荐扩展路径

1. 在 `.agents/extensions/` 或 `docs/plans/active/` 写清楚功能草案。
2. 明确影响层级：`core`、`Rust services`、`commands/events`、`bridge`、`renderer`、`styles`。
3. 补充或复用 `packages/core` 类型。
4. 新建或扩展 `apps/tauri/src-tauri/src/services/*`。
5. 经由 Rust command/event 和 `tauri-api.ts` 暴露能力。
6. 最后接到 renderer 页面、feature component 或 hook。
7. 如果涉及视觉，先收敛 token 和 theme vars，再做组件样式。

## 9. 近期优先级

### 已完成 ✅

1. 质量门禁三件套：ESLint/Prettier + Husky 提交门禁 + CI 测试集成
2. `workspace-service.ts` 按 `tabs / sessions / transfers` 拆子模块
3. `App.tsx` 拆分：7 个 hooks + ModalPortalManager + ErrorBoundary（3898 → 1698 行）
4. SSH 与 FTP controller 物理分离
5. 共享类型收敛到 `packages/core`
6. 系统信息采集多平台化：Linux / BusyBox / Windows collector + parser 归一化 + CRLF 加固
7. Windows 终端 POSIX 注入门控 + PowerShell 采集多级 fallback
8. as any 清理（ssh-session-controller 零命中）+ renderer :any 清理（零命中）
9. SSH/FTP controller 直接测试：生命周期、Windows/POSIX 注入门控、FTP 重连与操作串行化

### 当前重点 🔜

1. 继续稳定主题系统，避免颜色、阴影、圆角散落在业务组件里
2. 评估 Zustand 状态管理（App.tsx 已拆分 56%，hooks 方案已足够，非必须迁移）
3. 继续扩展 Rust SSH/FTP service 异常与协议边界测试（基础生命周期覆盖已落地）

### 可接受债务 📋

- 敏感信息明文存储 profile（safeStorage 暂缓，见硬性边界）
- 无 store（hooks 方案已满足当前需要，Zustand 按需推进）

## 10. 发版操作规范

### 版本号管理（硬性要求）

**禁止手动单独修改任何 workspace 的 `version` 或 `dependencies` 里的内部包版本。**

正确做法：

1. 只改根目录 `package.json` 的 `version` 字段
2. 立即运行 `npm run sync:version`
3. 脚本会自动同步所有 workspace 的 `version` + `dependencies` 里的 `@fileterm/*` 引用 + `package-lock.json`

### 发版 SOP

```
# 1. 在功能分支完成开发和版本号更新，提 PR 合入 main，确认 CI 全绿
# 2. 从已合并的 main 创建仅用于打包发布的 release 分支
git checkout main
git pull origin main
git checkout -b release/x.x.x
git push origin release/x.x.x

# 3. 仅在 release 分支对应提交打 tag，自动触发构建和 GitHub Release
git tag vx.x.x
git push origin vx.x.x
```

版本号更新必须在合入 `main` 前完成：

```
# 只改根 package.json 的 version 字段
npm run sync:version
```

**关键约束**：tag 必须指向 `release/*` 分支上的 commit，否则 `validate-release-tag` 步骤会拒绝构建。

## 11. 文档维护规则

- `AGENTS.md` 只放入口地图和硬约束，保持短小。
- 稳定架构事实放 `docs/architecture.md`。
- 设计规范放 `docs/design.md`。
- 阶段目标放 `docs/roadmap.md`。
- 跨文件或跨层任务放 `docs/plans/active/`，完成后移到 `docs/plans/completed/`。
- 已确认的架构选择放 `docs/decisions/`。
- 质量、测试、发布和安全检查放 `docs/quality/`。
- 历史 UI 优化记录放 `docs/quality/`（如 `MODAL_OPTIMIZATION_SUMMARY.md`）。
- 已隐藏但代码保留的 UI 功能记录在 `docs/hidden-features.md`。
- `.agents/` 只放协作草案和扩展设计，不放生产运行代码。
- 项目内技能统一放在 `.agents/skills/`，不要再写回 `.codex/`。

一句话结论：FileTerm 已完成质量防线建设与核心解耦，当前从"骨架搭建"进入"精细化稳定"阶段——边推进功能边守住 `protocol / service / UI / type / theme` 的边界。
