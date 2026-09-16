# 传输路径质量检查

## 检查范围

基于 main 提交 `c2354028`，检查共享包与 Tauri 的类型、静态检查、格式、Rust 测试和前端生产构建，并重点审查 Rust 传输服务的目标路径构造。

## 发现与修复

1. `targetName` 在上传、单文件下载及目录下载中直接参与路径拼接。`../outside` 可越过指定目录，下载中的绝对路径还会替换所选目录。现在服务层要求目标名称为单个有效路径组件，在创建任务与写入文件前拒绝非法名称。
2. 下载默认名称曾通过宿主平台的 `Path::file_name` 解析远端 POSIX 路径；Windows 会把远端名称中的反斜杠解释为分隔符。现在按远端 `/` 分隔符提取名称，再应用宿主文件系统规则。Windows 拒绝反斜杠、盘符、备用数据流、设备名及尾随点/空格；macOS/Linux 保留合法的反斜杠和冒号名称。
3. 目录扫描现在在递归或读取远端文件信息前检查路径属于所选远端根目录，并校验映射到本地的每个名称；忽略列表中的 `.` 与 `..` 条目。
4. SSH/SFTP、FTP 与 root 下载的本地 `.fileterm-part` 断点现在只接受普通文件；拒绝符号链接、FIFO 等特殊文件和 Unix 硬链接。Unix 以 `O_NOFOLLOW` 打开，且仅在确认已打开文件与断点长度后才清空新断点；最终下载目标为链接、特殊文件或 Unix 硬链接时也拒绝替换。

实现集中在 `services/transfers/path_validation.rs`，由原有 facade 引入；不改变协议客户端、IPC 类型或 renderer。

## 验证范围与限制

- 已通过：`npm run typecheck`、`npm run lint`、项目 Prettier 检查、Rust fmt 检查、严格 Clippy、前端生产构建。
- `npm run test:tauri`：639 项单元测试、10 项 CLI 测试和 21 项契约测试通过，共 670 项，无失败或忽略。
- `npm audit --omit=dev --audit-level=low`：0 个漏洞。
- `cargo audit`：`rustls` 已更新至 0.23.45，未报告漏洞；仍有上游 Linux GTK/Tauri 与 vendored SSH 依赖的维护状态警告。

- 新增回归覆盖：目标路径越界、Windows 路径与设备名、远端名称解析、Unicode/空格/隐藏文件、POSIX 特殊名称保留，以及下载断点/最终目标的链接与特殊文件拒绝。
- Windows 名称规则通过显式平台参数在本机测试；尚未进行 Windows/Linux 实机传输与三平台打包验收。
- 本轮校验处理新建任务的路径文本，不改写历史传输 journal，也不替代本地符号链接解析与文件系统权限控制。
- 本轮未改动超过 1000 行的源文件，无需结构拆分。
