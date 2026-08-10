# 凭据存储加密计划

状态：规划中（方案选型已确定，尚未开始改代码）
关联：[ai-copilot-integration.md](./ai-copilot-integration.md)、[local-terminal-mcp.md](./local-terminal-mcp.md)

## 1. 结论

FileTerm 当前所有凭据（AI Provider API Key、SSH 私钥口令、WebDAV 同步密码、S3 兼容备份密码、profile 密码、代理密码）以**明文 + Unix `0600` 权限 + 原子替换**存于本地文件。本计划在不动用户交互、不引入系统弹窗的前提下，给这些凭据文件增加**对称加密层**：

- 存盘前 AES-256-GCM 加密、读盘后解密，对调用方完全透明。
- 密钥从机器指纹派生，启动时自动解锁，**无主密码、无 Keychain 弹窗、无用户交互**。
- 与现有硬性边界兼容：不调用 macOS Keychain / Windows DPAPI / libsecret / KWallet。

## 2. 目标与非目标

### 目标

1. 凭据文件 `cat` 看不到明文。
2. 凭据文件被备份到云盘 / 误传 GitHub 时不泄露 API Key。
3. 合规扫描器扫不到明文 API Key、密码、私钥口令。
4. 多用户机器上其他用户读到凭据文件无法直接使用。
5. 调用方代码无感：读出来还是 `String` 明文，写进去还是 `String` 明文，加解密在存储层内部完成。

### 非目标

1. **不解决本机主动攻击者**：攻击者拿到本机用户权限 + 二进制 + machine-id = 仍可解密。这是任何应用级加密都解决不了的，只能靠 OS 用户隔离（已经通过 `0600` 权限做到）。
2. **不引入用户主密码**：不开机输密码、不弹密码设置窗。
3. **不接入系统钥匙串**：不动 macOS Keychain / Windows Credential Manager / libsecret / KWallet。
4. **不替代 `0600` 权限**：加密是额外层，文件权限保持不变。
5. **不改跨设备同步模型**：凭据本就不支持跨设备同步（设计如此），加密后依然不支持。

## 3. 威胁模型

| 威胁场景                    | 当前明文方案 | 加密后                              |
| --------------------------- | ------------ | ----------------------------------- |
| 用户 `cat` 文件看到 API Key | ❌ 暴露      | ✅ 解决（看到的是 base64 密文）     |
| 备份到云盘 / 误传 GitHub    | ❌ 暴露      | ✅ 解决（密文不可读）               |
| 合规扫描器扫到明文          | ❌ 触发      | ✅ 解决（无明文 pattern）           |
| 多用户机器其他用户读到      | ❌ 暴露      | ✅ 解决（需他们也没有本机用户权限） |
| 本机主动攻击者窃取          | ❌ 暴露      | ❌ 不解决（machine-id 可读）        |
| 撞库 / 字典攻击             | ❌ 直接暴露  | ✅ 解决（AES-GCM 不可字典攻击）     |

## 4. 方案选型

### 入选方案：AES-256-GCM + 机器指纹派生密钥

```
machine_id (OS-level)              app_salt (hardcoded)
   │                                  │
   └────── concat ───────────────────┘
                  │
                  ▼
            SHA-256 (32 bytes)
                  │
                  ▼
       AES-256-GCM key
                  │
   ┌──────────────┴──────────────┐
   │                             │
encrypt(plaintext)           decrypt(ciphertext)
   │                             │
   ▼                             ▼
base64(nonce || ciphertext || tag)
   │
   ▼
写入 ai-provider-secrets.json (替换原明文字段)
```

**核心库**：`aes-gcm` + `sha2` + `base64`（均为纯 Rust、轻量、已审计的 crate）

**密钥派生**：

- macOS：`IOPlatformUUID`（通过 `IOKit` 调用）
- Windows：`MachineGuid`（注册表 `HKLM\SOFTWARE\Microsoft\Cryptography`）
- Linux：`/etc/machine-id`（systemd 标准文件）
- 全部都是 OS 级稳定标识，重装系统 / 换机器才会变

**加密格式**：

```
base64( nonce[12] || ciphertext[N] || tag[16] )
```

### 不入选方案及理由

