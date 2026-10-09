---
name: fileterm-release
description: 准备或发布 FileTerm 版本说明、release 分支、tag 和 GitHub Release，以及排查发布流水线时使用；普通 PR 不触发发布。
---

# FileTerm GitHub Release

## 目标

把 FileTerm 的版本发布拆成两部分：

1. 自定义正文：版本简介、更新重点、主要 PR/Issue、使用提示和反馈渠道。
2. GitHub 官方生成区：`What's Changed`、`New Contributors`、`Full Changelog`、贡献者头像和贡献者展示。

自定义正文只负责第一部分，绝不能手写或覆盖第二部分。

## 发布任务所需资料

按正在执行的发布步骤读取：

- `docs/quality/git-branch-release-convention.md`
- 创建 tag、触发发布或排查流水线时：`.github/workflows/release.yml`
- 目标版本对应的 `docs/release-notes/release-notes-<version>.md`（如果已经存在）

以仓库文件为准，不要凭记忆替换发布命令、分支规则或版本同步方式。

## 执行范围

准备版本说明不等于授权合并、推送 tag 或发布；只执行用户要求的步骤，已有明确授权不重复询问。PR 使用普通 Merge，Issue 只用 Refs / Related to 关联，发布后也须按用户确认保留 Issue 生命周期。

## 标准流程

### 1. 在普通分支完成版本说明和版本号

- 功能、修复、文档和版本号改动先进入普通分支并提 PR 到 `main`。
- 版本号只修改根目录 `package.json` 的 `version` 字段。
- 修改后立即运行 `npm run sync:version`，再检查 workspace 版本和 lockfile。
- 发布说明文件应在合入 `main` 前进入 PR，并统一放在 `docs/release-notes/`，例如：
  `docs/release-notes/release-notes-2.2.0-beta.1.md`。
- 不要把日常功能改动直接推到 `main` 或 `release/*`。

### 2. 发布说明正文格式

编写或审查发布说明时读取 [双语模板与固定格式](references/release-notes.md)。保留双语顺序、各自的安装指南徽章、固定标题和逐字反馈块。

### 3. PR 合入 main

- 提交 PR 后等待所有必需 CI 通过。
- 默认使用 GitHub 普通 Merge（保留 merge commit），不要自行 Squash/Rebase。
- 合并后执行 `git fetch origin main`，确认 `origin/main` 包含发布说明和目标版本号。

### 4. 创建不可变 release 分支和 tag

从最新 `origin/main` 创建并推送：

```bash
VERSION=2.2.0-beta.1
git switch -c "release/$VERSION" origin/main
git push -u origin "release/$VERSION"
git tag -a "v$VERSION" -m "FileTerm v$VERSION"
git push origin "v$VERSION"
```

约束：

- `release/<version>` 只保存发布快照，不在上面补正文、修功能或改 workflow。
- tag 必须指向 `origin/release/*` 分支上的提交，否则 `validate-release-tag` 会拒绝发布。
- tag 名必须与根版本号、workspace 同步版本和 release notes 文件名一致。
- Beta/RC tag 含预发布后缀，GitHub Release 应显示为 prerelease。

### 5. 保留 GitHub 官方生成内容

#### Release title（硬性规则）

GitHub Release 的 **Title 必须与 tag 完全一致，只写版本号**，例如 tag 为 `v2.2.26-beta.3` 时，Title 必须是 `v2.2.26-beta.3`。禁止在前面添加 `FileTerm`，也不要附加 `Release`、`Beta` 或其他说明。此规则只针对 GitHub Release 的 Title 输入框；正文标题仍按上面的双语模板书写。

使用 `gh release create` 时显式传入 `--title "$TAG"`。自动发布 workflow 也应把 title 设为当前 tag；发布验收时确认 Release 页面显示的 title 与 tag 完全相同。

发布 workflow 创建 Release 时必须同时传入自定义正文和 `--generate-notes`，使用仓库现有 workflow 的方式：

```bash
gh release create "$TAG" \
  --title "$TAG" \
  --notes "$(cat "docs/release-notes/release-notes-${VERSION}.md")" \
  --generate-notes \
  --prerelease
```

如果仓库 workflow 已经负责创建 Release，不要手动重复创建；应只推送正确 tag，然后监督 workflow。不要改成只使用 `--notes-file`，也不要用自定义正文替代 `--generate-notes`。

### 6. 监督并验收

持续检查：

```bash
gh run list --workflow release.yml --limit 3
gh run watch <run-id> --interval 15
gh run view <run-id> --log-failed
gh release view "v$VERSION"
```

验收清单：

- `validate-release-tag` 通过。
- macOS arm64、macOS x64、Windows、Linux 构建和上传均通过。
- GitHub Release 存在，Beta/RC 标记为 prerelease；Release title 与 tag 完全相同，只包含 `v<版本号>`。
- 自定义正文存在，且其中的 Issues、PR、README、compare 链接可点击。
- 自定义正文之后出现 GitHub 自动生成的 `What's Changed`、`New Contributors`、`Full Changelog`。
- `Contributors` 区域由 GitHub 自动生成，能看到官方头像和贡献者内容。
- Release assets 数量和平台产物符合 workflow 预期。

若官方生成区缺失，先检查 workflow 是否仍同时使用 `--notes` 和 `--generate-notes`，不要通过手工复制贡献者名单来“补齐”。

## 失败处理

- CI 失败：先读取失败 job 的日志，修复必须进入普通分支并 PR 合入 `main`；不要在 `release/*` 上直接修。
- tag 指向错误提交：停止推进，先查远端 ref、发布状态与已有产物；不得把已发布版本静默改指。仅在用户已授权对应删除/重建操作后恢复；否则报告具体目标和状态。
- release notes 缺失：回到普通分支补文件并合入 `main`，再重新切 release 分支；不要直接修改已推送的 release 快照。
- 生成区缺失：检查 `--generate-notes`、GitHub 权限、tag 是否有对应前一版本和 PR 历史；不要手写头像或 Contributors。
- 链接失效：优先修正 Markdown 链接和 README 锚点，再进入发布流程；发布说明中的链接必须可直接在 GitHub Release 页面点击。

## FileTerm 习惯

- 面向用户的内容使用中文，GitHub 官方区保持 GitHub 自动生成的英文格式。
- 测试版重点写清楚“当前能做什么、不会自动做什么、用户需要确认什么”。
- 涉及 AI、终端、凭据或备份时，明确数据范围、人工确认和安全边界。
- 不提交密码、私钥、token 或未经脱敏的终端输出。
