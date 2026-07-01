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

## 技术栈

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

## 为什么做 FileTerm

远程工作每天都在发生，但工具常常被割裂成终端、文件管理器、传输窗口和连接配置。FileTerm 想做的是一个真正面向日常工作的桌面 remote workspace：

- 开 SSH 时，终端和 SFTP 文件面板自然联动。
- 开 FTP 时，界面直接进入 file-only 工作流。
- 多个连接通过 tabs 并行存在，不互相打断。
- 上传、下载、进度和错误状态进入统一 transfer system。
- 连接配置、工作区状态和主题体验都为长期使用而设计。

第一版目标不是“支持所有协议”，而是把 `SSH / SFTP / FTP` 这条最高频链路做稳、做顺、做漂亮。

## 核心能力

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| SSH profile 管理 | 已完成 | 新增、编辑、删除、文件夹分组、JSON 文件持久化 |
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
| 窗口管理 | 已完成 | 主窗口、连接管理器、命令管理器、文件编辑器独立窗口 |

## 外部开源项目

FileTerm 的核心交互里使用了两个成熟的开源项目：

- [xterm.js](https://xtermjs.org/)：用于 SSH 终端渲染、输入输出和窗口 resize。
- [Monaco Editor](https://microsoft.github.io/monaco-editor/)：用于文件编辑器，提供语法高亮、编辑体验和查找替换。

## 架构原则

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

## 快速开始

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

## 仓库结构

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

## 路线图

当前重点：

1. 稳住 `SSH / SFTP / FTP` MVP 主链路。
2. 拆分 `ipc.ts`、`workspace-service.ts`、`session-controllers.ts`、`App.tsx`。
3. 把领域类型继续收敛到 `packages/core`。
4. 完善 transfer center、错误提示、主题、终端输入、文件抽屉和桌面壳体验。
5. 准备 macOS / Windows 可分发版本。

完整计划见 [docs/roadmap.md](./docs/roadmap.md)。

## 协作方式

这个仓库把代码库本身当作记录系统：

- `AGENTS.md` 是入口地图，不是百科全书。
- 稳定架构事实写入 `docs/architecture.md`。
- 跨层任务写入 `docs/plans/active/`。
- 已确认的架构选择写入 `docs/decisions/`。
- `.agents/extensions/` 用于功能草案和扩展设计。

如果你要贡献一个较大的功能，建议先补一份 active plan，再开始改代码。

## 贡献者

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

<a id="english"></a>

---

## English

<p>
  <kbd><a href="#中文">中文</a></kbd>
  <kbd><a href="#english">English</a></kbd>
</p>

FileTerm is a modern desktop remote workspace for developers and operations teams. It brings SSH terminals, SFTP files, FTP files, workspace tabs, and transfer tasks into one focused desktop client.

### Highlights

- SSH sessions with terminal and SFTP file panels.
- File editing powered by Monaco Editor.
- FTP sessions with a clean file-only workflow.
- Workspace tabs for parallel remote work.
- A refined desktop shell with collapsible sidebars, a file drawer, workspace focus mode, and animated tab transitions.
- A floating frosted command bar above the terminal output.
- A two-pane Monaco editor with a file tree and editor area.
- Unified transfer center for uploads, downloads, progress, and errors.
- A layered Electron architecture: `main -> preload -> renderer`.
- MIT licensed and open for collaboration.

### Open Source Components

FileTerm uses two well-known open source projects in its core UI:

- [xterm.js](https://xtermjs.org/) for SSH terminal rendering, input/output, and resize handling.
- [Monaco Editor](https://microsoft.github.io/monaco-editor/) for file editing, syntax highlighting, and search/replace.

### Contributors

- **StOff31** built the complete backend logic, including Electron main process services, IPC, session control, file capabilities, and workspace state.
- **Flashhhhhhzj** refactored and designed the frontend styling, unified the design language, and shaped the theme tokens, component skins, and visual experience.

### Quick Start

```bash
npm install
npm run dev
```

### Docs

- [Architecture](./docs/architecture.md)
- [Roadmap](./docs/roadmap.md)
- [Agent Guide](./AGENTS.md)

<p align="right">
  <a href="#readme-top">Back to top</a>
</p>

## 开源协议

FileTerm 使用 [MIT License](./LICENSE) 开源。

## Star History

<a href="https://star-history.com/#St0ff3l/fileterm&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=St0ff3l/fileterm&type=Date&theme=dark" />
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=St0ff3l/fileterm&type=Date" />
    <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=St0ff3l/fileterm&type=Date" />
  </picture>
</a>
