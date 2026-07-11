# Git Branch and Release Convention

FileTerm 采用标准 GitHub 流程：所有功能、修复和版本号更新先通过 Pull Request 合入 `main`；`release/*` 仅保存从 `main` 切出的、用于打包发布的不可变版本快照。

## Branch responsibilities

- `main`：最新稳定基线，也是所有常规 Pull Request 的目标分支。
- `feat/*`、`fix/*`、`chore/*`：日常开发分支，完成验证后通过 Pull Request 合入 `main`。
- `release/<version>`：从包含目标版本号的 `main` 创建，只用于发布检查、tag 和打包；不接收常规功能开发。
- `hotfix/*`：已发布版本的紧急修复分支，修复后通过 Pull Request 合入 `main`。

## Release flow

1. 在功能、修复或版本分支完成改动；版本号只修改根 `package.json`，随后运行 `npm run sync:version`。
2. 提 Pull Request 到 `main`，并确认 CI、类型检查、测试与构建全部通过。
3. 合并 Pull Request 后，从最新 `main` 创建并推送 `release/<version>`。
4. 在该 release 分支对应提交创建 `v<version>` tag 并推送；GitHub Actions 验证 tag 位于 `release/*` 后执行 macOS 与 Windows 打包。
5. 稳定版本 tag（如 `v1.0.0`）创建正式 GitHub Release；带预发布后缀的 tag（如 `v1.0.1-beta.1`）创建 prerelease。

## Guardrails

- 不直接向 `main` 或 `release/*` 推送常规功能改动。
- `release/*` 只用于发布快照，不作为日常集成分支。
- tag、根版本号和各 workspace 的同步版本必须一致。
- tag 必须指向 `origin/release/*` 中的提交，否则发布工作流会拒绝构建。
