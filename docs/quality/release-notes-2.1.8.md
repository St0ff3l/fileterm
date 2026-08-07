## FileTerm 2.1.8

FileTerm 2.1.8 聚焦 SSH 私钥导入便利性、跨平台本地终端生命周期稳定性，以及发行流程质量门禁。

### 2.1.8 更新重点

- SSH 私钥导入：支持从文件选择或直接粘贴私钥文本；文本仅作为临时输入，不写入 profile、工作区快照或日志。
- 本地终端兼容性：加固 macOS、Windows、Linux 的 PTY 生命周期、输出流处理、终端尺寸同步和进程清理。
- 发行流程：发布说明保留版本正文，同时由 GitHub 自动生成变更记录和贡献者展示。

### 本版本包含的主要 PR 和问题修复

- [PR #179](https://github.com/St0ff3l/fileterm/pull/179)：修复本地终端在 Windows ConPTY 和跨平台 PTY 生命周期中的兼容性问题。
- [PR #182](https://github.com/St0ff3l/fileterm/pull/182)：支持 SSH 私钥文本导入，跟现有校验、去重、加密检测和安全存储流程保持一致。
- [Issue #181](https://github.com/St0ff3l/fileterm/issues/181)：支持直接填写或导入私钥文本。

> 遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

> 也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。
