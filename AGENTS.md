# TermDock Agent Guide

本文件是智能体进入 TermDock 仓库时的入口地图，不是完整手册。详细事实以 `docs/` 为准；当代码、设计或计划变化时，优先更新对应文档，不要把所有知识继续堆进这里。

## 1. 项目定位

TermDock 是面向开发者与运维场景的桌面远程工作台，技术栈以 Electron + React + TypeScript 为主，围绕 `SSH / SFTP / FTP` 构建可日常使用的多标签桌面客户端。

当前仓库已经不是单纯脚手架，而是进入“骨架 + 核心链路打通 + 分层整理”的阶段。

## 2. 先读哪里

- 架构地图：`docs/architecture.md`
- 路线图：`docs/roadmap.md`
- 进行中计划：`docs/plans/active/`
- 已完成计划：`docs/plans/completed/`
- 架构决策：`docs/decisions/`
- 质量与回归：`docs/quality/`
- 功能草案：`.agents/extensions/`
- 项目技能：`.agents/skills/`

如果任务只改一个小点，先读本文件和相关源码即可；如果任务跨 `main / preload / renderer / packages` 多层，必须先看 `docs/architecture.md` 和 `docs/plans/active/`。

## 3. 硬性边界

- `packages/core` 是领域模型的 single source of truth。
- Renderer 不直接访问 SSH / SFTP / FTP protocol clients。
- 所有系统能力必须走 `main -> preload -> renderer`。
- SSH/SFTP 与 FTP 在 controller/protocol 层保持分离，不做伪统一。
- Transfer 进度统一进入 transfer system，不在组件里零散维护。
- 新状态优先进入 `packages/core` 定义类型，再下沉到服务层和 UI。
- 新窗口能力先定义 IPC 边界，再做 renderer 交互。
- 主题样式优先走 `token -> theme vars -> component skins -> terminal colors`。

## 4. 代码位置

- Electron main process：`apps/desktop/src/main`
- IPC 注册与系统能力：`apps/desktop/src/main/ipc.ts`
- 业务服务层：`apps/desktop/src/main/services`
- Preload 安全 API：`apps/desktop/src/preload/preload.cts`
- React renderer：`apps/desktop/src/renderer`
- 终端组件：`apps/desktop/src/renderer/components/TerminalView.tsx`
- 主题样式：`apps/desktop/src/renderer/styles/themes/`
- 领域类型：`packages/core`
- 存储抽象：`packages/storage`
- 共享常量与轻量工具：`packages/shared`

## 5. 当前热点

这些文件功能集中，改动前要格外注意边界：

- `apps/desktop/src/main/ipc.ts`
- `apps/desktop/src/main/services/workspace-service.ts`
- `apps/desktop/src/main/services/session-controllers.ts`
- `apps/desktop/src/main/services/sessions/session-file-utils.ts`
- `apps/desktop/src/main/services/workspace/workspace-session-runtime.ts`
- `apps/desktop/src/renderer/App.tsx`
- `apps/desktop/src/renderer/features/system/`
- `apps/desktop/src/renderer/styles/themes/`

## 6. 推荐扩展路径

1. 在 `.agents/extensions/` 或 `docs/plans/active/` 写清楚功能草案。
2. 明确影响层级：`core`、`main services`、`ipc`、`preload`、`renderer`、`styles`。
3. 补充或复用 `packages/core` 类型。
4. 新建或扩展 `main/services/*`。
5. 经由 `ipc.ts` 和 `preload.cts` 暴露能力。
6. 最后接到 renderer 页面、feature component 或 hook。
7. 如果涉及视觉，先收敛 token 和 theme vars，再做组件样式。

## 7. 近期优先级

1. 把 `apps/desktop/src/main/ipc.ts` 拆成按领域分文件。
2. 把 `workspace-service.ts` 按 `tabs / sessions / transfers` 拆子模块。
3. 把 `renderer/App.tsx` 中连接管理、文件面板、传输面板、顶部标签拆成 feature 组件。
4. 把 SSH 与 FTP controller 从同一个文件里分离。
5. 把共享类型继续收敛到 `packages/core`。
6. 继续稳定主题系统，避免颜色、阴影、圆角散落在业务组件里。
7. 把系统信息采集从 Linux-only 脚本逐步整理成可扩展的多平台能力，先立住 `raw metrics -> localized renderer` 的边界。

## 8. 发版操作规范

### 版本号管理（硬性要求）

**禁止手动单独修改任何 workspace 的 `version` 或 `dependencies` 里的内部包版本。** 手动改只会改一部分，导致 CI `npm install` 时去 npm registry 查找根本不存在的私有包版本，直接报 404 失败。

正确做法：
1. 只改根目录 `package.json` 的 `version` 字段
2. 立即运行 `npm run sync:version`
3. 脚本会自动同步所有 workspace 的 `version` + `dependencies` 里的 `@termdock/*` 引用 + `package-lock.json`

### 发版 SOP

```
# 1. main 上开发完，CI 绿
# 2. 改版本号
vim package.json   # 只改 version 字段
npm run sync:version
git commit -am "chore: 版本号升级到 x.x.x"
git push origin main

# 3. 切 release 分支
git checkout -b release/x.x.x
git push origin release/x.x.x

# 4. 打 tag → 自动触发 CI 构建 + 发布 Release
git tag vx.x.x
git push origin vx.x.x

# 5. 发完后提 PR 合回 main
```

**关键约束**：tag 必须指向 `release/*` 分支上的 commit，否则 `validate-release-tag` 步骤会拒绝构建。

## 9. 文档维护规则

- `AGENTS.md` 只放入口地图和硬约束，保持短小。
- 稳定架构事实放 `docs/architecture.md`。
- 阶段目标放 `docs/roadmap.md`。
- 跨文件或跨层任务放 `docs/plans/active/`，完成后移到 `docs/plans/completed/`。
- 已确认的架构选择放 `docs/decisions/`。
- 质量、测试、发布和安全检查放 `docs/quality/`。
- `.agents/` 只放协作草案和扩展设计，不放生产运行代码。
- 项目内技能统一放在 `.agents/skills/`，不要再写回 `.codex/`。
- 系统信息、监控、诊断类能力优先把采集规划写进 `docs/plans/active/`，并保持 renderer 不承担平台差异判断。

一句话结论：TermDock 现在最重要的不是继续堆功能，而是边推进功能边把 `protocol / service / UI / type / theme` 的边界立住。
