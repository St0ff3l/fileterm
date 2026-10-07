## FileTerm 2.2.26-beta.8

本测试版修复 Linux 无法打开外部链接的问题，并让 x86_64 Debian 安装包自动兼容 Debian 11。

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.2.26-beta.8 更新重点

- **Linux 外部链接**：下载页和更新提示现在通过系统启动器打开浏览器；Arch + LXQt 会继续尝试可用的启动器，启动失败时会在界面显示错误。
- **Debian 安装包**：x86_64 用户继续下载同一个 .deb。安装包会检查系统运行库是否可用；Debian 11 自动使用随包提供的兼容运行库，较新系统继续使用系统运行库。arm64 包保持系统依赖，要求 Debian 12 或更新版本。
- **运行库更新**：Debian 11 兼容运行库通过 FileTerm 的 .deb 更新一起升级，不单独修改或覆盖系统库。

### 本版本包含的主要 PR 和问题修复

- 待合并 PR 链接将在 beta8 发布前补入。

完整变更记录请查看 [v2.2.26-beta.7 与 v2.2.26-beta.8 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.7...v2.2.26-beta.8)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.8

This beta fixes Linux external links and makes the x86_64 Debian installer automatically compatible with Debian 11.

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Linux external links**: Download pages and update hints now open through the system launcher. On Arch + LXQt, FileTerm continues trying available launchers and shows an error in the UI when launch fails.
- **Debian installer**: x86_64 users keep downloading the same .deb. It checks whether the system runtime is available; Debian 11 automatically uses the compatibility runtime included in the package, while newer systems continue using their system runtime. The arm64 package retains system dependencies and requires Debian 12 or newer.
- **Runtime updates**: Debian 11 compatibility libraries are upgraded with FileTerm through the .deb; they do not modify or replace system libraries.

### Main PRs and issues

- The merged pull request link will be added before the beta8 release.

See the [comparison between v2.2.26-beta.7 and v2.2.26-beta.8](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.7...v2.2.26-beta.8) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
