# FileTerm 2.0.1

## 正式包终端与编辑器渲染修复

修复 Rust + Tauri 正式包在 Windows 与 macOS 上可能出现的终端和 Monaco 文件编辑器字号异常、行层错位或画布残影问题。

- 终端与 Monaco 统一使用随包提供的 JetBrains Mono，不再依赖各系统是否安装 SF Mono、Menlo 或 Consolas。
- 本地字体完成加载后，自动重新测量 xterm 终端和 Monaco 编辑器的字形与布局。
- 窗口缩放、跨显示器 DPI 变化或 WebView 像素比变化后，自动刷新终端与编辑器的缓存尺寸。

如果仍有问题，请在 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交操作系统版本、显示缩放比例、FileTerm 版本、复现步骤和脱敏日志。请勿提交密码、私钥或 token。
