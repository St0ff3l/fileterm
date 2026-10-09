# 平台与协议兼容约束

修改图标、tray、SSH 平台探测、CWD、权限同步或凭据存储时读取相关条目。以下为项目约束，不因模型升级放宽。

### 平台兼容边界

- **macOS 应用图标安全留白**：Dock 和 `⌘Tab` 使用 `icon.icns` 时，macOS 专用主图标必须在 1024×1024 画布中保留约 100px 的透明安全边距（有效图形约 824×824），不得直接使用满版主图标；Windows/Linux/WebView 继续使用完整主图标。macOS 菜单栏托盘仍使用独立的黑色单色透明 Template 图标，不能用 `icon.icns` 代替。
- **Linux tray 图标像素格式**：Linux tray 只能使用本地 8-bit RGBA 位图；`tauri.release.linux.conf.json` / `tauri.linux.conf.json` 引用的 `32x32.png`、`128x128.png`、`128x128@2x.png` 必须保持 8-bit RGBA。严禁把 Tauri 根据 bundle 首张 PNG 生成的 `default_window_icon()` 直接传给 Linux tray，避免 16-bit PNG 的原始缓冲触发 `wrong data size` 启动失败。
- **CWD 目录跟随**：终端工作目录 (CWD) 变化通过底层会话流安全捕获，经 runtime 广播同步给文件管理器，严禁 UI 层轮询或直接探测平台路径。
- **POSIX CWD 注入门控**：POSIX shell setup 仅对 `linux` / `busybox` 返回 true。Windows / unknown 平台**严禁注入** Linux shell CWD 脚本，采用 fail-closed 双重门控（平台探测和脚本注入各一道）。
- **CRLF 归一化**：系统指标解析入口必须对远端输出做 CRLF / CR → LF 归一化，避免 `'windows\r'` 等污染导致平台误判。
- **Sudo 与 Root 状态同步**：终端执行 `sudo` 或切换用户态需被底层 runtime 解析，双向同步到文件管理器权限模型。

### 资源与安全边界

- **离线资源就地化**：所有图标、字体与基础样式资源预置在代码库中打包输出，严禁运行时动态拉取外部 CDN 资源。
- **macOS 钥匙串规避**：禁用 safeStorage，用品牌重命名等替代机制存储凭据，避免触发 macOS 系统安全弹窗。
- **旧 Comware SSH 兼容边界**：`vendor/russh` 基于 `russh 0.63.1`，通过
  `[patch.crates-io]` 供 Tauri 使用。它只保留一个显式开启的窄范围兼容分支：远端 SSH
  identification 精确匹配 Comware、且协商到 `diffie-hellman-group-exchange-sha1` 时，请求
  `1024/1024/8192`；开启兼容选项时仍先按正常算法协商，默认安全算法不放宽。普通 Linux/Windows SSH、其他网络设备、
  `network-device` 模式、Banner 识别和服务器探测禁用由应用层逻辑负责，不由该兼容分支改变。
  升级该依赖时必须保留此边界，并重新运行 Rust 测试与 Clippy。
- 连接的 `group`（文件夹名）和 `parentId`（文件夹 ID）必须双向同步，存储层负责自愈。

## 已知文档与实现差异

POSIX 注入约束保留 linux / busybox 的既定白名单；当前 Rust `sessions/ssh/shell/shell_setup.rs` 和测试另有 darwin 分支。涉及该行为的任务应先核对差异，不在指令精简中修改协议行为或静默扩展白名单。
