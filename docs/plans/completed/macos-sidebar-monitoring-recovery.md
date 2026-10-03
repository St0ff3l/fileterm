# macOS 侧栏与监控恢复

## 目标与证据

2.2.21 日志确认两个独立故障：

- `Armbian 26.8.1 trixie` 的完整身份被 Linux 发行版名称校验拒绝，监控进入 `unsupported / target-identity-invalid`，Renderer 因能力禁用强制折叠侧栏。
- React #185 的应用栈落在通用 `ContextMenu` 的布局定位更新，组件栈为终端右键菜单。日志中的 bundle 第 42 行对应本地构建中的相同函数和定位逻辑。

## 版本回归范围

对比 `v2.2.8-beta.2` 与 `v2.2.21`：

- beta.2 已有“监控不可用则强制折叠”的 Renderer 逻辑，但没有首个监控样本的 `target_identity_is_valid` 校验。
- `3f276704`（首次包含于 `v2.2.8-beta.3`）为 Go 跳板机兼容新增首个样本身份校验，拒绝不兼容身份并禁用监控能力。Linux 发行版名称列表遗漏 Armbian，因此普通 Armbian 连接也触发旧侧栏强制折叠逻辑。
- 通用菜单和终端右键菜单源码在 beta.2 到当前 HEAD 之间没有变化。#185 的重复布局更新属于已有隐患，不能仅凭升级前未遇到而归因于最近的菜单改动。

## 改动范围

- Rust：补充 Armbian 名称，并采集 `os-release` 的 `ID` / `ID_LIKE`，以完整发行版标识或所属家族识别 Linux 派生系统，保留旧显示名称 fallback。补充 `/usr/lib/os-release` 与 `NAME` fallback，清除继承环境中的同名字段，避免缺失文件时误用陈旧身份。
- Renderer workspace：侧栏折叠与监控能力分离；手动展开退出当前标签专注模式。
- Renderer sidebar：监控不可用时保留连接摘要，隐藏资源与磁盘指标。
- 通用菜单：删除 DOM 测量后的定位 state，直接更新菜单 DOM，避免布局阶段再触发 React 更新；监听器只在挂载/卸载时安装/清理，通过 ref 调用最新关闭回调，避免父级更新恢复终端焦点。
- `SystemIdentity` 增加可选 `osId` / `osIdLike`，旧数据和 Windows / FreeBSD 不要求提供这些字段；不增加 command、event 或 bridge 方法。

## 源文件规模

POSIX 采集器原 `posix.rs` 为 843 行，非豁免业务文件。按职责迁至 `posix/`，由 `mod.rs` 保持原 facade，拆出监控命令 fallback 与系统身份采集/测试；主文件 707 行，fallback 140 行，身份模块 86 行。Rust 编译、采集脚本 shell 语法检查和既有 contract 测试覆盖迁移后的入口。

## 验证

- Renderer 状态回归覆盖监控禁用、能力未知、网络设备模式、反复展开和专注模式。
- 浏览器回归覆盖三种平台分支的 180 次侧栏展开/收起。
- 终端菜单浏览器回归覆盖相同位置下的重复父级更新、视口边缘定位、焦点保留与 Escape 的最新回调。
- Rust 身份校验覆盖 Armbian、Pop!_OS、KDE neon、elementary OS、NixOS、Solus、Deepin、Anolis、AlmaLinux、Azure Linux 和自定义 Debian 派生系统；同时验证 CRLF、家族列表空白分隔、完整 token 匹配、未知平台、内核不匹配以及带伪造 distro 字段的 JumpServer / Go / Koko 拒绝边界。
- 本地 shell 夹具实际执行身份脚本，验证 `/etc/os-release` 优先级、`/usr/lib/os-release` fallback、`NAME` fallback 和缺失文件时不继承环境字段。
- 运行项目类型、Lint、格式、CSS contract、Rust tests 和 Clippy 门禁。

2026-10-03 的开发版复测确认早期坐标相等保护仍触发 #185，最终改为直接 DOM 定位；用户重新加载后反馈恢复正常。更完整的定位证据、额外发现的代理/隧道表单循环与浏览器验证见 [React 更新循环排查](../../quality/react-update-loop-audit.md)。

原生 macOS WebView 与真实远端的发布后验证仍需用户确认；本地浏览器回归不替代该验证。
