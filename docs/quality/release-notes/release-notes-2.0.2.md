# FileTerm 2.0.2

## 正式包终端与编辑器渲染修复

修复 Windows 与 macOS 正式安装包中 xterm 终端文字异常放大、行间挤压，以及 Monaco 文件编辑器文字错位和残影问题。

- 修正 Tauri 生产 CSP 与 xterm、Monaco 运行时样式的兼容边界。
- 仅关闭 Tauri 对 `style-src` 的 nonce 改写，使本地受信任组件可以应用动态样式。
- `script-src` 继续保留 Tauri 的 hash/nonce 加固，未放宽脚本执行策略。
- 增加 CSP 配置契约测试，防止开发态正常、正式包失效的问题再次出现。
