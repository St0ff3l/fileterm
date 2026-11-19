# 已隐藏的功能入口

本文件记录当前 UI 上已隐藏但源代码仍然保留的功能模块，方便后续恢复或参考。

## 隐藏时间：2026-07-08

### 1. Docs 侧边栏入口

- **隐藏位置**：`home-workspace.tsx` 的 `sidebar-footer` 区域
- **保留代码**：`handleOpenDocs` 函数和 GitHub 链接仍可用
- **恢复方式**：在 `sidebar-footer` 中恢复 Docs 按钮即可

### 2. 页脚导航链接（Changelog / API Reference / Status）

- **隐藏位置**：`home-workspace.tsx` 的 `<footer>` 区域
- **保留代码**：`handleOpenDocs` handler 仍可用；页脚版权和上下文统计（连接数/文件夹数）保留
- **恢复方式**：在 footer 中恢复 `<nav className="footer-nav">` 即可

### 3. 页脚 System Latency 文字

- **隐藏位置**：`home-workspace.tsx` footer copyright 区域 `System: 0.1ms latency` 文本
- **恢复方式**：在 footer copyright `<span>` 中恢复该文字即可
