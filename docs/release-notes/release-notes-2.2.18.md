## FileTerm 2.2.18

修复 Linux AppImage 在 AppImageHub Firejail 测试中因目录权限不足而无法启动的问题，并调整发行文件名以符合 AppImageHub 命名要求。

### 2.2.18 更新重点

- **Linux AppImage**：打包前将 AppDir 目录规范为可供普通用户遍历，并确保可执行文件具有通用执行权限，修复 Firejail 沙箱启动失败。
- **AppImageHub 兼容性**：移除 AppImage 资产名中的 `linux`，符合其文件名检查规则。

### 本版本包含的主要 PR 和问题修复

- [AppImageHub PR #9146](https://github.com/AppImage/appimage.github.io/pull/9146)：重新测试 FileTerm AppImage 与目录索引兼容性。

完整变更记录请查看 [v2.2.17 与 v2.2.18 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.17...v2.2.18)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.18

Fix Linux AppImage startup failures in AppImageHub's Firejail test caused by insufficient directory permissions, and update the release asset name to meet AppImageHub's naming requirements.

### Highlights

- **Linux AppImage**: Normalize AppDir directories for traversal by unprivileged users and ensure executable files can be launched by them, fixing startup in the Firejail sandbox.
- **AppImageHub compatibility**: Remove `linux` from the AppImage asset name to satisfy its filename check.

### Main PRs and issues

- [AppImageHub PR #9146](https://github.com/AppImage/appimage.github.io/pull/9146): Retest FileTerm AppImage compatibility with the catalog.

See the [comparison between v2.2.17 and v2.2.18](https://github.com/St0ff3l/fileterm/compare/v2.2.17...v2.2.18) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
