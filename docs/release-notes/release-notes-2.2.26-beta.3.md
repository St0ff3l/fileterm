## FileTerm 2.2.26-beta.3

本测试版扩展 ARM64 和 Linux 包格式支持，重点验证 Windows ARM64、Linux ARM64、RPM 与 Arch 安装包的发布流程。

### 2.2.26-beta.3 更新重点

- **Windows ARM64**：新增签名 NSIS 安装包和 portable `.exe`，应用内更新按设备架构选择对应文件。
- **Linux ARM64 与包格式**：x86_64 和 ARM64 均提供 `.deb`、`.rpm`、`.AppImage` 及 `.zsync`；新增 Arch x86_64 与 Arch Linux ARM aarch64 的 Pacman `.pkg.tar.zst`。
- **安装包信息**：Linux 包包含桌面入口、图标、AppStream 元数据和 MIT 许可证；DEB、RPM、Pacman 包声明运行时依赖。RPM 需要发行版提供 WebKitGTK 4.1 与 OpenSSL 3。
- **测试版说明**：请按设备架构与发行版选择安装包，并反馈安装、启动或更新问题。

### 本版本包含的主要 PR 和问题修复

- [PR #291](https://github.com/St0ff3l/fileterm/pull/291)：增加 Linux ARM64、RPM、Arch Pacman 与 Windows ARM64 发布包和对应元数据校验。

完整变更记录请查看 [v2.2.26-beta.2 与 v2.2.26-beta.3 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.2...v2.2.26-beta.3)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.3

This beta expands ARM64 and Linux package coverage, with a focus on validating the Windows ARM64, Linux ARM64, RPM, and Arch release pipelines.

### Highlights

- **Windows ARM64**: Adds signed NSIS installer and portable `.exe` assets. In-app updates select the package that matches the device architecture.
- **Linux ARM64 and package formats**: Both x86_64 and ARM64 receive `.deb`, `.rpm`, `.AppImage`, and `.zsync` assets. Pacman `.pkg.tar.zst` packages are added for Arch x86_64 and Arch Linux ARM aarch64.
- **Package metadata**: Linux packages include desktop entries, icons, AppStream metadata, and the MIT license. DEB, RPM, and Pacman packages declare runtime dependencies. RPM requires a distribution that provides WebKitGTK 4.1 and OpenSSL 3.
- **Beta note**: Choose the package that matches your device architecture and distribution, then share any installation, startup, or update issues.

### Main PRs and issues

- [PR #291](https://github.com/St0ff3l/fileterm/pull/291): Adds Linux ARM64, RPM, Arch Pacman, and Windows ARM64 release packages with package metadata checks.

See the [comparison between v2.2.26-beta.2 and v2.2.26-beta.3](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.2...v2.2.26-beta.3) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
