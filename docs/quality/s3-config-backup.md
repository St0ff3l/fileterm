# S3 配置备份

FileTerm 可以将完整连接配置手动备份到 S3 兼容对象存储。备份包包含连接密码、私钥口令和代理密码；它只在 Rust 服务中序列化，renderer 不会读取明文内容。

## Cloudflare R2

1. 在 Cloudflare Dashboard 创建一个私有 R2 Bucket。
2. 创建仅限该 Bucket 的 S3 API token，并授予 **Object Read & Write** 权限。
3. 保存生成时显示的 Access Key ID 与 Secret Access Key；Secret 之后不能再次查看。
4. 在 FileTerm 的“设置 → 配置同步 → S3 配置备份”中选择 **Cloudflare R2**，填写：
   - Endpoint：`https://<ACCOUNT_ID>.r2.cloudflarestorage.com`
   - Bucket：刚创建的 Bucket
   - Object path：默认 `fileterm/connections.json`（Bucket 根目录下的 `fileterm/` 前缀）
   - Access Key ID / Secret Access Key
5. R2 自动固定为 `region=auto` 和 path-style 地址。保存后先点击“测试连接”（会验证 Bucket 与目标对象的读取权限），再上传或下载。

## 缤纷云 Bitiful S4

1. 在缤纷云控制台创建 Bucket，并创建一个仅限该 Bucket 的子账户 Access Key，授权对象读取与写入。
2. 在 FileTerm 的“设置 → 配置同步 → S3 配置备份”中选择 **缤纷云 Bitiful S4**，填写 Bucket、对象路径和 Access Key / Secret Key。
3. 该预设按官方 S3 SDK 示例固定使用 `https://s3.bitiful.net`、`region=cn-east-1` 和 virtual-hosted 地址（不是 path-style）。
4. 保存后点击“测试连接”，确认 Bucket 和目标路径权限，再上传或下载。

## 其他 S3 兼容存储

选择“自定义 / 其他 S3 兼容存储”，即可接入 AWS S3、MinIO、阿里云 OSS、腾讯云 COS、七牛、Wasabi、Backblaze B2、DigitalOcean Spaces 等实现 SigV4 的服务。请按对应服务文档填写 HTTPS Endpoint、region、Bucket；如果服务要求 `bucket.endpoint` 形式则关闭 path-style，否则开启它。

## 冲突与安全行为

- 测试连接只验证已保存的地址与凭据，不要求勾选“启用 S3 配置备份”或“启用 WebDAV 配置同步”；上传和下载仍要求显式启用对应同步目标。
- 首次上传若远端对象已经存在，必须先下载；避免覆盖另一台设备的配置。
- 后续上传会使用对象 ETag 条件写入；远端变更后会提示先下载处理冲突。
- 下载限制为 5 MB，并校验备份包内的 profile hash 后才导入。
- S3 endpoint 必须使用 HTTPS，且不允许在 endpoint 中嵌入凭据、对象路径或查询参数。
- 建议为 FileTerm 单独创建一个私有 Bucket，并将 token 权限限制到该 Bucket 的对象读写。
