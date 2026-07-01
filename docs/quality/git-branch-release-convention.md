# Git Branch and Release Convention

本文记录 FileTerm 当前采用的 Git 分支与发版协定。目标是保护 `main` 稳定性，同时让日常开发、发版测试、自动化打包和紧急修复有清晰入口。

## 1. 核心原则

- `main` 是稳定主分支，只放已经发布或确认可正式发布的代码。
- `main` 不按天更新，而是在正式发布、紧急修复发布或阶段稳定收口时更新。
- 功能开发不直接提交到 `main`。
- 发版准备、测试、打包和修复优先在版本 `release/*` 分支完成。
- 具体功能开发使用 `feat/*` 或 `feature/*` 分支。
- 常规修复使用 `fix/*` 分支。
- 已发布版本的紧急线上修复使用 `hotfix/*` 分支。

一句话：`feat / feature / fix` 可以频繁变化，`release/*` 用于版本集成，`main` 只在稳定发布点变化。

## 2. 分支职责

### main

`main` 代表当前最新稳定版本。

允许进入 `main` 的内容：

- 已通过发布检查的 `release/*` 分支。
- 已通过验证的 `hotfix/*` 分支。
- 明确确认可作为稳定基线的阶段性收口。

不建议直接进入 `main` 的内容：

- 未完成的功能。
- 只在本地验证过、还没有经过发版检查的代码。
- 会影响打包、协议链路或跨平台行为但没有完成回归的改动。

## 3. release 分支

`release/*` 用于发版集成、测试、打包和发布前修复。

当前约定每个目标版本保留一个发版分支：

```text
release/0.1.0-beta.10
release/1.0.0
```

历史上出现过 `release-20260608` 这类日期分支，后续不再新增这种命名。新的发版分支统一使用 `release/<version>`。

不在 `release/*` 上做大块功能开发。常规功能和修复先进入 `feat/*`、`feature/*` 或 `fix/*`，确认没问题后再通过 PR 合回对应 `release/*`。

```text
release/0.1.0-beta.10
  <- feat/command-manager-quick-commands
  <- fix/csp-unsafe-inline
```

`release/*` 分支承担以下职责：

- 接收已经开发完成并通过验证的 `feat/*`、`feature/*` 和 `fix/*`。
- 执行发布前质量检查。
- 处理发布前发现的问题。
- 确认可发布后合并到 `main`。
- 合并到 `main` 后，在 `main` 上创建版本 tag。

## 4. feature 与 fix 分支

`feat/*` 或 `feature/*` 用于功能开发，`fix/*` 用于常规缺陷修复。

推荐命名：

```text
feat/command-manager-quick-commands
feat/connection-group-dropdown
fix/file-size-decimal-units
feature/sftp-panel
```

不推荐命名：

```text
feature/release
fix/release
```

开发流程：

```text
release/0.1.0-beta.10
  -> feat/sftp-panel
  -> release/0.1.0-beta.10
  -> main
```

如果某个版本内需要拆更细的功能分支，可以从版本开发分支继续拉出具体功能分支：

```text
release/0.1.0-beta.10
  -> feat/sftp-panel
  -> release/0.1.0-beta.10
```

## 5. hotfix 分支

`hotfix/*` 用于已经发布版本的紧急修复。

推荐命名：

```text
hotfix/1.0.1
hotfix/fix-mac-launch
```

常见流程：

```text
main
  -> hotfix/1.0.1
  -> main
```

如果仍有活跃的 `release/*` 分支，修复完成后也需要同步回对应 `release/*`，避免后续版本丢失修复。

## 6. main 更新时机

`main` 的更新以发布状态为准，不以固定时间为准。

通常在以下场景更新：

1. 正式版本发布：`release/*` 验证通过后合并到 `main`，并打 `vx.y.z` tag。
2. 紧急修复发布：`hotfix/*` 验证通过后合并到 `main`，并打修复版本 tag。
3. 阶段稳定收口：当前阶段功能已经完成主要回归，可以作为新的稳定基线。

早期开发阶段可以让 `main` 更新频率略高，但一旦开始对外发版，`main` 应保持为最新稳定发布基线。

## 7. Tag 约定

版本 tag 在 `main` 上创建。

推荐格式：

```text
v1.0.0
v1.0.1
v0.1.0-beta.1
```

tag 创建前应确认：

- 目标 `release/*` 或 `hotfix/*` 已合并到 `main`。
- 发布前检查已经完成。
- 应用版本号与 tag 对应。
- 打包产物符合当前发布范围。

## 8. 推荐流程示例

以 `0.1.0-beta.10` 为例：

```text
main
  -> release/0.1.0-beta.10
  -> feat/*
  -> fix/*
  -> release/0.1.0-beta.10
  -> main
  -> tag v0.1.0-beta.10
```

实际操作顺序：

1. 从 `main` 拉出 `release/0.1.0-beta.10`。
2. 从 `release/0.1.0-beta.10` 拉出具体 `feat/*` 或 `fix/*` 分支。
3. 功能或修复完成后，通过 PR 合回 `release/0.1.0-beta.10`。
4. 在 `release/0.1.0-beta.10` 上完成测试、打包和发布前修复。
5. 确认可发布后，将 `release/0.1.0-beta.10` 合回 `main`。
6. 在 `main` 上打 `v0.1.0-beta.10` tag。

## 9. 与 CI 和发布检查的关系

本协定只定义分支流转。具体发布检查仍以专项文档和 GitHub Actions 为准：

- `.github/workflows/ci.yml`：在 `main` push 和 Pull Request 上运行 `npm ci`、`npm run typecheck` 与 `npm run build`。
- `.github/workflows/release.yml`：当前在 `v*-beta*` tag push 时构建 macOS 与 Windows 产物，并创建 prerelease。
- `release-beta-mac.md`：早期 `v0.1.0-beta.1` mac-only unsigned beta release 检查清单。

后续如果新增 Windows、Linux、签名、公证或自动更新发布流程，应在 `docs/quality/` 下补充对应检查清单。
