<a id="readme-top"></a>

<div align="center">
  <br />
  <img alt="FileTerm" src="https://readme-typing-svg.demolab.com?font=JetBrains+Mono&weight=800&size=34&duration=2600&pause=900&color=38BDF8&center=true&vCenter=true&width=760&lines=FileTerm;SSH+%2B+SFTP+%2B+FTP+Workspace;A+Modern+Remote+Desktop+Workbench" />
  <br />
  <br />

  <p>
    <strong>一个为开发者和运维场景打造的现代桌面远程工作台。</strong>
  </p>
  <p>
    SSH 终端、SFTP 文件、FTP 文件、多标签工作区和传输任务中心，收束到一个顺手的桌面客户端里。
  </p>

  <p>
    <strong>A modern remote workspace desktop workbench built for developers and ops teams.</strong>
  </p>
  <p>
    SSH terminal, SFTP files, FTP files, multi-tab workspace, and transfer center all in one focused desktop client.
  </p>

  <p>
    <kbd><a href="#中文">中文</a></kbd>
    <kbd><a href="#english">English</a></kbd>
  </p>

  <p>
    <a href="./LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-111827?style=for-the-badge"></a>
    <img alt="Status" src="https://img.shields.io/badge/status-MVP%20in%20progress-22C55E?style=for-the-badge">
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-6366F1?style=for-the-badge">
  </p>
</div>

---

<a id="中文"></a>

## 中文

### 技术栈

| Desktop | Renderer | Language | Terminal | Editor | Protocols | Tooling |
| --- | --- | --- | --- | --- | --- | --- |
| <img src="https://img.shields.io/badge/Electron-42-47848F?style=flat-square&logo=electron&logoColor=white" alt="Electron" /> | <img src="https://img.shields.io/badge/React-19-149ECA?style=flat-square&logo=react&logoColor=white" alt="React" /> <img src="https://img.shields.io/badge/Vite-8-646CFF?style=flat-square&logo=vite&logoColor=white" alt="Vite" /> | <img src="https://img.shields.io/badge/TypeScript-6-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript" /> | <img src="https://img.shields.io/badge/xterm.js-111827?style=flat-square" alt="xterm.js" /> | <img src="https://img.shields.io/badge/Monaco%20Editor-007ACC?style=flat-square&logo=visualstudiocode&logoColor=white" alt="Monaco Editor" /> | <img src="https://img.shields.io/badge/ssh2-0F766E?style=flat-square" alt="ssh2" /> <img src="https://img.shields.io/badge/basic--ftp-2563EB?style=flat-square" alt="basic-ftp" /> | <img src="https://img.shields.io/badge/npm%20workspaces-CB3837?style=flat-square&logo=npm&logoColor=white" alt="npm workspaces" /> |

```txt
main process   ████████████████████  Electron services, IPC, protocol lifecycle
preload bridge ████████████████░░░░  Secure API boundary for renderer
renderer UI    ███████████████████░  React workspace, tabs, files, terminal
protocols      ███████████████░░░░░  SSH shell, SFTP, FTP adapters
theme system   ████████████████░░░░  tokens -> vars -> skins -> terminal colors
```

### 为什么做 FileTerm

远程工作每天都在发生，但工具常常被割裂成终端、文件管理器、传输窗口和连接配置。FileTerm 想做的是一个真正面向日常工作的桌面 remote workspace：

- 开 SSH 时，终端和 SFTP 文件面板自然联动。
- 开 FTP 时，界面直接进入 file-only 工作流。
- 多个连接通过 tabs 并行存在，不互相打断。
- 上传、下载、进度和错误状态进入统一 transfer system。
- 连接配置、工作区状态和主题体验都为长期使用而设计。

第一版目标不是“支持所有协议”，而是把 `SSH / SFTP / FTP` 这条最高频链路做稳、做顺、做漂亮。

