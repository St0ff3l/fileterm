## FileTerm 2.2.15

修复 SSH 连接后偶尔显示内部目录跟踪脚本的问题，并改善慢速连接和自定义 shell 提示符下的回显处理。

### 2.2.15 更新重点

- **SSH 终端稳定性**：继续隐藏自动安装目录跟踪钩子时产生的命令回显，避免长时间或重复的 shell 重绘泄漏脚本文本；保留自定义提示符和正常终端输出。

### 本版本包含的主要 PR 和问题修复

- [Issue #264](https://github.com/St0ff3l/fileterm/issues/264)：修复 SSH 连接后内部目录跟踪脚本回显到终端的问题。

完整变更记录请查看 [v2.2.14 与 v2.2.15 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.14...v2.2.15)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.15

Fix SSH sessions occasionally displaying FileTerm's internal CWD setup script and improve echo handling for slow connections and custom shell prompts.

### Highlights

- **SSH terminal stability**: Keep the automatic CWD hook hidden across long or repeated shell redraws while preserving custom prompts and normal terminal output.

### Main PRs and issues

- [Issue #264](https://github.com/St0ff3l/fileterm/issues/264): Fix the internal CWD setup script appearing in the terminal after connecting over SSH.

See the [comparison between v2.2.14 and v2.2.15](https://github.com/St0ff3l/fileterm/compare/v2.2.14...v2.2.15) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
