# SFTP 下载收尾与诊断（Refs #369）

状态：代码和本机自动化验证完成；Xlight/Windows 实机待验证，Issue 保持打开。

## 范围

- 下载按远端已知大小限制读取，不额外请求 EOF；拒绝未知大小、断点越界和提前 EOF。
- 下载读写和 flush 使用连接的操作超时并支持取消，失败保留断点。
- 普通 SFTP 的可选 SHA-256 仅在已探测到 SSH exec SHA-256 能力时执行；能力未知或不可用时记录跳过，保留服务层大小验证。root 模式保留原有校验要求，已支持的校验失败不降级。
- 修复同时覆盖单文件与目录下载共用入口，上传共用校验能力判断；不改变 SSH 算法或主机密钥策略。
- 记录通道获取、锁等待、远端属性/打开、本地打开、读取、flush、本地/远端校验、大小验证与提交阶段，关联 transfer_id，避免逐块日志。

## 验证结果

- 已通过 6 项针对性回归：已知大小无需 EOF、最后一块边界、空文件/完整断点、提前 EOF、读满后服务端返回 Permission denied、读取权限错误、停滞超时、取消、校验能力及 root 约束。
- 类型、lint、Prettier、Rust 格式与 Clippy（locked/all-targets/all-features，warnings denied）通过。
- Renderer：246 通过，1 项平台相关测试跳过；Rust：924 项单元测试、33 项集成/contract 测试通过。
- 最终代码的 6 项针对性回归再次通过；能力在传输收尾时读取，避免沿用传输开始时尚未完成的探测结果。
- Xlight/Windows 实机未具备，不能将本机回归等同于 Issue 已解决。

## 2026-11-04 补充回归

- 复用内存 SFTP 夹具，在文件末尾返回 `SSH_FX_PERMISSION_DENIED`。
- 正常下载读取完整内容后成功结束，断言未发送额外 READ。
- 主动越界 READ 验证服务端确实返回 `Permission denied: Permission denied`，与 Issue 日志一致；该结果仅验证修复覆盖此场景，不代表已确认 Xlight 根因。
- 补充后验证：6 项针对性回归、924 项 Rust 单元测试、33 项集成/contract 测试、Clippy 与 Rust 格式检查通过；本轮仅修改测试夹具与文档，未重复运行此前通过的 TypeScript/renderer 门禁。
