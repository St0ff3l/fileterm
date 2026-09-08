## FileTerm 2.2.8-beta.7

FileTerm 2.2.8-beta.7 聚焦 SSH/SFTP 启动响应、目录传输稳定性、连接配置管理和系统侧栏细节。

### 2.2.8-beta.7 更新重点

- **SSH/SFTP 启动响应**：主 shell 就绪后立即开放终端；平台探测、SFTP、资源监控和工作区快照在后台推进。Shell 启动步骤支持超时和取消，关闭连接时不会继续等待挂起请求。
- **目录与批量传输**：支持把本地文件夹拖放到 SFTP 文件区域上传；目录 manifest 使用轻量增量更新和持久化检查点，降低多文件上传期间的 UI 和日志开销，并保留可恢复传输与失败重试。
- **文件区域同步**：终端目录提示符即使路径不变也会刷新 SFTP 列表；提权读取使用有边界的 base64 正文，降低空文件和终端噪声导致的解码失败。
- **终端与 AI 兼容性**：修复 SSH 数据包边界造成的 UTF-8 替换字符；密钥登录会话执行普通 AI 只读命令时不再要求空密码字段。
- **连接配置与界面**：新增 Kubernetes SSH 连接预设，以及独立代理/隧道配置编辑入口；系统进程指标内容统一上下居中、左右左对齐，网络总计选择器取消蓝色焦点外圈并缩小文字。
- **安全边界**：敏感凭据、私钥口令、MFA/OTP 和终端内容不写入诊断日志；AI 工具调用不接受或保存一次性提权凭据。

### 本版本包含的主要 PR 和问题修复

- [Issue #234](https://github.com/St0ff3l/fileterm/issues/234)：降低多子文件上传的内存、日志和快照开销。
- [Issue #239](https://github.com/St0ff3l/fileterm/issues/239)：支持文件夹拖放上传，并让 SFTP 文件区域跟随终端操作刷新。
- [Issue #240](https://github.com/St0ff3l/fileterm/issues/240)：修复启用 Exec 后容器 SSH 用户识别和文件读取 base64 解码问题。
- [Issue #241](https://github.com/St0ff3l/fileterm/issues/241)：修复 Windows SSH 终端中跨数据包 UTF-8 字符损坏导致的问号和横线换行。
- [Issue #226](https://github.com/St0ff3l/fileterm/issues/226)：修复密钥登录时 AI 普通命令被错误要求密码字段。

完整变更记录请查看 [v2.2.8-beta.6 与 v2.2.8-beta.7 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.6...v2.2.8-beta.7)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.7

FileTerm 2.2.8-beta.7 focuses on SSH/SFTP startup responsiveness, directory-transfer stability, connection configuration, and system-sidebar polish.

### Highlights

- **SSH/SFTP startup responsiveness**: The terminal becomes available as soon as the main shell is ready, while platform probing, SFTP, resource monitoring, and workspace hydration continue in the background. Shell startup steps have bounded timeouts and cancellation, so closing a connection does not wait on stalled requests.
- **Directory and batch transfers**: Drag local folders into the SFTP file area to upload them. Directory manifests use lightweight incremental updates and persistent checkpoints to reduce UI and logging overhead during multi-file uploads while retaining resumable transfers and transient retries.
- **File-area synchronization**: SFTP listings refresh when terminal prompt markers repeat the same path, and privileged reads use framed base64 payloads to avoid decode failures from empty files and terminal noise.
- **Terminal and AI compatibility**: Preserve UTF-8 characters split across SSH data packets, and do not require an empty password field for ordinary AI read-only commands in key-authenticated sessions.
- **Connection configuration and UI**: Add a Kubernetes SSH preset and standalone proxy/tunnel editing flows. System process metrics are vertically centered and left aligned; the network total selector removes the blue focus ring and uses smaller text.
- **Security boundaries**: Sensitive credentials, private-key passphrases, MFA/OTP values, and terminal contents are excluded from diagnostics. AI tool calls do not accept or persist one-shot privilege credentials.

### Main PRs and issues

- [Issue #234](https://github.com/St0ff3l/fileterm/issues/234): Reduce memory, logging, and snapshot overhead for multi-file uploads.
- [Issue #239](https://github.com/St0ff3l/fileterm/issues/239): Support folder drag-and-drop uploads and refresh the SFTP file area after terminal operations.
- [Issue #240](https://github.com/St0ff3l/fileterm/issues/240): Fix container SSH identity handling and base64 decode failures when Exec is enabled.
- [Issue #241](https://github.com/St0ff3l/fileterm/issues/241): Fix split-packet UTF-8 corruption that caused replacement characters and wrapped terminal lines on Windows SSH sessions.
- [Issue #226](https://github.com/St0ff3l/fileterm/issues/226): Stop requiring a password field for ordinary AI commands in key-authenticated sessions.

See the [comparison between v2.2.8-beta.6 and v2.2.8-beta.7](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.6...v2.2.8-beta.7) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
