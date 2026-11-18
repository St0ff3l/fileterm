# macOS 签名与公证配置（Developer ID + notarize）

状态：**未启用**。当前发布流水线保留 ad-hoc 应用签名（`signingIdentity: "-"`），不接入 Apple Developer ID，也不执行公证。本文记录购买 Apple Developer Program 后的启用步骤、必须同步修改的构建脚本与工作流，以及未启用时对用户的影响。

## 1. 现状

- `.github/workflows/release.yml` 的两个 macOS job 没有配置任何 `APPLE_*` 环境变量，`tauri-bundler` 因此打印 `Warn skipping app notarization`，并以 `Signing with identity "-"` 完成 ad-hoc 签名。
- 产物 `FileTerm.app` 与 `.dmg` 没有 Apple 签名链，`spctl` 校验为 rejected；用户首次打开会收到 Gatekeeper 拦截提示。
- 应用内更新的 Ed25519 签名（`TAURI_SIGNING_PRIVATE_KEY`）与 Apple 签名、公证完全独立，公证不影响更新验签。

## 2. 启用条件

| 项目                          | 说明                                                                                          |
| ----------------------------- | --------------------------------------------------------------------------------------------- |
| Apple Developer Program       | 个人或组织账号，99 USD/年。免费账号不能公证，只能签名；官方文档已注明这一限制。               |
| Developer ID Application 证书 | 仅账号持有人（Account Holder）可创建，用于 App Store 之外分发；按 CSR 流程签发后导出 `.p12`。 |
| Team ID                       | Apple Developer 账号 Membership details 中的 10 位标识。                                      |
| 公证凭据                      | Apple ID + App 专用密码，或 App Store Connect API Key（`.p8` + Key ID + Issuer ID）。         |
| macOS 设备                    | 生成 CSR、导出证书需要一台 Mac。                                                              |

Developer ID 证书自创建起 5 年有效；证书过期后仍可运行当年签名的旧包，但新版本必须用新证书重新签名。证书不能续期，到期需创建新证书并更新 secret。

## 3. GitHub Secrets

在仓库 Settings → Secrets and variables → Actions 中添加：

| Secret                       | 内容                                                                                                 |
| ---------------------------- | ---------------------------------------------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | `.p12` 的 base64 文本，可用 `openssl base64 -A -in certificate.p12 -out certificate-base64.txt` 生成 |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设置的密码                                                                             |
| `APPLE_SIGNING_IDENTITY`     | 证书的完整名称，例如 `Developer ID Application: Example Ltd (ABCDE12345)`                            |
| `APPLE_ID`                   | 用于公证的 Apple ID                                                                                  |
| `APPLE_PASSWORD`             | App 专用密码（不是 Apple ID 登录密码）                                                               |
| `APPLE_TEAM_ID`              | Team ID                                                                                              |

使用 API Key 时可改用 `APPLE_API_KEY`、`APPLE_API_ISSUER` 与 `APPLE_API_KEY_PATH`，此时不需要 `APPLE_ID` 和 `APPLE_PASSWORD`。

`APPLE_SIGNING_IDENTITY` 会覆盖 `tauri.conf.json` 的 `bundle > macOS > signingIdentity`，因此 `tauri.release.macos.conf.json` 里现有的 `"-"` 不需要删除，也不能只删它而不配 secret——本地未配置 secret 时仍需 `"-"` 提供 ad-hoc 签名。若 Apple ID 关联多个团队，还需 `APPLE_PROVIDER_SHORT_NAME` 指定公证团队。

`APPLE_SIGNING_IDENTITY` 必须与 `APPLE_CERTIFICATE` 中的证书一致，否则 bundler 会直接报 `certificate from APPLE_CERTIFICATE ... does not match provided identity`。

## 4. 启用前必须修改的代码

### 4.1 Dock tile plug-in 改用 Developer ID 签名

`apps/tauri/src-tauri/scripts/build-dock-tile-plugin.mjs` 目前固定以 ad-hoc 身份签名：

```js
run('codesign', ['--force', '--sign', '-', '--options', 'runtime', '--timestamp=none', pluginDirectory])
```

