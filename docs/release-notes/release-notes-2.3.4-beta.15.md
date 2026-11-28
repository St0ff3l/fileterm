## FileTerm 2.3.4-beta.15

FileTerm 2.3.4-beta.15 新增可自托管的 Docker Web 版，并与桌面版使用同一个发布 tag。

**下载地址**

| 平台    | 类型                        | 下载                                                                                                                                                                                                                                                                                   |
| ------- | --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | DMG                         | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-macos-x64.dmg)                                      |
| Windows | Installer                   | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-arm64-setup.exe)                                |
| Windows | Portable                    | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB                         | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.deb)                                          |
| Linux   | RPM                         | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.rpm)                                          |
| Linux   | AppImage                    | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.AppImage)                                |
| Linux   | Arch                        | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.pkg.tar.zst)                          |
| Linux   | Docker Web（AMD64 / ARM64） | `v2.3.4-beta.15` · [Docker Hub](https://hub.docker.com/r/st0ff3l/fileterm/tags) · [GHCR](https://github.com/St0ff3l/fileterm/pkgs/container/fileterm) · [部署说明](https://github.com/St0ff3l/fileterm/blob/v2.3.4-beta.15/apps/tauri/web/README.md)                                   |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.4-beta.15 更新重点

- **Docker Web**：浏览器可直接使用 FileTerm 界面和服务端连接功能。默认监听部署机器的 `127.0.0.1:34538`，配置和连接资料保存在 Docker 数据卷中。
- **首次登录**：没有配置部署凭据时，使用 `admin / admin` 登录并进入 Web 登录设置，完成密码修改后才能访问工作区。

### 本版本包含的主要 PR 和问题修复

- [PR #421](https://github.com/St0ff3l/fileterm/pull/421)：新增 Docker Web 运行时和镜像发布。
- [Issue #162](https://github.com/St0ff3l/fileterm/issues/162)：Docker 部署需求。

完整变更记录请查看 [v2.3.4-beta.14 与 v2.3.4-beta.15 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.14...v2.3.4-beta.15)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。\
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.4-beta.15

FileTerm 2.3.4-beta.15 introduces a self-hosted Docker Web edition that shares the desktop release tag.

**Downloads**

| Platform | Type                       | Downloads                                                                                                                                                                                                                                                                                 |
| -------- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG                        | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-macos-x64.dmg)                                         |
| Windows  | Installer                  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-arm64-setup.exe)                                   |
| Windows  | Portable                   | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB                        | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.deb)                                             |
| Linux    | RPM                        | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.rpm)                                             |
| Linux    | AppImage                   | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.AppImage)                                   |
| Linux    | Arch                       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.4-beta.15/FileTerm-2.3.4-beta.15-linux-arm64.pkg.tar.zst)                             |
| Linux    | Docker Web (AMD64 / ARM64) | `v2.3.4-beta.15` · [Docker Hub](https://hub.docker.com/r/st0ff3l/fileterm/tags) · [GHCR](https://github.com/St0ff3l/fileterm/pkgs/container/fileterm) · [Deployment guide](https://github.com/St0ff3l/fileterm/blob/v2.3.4-beta.15/apps/tauri/web/README.md)                              |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Docker Web**: Use the FileTerm interface and server-side connections directly in the browser. It listens on `127.0.0.1:34538` on the deployment host by default and keeps settings and connection profiles in a Docker volume.
- **First sign-in**: With no deployment credentials, sign in with `admin / admin` and change the password in Web Login settings to open the workspace.

### Main PRs and issues

- [PR #421](https://github.com/St0ff3l/fileterm/pull/421): Adds the Docker Web runtime and image release.
- [Issue #162](https://github.com/St0ff3l/fileterm/issues/162): Docker deployment request.

See the [comparison between v2.3.4-beta.14 and v2.3.4-beta.15](https://github.com/St0ff3l/fileterm/compare/v2.3.4-beta.14...v2.3.4-beta.15) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.\
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