### 核心能力

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| SSH profile 管理 | 已完成 | 新增、编辑、删除、文件夹分组、JSON 文件持久化、group/parentId 双向同步 |
| FTP profile 管理 | 已完成 | 独立于 SSH 的连接模型 |
| SSH shell | 已完成 | xterm.js 渲染、输入输出、自适应 resize、搜索、剪贴板互通、雾透悬浮命令输入条 |
| 文件编辑器 | 已完成 | Monaco Editor 提供双栏文件树/编辑区、语法高亮、查找替换、编码与语言切换 |
| SFTP 文件管理 | 已完成 | 远程目录浏览、读/写/新建/删除/重命名/权限修改 |
| FTP 文件管理 | 已完成 | FTP 会话与远程文件能力 |
| Transfer center | 已完成 | 上传下载任务队列、进度、速度、取消、文件/文件夹递归 |
| Workspace tabs | 已完成 | 多标签并行连接、断开/重连、session 状态持久化、标签切换动效 |
| Theme system | 已完成 | tokens → CSS vars → skin，深色/浅色主题一键切换 |
| 远程连接状态 | 已完成 | 连接状态提示、系统资源监控面板、侧栏收起态资源摘要 |
| 命令模板 | 已完成 | 快捷命令模板、文件夹分组、参数占位符、一键发送 |
| 桌面壳与布局 | 已完成 | macOS 标题栏避让、侧栏收起、文件面板抽屉、工作区焦点模式、macOS template 托盘图标 |
| 工作区侧边栏 | 已完成 | 概览 → 连接管理器 → 命令管理器 → 设置四页导航 |
| 窗口管理 | 已完成 | 主窗口、连接管理器、命令管理器、文件编辑器独立窗口 |

### 外部开源项目

FileTerm 的核心交互里使用了两个成熟的开源项目：