公证要求 bundle 内所有嵌套代码（包括 `Contents/PlugIns/FileTermDockTile.docktileplugin`）都带有有效的 Developer ID 签名和安全时间戳；嵌套的 ad-hoc 代码会让 notarytool 以 `code object is not signed with a valid Developer ID certificate` 之类的错误拒绝整个 App。

需改为读取环境变量，未配置时回退 ad-hoc，保持本地开发行为不变：

```js
const signingIdentity = process.env.APPLE_SIGNING_IDENTITY?.trim() || '-'
const codesignArgs = ['--force', '--sign', signingIdentity, '--options', 'runtime']
if (signingIdentity !== '-') {
  codesignArgs.push('--timestamp')
}
run('codesign', [...codesignArgs, pluginDirectory])
```

注意两点：`--timestamp=none` 只能在 ad-hoc 场景使用，真实身份下必须改为安全时间戳；签名要在 `beforeBundleCommand` 阶段完成，之后 Tauri 按 inside-out 顺序签名主程序与 frameworks。

### 4.2 工作流注入凭据

在 `release.yml` 的 `build-macos-arm64` 与 `build-macos-x64` 两个 job 的 `env` 中追加：

```yaml
APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
APPLE_ID: ${{ secrets.APPLE_ID }}
APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
```

不需要手工 `security import`：bundler 检测到 `APPLE_CERTIFICATE` 后会把证书导入临时 keychain 并用于签名。

### 4.3 公证触发方式

`tauri-bundler` 在 App 打包完成、签名成功后自动执行公证：

- 存在 `APPLE_CERTIFICATE` 时会先导入证书；身份非 `-` 时才真正签名。
- 签名身份不是 `-` 且提供了 `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`（或 API Key）时，调用 `xcrun notarytool submit --wait` 并在成功后 `xcrun stapler staple` 到 `.app`。
- 缺少凭据时只打印 `skipping app notarization` 警告，构建继续——即当前状态。
- `.dmg` 在 App 公证、staple 之后生成，并以同一身份签名（identity 为 `"-"` 时跳过，见 tauri-apps/tauri#12288）；DMG 本身不单独公证，票据随 App 一起传递，Gatekeeper 校验能通过。

## 5. 产物验证

打包后在本机执行：

```bash
# 签名链与 Team
codesign -dv --verbose=4 FileTerm.app

# 签名完整性（含嵌套代码）
codesign --verify --deep --strict --verbose=2 FileTerm.app

# 公证结果（应显示 accepted / Notarized Developer ID）
spctl --assess --type execute -vv FileTerm.app
xcrun stapler validate FileTerm.app

# DMG 外层签名
spctl --assess --type open --context context:primary-signature -v FileTerm.dmg
```

公证失败时，Tauri 会在日志中输出 submission id，可用 `xcrun notarytool log <submission-id> --apple-id ... --team-id ...` 拉取明细；常见原因是嵌套代码未签名、缺少 hardened runtime 或使用了 `--timestamp=none`。`bundle > macOS > hardenedRuntime` 默认即为 `true`，无需额外配置。

## 6. 成本与流水线影响

- 每个架构的公证提交通常需要 2–5 分钟，release 总时长增加约 5–10 分钟。
- 公证失败会让发布 job 直接失败，不再静默产出未公证包。
- Windows/Linux 的签名与更新资产不受影响；macOS 的 `.app.tar.gz` 更新包由已公证并 staple 的 App 打包，因此更新安装同样带公证票据。

## 7. 未启用时的用户指引

维持 ad-hoc 签名期间，在 Release 说明或下载页给出口径一致的放行步骤：

1. 右键 `FileTerm.app` → 打开 → 在弹窗中再次确认「打开」；或
2. 系统设置 → 隐私与安全性 → 仍要打开；或
3. 终端执行 `xattr -dr com.apple.quarantine /Applications/FileTerm.app`。

不要建议用户关闭 Gatekeeper 或 SIP。

## 8. 参考

- [Tauri v2 macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri CLI 环境变量说明](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/ENVIRONMENT_VARIABLES.md)
- [Apple：创建证书签名请求](https://developer.apple.com/help/account/create-certificates/create-a-certificate-signing-request)
- [Apple：notarytool 日志](https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution)
