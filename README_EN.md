<a id="readme-top"></a>

<div align="center">
  <br />
  <img alt="FileTerm" src="https://readme-typing-svg.demolab.com?font=JetBrains+Mono&weight=800&size=34&duration=2600&pause=900&color=38BDF8&center=true&vCenter=true&width=760&lines=FileTerm;SSH+%2B+SFTP+%2B+FTP+Workspace;A+Modern+Remote+Desktop+Workbench" />
  <br />
  <br />
  <p><strong>A modern desktop remote workspace for developers and operations teams. The official release is now available.</strong></p>
  <p>SSH terminals, SFTP and FTP files, workspace tabs, and transfer tasks in one focused desktop client.</p>
  <p>
    <kbd><a href="./README.md">中文</a></kbd>
    <kbd>English</kbd>
  </p>
  <p>
    <a href="./LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-111827?style=for-the-badge"></a>
    <a href="https://github.com/St0ff3l/fileterm/releases/latest"><img alt="Status" src="https://img.shields.io/badge/status-Official%20Release-22C55E?style=for-the-badge"></a>
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-6366F1?style=for-the-badge">
    <a href="https://github.com/St0ff3l/fileterm/stargazers"><img alt="GitHub stars" src="https://img.shields.io/github/stars/St0ff3l/fileterm?style=for-the-badge&logo=github&label=stars"></a>
  </p>
</div>

---

## Download the Official Release

