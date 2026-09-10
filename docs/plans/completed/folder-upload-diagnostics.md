# 文件夹上传诊断与批次隔离

## 背景与结论

现场反馈 Windows 下 `files`、`shared` 文件夹未上传。缺少原始目录、协议和日志，尚不能确认现场根因。

之前的 `11048bc1` 提供 manifest 执行阶段的文件失败隔离和瞬时重试；`83286dab` 合并批量上传快照更新。但 renderer 创建任务的循环遇到任意异常就退出，后续选择路径不会尝试，之前的重试无法覆盖这个阶段。

这次重点核对 Windows 后，发现一个只在 Windows 生效的高概率原因：`3955e6b3` 为避免静默漏传，曾把所有 `FILE_ATTRIBUTE_REPARSE_POINT` 都当作 Junction 拒绝。这个属性不只表示符号链接或 Junction，也会出现在 WOF 压缩文件、OneDrive/Cloud Files 占位文件等仍可由 Windows 正常读取的条目上。macOS 不执行这段 `cfg(windows)` 逻辑，所以同一目录在 macOS 上可能正常。

截图中的 `files` 与 `shared` 是 Office Tool Plus 的运行时目录，目录名本身没有特殊处理。问题应落在目录内某个条目的 Windows 文件属性、访问权限或读取时机，不能仅凭截图确定具体条目。

## 实现

- `upload-batch.ts` 逐个提交去重后的路径，单项失败后继续提交，其余成功任务正常运行；批次结束只应用最后一个成功快照，再集中报告每个失败路径与原因。
- Rust 在上传入口生成传输 ID，创建成功后继续作为任务 ID；创建失败也能在日志中定位。请求日志记录本地路径、远端目录和标签页；目录扫描记录协议、权限模式、数量、字节数和耗时；创建失败记录原因与耗时。
- 扫描读取目录项、文件类型和元数据的错误均携带具体路径；远端建目录失败记录任务 ID、协议、方向、路径及重试次数，任务错误也保留目录路径。
- Windows 扫描现在读取重解析点 tag：只拒绝符号链接、Junction 和无法识别的重解析点，允许 WOF、旧版 placeholder 及 Cloud Files tag；拒绝日志包含完整本地路径、`tag` 和文件属性值。上传文件的 metadata/open 错误也包含完整本地路径。
- 保持符号链接/Junction、不支持类型和不可读目录显式失败，不将漏传伪装成成功；空目录仍进入目录 manifest。

## 验证

- `node --test apps/tauri/tests/upload-batch.test.mjs`：3 项通过，覆盖首项/中项失败后继续、路径去重、末项失败保留快照、全失败和空批次。
- `npm run test:tauri`：610 个单元测试、10 个 CLI 集成测试和 21 个契约测试通过。
- 新增空目录保留、扫描失败携带路径的 Rust 用例，与原有嵌套/隐藏文件、符号链接拒绝用例一同针对性回归。
- 类型检查、ESLint、Prettier、Clippy 通过。

## 现场复测

最终跨平台复核补充：

- 本地路径按原生组件转换为远端相对路径，保留 Unix 文件名里的反斜杠；越界路径和非 UTF-8 路径明确失败，禁止有损改名。
- SFTP 与 FTP 目录上传均在提交文件前验证远端临时文件大小；零字节文件也要求远端条目存在。此处增加每个 SFTP 文件一次 stat 往返，以检查上传期间源文件长度变化造成的截断或多传；并非目录的原子快照或全量内容哈希验证。
- Windows 查询重解析 tag 使用零数据访问权限的句柄，避免单纯读取属性额外要求文件内容读取权限。
- 新增完整清单回归：中文、空格、隐藏条目、空目录和零字节文件；Unix 反斜杠、非法编码与 socket 显式失败；Windows 盘符、UNC 与扩展路径转换用例。
- 上传按钮和选择器 UI 保持现状：按钮仍调用原有文件选择器；文件夹继续使用原有拖拽或本地文件列表上传入口，不新增文件夹选择按钮。
- Windows 重解析误判属于已确认的代码缺陷，但截图无法证明原始目录具有这些属性；现场根因仍待日志确认。当前只在 macOS 运行测试，Windows 专属用例待 Windows CI 执行，Linux 尚未实机验证。

使用包含本修复的版本，将普通文件与 `files`、`shared` 一起拖入远程文件区。确认普通文件不因其他路径失败而漏传；失败文件夹应显示具体原因。从设置打开日志目录，提供同一时间段的 `transfer:` 日志，并记录使用 SSH/SFTP 还是 FTP、目标路径及是否 root 模式。

若需要在 Windows 现场先确认属性，可执行：

```powershell
Get-ChildItem -LiteralPath "C:\路径\files", "C:\路径\shared" -Force -Recurse |
  Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint } |
  Select-Object FullName, Attributes
```

日志中的 `Windows 符号链接/Junction`、`Windows 不支持的重解析点` 或 `Windows 重解析点无法读取 tag` 会直接指出阻断扫描的条目；若没有这些信息，则继续检查同一条目后续的 `无法读取本地上传文件` / `无法打开本地上传文件`。

按传输 ID 检查 `upload requested`、`local directory scan started/completed`、`upload queued`、`upload creation failed`、`directory preparation failed`。只有扫描开始没有扫描完成时，结合创建失败原因检查目录权限、链接/Junction 或文件消失；已入队则继续查看执行阶段错误。

本机未实测 Windows Explorer/WebView2、用户原始目录及真实远端，现场问题仍需复测确认。Windows 目标交叉编译还受 macOS 缺少 MSVC/Windows SDK 影响，不能替代 Windows CI 或实机验证。本次没有发版或关闭 Issue。
