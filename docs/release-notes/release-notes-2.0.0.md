# FileTerm 2.0.0

## Rust + Tauri 重构版本

2.0.0 是 FileTerm 从 Electron 运行时迁移到 Rust + Tauri 的重构版本。当前正式版主要维护 Tauri/Rust 链路，Electron 代码仅作为历史参考。

> ⚠️ 这是一次较大的底层重构。2.0.0 已正式发布，但仍属于早期迁移版本，可能存在平台、连接协议、窗口或数据迁移方面的小问题，请先做好数据备份并及时反馈。

本版本包含：

- Rust/Tauri 主运行时、窗口、托盘、连接、终端、文件管理和传输链路。
- Windows Tauri 签名应用内更新：下载后验签，重启安装。
- macOS arm64/x64 ad hoc 签名 DMG；检查更新后跳转 GitHub Release 手动下载。
- SSH、SFTP、FTP、WebDAV、凭据导入导出和跨平台窗口行为的兼容性修复。
- Tauri-only 的质量检查、打包和 GitHub Release 工作流。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#社区交流) 扫描二维码进群，也可加入 QQ 群 `534418986`。

Electron 版本不会通过这条 Tauri 更新链路自动升级。旧 Electron 安装包只能继续使用 Electron 自己的更新机制（如果该旧版本已配置），不能由 2.0.0 的 Tauri updater 直接覆盖安装。
