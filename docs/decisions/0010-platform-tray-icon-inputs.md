# ADR-0010: 平台托盘图标使用专用的 8-bit 位图输入

## 状态

Accepted（2026-09-19；2026-10-23 更新高清应用图标来源）

## 背景

FileTerm 的 tray 在 macOS、Windows 和 Linux 上由不同的原生后端渲染。macOS 需要独立的单色 Template 图标；Windows tray 使用 256px、Linux tray 使用 32px 的 8-bit RGBA PNG。Windows 桌面快捷方式使用包含多尺寸图层的 ICO，运行时窗口与任务栏使用 1024px 图像。

Tauri 会把 Linux bundle 配置中第一张 PNG 嵌入为原始像素缓冲。若 PNG 是 16-bit RGBA，32×32 图会携带 8192 字节，而 tray 后端只接受 4096 字节的 32bpp 数据。将 `default_window_icon()` 直接交给 Linux tray 会使应用在 setup hook 中失败，影响所有 Linux 安装包，包括 `.AppImage`、`.deb`、`.rpm` 和 Pacman 包。

## 决策

1. macOS tray 继续使用独立的 `tray/macos/template@2x.png`，并以 template 模式渲染。
2. Windows tray 使用 `app/shared/*-256.png`，Linux tray 使用 `tray/linux/*-32.png`。两者必须是满版 8-bit RGBA PNG，不能直接使用 Tauri 自动生成的 `default_window_icon()`。
3. Windows 快捷方式 ICO 包含 16、20、24、32、40、48、64、128、256px 图层；运行窗口的大小 HICON 均使用 1024px 图。Windows/Linux 的满版 1024px 母图从 Icon Composer 工程直接导出到 `app/shared/`，不带 macOS 安全边距，也不从较小位图放大；Linux 包内图标及窗口/启动器图标必须保持 8-bit RGBA 与满版构图。
4. Linux bundle 使用的 `app/linux/32x32.png`、`app/linux/128x128.png` 与 `app/linux/128x128@2x.png` 必须为 8-bit RGBA，且不带 macOS Dock 透明安全边距。保留 `128x128@2x.png` 文件名，让 Tauri 将 256px 图输出到 `hicolor/256x256@2`。Linux 还安装 `app/shared/classic-1024.png` 到 1024px hicolor 目录和 pixmaps。运行 `apps/tauri/scripts/generate-platform-app-icons.py` 将母图规范为 8-bit sRGB RGBA，并生成较小的 Windows/Linux 图标；该脚本不得修改 Dock 图或 `app/macos/icon.icns`。
5. Rust 回归测试检查 PNG 的 IHDR 位深、RGBA 颜色类型、原生尺寸和透明边界，防止后续资产更新重新引入缩小图案或 16-bit 图像。

## 影响

### 正面影响

- Linux tray 初始化拥有确定的 32bpp 输入，各 Linux 安装包不再因位图字节数不匹配而在启动阶段崩溃。
- 平台图标职责清晰：macOS template、Windows 1024px 运行时应用图、多尺寸 ICO 与 256px tray PNG、Linux 满版 8-bit RGBA 位图不会相互复用。
- 资产编码错误能够在常规 Rust 测试中被发现，不依赖桌面环境复现。

### 限制与非目标

- 此决策不改变 Linux 桌面是否显示 tray 的桌面环境策略，也不处理 GTK 主题自身的 CSS warning。
- 这不是 UI 图标规范；renderer 内图标仍遵循 `apps/tauri/src/renderer/assets/icons/` 的本地 SVG 分类规则。

## 实现位置

- `apps/tauri/src-tauri/src/lib/runtime.rs`：按平台选择 tray 图标。
- `apps/tauri/src-tauri/icons/tray/linux/*-32.png`：Linux tray 的显式 8-bit RGBA 输入。
- `apps/tauri/src-tauri/icons/app/shared/`：Windows/Linux 共用的满版变体 PNG，1024px 用于窗口/任务栏与 Linux 启动器，256px 用于 Windows tray，512px 用于设置预览。
- `apps/tauri/src-tauri/icons/app/windows/`：Windows 动态多尺寸 ICO。
- `apps/tauri/src-tauri/icons/app/{linux,macos}/`：Linux bundle 图标与 macOS ICNS。
- `apps/tauri/src-tauri/icons/tray/macos/`：macOS template tray 资源。
- `apps/tauri/scripts/generate-platform-app-icons.py`：从满版 1024px 母图生成 Windows/Linux 的派生图标及 Linux bundle 图标。
- `apps/tauri/src-tauri/src/lib/tests/mod.rs`：Linux bundle 图标格式回归测试。
