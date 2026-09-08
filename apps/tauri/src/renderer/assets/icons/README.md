# FileTerm UI Functional Icons (UI 功能矢量图标库)

本目录归档 FileTerm 前端渲染层使用的全部离线 UI 功能矢量图标（SVG）。

## 1. 资产边界：应用图标 vs UI 功能图标

在 FileTerm 中，**应用身份图标**与 **UI 功能图标**具有严格的物理与架构隔离，禁止混淆：

| 类型                                       | 归属与路径                                                                                                         | 规范与用途                                                                                                                                                                          |
| :----------------------------------------- | :----------------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **应用身份与平台图标**<br>_(App Identity)_ | • `apps/tauri/assets/icons/trayTemplate.svg`<br>• `apps/tauri/src-tauri/icons/*`<br>• `apps/tauri/public/icon.png` | 供操作系统、Dock、任务栏、系统托盘与安装包使用的客户端本体图标。<br>macOS Dock 图标严格遵守 1024×1024 画布与约 100px 安全留白（图形 824×824）；macOS 托盘为纯黑单色 Template 图标。 |
| **UI 业务与功能图标**<br>_(UI Functional)_ | • `apps/tauri/src/renderer/assets/icons/<category>/*`                                                              | 供应用内界面控件（按钮、树状图、侧边栏、对话框、设置项等）渲染使用的矢量图标。<br>统一采用标准 SVG，离线就地化，按功能域分类存储。                                                  |

---

## 2. 目录分类与清单

```text
apps/tauri/src/renderer/assets/icons/
├── actions/       # 操作与编辑功能（增删改查、保存、刷新等）
├── navigation/    # 页面导航与窗口控制（前进后退、展开折叠、全屏、关闭等）
├── files/         # 文件管理与存储操作（文件夹、文件文档、上传下载等）
├── network/       # 连接、协议与终端（终端模拟、局域网、DNS、云同步等）
├── security/      # 安全凭据与权限（密码锁、密钥、盾牌、指纹、可见性等）
└── system/        # 系统配置与状态（偏好设置、调色板、语言、更新、日志等）
```

### 分类清单（共 59 个图标）

- **`actions/` (15)**: `add.svg`, `add_comment.svg`, `check.svg`, `delete.svg`, `delete_sweep.svg`, `drag_indicator.svg`, `edit.svg`, `edit_note.svg`, `more_horiz.svg`, `refresh.svg`, `restart_alt.svg`, `save.svg`, `save_as.svg`, `search.svg`, `stop.svg`
- **`navigation/` (9)**: `apps.svg`, `arrow_back.svg`, `arrow_forward.svg`, `chevron_right.svg`, `close.svg`, `close_fullscreen.svg`, `dashboard.svg`, `open_in_full.svg`, `open_in_new.svg`
- **`files/` (6)**: `description.svg`, `download.svg`, `folder.svg`, `folder_open.svg`, `folder_shared.svg`, `upload_file.svg`
- **`network/` (7)**: `account_tree.svg`, `cloud.svg`, `cloud_sync.svg`, `dns.svg`, `lan.svg`, `settings_ethernet.svg`, `terminal.svg`
- **`security/` (9)**: `admin_panel_settings.svg`, `fingerprint.svg`, `key.svg`, `key_off.svg`, `lock.svg`, `shield.svg`, `visibility.svg`, `visibility_off.svg`, `vpn_key.svg`
- **`system/` (13)**: `auto_awesome.svg`, `forum.svg`, `history.svg`, `info.svg`, `palette.svg`, `preview.svg`, `progress_activity.svg`, `settings.svg`, `settings_suggest.svg`, `star.svg`, `system_update.svg`, `translate.svg`, `tune.svg`

---

## 3. 技术规范

1. **`fill="currentColor"` 颜色继承**：
   所有 SVG 图标必须将填充设为 `currentColor`，严禁写死 hex/rgb 颜色值。这保证图标能自然跟随 CSS `--theme-text`、`--folder-accent`、hover 或 active 伪类自动变色。
2. **离线就地化**：
   100% 静态打包随应用分发，禁止运行时调用任何在线 CDN。
3. **彻底杜绝 FOUT 闪烁**：
   纯矢量 SVG 渲染不依赖 WebFont 字体文件的异步加载与排版引擎字符替换，首屏即可瞬时精准渲染，杜绝字母连字（Ligature）闪烁。
