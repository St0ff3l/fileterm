# WebDAV/S3 远程备份加密计划

状态：实现已完成（2026-08-16）；全量质量门禁与三端打包验收已集中到[统一验收计划](../active/release-candidate-acceptance.md)

关联：[架构地图](../../architecture.md)、[密钥存储加密计划](./secret-storage-encryption.md)

## 1. 范围与结论

只加密 WebDAV/S3 远程备份，不改变用户主动导出 JSON 的行为：

- WebDAV 和 S3 共用 `webdav::export_bundle()`，上传一律生成 schema v3 密文包。
- 用户从设置中点击上传或下载时，主窗口弹出一次性备份主密码输入框。
- 主密码只在 Rust 任务与 renderer 当前表单内存中短暂存在，不写入配置、聊天记录、终端 transcript、日志或 MCP/CLI 结果。
- 用户主动导出连接 JSON（`app_export_connections` / `app_export_connections_as_files`）继续保持明文，便于用户自行处理和迁移。

## 2. v3 包格式

整个 profiles JSON 在上传前使用 AES-256-GCM 加密。密钥由用户主密码通过 Argon2id 派生：

```json
{
  "schemaVersion": 3,
  "containsSecrets": true,
  "generatedAt": "2026-08-13T00:00:00Z",
  "encryption": {
    "algorithm": "AES-256-GCM",
    "kdf": "Argon2id",
    "version": "0x13",
    "memoryKiB": 65536,
    "iterations": 3,
    "parallelism": 1,
    "salt": "base64url-no-pad",
    "nonce": "base64url-no-pad"
  },
  "ciphertext": "base64url-no-pad(ciphertext || tag)",
  "contentHash": "sha256-hex(ciphertext)"
}
```

固定约束：

- salt 16 bytes，nonce 12 bytes，密钥 32 bytes。
- Argon2id 使用 64 MiB、3 passes、1 lane，密码至少 8 个字符，并且必须同时包含大写字母和小写字母。
- 加密元数据作为 AES-GCM AAD，篡改参数、密文或 hash 都会失败。
- 允许解读 `PBKDF2-HMAC-SHA256` v3 兼容包（100,000–1,000,000 次迭代），新上传不再使用 PBKDF2。
- 不使用本机设备绑定密钥；同一备份可以跨设备恢复，只要用户记得主密码。

## 3. 兼容与升级

下载按 `schemaVersion` 分流：

- v3：要求一次性输入主密码，解密成功后再合并连接。
- v1/v2/无版本旧包：仍可导入，不要求密码；结果提示“备份未加密，建议重新上传”。
- 未知版本、坏 hash、坏密文、错误密码：拒绝导入，不修改本地连接。

上传始终生成 v3，因此旧明文备份在用户下一次上传后会被自然升级。用户主动导出的 JSON 不参与这条升级路径。

## 4. 交互边界

密码提示使用独立的 `backup:password-request` 事件和 `app_resolve_backup_password` command：

- 不复用终端输入或交互式 SSH exec 输入，避免密码被写进可见终端。
- 只接受主窗口 renderer 的已注册监听；renderer 不可用时上传/下载 fail closed。
- 密码输入只用于当前一次上传或下载，取消、超时、窗口卸载后清理 pending sender。
- WebDAV/S3 的 HTTP/S3 认证密码仍由既有配置存储负责；备份主密码是独立的、不会持久化的密钥材料。

## 5. 验收范围（已转移）

本计划原有的 v3/v2 兼容性、密文请求体、明文导出不变、质量门禁和三平台密码交互验收范围已转移到[统一验收计划](../active/release-candidate-acceptance.md)。实现与自动化回归的事实仍保留在本页；外部打包环境的实际通过结果以统一计划的证据记录为准。
