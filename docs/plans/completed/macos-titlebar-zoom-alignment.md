# macOS 标题栏缩放对齐

状态：实现与自动化验证完成（2026-09-28）

## 问题与范围

WebView 界面缩放会放大 48px 顶部标签栏与 FileTerm 字标，但 AppKit 红黄绿按钮仍按固定 48pt 布局，导致放大后按钮偏上。修复仅涉及 macOS 原生标题栏几何与现有 UI preferences 保存链路。

## 实现

- 原生标题栏容器高度使用 `48 × uiZoomPercent / 100` 逻辑点，三个按钮中心沿此高度居中。
- 读取保存的缩放设置，覆盖启动、窗口 resize、跨显示器 DPI 和全屏恢复校准。
- 修改 UI 缩放后主动触发现有去抖校准；按钮保留 AppKit 的 frame、bounds 和圆形绘制。
- 同步设计与架构说明，修正已过时的强制 frame/绘制层缩放描述。

## 验证

- [x] 80% / 100% / 120% / 140% / 200% 的原生几何回归，覆盖 820pt / 1000pt 窗口高度。
- [x] TypeScript、lint、Prettier、CSS contract、Rust fmt 与严格 Clippy。
- [x] 652 个 Rust unit、10 个 CLI integration、21 个 contract 测试通过。

## 运行态验收说明

UI 自动化识别到的已安装 FileTerm 是旧版 2.2.8，无法绑定当前裸开发进程，因此未将该旧版窗口作为修复后的视觉证据。原生窗口改动需要重新构建并重启当前开发版；实际包中的缩放、全屏恢复及跨显示器视觉验收沿用[桌面 UI 回归清单](../../quality/desktop-ui-regression-checklist.md)。Windows/Linux 的相关分支由平台门控与 PR CI 验证。

CSS contract 的 11 个历史 `!important` 与约 1 个历史直接色值告警未增加。改动涉及的业务文件均低于 800 行，无需拆分。
