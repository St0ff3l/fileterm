---
name: fileterm-release
description: FileTerm 专用 GitHub Release 发布流程。用于编写版本说明、创建 release 分支和 tag、触发并监督 Tauri 发布流水线，以及确保 GitHub 自动生成 What's Changed、Full Changelog、New Contributors 和贡献者头像区域。适用于正式版、Beta/RC 测试版和用户要求“提 PR、合 main、打 tag、发版”的任务。
---

# FileTerm GitHub Release

## 目标

把 FileTerm 的版本发布拆成两部分：

1. 自定义正文：版本简介、更新重点、主要 PR/Issue、使用提示和反馈渠道。
2. GitHub 官方生成区：`What's Changed`、`New Contributors`、`Full Changelog`、贡献者头像和贡献者展示。

自定义正文只负责第一部分，绝不能手写或覆盖第二部分。

## 必须先读取

进入仓库后先读取：

- `AGENTS.md`
- `docs/quality/git-branch-release-convention.md`
- `.github/workflows/release.yml`
- 目标版本对应的 `docs/quality/release-notes-<version>.md`（如果已经存在）

以仓库文件为准，不要凭记忆替换发布命令、分支规则或版本同步方式。

## 标准流程

### 1. 在普通分支完成版本说明和版本号

- 功能、修复、文档和版本号改动先进入普通分支并提 PR 到 `main`。
- 版本号只修改根目录 `package.json` 的 `version` 字段。
- 修改后立即运行 `npm run sync:version`，再检查 workspace 版本和 lockfile。
- 发布说明文件应在合入 `main` 前进入 PR，例如：
  `docs/quality/release-notes-2.2.0-beta.1.md`。
- 不要把日常功能改动直接推到 `main` 或 `release/*`。

### 2. 发布说明正文格式

推荐结构：中文正文、英文正文、GitHub 官方生成区。中文和英文都属于自定义正文，英文版本紧跟在中文版本后面；官方生成区必须由 GitHub 在最后追加。

以下标题属于固定格式，必须原样保留，不得改写成“相关 Pull Request”“本版本包含的主要 PR”或其他近义标题：中文使用 `### 本版本包含的主要 PR 和问题修复`、`### 反馈与支持`，英文使用 `### Main PRs and issues`、`### Feedback & Support`。

`### 反馈与支持` 以及其下的两段中文正文、空行和链接组成一个逐字固定块，必须整体复制，不得改写、拆分、改成列表或替换链接：

```md
### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。
```

```md
## FileTerm <version>

一句话版本简介。

### <version> 更新重点

- **功能主题**：用户能感知的变化和边界。
- **稳定性/兼容性**：平台或核心链路变化。
- **安全与隐私**：数据发送、权限、凭据和人工确认边界。

### 本版本包含的主要 PR 和问题修复

- [PR #123](https://github.com/St0ff3l/fileterm/pull/123)：简要说明。
- [Issue #456](https://github.com/St0ff3l/fileterm/issues/456)：简要说明。

完整变更记录请查看 [v<old> 与 v<version> 的比较](https://github.com/St0ff3l/fileterm/compare/v<old>...v<version>)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm <version>

One-sentence release summary in English.

### Highlights

- **Feature theme**: Describe the user-visible change and its boundaries.
- **Stability and compatibility**: Describe platform or core workflow changes.
- **Security and privacy**: Describe data scope, permissions, credentials, and confirmation boundaries.

### Main PRs and issues

- [PR #123](https://github.com/St0ff3l/fileterm/pull/123): Short description.
- [Issue #456](https://github.com/St0ff3l/fileterm/issues/456): Short description.

See the [comparison between v<old> and v<version>](https://github.com/St0ff3l/fileterm/compare/v<old>...v<version>) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
```

链接要求：

- GitHub Issues 使用完整可点击链接：`[GitHub Issues](https://github.com/St0ff3l/fileterm/issues)`。
- PR 使用 `/pull/<number>`，Issue 使用 `/issues/<number>`。
- 版本对比使用 `/compare/v<old>...v<new>`，例如：
  `[Full Changelog](https://github.com/St0ff3l/fileterm/compare/v2.1.6...v2.2.0-beta.1)`。
- README 社区入口固定使用 `https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81`，并确认锚点与 README 标题一致。
- 发布正文中的链接必须是 Markdown 链接，不要只写裸 URL，也不要把本地文件路径写入 release notes。
- 中文正文之后必须紧跟英文正文；英文正文应翻译相同的功能范围、安全边界和反馈信息，不要新增未在中文正文确认的功能。

禁止在自定义正文中添加：

- `### Contributors`、贡献者用户名列表或头像 URL。
- 手写 `What's Changed`、`New Contributors`、`Full Changelog` 区域。
- 与本版本无关的 MCP CLI、内部试验或未发布功能；除非用户明确要求写入。

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

发布 workflow 创建 Release 时必须同时传入自定义正文和 `--generate-notes`，使用仓库现有 workflow 的方式：

```bash
gh release create "$TAG" \
  --title "FileTerm ${VERSION}" \
  --notes "$(cat "docs/quality/release-notes-${VERSION}.md")" \
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
- GitHub Release 存在，Beta/RC 标记为 prerelease。
- 自定义正文存在，且其中的 Issues、PR、README、compare 链接可点击。
- 自定义正文之后出现 GitHub 自动生成的 `What's Changed`、`New Contributors`、`Full Changelog`。
- `Contributors` 区域由 GitHub 自动生成，能看到官方头像和贡献者内容。
- Release assets 数量和平台产物符合 workflow 预期。

若官方生成区缺失，先检查 workflow 是否仍同时使用 `--notes` 和 `--generate-notes`，不要通过手工复制贡献者名单来“补齐”。

## 失败处理

- CI 失败：先读取失败 job 的日志，修复必须进入普通分支并 PR 合入 `main`；不要在 `release/*` 上直接修。
- tag 指向错误提交：停止发布，删除错误 tag/release 分支后，按最新 `origin/main` 重新创建；操作前确认目标和远端状态。
- release notes 缺失：回到普通分支补文件并合入 `main`，再重新切 release 分支；不要直接修改已推送的 release 快照。
- 生成区缺失：检查 `--generate-notes`、GitHub 权限、tag 是否有对应前一版本和 PR 历史；不要手写头像或 Contributors。
- 链接失效：优先修正 Markdown 链接和 README 锚点，再进入发布流程；发布说明中的链接必须可直接在 GitHub Release 页面点击。

## FileTerm 习惯

- 面向用户的内容使用中文，GitHub 官方区保持 GitHub 自动生成的英文格式。
- 测试版重点写清楚“当前能做什么、不会自动做什么、用户需要确认什么”。
- 涉及 AI、终端、凭据或备份时，明确数据范围、人工确认和安全边界。
- 不提交密码、私钥、token 或未经脱敏的终端输出。
