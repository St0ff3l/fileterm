# FileTerm Agent Guide

FileTerm 是 Rust + Tauri + React + TypeScript 桌面远程工作台。仅维护 `apps/tauri` 运行时；不得重新引入已移除的 Electron 目录、依赖、脚本或 CI 配置。

## 工作范围与 Git

- 默认直接在用户打开或指定的项目目录工作；仅在用户明确要求时使用 worktree。切换分支前检查并保留未提交修改，不覆盖用户工作。
- 新分支按任务选择 `feat/`、`fix/`、`chore/`、`docs/`、`release/<version>`；小写 kebab-case，不默认使用 `codex/`。
- PR 默认使用普通 Merge（Create a merge commit）；用户明确指定时才使用其他策略。
- 修复、创建 PR 或合并 PR 不表示 Issue 已解决。使用 `Refs #123` / `Related to #123`，禁止自动关闭关键词；仅在用户明确确认发布并验证通过且允许关闭后关闭 Issue。
- 在已授权范围内完成实现、验证和必要修复；只在缺少关键输入或需要超出授权范围时询问。完成报告说明改动、验证结果与未验证项。

## 按任务读取

只读取当前任务需要的资料；小修正不要求先读全部文档。

| 任务                                                      | 入口                                                                                                       |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| 跨 Rust / bridge / renderer / packages 层，或改变服务边界 | [架构](docs/architecture.md) 与相关 [进行中计划](docs/plans/active/)                                       |
| React/TSX、组件 CSS、主题、控件或独立窗口 UI              | [通用组件 skill](.agents/skills/common-components-skill/SKILL.md)；视觉设计另见 [设计规范](docs/design.md) |
| 平台差异、原生窗口/菜单、快捷键、终端手势/字体            | [跨平台 skill](.agents/skills/multiplatform-desktop-skill/SKILL.md)                                        |
| 图标/tray、SSH 平台探测、CWD、sudo/root、凭据与旧 Comware | [平台与协议约束](docs/quality/agent-platform-contracts.md)                                                 |
| 运行或截图验证桌面 UI                                     | [开发版识别与预览](docs/quality/desktop-ui-preview.md)                                                     |
| UI/UX 方案、配色/布局探索或体验审查                       | [设计检索 skill](.agents/skills/ui-ux-pro-max/SKILL.md)                                                    |
| 版本说明、tag、Release 与发布流水线                       | [发布 skill](.agents/skills/fileterm-release/SKILL.md)                                                     |
| 阶段目标 / 隐藏入口 / 架构决策                            | [路线图](docs/roadmap.md)、[隐藏功能](docs/hidden-features.md)、[决策](docs/decisions/)                    |

## 架构与数据边界

- `packages/core` 是领域类型的唯一来源；存储抽象在 `packages/storage`，共享常量在 `packages/shared`。
- 系统能力统一走 `Rust commands/events → apps/tauri/src/bridge/tauri-api.ts → renderer`；renderer 不直接访问协议 clients。新窗口先定义 IPC 边界。
- SSH/SFTP 与 FTP 在 controller/protocol 层保持分离。Transfer 进度进入 Rust transfer service；会话事件由 workspace runtime 统一分发。
- CWD 与 sudo/root 状态从底层会话流解析，经 runtime 同步文件管理器；禁止 UI 轮询或直接探测路径。POSIX 注入与 CRLF 归一化遵守平台与协议约束。
- 连接 `group` 与 `parentId` 双向同步，存储层负责自愈。凭据存储保持既定策略，不重新引入 safeStorage 或系统钥匙串弹窗。
- 图标、字体和基础样式随应用离线打包，不从 CDN 动态加载。

## 命名与文件规模

- TS/TSX 业务文件使用 kebab-case，Rust 文件/模块使用 snake_case。
- 修改的源文件在改动前或后超过 1000 行时，直接按职责拆分，业务代码单文件目标不超过 800 行；无需为拆分另行确认。
- 豁免：CSS、i18n 字典、类型/常量聚合、生成代码、vendor、测试与文档。数据中心型文件不受 800 行限制，超过 3000 行建议按域拆分并保持 re-export。
- 同模块拆分文件放在自己的目录内，Rust 用 `mod.rs` 保持 facade；不把 `ai_*.rs` / `mcp_*.rs` 等堆在父目录。临时 `include!` 也从模块目录 facade 引入。
- 拆分报告说明前后行数、豁免情况、职责模块与验证；不做机械均分。

## Renderer 约束

- 复用现有组件。新公共视觉组件放 `apps/tauri/src/renderer/components/common/<name>/`，CSS 共址；业务组件放 `features/<feature>/`。`features/common/` 保留行为工具、辅助组件与兼容导出；`styles/features/` 是旧样式迁移区。
- 主题值按 `--ref-* → semantic.css 语义变量 → 组件 CSS` 流动；布局与主题颜色分开。公共控件与验收规则以通用组件 skill 为准。
- 表单下拉使用 `DropdownSelect`，破坏性操作使用 `ConfirmActionDialog`，新增纵向滚动默认使用 `VerticalScrollbar`。
- 语言选择器名称保持本地自称（如 `简体中文`、`English`、`한국어`），不随当前界面语言翻译。

## 质量门禁

代码改动需通过以下项目门禁；纯文档/skill 指令改动检查格式、引用和 skill 结构，无需运行应用全量测试。失败后修复本次改动引入的问题，重跑受影响检查；报告已有失败或无法运行的检查。

| 门禁               | 仓库根目录命令                                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------------------------------- |
| 类型               | `npm run typecheck`（共享包与 Tauri；仅 renderer 可用 `npm run typecheck -w @fileterm/tauri` 定位）                 |
| 静态               | `npm run lint`                                                                                                      |
| 格式               | `npx prettier --check apps/tauri packages/core packages/shared packages/storage`                                    |
| Renderer 测试      | `npm run test:renderer -w @fileterm/tauri`                                                                          |
| Rust/contract 测试 | `npm run test:tauri`                                                                                                |
| Rust 静态          | `cargo clippy --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings` |

`pre-commit` 运行 lint-staged（脚本、TS、JSON、Markdown、CSS、YAML 等暂存文件）；`pre-push` 只运行 `npm run typecheck`，不代替其他门禁。CI 实际覆盖以 `.github/workflows/ci.yml` 为准。

## 文档与发布

- 架构事实放 `docs/architecture.md`，设计放 `docs/design.md`，阶段目标放 `docs/roadmap.md`，质量/回归放 `docs/quality/`，决策放 `docs/decisions/`。
- 跨文件或跨层任务的计划放 `docs/plans/active/`，完成后移到 `completed/`；已有相关计划优先更新。`.agents/` 放协作草案、扩展设计与 skills，不放生产运行代码。
- 项目 skills 统一放 `.agents/skills/`，不写回 `.codex/`。
- 版本只改根 `package.json` 后立即 `npm run sync:version`。日常改动与版本说明经 PR 合入 main；`release/<version>` 从最新 main 创建，保持不可变。
- 发布说明在 `docs/release-notes/release-notes-<version>.md`；`v<version>` tag 与同步版本一致且指向 `origin/release/*` 中的提交。GitHub 自动生成区由 workflow 的 `--notes` 与 `--generate-notes` 保留，不手写。
