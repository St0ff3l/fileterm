# FileTerm 全仓品牌重命名

## 范围

- 产品展示名、窗口标题、托盘提示、安装包名与日志前缀统一为 `FileTerm`。
- npm 根包与内部 workspace 统一为 `fileterm` 和 `@fileterm/*`。
- Electron `appId`、preload 全局 API、内部事件、环境变量、临时文件与主题 ID 使用新标识。
- README、架构、质量文档、项目技能和 CI/release workflow 同步更新。

## 兼容处理

- 首次启动时，从旧用户目录迁移应用自有 JSON 数据。
- 不迁移 Chromium session、缓存和日志，避免旧运行态污染新应用目录。
- Electron main 构建前清空 `dist-electron`，防止旧目录结构的产物被安装包继续收录。

## 验证

- 跟踪文件与构建产物的旧标识扫描为零。
- `npm install --ignore-scripts` 通过。
- `npm run typecheck` 通过。
- `npm run build` 通过。

## 仓库外操作

- GitHub 仓库完成重命名后，将本地 `origin` 更新为新地址。
- 将本地工作目录改名为 `fileterm`，并按需刷新 IDE 项目元数据。
