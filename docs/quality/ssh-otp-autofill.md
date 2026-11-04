# SSH 动态验证码自动填写验收

在 SSH 连接的“二次验证（可选）”区域配置 OTP，可配合密码或私钥登录。FileTerm 支持保存验证器的 Base32 OTP 密钥，SSH keyboard-interactive 登录时生成并提交 TOTP：SHA-1、6 位、30 秒周期。密钥不是手机上不断变化的 6 位数字，也不是二维码链接；服务器必须已为该账号开启相应的验证器认证。

## 不用准备服务器的自动测试

在仓库根目录运行：

```bash
cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --lib otp -- --test-threads=1
node --test apps/tauri/tests/ssh-otp-settings.test.mjs
```

覆盖 RFC 6238 标准向量、周期切换、密钥格式校验、编辑时保留/替换/清除、日志脱敏及真实本地 SSH 握手。SSH 夹具验证正确密钥自动通过、错误密钥拒绝、重复提示转手动、未保存密钥转手动、短信提示转手动、自定义识别文本、多个匹配提示转手动。加密落盘与快照脱敏的回归另在 `profile_ops` 测试中覆盖，运行 `npm run test:tauri` 可一并检查。

## 在 FileTerm 界面亲手试一次

1. 在仓库根目录启动本地测试 SSH 服务，保持终端运行：

   ```bash
   cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --lib otp_manual_fixture -- --ignored --nocapture
   ```

   看到 `OTP test SSH: 127.0.0.1:xxxxx` 后，记下实际端口。服务仅监听本机，重启会更换端口与主机指纹；按 Ctrl-C 停止，不修改系统 SSH 配置。

2. 从当前项目启动或复用 `npm run dev` 的开发版，按[开发版识别与预览](desktop-ui-preview.md)核对进程。新建 SSH 连接：

   | 设置             | 值                                                       |
   | ---------------- | -------------------------------------------------------- |
   | 主机             | `127.0.0.1`                                              |
   | 端口             | 终端打印的实际端口                                       |
   | 用户名           | `alice`                                                  |
   | 认证方式         | `键盘交互（MFA）`（英文为 `Keyboard-interactive / MFA`） |
   | 密码             | 留空                                                     |
   | OTP 密钥         | `GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ`                       |
   | 交互认证识别文本 | 留空；也可填写 `Verification code`                       |

   上述密钥是 RFC 示例，**仅用于本地测试**。

3. 点击「测试连接」，确认本次本地服务的主机指纹。预期：直接提示连接成功，不弹出验证码输入框。保存并重新编辑，OTP 密钥框应为空并显示已保存状态；不重新填写直接保存，再测试仍应成功。

4. 把 OTP 密钥替换为另一条合法 Base32 密钥 `JBSWY3DPEHPK3PXP`，再测试。预期：认证失败，不会拿普通登录密码冒充验证码。

5. 清除已保存的 OTP 密钥，保存并再次测试。预期：出现手动验证码弹窗。可在手机验证器中以「基于时间」添加步骤 2 的示例密钥，填入当前 6 位数字，认证应成功。也可以把识别文本改成不匹配的文本，验证同样转回手动输入。

6. 如需打开终端观察，关闭该测试连接的 SFTP、exec、资源监控和 Shell 集成，再连接。终端应显示 `FileTerm OTP authentication passed.`。该夹具只回显输入，不执行真实命令，不提供文件管理和系统监控。验收后删除测试连接并停止夹具。

## 真实服务器验收

使用已开启 TOTP 的测试账号，从账号绑定验证器时取得同一条 Base32 密钥。密码认证、私钥认证和跳板连接仍按原有方式配置，OTP 密钥属于各自需要 MFA 的 SSH 连接；客户端与服务器系统时间应同步。

默认只识别明确的验证码提示。特殊服务器可填写「交互认证识别文本」：按不区分大小写的普通子串匹配，不支持正则。识别文本不应填写密码、OTP 密钥或当前验证码。短信、邮件、恢复码、推送等提示仍手动处理；一轮中多个提示匹配时也不自动填。每次 SSH 认证只自动提交一次，后续提示转手动，服务器直接拒绝时正常报认证失败。

密钥与其他连接凭据一样进入本地加密凭据文件；公开配置/快照只显示存在标记，不返回密钥。FileTerm 原生导出与加密备份包含该凭据，导出文件按敏感资料保管。当前不支持 HOTP、二维码链接、8 位码、SHA-256/SHA-512 或自定义周期。

## 同类实现参考

检查了 [guopeizhao 的公开 Star 列表](https://github.com/guopeizhao?tab=stars)，未确认 Issue 截图的产品出处。[Issue #375](https://github.com/St0ff3l/fileterm/issues/375) 也未注明来源。

另参考 [OxideTerm 的 SSH TOTP 应答策略](https://github.com/AnalyseDeCircuit/oxideterm/blob/d9ce2404c9cf801bdd394ffd0977024833cb0056/crates/oxideterm-ssh/src/transport/totp.rs)：唯一匹配、每次认证一次自动尝试、手动回答完成后再生成验证码。FileTerm 复用自己的凭据存储与认证实现。
