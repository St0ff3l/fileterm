# macOS 签名与公证配置（Developer ID + notarize）

当前发布继续使用 **ad-hoc 签名**（`signingIdentity: "-"`）。目前没有加入 Apple Developer Program、启用 Developer ID 签名或公证的计划。第 2 至 6 节仅供将来改变发布策略时参考。当前用户指引见第 7 节。

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

`APPLE_SIGNING_IDENTITY` 会覆盖 `bundle > macOS > signingIdentity`。保留 `tauri.release.macos.conf.json` 中的 `"-"`，以便未配置证书的环境继续使用 ad-hoc 签名。使用 Apple ID 公证时，用 `APPLE_TEAM_ID` 选择 Developer ID 证书所属的团队。Tauri CLI 2.11.4 的公证逻辑读取该 Team ID。

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

Developer ID 签名使用安全时间戳。插件在 `beforeBundleCommand` 阶段签名，因此该阶段开始前，`codesign` 就需要能访问证书和私钥。之后 Tauri 再签名主应用及其 frameworks。

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

如果将来启用 Developer ID，两个 macOS job 都要在运行打包命令前创建并解锁临时 keychain，把 `.p12` 导入其中，并授权 CI 中的 `codesign` 非交互使用私钥。将临时 keychain 加入用户的 keychain 搜索列表，并先用 `security find-identity -v -p codesigning` 确认证书可见。Tauri 自己的证书导入发生在 `beforeBundleCommand` 之后，只供 bundler 签名阶段使用。

### 4.3 公证触发方式

`tauri-bundler` 在 App 打包完成、签名成功后自动执行公证：

- bundler 在插件构建 hook 之后导入 `APPLE_CERTIFICATE`，再使用 Developer ID 对主应用签名。
- 签名身份不是 `-` 且提供了 `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`（或 API Key）时，调用 `xcrun notarytool submit --wait` 并在成功后 `xcrun stapler staple` 到 `.app`。
- 当前 ad-hoc 发布未提供公证凭据，bundler 跳过公证。启用 Apple ID 公证时，Apple ID、App 专用密码和 Team ID 三项必须配齐。Apple ID 和密码已设置、Team ID 缺失时，bundler 会终止构建。
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
- 已发起的公证提交失败时，发布 job 会失败。启用时应在打包前检查公证凭据，并在上传前验证票据。
- Windows/Linux 的签名与更新资产不受影响；macOS 的 `.app.tar.gz` 更新包由已公证并 staple 的 App 打包，因此更新安装同样带公证票据。

## 7. 未启用时的用户指引

从 FileTerm 官方 Release 下载并安装应用后，如果首次打开时被 macOS 拦截：

1. 打开「系统设置 → 隐私与安全性」，向下滚动到「安全性」。
2. 找到 FileTerm 的提示，点按「仍要打开」。
3. 在确认弹窗中点按「打开」。

也可在终端执行 `xattr -dr com.apple.quarantine /Applications/FileTerm.app`。不要建议用户关闭 Gatekeeper 或 SIP。

## 8. 参考

- [Tauri v2 macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri CLI 2.11.4 打包 hook 顺序](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/bundle.rs#L178-L207)
- [Tauri CLI 2.11.4 公证凭据](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle/macos/sign.rs#L89-L116)
- [Apple：创建证书签名请求](https://developer.apple.com/help/account/create-certificates/create-a-certificate-signing-request)
- [Apple：notarytool 日志](https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution)
- [Apple：在 Mac 上安全地打开 App](https://support.apple.com/zh-cn/102445)
