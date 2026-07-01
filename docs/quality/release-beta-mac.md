# FileTerm Beta Release Checklist

本文记录 `v0.1.0-beta.1` 的 mac-only、unsigned release 约定。这个版本的目标不是正式商用发布，而是先把可分发链路跑通，方便早期用户和内部测试。

## 1. 发布范围

- 仅发布 macOS arm64。
- 不做代码签名。
- 不做 notarization。
- 将 GitHub Release 标记为 prerelease。

## 2. 产物

预期产物由 `electron-builder` 生成：

- `FileTerm-v0.1.0-beta.1-arm64.dmg`
- `FileTerm-v0.1.0-beta.1-arm64.zip`

## 3. 发布前检查

1. 运行 TypeScript 检查。
2. 运行桌面应用构建。
3. 运行 mac 打包命令。
4. 确认 `apps/desktop/release/` 里出现 `dmg` 和 `zip`。
5. 确认版本号为 `0.1.0-beta.1`。

如需清理本地产物后再重打，可执行：

```bash
npm run clean:release -w @fileterm/desktop
```

## 4. 打 tag

```bash
git tag -a v0.1.0-beta.1 -m "FileTerm v0.1.0-beta.1"
git push origin v0.1.0-beta.1
```

## 5. GitHub Release 行为

Release workflow 只在 tag push 时触发，并且：

- 只在 macOS runner 上打包。
- 产物自动附加到 Release。
- Release 使用 prerelease 标记。

## 6. 用户提示

在 release notes 里明确说明：

- 这是未签名 beta。
- macOS 可能会触发系统安全提示。
- 首次打开可能需要在系统设置里手动允许。
