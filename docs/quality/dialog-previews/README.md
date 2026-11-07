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

标题栏统一为 44px，标题 13px / 500，图标 16px；弹窗整体高度随内容适配。66 组计算样式核对通过。760×680 下连接与命令编辑器操作区保持可见。原生窗口系统标题栏几何未改。

文件编辑器浅色预览初始化时出现一次 Monaco ResizeObserver 通知；截图与布局正常。真实 SSH/SMB 交互和 Windows/Linux 原生窗口未验证。
