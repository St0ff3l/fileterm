## FileTerm 2.2.5

FileTerm 2.2.5 聚焦连接测试、终端设置与 Windows portable 数据兼容性。

### 2.2.5 更新重点

- **连接测试**：支持在不保存配置的前提下测试 SSH、FTP、Telnet 和 Serial 连接，并提供本地化的成功与错误反馈。
- **终端与命令体验**：改进命令模板参数输入与执行，修复中文输入法和特殊字符场景下的终端输入丢失；新增按平台选择本地终端 Shell，并增强设置搜索体验。
- **Windows portable 兼容性**：portable 版本将 FileTerm 自有配置保存到可执行文件旁的 `config/`；首次使用空配置目录时迁移旧 Tauri portable 数据，兼容 Electron 的旧 `FileTerm` 目录，并让 MCP 运行时描述文件使用同一目录。已有配置不会被覆盖，日志不会被迁移。
- **安全与构建稳定性**：凭据密文仍绑定当前 Windows 设备，迁移到另一台电脑后需要重新配置凭据；同时修正版本同步的换行处理、仓库 EOL 属性和 Windows 提交钩子兼容性。

### 本版本包含的主要 PR 和问题修复

- [PR #221](https://github.com/St0ff3l/fileterm/pull/221)：连接测试、终端设置、命令与输入稳定性，以及 Windows portable 存储兼容性。

完整变更记录请查看 [v2.2.4 与 v2.2.5 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.4...v2.2.5)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.5

FileTerm 2.2.5 focuses on connection testing, terminal settings, and Windows portable data compatibility.

### Highlights

- **Connection testing**: Test SSH, FTP, Telnet, and Serial connections without saving a profile first, with localized success and error feedback.
- **Terminal and command experience**: Improve command-template parameter input and execution, fix terminal input loss with Chinese IMEs and special characters, add platform-aware local Shell selection, and improve settings search.
- **Windows portable compatibility**: Portable builds store FileTerm-owned data in `config/` beside the executable, migrate legacy Tauri portable data on an empty first-run directory, recognize the legacy Electron `FileTerm` directory, and keep the MCP runtime descriptor on the same root. Existing config is never overwritten, and logs are not migrated.
- **Security and build stability**: Encrypted credentials remain bound to the current Windows device and require reconfiguration on another computer; version-sync line endings, repository EOL attributes, and the Windows commit hook are also stabilized.

### Main PRs and issues

- [PR #221](https://github.com/St0ff3l/fileterm/pull/221): Connection testing, terminal settings, command and input stability, and Windows portable storage compatibility.

See the [comparison between v2.2.4 and v2.2.5](https://github.com/St0ff3l/fileterm/compare/v2.2.4...v2.2.5) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
