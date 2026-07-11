# 已隐藏但代码保留的功能

本文件记录当前 UI 上已隐藏但源代码仍然保留的功能模块，方便后续恢复或参考。

## 隐藏时间：2026-07-08

### 1. 快速连接（Quick Connect）侧边栏入口

- **隐藏位置**：`apps/desktop/src/renderer/features/workspace/HomeWorkspace.tsx` 侧边栏导航
- **保留代码**：
  - `QuickLinksPage.tsx` 组件完整保留
  - `activeTab === 'quick-links'` 的路由和渲染逻辑完整保留
  - i18n 中 `quickConnect` 键值保留
- **恢复方式**：在 `HomeWorkspace.tsx` 的 `<nav className="sidebar-nav">` 中恢复 quick-links 按钮即可

### 2. Docs 侧边栏入口

- **隐藏位置**：`HomeWorkspace.tsx` 的 `sidebar-footer` 区域
- **保留代码**：`handleOpenDocs` 函数和 GitHub 链接仍可用
- **恢复方式**：在 `sidebar-footer` 中恢复 Docs 按钮即可

### 3. 页脚导航链接（Changelog / API Reference / Status）

- **隐藏位置**：`HomeWorkspace.tsx` 的 `<footer>` 区域
- **保留代码**：`handleOpenDocs` handler 仍可用；页脚版权和上下文统计（连接数/文件夹数）保留
- **恢复方式**：在 footer 中恢复 `<nav className="footer-nav">` 即可

### 4. 页脚 System Latency 文字

- **隐藏位置**：`HomeWorkspace.tsx` footer copyright 区域 `System: 0.1ms latency` 文本
- **恢复方式**：在 footer copyright `<span>` 中恢复该文字即可