| 方案                             | 不选理由                                                                                                 |
| -------------------------------- | -------------------------------------------------------------------------------------------------------- |
| **Tauri Stronghold**             | 二进制 +5MB，启动 +100~300ms（Argon2id 派生），Stronghold 是 KV store 与现有 JSON 结构不兼容，过度工程化 |
| **macOS Keychain / safeStorage** | 违反 AGENTS.md 硬性边界「macOS 钥匙串规避」，首次访问弹系统授权窗                                        |
| **Windows DPAPI 单平台**         | 三平台行为不一致，要写两套逻辑，违背「跨平台一致」原则                                                   |
| **用户主密码 + Argon2id**        | 体验最差（每次启动输密码），跟现有所有凭据策略不一致，需要写一整套密码强度 / 密钥派生 / 内存清零逻辑     |
| **`age` / SOPS**                 | 密钥文件本身要存哪？绕一圈回到原点                                                                       |
| **机器指纹派生 + 硬编码密钥**    | 本机攻击者拿到二进制 = 拿到密钥，安全性等同明文                                                          |

## 5. 影响范围

### 5.1 凭据文件清单

| 文件                       | 内容                                               | 加密策略                 |
| -------------------------- | -------------------------------------------------- | ------------------------ |
| `ai-provider-secrets.json` | AI Provider API Key                                | `api_key` 字段加密       |
| `ssh-key-secrets.json`     | SSH 私钥口令                                       | `passphrase` 字段加密    |
| `profile-secrets.json`     | profile 密码、代理密码、WebDAV 密码、S3 Secret Key | 对应字段加密             |
| `webdav-sync.json`         | WebDAV 完整同步包                                  | 已是完整加密包，整体加密 |
| `s3-backup.json`           | S3 备份完整包                                      | 同上                     |

### 5.2 代码改动点

- 新建 `apps/tauri/src-tauri/src/services/secret_crypto.rs`：
  - `derive_key() -> [u8; 32]`：三平台机器指纹派生
  - `encrypt(plaintext: &str) -> Result<String>`：返回 base64 密文
  - `decrypt(encrypted: &str) -> Result<String>`：解密
  - `is_encrypted(value: &str) -> bool`：判断字段是否已是密文（迁移用）
- 改 `services/ai.rs`：`StoredProviderSecret.api_key` 写入前 `encrypt`、读取后 `decrypt`
- 改 `services/ssh_keys.rs`：`StoredKeySecret.passphrase` 同上
- 改 `services/profiles.rs` / `storage/profile_repository.rs`：profile / WebDAV / S3 凭据同上
- 改 `services/webdav.rs` / `services/s3_backup.rs`：完整加密包整体加密

### 5.3 不改动的部分

- `packages/core` 类型：`AiProviderDraft` / `SshProfile` / `FtpProfile` 等 renderer 类型保持不变，加密只在 Rust 存储层。
- Bridge 层：`tauri-api.ts` 不动，读写接口签名不变。
- Renderer：UI 完全无感，`hasApiKey` / `hasSavedPassword` 标记语义不变。
- 文件权限：Unix `0600` 保持不变。
- 原子替换：`write_restricted_file` + `replace_file_atomically` 链路保持不变。
- MCP / CLI：通过 bridge 走桌面进程，桌面进程在内存里解密后传给子进程，子进程拿到的还是明文（这部分架构天然契合，无需改动）。

## 6. 迁移逻辑

启动时读凭据文件，对每个字段：

1. `is_encrypted(value)` 判断是否已是 base64 密文
2. 如果是明文：`encrypt` 后写回，标记为已迁移
3. 如果已是密文：`decrypt` 给调用方使用

迁移是一次性的：首次启动加密所有明文，后续直接读密文。失败回滚：解密失败时保留原文件 + 写日志，不破坏数据。

## 7. 测试覆盖

### 7.1 单元测试（`secret_crypto.rs`）

- `encrypt_decrypt_round_trip`：明文 → 加密 → 解密 → 还原
- `encrypt_produces_different_ciphertext`：相同明文每次加密结果不同（随机 nonce）
- `decrypt_rejects_truncated_data`：截断的 base64 密文解密失败
- `decrypt_rejects_tampered_tag`：篡改 GCM tag 解密失败
- `is_encrypted_correctly_detects_plaintext`：明文不被误判为密文
- `derive_key_is_stable_across_calls`：同一机器多次派生密钥一致
- `derive_key_changes_with_different_salt`：不同 salt 派生不同密钥
- `empty_string_encrypts_to_nonempty_ciphertext`：空字符串也能加密

