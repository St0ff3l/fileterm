# 弹窗预览

共 33 种真实组件、66 张深浅色截图。使用合成数据；不是原生桌面窗口截图。

记录日期：2026-11-07。截图反映当前本地 UI 改动，提交本预览不表示对应源码已提交或发布。

打开 [可筛选预览索引](index.html)，点击图片可放大；或查看 [深色总览](overview-dark.png) / [浅色总览](overview-light.png)。

| 弹窗               | 深色                                | 浅色                                 |
| ------------------ | ----------------------------------- | ------------------------------------ |
| 连接 SMB 网络共享  | [查看](dark/smb-credentials.png)    | [查看](light/smb-credentials.png)    |
| 选择 SMB 共享目录  | [查看](dark/smb-shares.png)         | [查看](light/smb-shares.png)         |
| 启用 root 视角     | [查看](dark/root-access.png)        | [查看](light/root-access.png)        |
| sudo / su 密码输入 | [查看](dark/sudo-password.png)      | [查看](light/sudo-password.png)      |
| SSH 登录凭据       | [查看](dark/ssh-login.png)          | [查看](light/ssh-login.png)          |
| SSH 多轮认证       | [查看](dark/ssh-keyboard.png)       | [查看](light/ssh-keyboard.png)       |
| SSH 主机指纹确认   | [查看](dark/host-verification.png)  | [查看](light/host-verification.png)  |
| SSH 主机指纹变化   | [查看](dark/host-changed.png)       | [查看](light/host-changed.png)       |
| 私钥口令           | [查看](dark/key-passphrase.png)     | [查看](light/key-passphrase.png)     |
| 备份加密密码       | [查看](dark/backup-password.png)    | [查看](light/backup-password.png)    |
| 新建文件           | [查看](dark/new-file.png)           | [查看](light/new-file.png)           |
| 重命名             | [查看](dark/rename-file.png)        | [查看](light/rename-file.png)        |
| 文件权限           | [查看](dark/file-permissions.png)   | [查看](light/file-permissions.png)   |
| 文件冲突处理       | [查看](dark/file-conflict.png)      | [查看](light/file-conflict.png)      |
| 路径收藏           | [查看](dark/bookmarks.png)          | [查看](light/bookmarks.png)          |
| 新增运行时隧道     | [查看](dark/runtime-tunnel.png)     | [查看](light/runtime-tunnel.png)     |
| 新增代理配置       | [查看](dark/proxy-profile.png)      | [查看](light/proxy-profile.png)      |
| 新增隧道配置       | [查看](dark/tunnel-profile.png)     | [查看](light/tunnel-profile.png)     |
| 新建命令模板       | [查看](dark/command-editor.png)     | [查看](light/command-editor.png)     |
| 新建连接           | [查看](dark/connection-editor.png)  | [查看](light/connection-editor.png)  |
| 连接导入预览       | [查看](dark/connection-import.png)  | [查看](light/connection-import.png)  |
| 导入 SSH 密钥      | [查看](dark/key-import.png)         | [查看](light/key-import.png)         |
| 编辑 SSH 密钥备注  | [查看](dark/key-note.png)           | [查看](light/key-note.png)           |
| 调用详情           | [查看](dark/call-detail.png)        | [查看](light/call-detail.png)        |
| MCP 操作审批       | [查看](dark/mcp-approval.png)       | [查看](light/mcp-approval.png)       |
| 删除确认           | [查看](dark/delete-confirm.png)     | [查看](light/delete-confirm.png)     |
| 关闭窗口确认       | [查看](dark/close-confirm.png)      | [查看](light/close-confirm.png)      |
| 便携版更新提示     | [查看](dark/portable-update.png)    | [查看](light/portable-update.png)    |
| 文件编辑器         | [查看](dark/file-editor.png)        | [查看](light/file-editor.png)        |
| 连接管理器弹窗     | [查看](dark/connection-manager.png) | [查看](light/connection-manager.png) |
| 命令管理器弹窗     | [查看](dark/command-manager.png)    | [查看](light/command-manager.png)    |
| 设置弹窗           | [查看](dark/settings.png)           | [查看](light/settings.png)           |
| 异常详情           | [查看](dark/error-dialog.png)       | [查看](light/error-dialog.png)       |

表单类标题栏统一为 44px，标题 13px / 500，图标 16px；弹窗整体高度随内容适配。普通关闭/删除确认与便携版更新提示保留独立 ConfirmActionDialog 模板：18px / 650 标题、24px 留白，不增加表单标题栏分隔线。60 组表单样式及 6 组确认模板样式核对通过。760×680 下连接与命令编辑器操作区保持可见。原生窗口系统标题栏几何未改。

文件编辑器浅色预览初始化时出现一次 Monaco ResizeObserver 通知；截图与布局正常。真实 SSH/SMB 交互和 Windows/Linux 原生窗口未验证。

## 普通确认模板范围复核

2026-11-07 检查全部 30 个 ConfirmActionDialog 调用点：27 个普通确认/提示保持原模板；仅审批、调用详情、密钥表单三个调用点显式使用表单样式。覆盖删除连接/命令/密钥/代理/主题/字体/AI 配置、删除文件/收藏/聊天内容、退出与关闭会话、放弃编辑、传输确认/错误/清理、串口控制、停止监控、安全密码重置和便携更新等入口。

移除了父弹窗上对 `--dialog-surface` 的局部重定义，避免通过继承影响内嵌确认提示。普通确认组件的 TSX/CSS 与修改前一致。

浏览器用真实 ConfirmActionDialog 做 60 组对比：2 个主题 × 2 个窗口宽度（1200/500）× 3 个容器（独立/设置弹窗/表单）× 5 个状态（默认/移除锁定密码/重置备份密码/错误/额外按钮）。禁用与启用新统一样式后，确认窗及其全部子节点的颜色、字体、边框、留白、尺寸等 20 项计算样式无差异；没有任何新增统一样式规则命中普通确认窗。测试另外使用不同于表单背景的 dialog-surface 值检查继承隔离。

本次范围修正后，247 项 renderer 测试、Prettier、CSS contract 及 Git 空白检查通过。普通确认模板组件源码未改；未重跑原生桌面验证。

## 编辑器与管理器顶部背景

代理配置、隧道配置、连接编辑器、连接管理器、命令管理器和设置弹窗的顶部背景恢复使用 `--surface-sidebar`，不再采用 SMB 正文背景；高度及其余样式保持当前状态。当前用户深色主题为 #242424，FileTerm Light 为白色。六类窗口的深浅色共 12 组顶部颜色核对通过，原图和总览已更新。
