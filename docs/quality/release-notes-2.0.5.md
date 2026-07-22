## FileTerm 2.0.5

**Rust + Tauri 稳定性与 Windows 体验更新**

FileTerm 2.x 继续维护 Rust/Tauri 主运行时，Electron 代码仅作为历史参考。本版本聚焦 SSH 会话稳定性、SMB 文件访问、Windows 图标与本地磁盘导航，以及命令管理器工作区交互。

### 2.0.5 更新重点

- **SSH 会话稳定性**：增加认证、键盘交互和隧道转发超时，修复远程转发与终端 worker 在异常情况下卡死的问题，并补充连接日志。
- **SMB 共享访问**：完善凭据验证、共享目录选择和本地文件面板状态处理；Windows 在盘符根目录点击上级时可返回“此电脑”磁盘列表。
- **命令管理器体验**：细化命令管理器工作区布局与交互状态，保持临时命令编辑器与工作区行为一致。
- **Windows 图标与打包**：使用多尺寸 ICO 资源加载应用和托盘图标，避免开发态或高 DPI 环境选择低分辨率图标。
- **质量门禁**：继续收紧 Tauri/Rust 格式、类型、协议夹具和依赖安全检查。

### 本版本包含的主要 PR

- [PR #131](https://github.com/St0ff3l/fileterm/pull/131)：修复 Windows 图标资源和本地磁盘根目录导航。
- [PR #130](https://github.com/St0ff3l/fileterm/pull/130)：细化命令管理器工作区 UI。
- [PR #129](https://github.com/St0ff3l/fileterm/pull/129)：完善 SMB 共享流程和界面本地化。
- [PR #128](https://github.com/St0ff3l/fileterm/pull/128)：修复 SSH worker 卡死和标题栏接缝问题。
- [PR #125–#127](https://github.com/St0ff3l/fileterm/compare/v2.0.4...v2.0.5)：SSH 中断响应、临时命令编辑器及 SMB/工作区稳定性改进。

### 2.x 重构版包含

- Rust/Tauri 主运行时、窗口、托盘、连接、终端、文件管理和传输链路。
- Windows Tauri 签名应用内更新：下载后验签，重启安装。
- macOS arm64/x64 ad hoc 签名 DMG；检查更新后跳转 GitHub Release 手动下载。
- SSH、SFTP、FTP、WebDAV、凭据导入导出和跨平台窗口行为的兼容性修复。
- Tauri-only 的质量检查、打包和 GitHub Release 工作流。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

> Electron 版本不会通过这条 Tauri 更新链路自动升级。旧 Electron 安装包只能继续使用 Electron 自己的更新机制（如果该旧版本已配置），不能由 2.x 的 Tauri updater 直接覆盖安装。