### 7.2 集成测试

- `ai_provider_secret_migration_from_plaintext`：旧版明文 `ai-provider-secrets.json` 启动后自动迁移为密文
- `ssh_key_secret_migration_from_plaintext`：同上
- `profile_secret_migration_from_plaintext`：同上
- `migration_is_idempotent`：已是密文不会再次迁移
- `migration_failure_preserves_original_file`：解密失败的文件保留原样

### 7.3 跨平台机器指纹读取

- macOS：`IOPlatformUUID` 非空
- Windows：`MachineGuid` 注册表读取成功
- Linux：`/etc/machine-id` 文件存在且非空
- 三平台派生密钥长度 = 32 字节

## 8. 安全审计要点

1. **nonce 唯一性**：每次加密生成新随机 nonce（`OsRng`），不复用。
2. **GCM tag 校验**：解密时严格校验 16 字节 tag，篡改即失败。
3. **密钥内存清零**：派生密钥在函数返回前 `zeroize`（引入 `zeroize` crate）。
4. **明文内存清零**：解密后的明文 String 在 drop 前 `zeroize`。
5. **日志脱敏**：加密 / 解密日志不写明文、不写密钥、不写 nonce。
6. **错误消息脱敏**：解密失败返回通用错误「凭据解密失败」，不暴露内部细节。

## 9. 二进制与启动开销预估

| 维度            | 增量                                                   |
| --------------- | ------------------------------------------------------ |
| 二进制体积      | +约 200KB（`aes-gcm` + `sha2` + `base64` + `zeroize`） |
| 启动耗时        | +5~15ms（首次派生密钥 + 读凭据解密）                   |
| 内存占用        | +几 KB（解密后的明文缓存）                             |
| Cargo.toml 依赖 | +4 个 crate                                            |

## 10. 风险

1. **machine-id 读取失败**：某些 Linux 容器没有 `/etc/machine-id`。**降级方案**：回退到应用首次启动生成的随机 UUID，存 `0600` 文件（这种场景下加密弱化但仍不弹窗）。
2. **跨平台密钥派生不一致**：如果未来 FileTerm 支持凭据跨设备同步，不同机器派生的密钥不同，密文无法解密。**当前不是问题**：凭据本就不支持跨设备同步。
3. **加密迁移失败**：解密失败时保留原文件 + 日志，不破坏数据，用户可手动恢复。
4. **AGENTS.md 硬性边界变更**：当前「macOS 钥匙串规避」边界不变，本方案不调用 Keychain，符合精神；但需要在 [architecture.md](../../architecture.md) 12.2 节补充加密层描述。

## 11. 推进步骤

1. 新建 `services/secret_crypto.rs` + 8 个单元测试
2. 改 `ai.rs` 的 `StoredProviderSecret.api_key` 走加密层
3. 改 `ssh_keys.rs` 的 `StoredKeySecret.passphrase` 走加密层
4. 改 profile / WebDAV / S3 凭据字段走加密层
5. 写迁移逻辑（自动判断明文 vs 密文）
6. 补 5 个集成测试
7. 三平台机器指纹读取 + 测试
8. typecheck + clippy + test:tauri 验收
9. 更新 [architecture.md](../../architecture.md) 12.2 节描述加密层
10. PR draft

## 12. 决策记录

- **为什么不做 Stronghold**：5MB 二进制 + 200ms 启动开销过大，Stronghold 是 KV store 与现有 JSON 结构不兼容，过度工程化。
- **为什么不做用户主密码**：体验最差，跟现有所有凭据策略不一致。
- **为什么不做 Windows DPAPI 单平台**：三平台行为不一致。
- **为什么接受本机攻击者弱点**：FileTerm 是个人 / 小团队桌面工具，威胁模型不包含本机主动攻击者；OS 用户隔离（`0600` 权限）已经覆盖该场景。
- **为什么不动 `packages/core` 类型**：加密只在 Rust 存储层，对调用方完全透明。
- **为什么选 AES-256-GCM**：纯 Rust 实现、crate 成熟、API 简单、性能足够、跨平台一致。
