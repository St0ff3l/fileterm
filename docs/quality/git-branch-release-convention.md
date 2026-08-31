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
4. 在该 release 分支对应提交创建 `v<version>` tag 并推送；GitHub Actions 验证 tag 位于 `release/*` 后执行各平台打包。
5. 稳定版本 tag（如 `v1.0.0`）创建正式 GitHub Release；带预发布后缀的 tag（如 `v1.0.1-beta.1`）创建 prerelease。

## Release notes 与贡献者展示

### 正文格式

每个版本可以新增 `docs/release-notes/release-notes-<version>.md`，例如 `docs/release-notes/release-notes-2.2.0-beta.1.md`；正文结构可参考仓库中现有的 `docs/release-notes/release-notes-*.md` 文件：

- 版本简介
- 更新重点
- 主要 PR 与问题修复
- 使用提示、反馈渠道或其他发布说明

正文中不要添加手写的 `### Contributors`、贡献者名单或头像链接。贡献者区域由 GitHub 根据本次 Release 的提交和 Pull Request 自动生成，避免手工名单与实际变更不一致。

### 自动生成规则

发布 workflow 始终调用 `gh release create --generate-notes`。如果存在版本说明文件，文件内容通过 `--notes` 作为自定义正文传入；不要改回只使用 `--notes-file`。这样 GitHub 会在自定义正文之后继续生成 `What's Changed`、`Full Changelog` 和带头像的 `Contributors` 区域。

发布完成后打开 Release 页面确认：自定义正文存在，且 GitHub 自动生成的贡献者头像区域也存在。

## Guardrails

- 不直接向 `main` 或 `release/*` 推送常规功能改动。
- `release/*` 只用于发布快照，不作为日常集成分支。
- tag、根版本号和各 workspace 的同步版本必须一致。
- tag 必须指向 `origin/release/*` 中的提交，否则发布工作流会拒绝构建。
