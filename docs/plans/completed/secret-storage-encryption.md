# 本机凭据字段加密

状态：实现已完成（2026-08-16）；三平台打包环境验收已集中到[统一验收计划](../active/release-candidate-acceptance.md)
关联：[AI Copilot 功能集成计划](./ai-copilot-integration.md)、[MCP / CLI interactive-exec 关闭记录](./mcp-cli-interactive-exec.md)、[架构地图](../../architecture.md)

## 1. 结论

FileTerm 不使用 macOS Keychain、Windows DPAPI 或 Linux credential store，因此不会在读取已保存连接时触发系统授权弹窗。为避免本地 JSON 中直接暴露凭据，Rust 存储层对各凭据字段采用 AES-256-GCM 加密：调用方仍只读写明文 `String`，加解密与迁移完全留在 main-side。

```text
安装目录内随机 seed（0600） ─┐
                              ├─ HMAC-SHA256 ─ AES-256-GCM key
当前设备稳定标识 ───────────────┘
                                       │
字段用途 + 记录 ID（AAD） ─────────────┼─ 加密 / 认证
                                       ▼
                         ftsec:v1:base64(nonce || ciphertext || tag)
```

每安装生成一次 32-byte 随机 seed；它和当前设备标识共同参与派生，因此单独复制一个凭据 JSON 到其他设备不能解密。每次写入都会使用新的 96-bit nonce，字段 scope 作为 AAD，阻止将某个密文复制到另一个 Provider、profile 或凭据类型后继续解密。

## 2. 覆盖范围

| 本地文件                   | 加密字段                          | scope 示例                                 |
| -------------------------- | --------------------------------- | ------------------------------------------ |
| `ai-provider-secrets.json` | `api_key`                         | `ai-provider/<provider-id>/api-key`        |
| `ssh-key-secrets.json`     | 私钥口令                          | `ssh-key/<key-id>/passphrase`              |
| `profile-secrets.json`     | 连接密码、私钥口令/路径、代理密码 | `profile/<profile-id>/<field>`             |
| `webdav-sync.json`         | WebDAV 密码                       | `webdav/password`                          |
| `s3-backup.json`           | Access Key ID、Secret Access Key  | `s3/access-key-id`、`s3/secret-access-key` |

`webdav-sync.json` 与 `s3-backup.json` 是本机配置文件，不是加密的远程备份包。WebDAV/S3 的远程备份，以及用户显式导出的 JSON，仍是既有跨设备迁移载体，按原有行为可能包含连接凭据；本计划不改变它们的导入导出语义。

## 3. 存储与迁移契约

1. 新值写入时，先对字段加密，再沿用现有 `write_restricted_file` 和原子替换链路落盘。
2. 读取 `ftsec:v1:` 密文时解密给 Rust 服务层；renderer、公开 workspace snapshot 和日志始终只得到非敏感标记。
3. 读取旧版明文时保留内存中的明文给本次调用，并立即通过加密写入器原子迁移；迁移后的字段不再出现明文。
4. 解密失败不覆盖原文件，返回“请在此设备重新配置该凭据”的通用错误，日志不记录密文、明文、nonce 或派生材料。
5. Unix 上 seed、key 和 secret 文件都保持 `0600`；Windows 依赖应用数据目录的 per-user ACL。

## 4. 威胁模型

| 场景                                 | 结果                                                 |
| ------------------------------------ | ---------------------------------------------------- |
| 在磁盘上直接 `cat` 单个凭据 JSON     | 看不到 API Key、密码或私钥口令明文                   |
| 单独误传一个凭据 JSON / 合规扫描     | 不包含可用明文；另一设备缺少 seed 和本机标识不能解密 |
| 密文被复制到另一个记录或字段         | AAD scope 不匹配，认证解密失败                       |
| 当前本机用户权限被攻破               | 不提供额外保护；应用可解密的内容，攻击者也可能取得   |
| 同时窃取完整应用数据与受害者运行环境 | 不作为本方案防御目标                                 |

这不是用户主密码或系统钥匙串替代品。目标是降低静态文件、备份误操作和日志/扫描误泄漏风险，同时保持 FileTerm 现有无弹窗体验。

## 5. 实现位置

- `apps/tauri/src-tauri/src/services/secret_crypto.rs`：seed 创建、三端设备标识、HMAC 派生、AES-GCM、版本前缀和 legacy 判断。
- `services/ai.rs`、`services/ssh_keys.rs`、`services/profile_ops.rs`、`storage/mod.rs`、`services/webdav.rs`、`services/s3_backup.rs`：字段级加密、读取迁移与原子持久化。
- `Cargo.toml`：`aes-gcm`、`zeroize`；Windows 仅额外启用只读 `MachineGuid` Registry feature。

实现明确不使用 safeStorage、Keychain、Credential Manager、DPAPI、libsecret、KWallet 或外部网络服务。

## 6. 已覆盖回归

- AES-GCM 往返、随机 nonce、篡改拒绝、scope 绑定与 legacy 明文迁移。
- Unix 下 installation seed 为 owner-only。
- AI Provider 密钥真实 JSON 加密与旧明文读取迁移。
- SSH 私钥口令、WebDAV 密码、S3 access/secret key 的旧明文读取迁移与二次读取稳定性。
- Profile secrets 在重建时加密、保留匹配密文、清理已删除 profile 的孤儿字段。
- `cargo fmt --check`、clippy、320 个 Rust unit tests、19 个 contract tests、Tauri typecheck、lint、Prettier 以及 macOS arm64 release `.app` / `.dmg` 构建。
- macOS、Windows、Linux CI 矩阵执行 `secret_crypto` 往返、篡改拒绝、scope 绑定与 legacy 判断测试，覆盖三端设备标识分支的编译与运行路径。
- PR CI 的 macOS、Windows、Linux package smoke 已确认无签名包可以生成；它不读取真实安装包中的既有凭据，因此不能替代下一节的跨平台迁移验收。

## 7. 验收范围（已转移）

本计划原有的 Windows/Linux 设备标识与加解密往返、三个实际打包应用中的旧明文迁移/重启读取/公开 bridge 脱敏验收范围已转移到[统一验收计划](../active/release-candidate-acceptance.md)。macOS arm64 release 已有的通过记录保留在上文；其余平台实际通过结果以统一计划的证据记录为准。