Download the latest release from [GitHub Releases](https://github.com/St0ff3l/fileterm/releases/latest):

- **macOS**: packages for Apple Silicon (arm64) and Intel (x64).
- **Windows**: x64 installer and portable packages.

Want to run from source or contribute? Continue to [Getting Started from Source](#getting-started-from-source).

## What Is FileTerm?

FileTerm is built for daily remote work by developers and operations teams. It brings remote terminals, file management, transfer tasks, and connection profiles into one desktop workspace.

- SSH sessions pair terminal work naturally with SFTP file panels.
- FTP/FTPS sessions use a focused file-only workflow.
- Multiple connections run side by side in tabs without interrupting each other.
- Uploads, downloads, progress, and errors are managed in one transfer center.
- Connection profiles, workspace state, and themes are designed for long-term daily use.

<p align="center"><img width="900" alt="FileTerm workspace preview" src="https://github.com/user-attachments/assets/06f14b5d-d722-456f-ac90-342a19b6915c" /></p>

The official release focuses on the most frequent `SSH / SFTP / FTP / FTPS` workflows: reliable remote terminals, file operations, and transfers.

## Core Capabilities

| Capability                  | Description                                                                                                                  |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| SSH profile management      | Create, edit, delete, and group SSH profiles with JSON persistence.                                                          |
| FTP/FTPS profile management | Uses a connection model kept separate from SSH and supports secure FTP transfers.                                            |
| SSH terminal                | Powered by xterm.js with input/output, resize, search, clipboard support, and a floating command bar.                        |
| SFTP file management        | Browse remote directories; read, write, create, delete, rename, and change permissions.                                      |
| FTP/FTPS file management    | FTP/FTPS sessions with remote browsing, file operations, and resumable transfers.                                            |
| File editor                 | A Monaco Editor-based two-pane file tree and editor with syntax highlighting, find/replace, encoding, and language controls. |
| Terminal directory sync     | Keeps the active SSH working directory and file manager in sync.                                                             |
| Root privilege sync         | Detects `sudo` / `su` in the terminal and synchronizes the file-manager permission context.                                  |
| Virtualized file list       | Uses virtual scrolling for efficient browsing of large remote directories.                                                   |
| Transfer center             | Centralized uploads, downloads, resume support, progress, speed, cancellation, and recursive folder transfers.               |
| Workspace tabs              | Parallel connections, disconnect/reconnect, persisted state, and animated tab transitions.                                   |
| Command templates           | Grouped quick commands, parameter placeholders, and one-click sending.                                                       |
| Theme system                | Dark and light themes through tokens, CSS variables, and component skins.                                                    |
| Remote status monitoring    | Connection status and system-resource summaries.                                                                             |
| Theme and desktop shell     | Dark and light themes, sidebars, file drawer, focus mode, and dedicated management windows.                                  |

## Technology Stack

| Desktop  | Renderer     | Language   | Terminal | Editor        | Protocols        | Tooling        |
| -------- | ------------ | ---------- | -------- | ------------- | ---------------- | -------------- |
| Electron | React + Vite | TypeScript | xterm.js | Monaco Editor | ssh2 + basic-ftp | npm workspaces |

```txt
main process   -> Electron services, IPC, protocol lifecycle
preload bridge -> Secure API boundary for renderer
renderer UI    -> React workspace, tabs, files, terminal
protocols      -> SSH shell, SFTP, FTP/FTPS adapters
theme system   -> tokens -> vars -> skins -> terminal colors
```

## Architecture Principles

```txt
Renderer UI
  -> Application State
    -> Preload API
      -> IPC
        -> Desktop Services
          -> Session Controllers
            -> Protocol Clients
```

- `packages/core` is the single source of truth for domain models.
- The renderer never accesses SSH, SFTP, or FTP/FTPS protocol clients directly.
- All system capabilities are exposed through `main -> preload -> renderer`.
- SSH/SFTP and FTP/FTPS remain separate at controller and protocol layers.
- Transfer progress enters one transfer system instead of being maintained separately by components.

See [docs/architecture.md](./docs/architecture.md) for the complete architecture map.

## Getting Started from Source

### Requirements

- Node.js >= 22.12.0
- npm

### Install and Run

```bash
npm install
npm run dev
```

### Common Commands

```bash
npm run dev
npm run typecheck
npm run build
npm run release:mac
npm run release:win
```

## Repository Layout

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
  AGENTS.md                  # map for human and AI collaborators
```

## Roadmap

Post-release priorities:

1. Keep improving the stability and usability of the `SSH / SFTP / FTP / FTPS` core workflows.
2. Split `workspace-service.ts`, `session-controllers.ts`, and `App.tsx` to clarify layer boundaries.
3. Continue consolidating domain types in `packages/core`.
4. Improve transfer tasks, errors, themes, terminal input, file drawers, and the desktop shell.
5. Maintain release quality and distribution for macOS and Windows.

See [docs/roadmap.md](./docs/roadmap.md) for the full plan.

## Open Source Components

- [xterm.js](https://xtermjs.org/) for SSH terminal rendering, input/output, and resize handling.
- [Monaco Editor](https://microsoft.github.io/monaco-editor/) for file editing, syntax highlighting, and search/replace.

## Contributing

This repository treats the codebase as its record system:

- [AGENTS.md](./AGENTS.md) is the entry map for collaboration.
- [docs/architecture.md](./docs/architecture.md) records stable architecture facts.
- [docs/plans/active](./docs/plans/active) contains active cross-layer plans.
- [docs/decisions](./docs/decisions) records confirmed architecture decisions.
- [.agents/extensions](./.agents/extensions) contains feature drafts and extension designs.

For a substantial feature, please add or update an active plan before writing code.

## Contributors

Thank you to everyone who makes FileTerm better.

<table>
  <tr>
    <td align="center" width="180">
      <a href="https://github.com/St0ff3l">
        <img src="https://avatars.githubusercontent.com/St0ff3l?s=120" width="72" height="72" alt="St0ff3l" />
        <br />
        <sub><b>St0ff3l</b></sub>
      </a>
    </td>
    <td>Built the backend core, including Electron main-process services, IPC, session control, file capabilities, and workspace state.</td>
  </tr>
  <tr>
    <td align="center" width="180">
      <a href="https://github.com/Flashhhhhhzj">
        <img src="https://avatars.githubusercontent.com/Flashhhhhhzj?s=120" width="72" height="72" alt="Flashhhhhhzj" />
        <br />
        <sub><b>Flashhhhhhzj</b></sub>
      </a>
    </td>
    <td>Redesigned and modernized the frontend styling, unified the design language, and shaped theme tokens, component skins, and the visual experience.</td>
  </tr>
</table>

## Community

Scan the QR code to join the **FileTerm** WeChat community for usage discussions, feedback, and product updates.

You can also join the QQ group: `534418986`.

![FileTerm WeChat community QR code](./docs/assets/fileterm-wechat-group-qr.jpg)

## Support the Project

If FileTerm helps you, please consider starring the project on [GitHub](https://github.com/St0ff3l/fileterm):

<a href="https://github.com/St0ff3l/fileterm/stargazers"><img alt="GitHub stars" src="https://img.shields.io/github/stars/St0ff3l/fileterm?style=for-the-badge&logo=github&label=Star%20FileTerm"></a>

## License

FileTerm is open-sourced under the [MIT License](./LICENSE).

<p align="right"><a href="#readme-top">Back to top</a></p>
