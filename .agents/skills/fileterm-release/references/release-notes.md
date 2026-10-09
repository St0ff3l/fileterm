# Release 自定义正文模板

### 2. 发布说明正文格式

推荐结构：中文正文、英文正文、GitHub 官方生成区。中文和英文都属于自定义正文，英文版本紧跟在中文版本后面；官方生成区必须由 GitHub 在最后追加。

每份 Release 正文都要在版本简介之后、更新重点之前放置对应语言的安装指南入口。中文区域只使用 `**安装指南**` 标题和一个中文安装指南徽章；英文区域只使用 `**Installation guides**` 标题和一个英文安装指南徽章。每个语言区域仅一个按钮，禁止重复放置中英文两个按钮或使用双语拼接标题。GitHub Release 正文使用可点击的 Shields 徽章实现按钮式入口；徽章统一使用中性深灰标签和标准 GitHub 蓝 `#0969DA`，链接固定指向 `main` 上维护的对应语言安装指南：

```md
**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)
```

撰写每个版本正文时，将上面的两个入口分别放入对应语言区域，不要在任一区域同时放置两个徽章。不要替换为本地文件路径、短链接或特定版本的分支链接。README 保留中英文安装指南入口。

以下标题属于固定格式，必须原样保留，不得改写成“相关 Pull Request”“本版本包含的主要 PR”或其他近义标题：中文使用 `### 本版本包含的主要 PR 和问题修复`、`### 反馈与支持`，英文使用 `### Main PRs and issues`、`### Feedback & Support`。

`### 反馈与支持` 以及其下的两段中文正文、空行和链接组成一个逐字固定块，必须整体复制，不得改写、拆分、改成列表或替换链接：

```md
### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。
```

```md
## FileTerm <version>

一句话版本简介。

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### <version> 更新重点

- **功能主题**：用户能感知的变化和边界。
- **稳定性/兼容性**：平台或核心链路变化。
- **安全与隐私**：数据发送、权限、凭据和人工确认边界。

### 本版本包含的主要 PR 和问题修复

- [PR #123](https://github.com/St0ff3l/fileterm/pull/123)：简要说明。
- [Issue #456](https://github.com/St0ff3l/fileterm/issues/456)：简要说明。

完整变更记录请查看 [v<old> 与 v<version> 的比较](https://github.com/St0ff3l/fileterm/compare/v<old>...v<version>)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm <version>

One-sentence release summary in English.

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Feature theme**: Describe the user-visible change and its boundaries.
- **Stability and compatibility**: Describe platform or core workflow changes.
- **Security and privacy**: Describe data scope, permissions, credentials, and confirmation boundaries.

### Main PRs and issues

- [PR #123](https://github.com/St0ff3l/fileterm/pull/123): Short description.
- [Issue #456](https://github.com/St0ff3l/fileterm/issues/456): Short description.

See the [comparison between v<old> and v<version>](https://github.com/St0ff3l/fileterm/compare/v<old>...v<version>) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
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
