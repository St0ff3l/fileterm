# 本地自动更新测试

这套流程使用 `electron-builder` 的 generic provider 和本机 HTTP 服务，不会创建 GitHub Release，也不会让其他人看到测试版本。正式打包仍然使用 `package.json` 中的 GitHub provider。

## Windows 测试

先构建 renderer 和 main：

```bash
npm run build -w @fileterm/desktop
```

在 `apps/desktop` 目录中分别构建两个版本。第一个版本安装后，再用第二个版本的文件作为本地更新源：

```bash
cd apps/desktop
npx electron-builder --config electron-builder.update-test.yml --win --x64 \
  --config.extraMetadata.version=1.0.0 \
  --config.directories.output=release/update-test-v1
npx electron-builder --config electron-builder.update-test.yml --win --x64 --config.extraMetadata.version=1.0.1
```

将 `release/update-test` 目录作为更新源启动 HTTP 服务：

```bash
python3 -m http.server 8765 --directory release/update-test
```

开发版现在会自动读取本地更新配置，直接启动即可：

```bash
npm run dev
```

开发进程会读取 `apps/desktop/dev-app-update.yml`，允许未打包的 Electron 进程访问本地 generic 更新源。若 HTTP 服务未启动，设置页会显示检查失败并保留重试按钮。

安装并启动 `1.0.0` 的 NSIS 安装包，在设置 → 应用更新中点击检查更新。预期流程是：发现 `1.0.1` → 下载 → 重启并更新。

测试完成后关闭 HTTP 服务即可；`release/update-test` 和 `release/update-test-v1` 都可以删除。

## macOS 测试

本地配置默认使用 arm64、未签名的 DMG / ZIP：

```bash
cd apps/desktop
npx electron-builder --config electron-builder.update-test.yml --mac --arm64 \
  --config.extraMetadata.version=1.0.0 \
  --config.directories.output=release/update-test-v1
npx electron-builder --config electron-builder.update-test.yml --mac --arm64 \
  --config.extraMetadata.version=1.0.1
python3 -m http.server 8765 --directory release/update-test
```

另开一个终端运行普通开发命令：

```bash
npm run dev
```

将 `1.0.0` 的 DMG 中的应用拖入测试目录后启动，在设置 → 应用更新中检查 `1.0.1`。macOS 更新下载优先使用 ZIP，不是 DMG。

未签名包可以验证更新状态、元数据读取和下载流程；如果点击“重启并更新”后被系统拒绝，这是预期的签名限制。要验证完整替换流程，需要在构建时提供 `CSC_LINK` / `CSC_KEY_PASSWORD`，并使用 Developer ID Application 证书签名。

## 注意事项

- 必须测试已安装的 NSIS 版本，不能用 `npm run dev` 或 portable 包验证覆盖安装。
- 本地配置中的 `latest.yml` 和安装包必须位于 HTTP 服务根目录。
- macOS 自动更新还需要签名；未签名本地构建可以看 UI，但不能据此判断重启替换链路成功。
