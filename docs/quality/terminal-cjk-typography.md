# 终端中文标点网格回归

Refs #254。修复范围为 xterm 6 DOM renderer 的中文标点宽度漂移。

Chromium 默认压缩连续全角标点；xterm `WidthCache` 使用同一字符重复
32 次的平均宽度，而实际行中的单个标点没有同样的压缩。由此产生过大的
`letter-spacing` 补偿。上游分析：
https://github.com/xtermjs/xterm.js/issues/6058 。

`components/terminal-view.css` 在 `.terminal-host .xterm` 设置
`text-spacing-trim: space-all`，让显示行与隐藏测量容器继承一致的排版规则；
同时用 `text-autospace: no-autospace` 禁用混排自动间距。保留原来的字体、
Unicode 11 字宽、字号、行距及字体加载/DPI 度量同步。

## 自动浏览器回归

安装 Playwright 并提供本机 Chrome 后运行：

```sh
node apps/tauri/tests/browser/terminal-typography.mjs
```

也可设置 `PLAYWRIGHT_MODULE_PATH` 指向已有 Playwright 包目录，无需修改项目依赖。
测试先复现默认样式的漂移，再加载生产 CSS；比较每行左右边框的 DOM 坐标与
xterm buffer 列位置，允许不足 1 CSS px 的字体测量取整误差。

覆盖内置 JetBrains Mono、SF Mono/Menlo 系统回退字体栈，12/16/24px 字号，
1/1.25/2 DPI，原始 Issue 样例、连续标点、中英混排和块绘制字符。
macOS Chrome 实测：修复前最大误差约 35px，修复后小于 0.13px。

本次验证通过：Tauri typecheck、全量 ESLint/Prettier、CSS contract、现有
renderer 测试、Rust unit/contract 测试、Clippy（all-targets/all-features）
及生产 renderer 构建。CSS contract 仍报告既有的 11 处 `!important` 和约
1 处遗留直接颜色值，本次没有新增这些债务。

## 发布前平台复核

在 Windows 11 WebView2 连接 Ubuntu 22.04，用 nano 输入：

```text
│123./;│
│123。│
│。。│
```

检查 `。` 占两格，移动光标、缩放、切换字体后仍与文本位置一致；在 macOS
WKWebView 与 Linux WebKitGTK 重复检查。当前自动化只验证本机 Chrome，
尚未代替三端原生 WebView 手测。

Issue 提及的 Claude Code 白色主题“豆腐块”缺少独立复现样例。块绘制字符
网格测试不等同于字体覆盖测试，不能据此认定缺字问题已解决。需在 Windows
使用实际 Claude Code 输出复核；本次不替换用户字体或改变 ANSI 颜色。