- [xterm.js](https://xtermjs.org/)：用于 SSH 终端渲染、输入输出和窗口 resize。
- [Monaco Editor](https://microsoft.github.io/monaco-editor/)：用于文件编辑器，提供语法高亮、编辑体验和查找替换。

### 架构原则

FileTerm 从第一天就避免把远程协议揉成一个模糊的大对象。

```txt
Renderer UI
  -> Application State
    -> Preload API
      -> IPC
        -> Desktop Services
          -> Session Controllers
            -> Protocol Clients
```

硬性边界：

- `packages/core` 是领域模型的 single source of truth。
- Renderer 不直接访问 SSH / SFTP / FTP protocol clients。
- 所有系统能力必须走 `main -> preload -> renderer`。
- SSH/SFTP 与 FTP 在 controller/protocol 层保持分离。
- Transfer 进度统一进入 transfer system，不在组件里零散维护。

更完整的说明见 [docs/architecture.md](./docs/architecture.md)。

### 快速开始

要求：

- Node.js >= 20
- npm

```bash
npm install
npm run dev
```

常用命令：

```bash
npm run dev
npm run typecheck
npm run build
```

### 仓库结构

```txt
fileterm/
  apps/
    desktop/                 # Electron + React desktop app
      src/
        main/                # main process, IPC, services
        preload/             # secure renderer API
        renderer/            # React workspace UI
  packages/
    core/                    # domain types
    storage/                 # repository abstractions
    shared/                  # shared constants and utilities
  docs/
    architecture.md          # architecture map
    roadmap.md               # product roadmap
    plans/                   # active and completed execution plans
    decisions/               # architecture decisions
    quality/                 # quality and release checks
  AGENTS.md                  # short map for human/AI collaborators
```

### 路线图

当前重点：

1. 稳住 `SSH / SFTP / FTP` MVP 主链路。
2. 拆分 `ipc.ts`、`workspace-service.ts`、`session-controllers.ts`、`App.tsx`。
3. 把领域类型继续收敛到 `packages/core`。
4. 完善 transfer center、错误提示、主题、终端输入、文件抽屉和桌面壳体验。
5. 准备 macOS / Windows 可分发版本。

完整计划见 [docs/roadmap.md](./docs/roadmap.md)。

### 协作方式

这个仓库把代码库本身当作记录系统：

- `AGENTS.md` 是入口地图，不是百科全书。
- 稳定架构事实写入 `docs/architecture.md`。
- 跨层任务写入 `docs/plans/active/`。
- 已确认的架构选择写入 `docs/decisions/`。
- `.agents/extensions/` 用于功能草案和扩展设计。

如果你要贡献一个较大的功能，建议先补一份 active plan，再开始改代码。

### 贡献者

感谢每一位让 FileTerm 变得更好的贡献者。

<table>
  <tr>
    <td align="center" width="180">
      <a href="https://github.com/St0ff3l">
        <img src="https://avatars.githubusercontent.com/St0ff3l?s=120" width="72" height="72" alt="StOff31" />
        <br />
        <sub><b>St0ff3l</b></sub>
      </a>
    </td>
    <td>
      构建了完整的后端逻辑，打通 Electron main process、IPC、会话控制、文件能力与工作区状态等核心链路。
    </td>
  </tr>
  <tr>
    <td align="center" width="180">
      <a href="https://github.com/Flashhhhhhzj">
        <img src="https://avatars.githubusercontent.com/Flashhhhhhzj?s=120" width="72" height="72" alt="Flashhhhhhzj" />
        <br />
        <sub><b>Flashhhhhhzj</b></sub>
      </a>
    </td>
    <td>
      重构并设计了前端样式，统一设计语言，推动主题 token、组件皮肤和整体视觉体验成形。
    </td>
  </tr>
</table>

---

<a id="english"></a>

## English

### Tech Stack

| Desktop | Renderer | Language | Terminal | Editor | Protocols | Tooling |
| --- | --- | --- | --- | --- | --- | --- |
| <img src="https://img.shields.io/badge/Electron-42-47848F?style=flat-square&logo=electron&logoColor=white" alt="Electron" /> | <img src="https://img.shields.io/badge/React-19-149ECA?style=flat-square&logo=react&logoColor=white" alt="React" /> <img src="https://img.shields.io/badge/Vite-8-646CFF?style=flat-square&logo=vite&logoColor=white" alt="Vite" /> | <img src="https://img.shields.io/badge/TypeScript-6-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript" /> | <img src="https://img.shields.io/badge/xterm.js-111827?style=flat-square" alt="xterm.js" /> | <img src="https://img.shields.io/badge/Monaco%20Editor-007ACC?style=flat-square&logo=visualstudiocode&logoColor=white" alt="Monaco Editor" /> | <img src="https://img.shields.io/badge/ssh2-0F766E?style=flat-square" alt="ssh2" /> <img src="https://img.shields.io/badge/basic--ftp-2563EB?style=flat-square" alt="basic-ftp" /> | <img src="https://img.shields.io/badge/npm%20workspaces-CB3837?style=flat-square&logo=npm&logoColor=white" alt="npm workspaces" /> |

```txt
main process   ████████████████████  Electron services, IPC, protocol lifecycle
preload bridge ████████████████░░░░  Secure API boundary for renderer
renderer UI    ███████████████████░  React workspace, tabs, files, terminal
protocols      ███████████████░░░░░  SSH shell, SFTP, FTP adapters
theme system   ████████████████░░░░  tokens -> vars -> skins -> terminal colors
```

### Why FileTerm

Remote work happens daily, but tools are often fragmented into separate applications for terminal emulators, file managers, transfer dialogs, and connection configuration. FileTerm aims to provide a unified desktop remote workspace for day-to-day operations:

- When opening an SSH session, the terminal and the SFTP file explorer panel are naturally linked.
- When opening an FTP connection, the interface directly initiates a clean, file-only workflow.
- Multiple active sessions run in parallel via workspace tabs without interrupting each other.
- Uploads, downloads, progress queues, and error statuses are handled by a unified transfer system.
- Connection profiles, workspace state management, and the overall visual themes are optimized for long-term usability.

The goal of the initial version is not to support every possible protocol, but rather to build a highly stable, seamless, and visually appealing experience for the core `SSH / SFTP / FTP` workflows.

### Core Capabilities

| Capability | Status | Description |
| --- | --- | --- |
| SSH Profile Management | Completed | Create, edit, delete connection profiles, group profiles into folders, persist using JSON files, and synchronize `group` name and `parentId` bidirectionally |
| FTP Profile Management | Completed | Separate connection model independent of SSH profiles |
| SSH Shell | Completed | Powered by xterm.js, input/output handling, adaptive resizing, text search, clipboard sync, and floating frosted command bar |
| File Editor | Completed | Powered by Monaco Editor, dual-pane layout (file tree & edit area), syntax highlighting, search/replace, encoding, and language selection |
| SFTP File Explorer | Completed | Directory navigation, read/write actions, create/delete files/folders, rename, and permissions modification (chmod) |
| FTP File Explorer | Completed | Clean file-only session management and remote file actions |
| Transfer Center | Completed | Upload/download queue, progress updates, speed rates, task cancellation, and recursive directory handling |
| Workspace Tabs | Completed | Multi-tab parallel connections, disconnect/reconnect, session persistence, and smooth tab transition animations |
| Theme System | Completed | tokens → CSS variables → components skins, and one-click dark/light mode toggle |
| Connection Status Panel | Completed | Active connection health indicator, system resource usage graphs, and sidebar collapsed metadata overview |
| Command Templates | Completed | Quick snippets, folder nesting, parameter placeholders, and single-click terminal dispatching |
| Desktop Shell & Layout | Completed | macOS native title bar spacing, collapsible sidebars, file drawer panel, focus mode toggle, and macOS template tray icon |
| Workspace Sidebar | Completed | Overview → Connection Manager → Command Manager → Settings navigation |
| Window Manager | Completed | Independent windows for main app, connection manager, command manager, and file editor |

### External Open Source Projects

FileTerm's core interactions leverage two mature open-source projects:

- [xterm.js](https://xtermjs.org/): Used for SSH terminal rendering, user input, and terminal dimensions resizing.
- [Monaco Editor](https://microsoft.github.io/monaco-editor/): Powering the file editor with rich syntax highlighting, code editing, and search/replace features.

### Architecture Principles

From day one, FileTerm has avoided mixing different remote protocols into a single bloated class.

```txt
Renderer UI
  -> Application State
    -> Preload API
      -> IPC
        -> Desktop Services
          -> Session Controllers
            -> Protocol Clients
```

Hard Boundaries:

- `packages/core` acts as the single source of truth for all domain models.
- The React Renderer process never directly accesses SSH / SFTP / FTP protocol clients.
- All OS-level or backend capabilities must travel through the `main -> preload -> renderer` bridge.
- SSH/SFTP and FTP controller/protocol layers are strictly separated.
- File transfers and progress monitoring are handled globally by the unified transfer system.

For a detailed walkthrough, please see [docs/architecture.md](./docs/architecture.md).

### Quick Start

Prerequisites:

- Node.js >= 20
- npm

```bash
npm install
npm run dev
```

Available npm scripts:

```bash
npm run dev
npm run typecheck
npm run build
```

### Repository Structure

```txt
fileterm/
  apps/
    desktop/                 # Electron + React desktop app
      src/
        main/                # main process, IPC, services
        preload/             # secure renderer API
        renderer/            # React workspace UI
  packages/
    core/                    # domain types
    storage/                 # repository abstractions
    shared/                  # shared constants and utilities
  docs/
    architecture.md          # architecture map
    roadmap.md               # product roadmap
    plans/                   # active and completed execution plans
    decisions/               # architecture decisions
    quality/                 # quality and release checks
  AGENTS.md                  # short map for human/AI collaborators
```

### Roadmap

Current Priorities:

1. Finalize and stabilize the core `SSH / SFTP / FTP` MVP path.
2. Refactor and split code: `ipc/`, `workspace-service.ts`, `session-controllers.ts`, and `App.tsx`.
3. Consolidate and move all shared types into `packages/core`.
4. Improve the transfer center UI, global error reporting, theme stability, terminal input shortcuts, file drawers, and desktop shell integration.
5. Package and prepare production-ready distributables for macOS and Windows.

For the full scope, read [docs/roadmap.md](./docs/roadmap.md).

### Collaboration Workflows

This repository treats the codebase itself as the source of documentation:

- `AGENTS.md` is a short entry-level guide, not an exhaustive documentation hub.
- Stable structural information belongs in `docs/architecture.md`.
- Active multi-file development plans go to `docs/plans/active/`.
- Documented engineering decisions are kept in `docs/decisions/`.
- Feature drafts and draft proposals reside in `.agents/extensions/`.

If you are planning to contribute a significant feature, we recommend writing an active plan before making any code modifications.

### Contributors

We thank everyone who has contributed to making FileTerm a better workspace.

<table>
  <tr>
    <td align="center" width="180">
      <a href="https://github.com/St0ff3l">
        <img src="https://avatars.githubusercontent.com/St0ff3l?s=120" width="72" height="72" alt="StOff31" />
        <br />
        <sub><b>St0ff3l</b></sub>
      </a>
    </td>
    <td>
      Designed and implemented the core backend logic, connecting Electron main processes, IPC handlers, session controllers, file services, and workspace state synchronization.
    </td>
  </tr>
  <tr>
    <td align="center" width="180">
      <a href="https://github.com/Flashhhhhhzj">
        <img src="https://avatars.githubusercontent.com/Flashhhhhhzj?s=120" width="72" height="72" alt="Flashhhhhhzj" />
        <br />
        <sub><b>Flashhhhhhzj</b></sub>
      </a>
    </td>
    <td>
      Redesigned and modernized the frontend styling, aligned theme tokens, created component skins, and polished the overall visual interface and layout.
    </td>
  </tr>
</table>

---

## 开源协议 / License

FileTerm is open-source software licensed under the [MIT License](./LICENSE).

## Star History

<a href="https://star-history.com/#St0ff3l/fileterm&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=St0ff3l/fileterm&type=Date&theme=dark" />
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=St0ff3l/fileterm&type=Date" />
    <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=St0ff3l/fileterm&type=Date" />
  </picture>
</a>
