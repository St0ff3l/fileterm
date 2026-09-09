## FileTerm 2.2.8-beta.8

FileTerm 2.2.8-beta.8 adds path bookmarks and refines macOS Overlay window controls.

### 2.2.8-beta.8 更新重点

- **路径收藏**：在本地和远程文件地址栏旁管理收藏，可收藏当前目录，也可从文件或目录右键菜单添加；支持打开、上下排序和取消收藏。
- **收藏隔离与持久化**：本地收藏共享，远程收藏按连接配置隔离并持久化保存；收藏打开失败时保留条目并显示已有错误反馈，不会自动删除。
- **macOS 窗口兼容性**：Overlay 标题栏下保留 AppKit 原生 traffic light 的尺寸和图层绘制，只校准位置，避免部分 macOS 版本出现扁椭圆按钮。
- **界面细节**：收藏弹窗使用统一确认弹窗、滚动条、主题语义变量和离线图标；地址栏操作间距适配收藏入口。

### 本版本包含的主要 PR 和问题修复

- [Issue #228](https://github.com/St0ff3l/fileterm/issues/228)：支持收藏常用本地和远程文件路径。
- [Issue #243](https://github.com/St0ff3l/fileterm/issues/243)：修复 macOS Overlay 标题栏原生窗口控制按钮可能显示为扁椭圆的问题。

完整变更记录请查看 [v2.2.8-beta.7 与 v2.2.8-beta.8 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.7...v2.2.8-beta.8)。

### 反馈与支持

> 遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> 也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.8-beta.8

FileTerm 2.2.8-beta.8 adds path bookmarks and refines macOS Overlay window controls.

### Highlights

- **Path bookmarks**: Manage local and remote bookmarks beside file-path bars. Bookmark the current directory or add files and directories from the context menu, then open, reorder, or remove them.
- **Bookmark isolation and persistence**: Local bookmarks are shared, while remote bookmarks are isolated by connection profile and persisted. Failed opens keep the bookmark and use the existing error feedback.
- **macOS window compatibility**: Preserve AppKit's native traffic-light dimensions and layer drawing under the Overlay title bar, calibrating position only to avoid flattened oval buttons on affected macOS versions.
- **UI details**: The bookmark dialog uses the shared confirmation dialog, scrollbar, semantic theme variables, and offline icons; address-bar spacing accommodates the new control.

### Main PRs and issues

- [Issue #228](https://github.com/St0ff3l/fileterm/issues/228): Add bookmarks for commonly used local and remote file paths.
- [Issue #243](https://github.com/St0ff3l/fileterm/issues/243): Fix potentially flattened oval native window controls under the macOS Overlay title bar.

See the [comparison between v2.2.8-beta.7 and v2.2.8-beta.8](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.7...v2.2.8-beta.8) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
