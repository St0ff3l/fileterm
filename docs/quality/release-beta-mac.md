# FileTerm macOS Release Checklist

本文记录 macOS 桌面包的发布约定。运行时为 Rust + Tauri；历史桌面实现已彻底移除，所有命令改用 `apps/tauri` 的 Tauri 构建脚本。

## 1. 发布范围

- 发布 macOS Apple Silicon（arm64）与 Intel（x64）两种架构的 `.dmg`。
- 当前发布配置（`tauri.release.macos.conf.json`）`signingIdentity` 为 `-`，即未签名构建；不做 notarization。
- macOS 更新入口当前回退为打开 GitHub Release 下载页，不做应用内自动更新。
- 是否标记为 prerelease 以 `release.yml` 实际行为为准。

## 2. 产物

由 `npm run release:mac` / `npm run release:mac:x64`（基于 `tauri.release.macos.conf.json`）生成：

- `FileTerm-<version>-macos-arm64.dmg`
- `FileTerm-<version>-macos-x64.dmg`

产物位于 `apps/tauri/src-tauri/target/<target>/release/bundle/dmg/`。

## 3. 发布前检查

1. 运行共享包构建与 Tauri 类型检查：`npm run build:packages && npm run typecheck -w @fileterm/tauri`。
2. 运行 macOS 打包命令：`npm run release:mac -w @fileterm/tauri` 与 `npm run release:mac:x64 -w @fileterm/tauri`。
3. 确认 `apps/tauri/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/` 与 `x86_64-apple-darwin/.../bundle/dmg/` 下分别出现对应 `.dmg`。
4. 确认根目录 `package.json` 版本号（与 `npm run sync:version` 同步后的 workspace 版本）正确。
5. 如需清理本地产物后再重打，可执行 `npm run clean:release -w @fileterm/tauri`。

## 4. 打 tag

```bash
git tag -a v<version> -m "FileTerm v<version>"
git push origin v<version>
```

## 5. GitHub Release 行为

Release workflow（`.github/workflows/release.yml`）在 tag push 时触发，并且：

- macOS arm64 / x64 分别在 `macos-14` runner 上打包。
- 产物自动附加到 Release。
- Windows（NSIS + 更新器签名）与 Linux（deb / AppImage）在同一流水线中一并产出。

## 6. 用户提示

在 release notes 里明确说明：

- 这是未签名构建。
- macOS 可能会触发系统安全提示。
- 首次打开可能需要在“系统设置 → 隐私与安全性”中手动允许。
- 更新方式：前往 GitHub Release 下载新版 DMG 覆盖安装。
