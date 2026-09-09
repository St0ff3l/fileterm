# 路径收藏与 macOS 原生窗口按钮

Refs #228, #243

## 路径收藏

本地和远程地址栏旁提供星形入口，可收藏当前目录、打开文件/目录、上下排序、取消收藏。文件右键菜单提供添加入口。取消收藏使用通用确认弹窗；列表使用公用纵向滚动条，长路径省略并保留完整 tooltip。

`packages/core` 定义 `PathBookmark`，经 Tauri bridge 调用 Rust commands。Rust 用互斥锁串行完成读取、去重、增删和排序，原子替换 `path-bookmarks.json`。本地使用 `local` scope；远程使用 `remote:<profileId>`，不使用临时 tab ID。相同路径在不同连接中独立保存，排序不会覆盖其他连接的数据。Windows 便携目录迁移包含此文件。

收藏列表每次打开从存储重新读取。点击收藏复用已有文件/目录打开链路，不提前探测远端或改变协议实现；失效路径、断线和权限问题通过已有状态错误反馈显示，收藏不会被自动删除。重命名、分组、拖动排序、Finder 拖入是原 Issue 的扩展建议，本次采用上下移动满足排序需求。

## macOS 窗口按钮

Issue 报告 macOS 15.7.9 上按钮呈扁椭圆；用户确认 macOS 26、27 未观察到此问题。

原校准代码将原生标准窗口按钮当成普通 NSButton 调用 `sizeToFit`，强制设置 14×14 frame，并额外缩放 CALayer。现在只调用 `setFrameOrigin`，保留 AppKit 自身的尺寸、bounds 和图层绘制比例。三个按钮的目标中心、48px 标题栏避让以及全屏退出后的延迟重定位保持原有逻辑。

参考：[Apple NSView frame](https://developer.apple.com/documentation/appkit/nsview/frame)、[setFrameOrigin](<https://developer.apple.com/documentation/appkit/nsview/setframeorigin(_:)>)。这是针对源码中比例干预的修复，不能把通过编译等同于 macOS 15 实机验证。

## 验证

- 类型检查、Lint、Prettier、CSS contract、Rust tests、Clippy。
- Rust 测试覆盖跨连接隔离、去重、排序、删除隔离与序列化。
- 浏览器临时组件夹具验证添加当前目录、排序、取消确认与长路径布局；夹具不写真实收藏存储，验收后删除。
- CSS contract 历史债务保持 11 处 `!important` 与约 1 处直接色值，本次无新增。
- 待目标环境验收：macOS 15 按钮圆形、hover 与全屏恢复；macOS 26/27 外观回归；Windows/Linux WebView 与真实 SSH/SFTP/FTP 收藏路径打开。
- Issue 保持打开，须发布并由用户验证后另行确认关闭。
