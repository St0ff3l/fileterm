## FileTerm 2.2.26

FileTerm 2.2.26 fixes terminal glyph selection and Windows password prompts, adds bundled font choices, and expands desktop and Linux package support.

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.2.26 更新重点

- **终端显示与输入**：修复 JetBrains Mono、Cascadia Code 等字体下选中特殊符号时字形被裁切的问题；修复 Windows 本地终端输入乱序可能导致 SCP 密码提示重复的问题。
- **字体**：新增内置 Cascadia Code，改进 macOS 系统字体识别，并提供共用字体导入入口；导入后可分别在界面字体和代码字体下拉框中选择。
- **安装包与平台支持**：提供 Windows ARM64 和 Linux ARM64 构建，并补齐 RPM、Arch Linux 包；x86_64 Debian 包可在 Debian 11 使用兼容运行库。
- **桌面体验**：改进 macOS DMG Finder 布局，统一侧栏文字对比度，并调整 Linux 窗口外观、外部链接和最小化窗口关闭流程。

### 本版本包含的主要 PR 和问题修复

- [PR #300](https://github.com/St0ff3l/fileterm/pull/300)：改进多种等宽字体下终端特殊字形的选区绘制，打包 Cascadia Code 并补充字体许可证与产物校验；简化字体导入和选择流程。
- [PR #286](https://github.com/St0ff3l/fileterm/pull/286)：改善侧栏未选中文字对比度。
- [PR #287](https://github.com/St0ff3l/fileterm/pull/287)、[PR #288](https://github.com/St0ff3l/fileterm/pull/288)：改进 macOS DMG 安装界面及 Finder 布局，并修复终端特殊字符选区显示。
- [PR #289](https://github.com/St0ff3l/fileterm/pull/289)：修复 Windows 本地终端输入乱序导致隐藏密码提示重复的问题。
- [PR #290](https://github.com/St0ff3l/fileterm/pull/290)：修复不同等宽字体下鼠标选区遮挡终端特殊字符的问题。
- [PR #291](https://github.com/St0ff3l/fileterm/pull/291)、[PR #292](https://github.com/St0ff3l/fileterm/pull/292)、[PR #293](https://github.com/St0ff3l/fileterm/pull/293)：增加 Windows/Linux ARM64、RPM 和 Arch Linux 软件包及校验。
- [PR #294](https://github.com/St0ff3l/fileterm/pull/294)：增加中英文安装指南。
- [PR #295](https://github.com/St0ff3l/fileterm/pull/295)：统一侧栏文字颜色并调整 Windows/Linux 菜单栏布局。
- [PR #296](https://github.com/St0ff3l/fileterm/pull/296)、[PR #297](https://github.com/St0ff3l/fileterm/pull/297)、[PR #298](https://github.com/St0ff3l/fileterm/pull/298)：改进 AI Copilot 和 Linux 桌面窗口行为。
- [PR #299](https://github.com/St0ff3l/fileterm/pull/299)：修复 Linux 外部链接启动，并为 x86_64 Debian 包增加 Debian 11 兼容运行库。

完整变更记录请查看 [v2.2.25 与 v2.2.26 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.25...v2.2.26)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26

FileTerm 2.2.26 fixes terminal glyph selection and Windows password prompts, adds bundled font choices, and expands desktop and Linux package support.

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Terminal rendering and input**: Fixes clipped special glyphs with fonts such as JetBrains Mono and Cascadia Code, and fixes out-of-order Windows local terminal input that could repeat SCP password prompts.
- **Fonts**: Adds bundled Cascadia Code, improves macOS system font detection, and provides one shared font import action. Imported fonts can be selected separately for the interface and code.
- **Packages and platform support**: Adds Windows ARM64 and Linux ARM64 builds, RPM and Arch Linux packages, and a compatible runtime in the x86_64 Debian package for Debian 11.
- **Desktop experience**: Improves the macOS DMG Finder layout, sidebar text contrast, and Linux window appearance, external links, and close flow for minimized windows.

### Main PRs and issues

- [PR #300](https://github.com/St0ff3l/fileterm/pull/300): Improves terminal selection rendering for special glyphs across monospace fonts, bundles Cascadia Code with license notices and artifact validation, and simplifies font import and selection.
- [PR #286](https://github.com/St0ff3l/fileterm/pull/286): Improves unselected sidebar text contrast.
- [PR #287](https://github.com/St0ff3l/fileterm/pull/287) and [PR #288](https://github.com/St0ff3l/fileterm/pull/288): Improve the macOS DMG installer and Finder layout, and fix selected terminal glyph rendering.
- [PR #289](https://github.com/St0ff3l/fileterm/pull/289): Fixes out-of-order Windows local terminal input that could repeat hidden password prompts.
- [PR #290](https://github.com/St0ff3l/fileterm/pull/290): Fixes terminal glyphs being covered during mouse selection with different monospace fonts.
- [PR #291](https://github.com/St0ff3l/fileterm/pull/291), [PR #292](https://github.com/St0ff3l/fileterm/pull/292), and [PR #293](https://github.com/St0ff3l/fileterm/pull/293): Add Windows/Linux ARM64, RPM, and Arch Linux packages and their validation.
- [PR #294](https://github.com/St0ff3l/fileterm/pull/294): Adds Chinese and English installation guides.
- [PR #295](https://github.com/St0ff3l/fileterm/pull/295): Unifies sidebar text colors and adjusts the Windows/Linux menu bar layout.
- [PR #296](https://github.com/St0ff3l/fileterm/pull/296), [PR #297](https://github.com/St0ff3l/fileterm/pull/297), and [PR #298](https://github.com/St0ff3l/fileterm/pull/298): Improve AI Copilot and Linux desktop window behavior.
- [PR #299](https://github.com/St0ff3l/fileterm/pull/299): Fixes Linux external-link launching and adds a Debian 11 compatibility runtime to the x86_64 Debian package.

See the [comparison between v2.2.25 and v2.2.26](https://github.com/St0ff3l/fileterm/compare/v2.2.25...v2.2.26) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
