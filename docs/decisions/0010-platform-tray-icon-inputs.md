# ADR-0010: 平台托盘图标使用专用的 8-bit 位图输入

## 状态

Accepted（2026-09-19）

## 背景

FileTerm 的 tray 在 macOS、Windows 和 Linux 上由不同的原生后端渲染。macOS 需要独立的单色 Template 图标，Windows 使用多尺寸 ICO；Linux 的 GTK tray 后端要求每个像素恰好为 4 个 8-bit RGBA 字节。

Tauri 会把 Linux bundle 配置中第一张 PNG 嵌入为原始像素缓冲。若 PNG 是 16-bit RGBA，32×32 图会携带 8192 字节，而 tray 后端只接受 4096 字节的 32bpp 数据。将 `default_window_icon()` 直接交给 Linux tray 会使应用在 setup hook 中失败，影响 `.AppImage` 和 `.deb` 两种包。

## 决策

1. macOS tray 继续使用独立的 `trayTemplate@2x.png`，并以 template 模式渲染；Windows tray 继续显式读取 `icon.ico`。
2. Linux tray 显式读取 `apps/tauri/src-tauri/icons/32x32.png`，不复用 Tauri 自动生成的 `default_window_icon()`。
3. Linux bundle 使用的 `32x32.png`、`128x128.png` 和 `128x128@2x.png` 必须为 8-bit RGBA。图案、颜色、透明度和尺寸可以保持不变，只允许为兼容性调整 PNG 的通道位深。
4. Rust 回归测试检查这些 PNG 的 IHDR 位深与 RGBA 颜色类型，防止后续资产工具重新导出 16-bit 图像。

## 影响

### 正面影响

- Linux tray 初始化拥有确定的 32bpp 输入，AppImage 与 deb 不再因位图字节数不匹配而在启动阶段崩溃。
- 平台图标职责清晰：macOS template、Windows ICO、Linux 8-bit RGBA 位图不会相互复用。
- 资产编码错误能够在常规 Rust 测试中被发现，不依赖桌面环境复现。

### 限制与非目标

- 此决策不改变 Linux 桌面是否显示 tray 的桌面环境策略，也不处理 GTK 主题自身的 CSS warning。
- 这不是 UI 图标规范；renderer 内图标仍遵循 `apps/tauri/src/renderer/assets/icons/` 的本地 SVG 分类规则。

## 实现位置

- `apps/tauri/src-tauri/src/lib/runtime.rs`：按平台选择 tray 图标。
- `apps/tauri/src-tauri/icons/32x32.png`：Linux tray 的显式 8-bit RGBA 输入。
- `apps/tauri/src-tauri/src/lib/tests.rs`：Linux bundle 图标格式回归测试。
