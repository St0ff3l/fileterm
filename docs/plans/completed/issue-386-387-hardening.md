# Issue 386/387 修复复查与加固

Refs #386、#387。按单个桌面用户、多标签并发传输与其他设备并发备份的使用情形检查。

- 上传大文件进入远端 SHA-256 后继续响应取消，不保留最长可达数小时的后台等待任务。
- SHA-256 仅在远端命令正常退出且哈希一致时通过；文件名中的内部不可用标记不能绕过校验。
- 补齐 13 GiB、分块向上取整、双遍校验、操作超时下限、24 小时默认上限及取消上限的回归测试。
- 核对 WebDAV/S3 均仅将明确的 404 识别为不存在；WebDAV HTTP 回归覆盖 404 错误页携带 ETag、200 无 ETag、403 和 500。既有条件 PUT 与并发创建冲突保护保留。

未连接真实远端服务器；本次不关闭 Issue。

## 验证结果

- npm run typecheck、npm run lint、全量 Prettier、cargo fmt、Clippy 均通过。
- Renderer 261 项通过；Rust 单元测试 988 项通过、1 项忽略，集成测试 34 项通过。
- 新增 4 个 SHA-256 回归用例与 1 个 HTTP 多状态探测用例。备份原有 412 并发创建冲突、重建成功和旧版本保护用例继续通过。
- 取消路径采用与现有下载校验一致的优先取消分支；未做真实服务器上的慢校验取消实测。
- 复查未发现 #387 条件 PUT 策略需要进一步放宽的情况；权限错误、服务端错误与缺少 ETag 不能作为远端对象不存在处理。

修改范围：shared.rs 的校验结果验证、dispatch_transfer.rs 的上传取消、SSH/WebDAV 测试与本记录。无新依赖，无新增设置；修改的 Rust 业务文件均未超过 800 行，测试文件为规模豁免。

## 第二轮加固（2026-11-11）

- 修复已取消/暂停/失败/完成任务被迟到进度或校验通知重新激活的竞态，同时保护排队态；finalizing 不回退为 verifying。
- 抽出实际使用的进度/校验状态更新逻辑，增加停止态业务字段不变、阶段与提示保留、正常进入校验三类回归测试。
- 复核备份版本校验与条件 PUT，保留 WebDAV 合并时以 GET 返回的 ETag 绑定上传版本的行为，兼容 HEAD 不返回 ETag、GET 返回 ETag 的服务器。
- WebDAV/S3 增加结构化上传预检日志；补齐 s3-backup 的 integration 分类映射。日志只记录 ETag 存在标志与策略，不包含凭据或备份内容。
- 诊断字段与查看方法补充到 docs/quality/logging.md。

本轮验证结果：

- 状态回归 3 项、日志分类回归 1 项均通过；全量 Rust 单元测试 992 项通过、1 项忽略，集成测试 34 项通过。
- 随后新增的 HEAD 无 ETag / GET 有 ETag 合并上传 HTTP 用例单独运行通过；最新代码的 Clippy 全目标检查通过。
- Renderer 261 项通过；typecheck、lint、全量 Prettier、cargo fmt 与 diff whitespace 检查通过。
- 已补诊断字段说明、排查入口及保密边界；尚未在真实服务上观察新日志或进行多窗口/慢校验取消实测。Issue 保持打开。

本轮修改文件：services/transfers/runtime.rs 及状态测试、services/webdav/sync.rs 与 HTTP 测试、services/s3_backup/sync.rs、services/logging/category.rs、docs/quality/logging.md 和本记录。
