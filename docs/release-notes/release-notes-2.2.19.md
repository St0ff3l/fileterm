## FileTerm 2.2.19

为 Linux AppImage 增加外部差量更新支持，并统一各平台发行文件的命名。

### 2.2.19 更新重点

- **AppImage 差量更新**：内嵌 GitHub Release 更新信息，同时发布配套 `.AppImage.zsync`，供 AppImageUpdate 等外部工具检查和下载更新。正式版和预发布版使用独立更新渠道。
- **发行文件命名**：恢复 `FileTerm-2.2.19-linux-x86_64.AppImage`，与其他平台产物保持一致；AppImageHub 对 `linux` 字样的命名建议不再作为发布失败条件。
- **打包完整性**：上传前验证更新信息及 `.zsync` 的文件名、URL、大小和 SHA-1，确保差量元数据对应最终 AppImage。

### 本版本包含的主要 PR 和问题修复

- [PR #274](https://github.com/St0ff3l/fileterm/pull/274)：AppImage 更新信息、差量元数据生成与发布校验。
- [AppImageHub PR #9146](https://github.com/AppImage/appimage.github.io/pull/9146)：同步新版本并重新测试目录收录兼容性。

完整变更记录请查看 [v2.2.18 与 v2.2.19 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.18...v2.2.19)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.19

Enable external delta updates for the Linux AppImage and align release asset naming across platforms.

### Highlights

- **AppImage delta updates**: Embed GitHub Release update information and publish a matching `.AppImage.zsync` for external tools such as AppImageUpdate. Stable releases and prereleases use separate update channels.
- **Release asset naming**: Restore `FileTerm-2.2.19-linux-x86_64.AppImage` to match the naming of other platform assets. AppImageHub's naming recommendation about `linux` no longer fails release validation.
- **Package integrity**: Validate embedded update information and the zsync filename, URL, size, and SHA-1 before uploading, ensuring that delta metadata matches the final AppImage.

### Main PRs and issues

- [PR #274](https://github.com/St0ff3l/fileterm/pull/274): AppImage update information, delta metadata generation, and release validation.
- [AppImageHub PR #9146](https://github.com/AppImage/appimage.github.io/pull/9146): Retest catalog compatibility against the new release.

See the [comparison between v2.2.18 and v2.2.19](https://github.com/St0ff3l/fileterm/compare/v2.2.18...v2.2.19) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
