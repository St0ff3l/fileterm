# FileTerm Agent Guide

本文件是智能体进入 FileTerm 仓库时的入口地图，不是完整手册。详细事实以 `docs/` 为准；当代码、设计或计划变化时，优先更新对应文档，不要把所有知识继续堆进这里。

## 1. 项目定位

FileTerm 是面向开发者与运维场景的 Rust + Tauri 桌面远程工作台，围绕 `SSH / SFTP / FTP` 构建可日常使用的多标签桌面客户端。技术栈为纯 Rust + Tauri，仓库中不存在 Electron 代码目录，历史 Electron 实现已彻底移除，不得在任何文档、脚本或 CI 配置中重新引用。

当前阶段：**Tauri 主链路稳定与发行收口**。质量门禁覆盖共享包与 Rust/Tauri，Windows 使用签名的 Tauri 应用内更新；macOS 继续检查后跳转 GitHub Release 下载。

## 2. 先读哪里

- 架构地图：`docs/architecture.md`
- 设计规范：`docs/design.md`
- 路线图：`docs/roadmap.md`
- 进行中计划：`docs/plans/active/`
- 已完成计划：`docs/plans/completed/`
- 架构决策：`docs/decisions/`
- 质量与回归：`docs/quality/`
- 已隐藏功能：`docs/hidden-features.md`
- 功能草案：`.agents/extensions/`
- 项目技能：`.agents/skills/`

如果任务只改一个小点，先读本文件和相关源码即可；如果任务跨 `Rust commands / bridge / renderer / packages` 多层，必须先看 `docs/architecture.md` 和 `docs/plans/active/`。

## 3. 硬性边界

### 架构边界

- `packages/core` 是领域模型的 single source of truth。
- Renderer 不直接访问 SSH / SFTP / FTP protocol clients。
- 所有系统能力必须走 `Rust commands/events -> tauri-api.ts -> renderer`。
- SSH/SFTP 与 FTP 在 controller/protocol 层保持分离，不做伪统一。
- Transfer 进度统一进入 Rust transfer service，不在组件里零散维护。
- 会话事件通过 Rust workspace runtime 的统一 event/channel 边界分发，不分散监听协议 worker。
- 新状态优先进入 `packages/core` 定义类型，再下沉到服务层和 UI。
- 新窗口能力先定义 IPC 边界，再做 renderer 交互。
- 主题样式优先走 `token -> theme vars -> component skins -> terminal colors`。

### 平台兼容边界

- **CWD 目录跟随**：终端工作目录 (CWD) 变化通过底层会话流安全捕获，经 runtime 广播同步给文件管理器，严禁 UI 层轮询或直接探测平台路径。
- **POSIX CWD 注入门控**：`supportsPosixShellSetup()` 仅对 `linux` / `busybox` 返回 true。Windows / unknown 平台**严禁注入** Linux shell CWD 脚本，采用 fail-closed 双重门控（`detectPlatformAndSetupShell` + `injectShellSetup` 各一道）。
- **CRLF 归一化**：系统指标解析入口必须对远端输出做 `replace(/\r\n?/g, '\n')` 归一化，避免 `'windows\r'` 等污染导致平台误判。
- **Sudo 与 Root 状态同步**：终端执行 `sudo` 或切换用户态需被底层 runtime 解析，双向同步到文件管理器权限模型。

### UI 与公用组件边界

- **语言名称使用本地自称**：语言选择器中的语言名称必须保持该语言自己的写法（如 `简体中文`、`English`、`한국어` / `조선어`），不得根据当前界面语言翻译；新增语言时同样遵循此规则。
- **下拉框统一走 DropdownSelect**：所有表单与设置项的下拉菜单必须统一使用公用组件 `<DropdownSelect>`，严禁直写原始 HTML `<select>` 标签（确保 macOS 下包裹 `ft-select-shell` 外壳，Windows / Linux 下 100% 触发自绘 React Portal 弹出菜单）。
- **下拉箭头随控件缩放**：`DropdownSelect` 的箭头必须由组件根据当前控件实际高度自适应（覆盖 macOS 原生外壳和 Windows/Linux 自绘触发器），业务组件不得写死一套箭头尺寸或覆盖共享计算；新增紧凑/表单尺寸时必须检查箭头与文字的垂直对齐。
- **图标矢量就地化**：所有按钮与视觉图标优先使用预置的离线 SVG 图标组件 `<AppIcon />`，严禁新增 `<span className="material-symbols-outlined">` 依赖外部字体/WebFont 图标。
- **二次确认弹窗统一**：所有破坏性/危险操作（如删除、清空等）必须调用项目通用的 `<ConfirmActionDialog>` 确认弹窗组件，严禁在桌面 Webview 环境中使用原生 `window.confirm()`。
- **按钮尺寸高度规范**：同一操作组/表单行内的按钮必须具有严格统一的高度（如 32px 紧凑型 / 36px 表单型）、边框半径与内边距，禁止主次按钮尺寸参差不齐。
- **颜色语义边界**：`--focus-outline` 只用于焦点/选中/拖拽目标的描边或光环；文件相关操作使用 `--folder-accent`，实心主按钮使用 `--button-primary-*`，不要用描边色填充按钮。
- **滚动条统一走公用组件**：Renderer 中新增或改造的纵向滚动区域，默认必须复用 `features/common/VerticalScrollbar.tsx`，像终端区、文件区一样通过 `scrollRef` 绑定，并隐藏容器原生纵向滚动条；除非用户明确要求特殊行为，禁止在业务组件里单独绘制一套滚动条。横向滚动、第三方编辑器内部滚动和协议组件自带滚动可保留各自实现，但不得替代纵向公用滚动条。

### 资源与安全边界

- **离线资源就地化**：所有图标、字体与基础样式资源预置在代码库中打包输出，严禁运行时动态拉取外部 CDN 资源。
- **macOS 钥匙串规避**：禁用 safeStorage，用品牌重命名等替代机制存储凭据，避免触发 macOS 系统安全弹窗。
- **旧 Comware SSH 兼容边界**：`vendor/russh` 基于 `russh 0.63.1`，通过
  `[patch.crates-io]` 供 Tauri 使用。它只保留一个显式开启的窄范围兼容分支：远端 SSH
  identification 精确匹配 Comware、且协商到 `diffie-hellman-group-exchange-sha1` 时，请求
  `1024/1024/8192`；开启兼容选项时仍先按正常算法协商，默认安全算法不放宽。普通 Linux/Windows SSH、其他网络设备、
  `network-device` 模式、Banner 识别和服务器探测禁用由应用层逻辑负责，不由该兼容分支改变。
  升级该依赖时必须保留此边界，并重新运行 Rust 测试与 Clippy。
- 连接的 `group`（文件夹名）和 `parentId`（文件夹 ID）必须双向同步，存储层负责自愈。

### 文件规模边界

- **1000 行提醒阈值**：修改任何源文件时，若该文件在改动前或改动后超过 1000 行，必须先停下来向用户报告当前行数，并询问是否顺带拆分，得到明确答复后再继续修改。**严禁默认继续往大文件里堆代码。**
- **报告口径**：一句话说明行数、是否属于豁免类别、建议的拆分方向。不要主动展开成完整方案，等用户确认后再做。
- **豁免类别**（超过 1000 行也不必提醒拆分）：样式表 `*.css`、i18n 字典、类型与常量聚合文件（如 `packages/core/src/index.ts`）、生成代码（`src-tauri/gen/schemas/`、`vendor/`）、测试文件、文档。
- **拆分硬目标**：业务代码单文件控制在 800 行以内。数据中心型文件不受此限，但超过 3000 行时建议按域切分并保持对外 re-export 入口不变。
- **拆分顺序**：优先按职责边界切分（如命令分发表按业务分组委托、巨型函数先收敛局部变量再拆），禁止为了降行数做无意义的机械均分。

## 4. 代码位置

- Tauri Rust backend：`apps/tauri/src-tauri/src/`
- Tauri bridge：`apps/tauri/src/bridge/tauri-api.ts`
- Tauri renderer：`apps/tauri/src/renderer`
- Renderer hooks：`apps/tauri/src/renderer/hooks/`
  - `useWorkspaceTabs.ts`、`useWorkspaceModals.ts`、`useFileOperations.ts`
  - `useSshInteractions.ts`、`useFileEditor.ts`、`useWorkspaceIpcSync.ts`
  - `useWorkspaceDataOps.ts`
- Renderer 通用组件：`apps/tauri/src/renderer/features/common/`；跨功能组件的样式统一维护在 `apps/tauri/src/renderer/styles/features/common-controls.css`。
- Layout、ErrorBoundary、工作区、终端和主题组件：位于 `apps/tauri/src/renderer/`。
- 领域类型：`packages/core`
- 存储抽象：`packages/storage`
- 共享常量：`packages/shared`

## 5. 当前侧边栏布局

侧边栏导航顺序：**概览 → 连接管理器 → 命令管理器 → 设置**

已从 UI 隐藏但代码保留的功能见 `docs/hidden-features.md`，包括：

- 快速连接（Quick Connect）侧边栏入口
- Docs 侧边栏入口
- 页脚 Changelog / API Reference / Status 导航
- 页脚 System Latency 文字

## 6. 当前热点

这些文件功能集中，改动前要格外注意边界：

- `apps/tauri/src-tauri/src/`：Rust commands、services、sessions、transfers 与 storage。
- `apps/tauri/src/bridge/tauri-api.ts` 与 `apps/tauri/src/renderer/`：Tauri 专用 bridge 与 UI。

## 7. 质量门禁（已落地）

所有代码改动必须通过以下门禁，pre-push 自动阻断不通过项：

| 门禁          | 命令                                                                             | 状态                      |
| ------------- | -------------------------------------------------------------------------------- | ------------------------- |
| 类型检查      | `npm run typecheck -w @fileterm/tauri`                                           | Tauri renderer            |
| 静态检查      | `npm run lint`                                                                   | Tauri 与共享源码          |
| 格式检查      | `npx prettier --check apps/tauri packages/core packages/shared packages/storage` | Tauri 与共享源码          |
| Tauri 测试    | `npm run test:tauri`                                                             | Rust unit + contract 测试 |
| Rust 静态检查 | `cargo clippy --locked --all-targets --all-features -- -D warnings`              | Rust/Tauri crate          |

提交门禁：

- **pre-commit**（`.husky/pre-commit`）：`npx lint-staged` — 对暂存 `.ts/.tsx` 文件执行 prettier + eslint --fix
- **pre-push**（`.husky/pre-push`）：`npm run typecheck`（仅共享包与 Tauri）— 失败阻断 push

CI（`.github/workflows/ci.yml`）：push/PR 时只执行共享包与 Rust/Tauri 的 typecheck、lint、format、Rust tests、生产构建与协议夹具。

测试覆盖分布：

- `apps/tauri/src-tauri/src/`：Rust unit/contract、协议夹具与 socket lifecycle 测试。

## 8. 推荐扩展路径

1. 在 `.agents/extensions/` 或 `docs/plans/active/` 写清楚功能草案。
2. 明确影响层级：`core`、`Rust services`、`commands/events`、`bridge`、`renderer`、`styles`。
3. 补充或复用 `packages/core` 类型。
4. 新建或扩展 `apps/tauri/src-tauri/src/services/*`。
5. 经由 Rust command/event 和 `tauri-api.ts` 暴露能力。
6. 最后接到 renderer 页面、feature component 或 hook。
7. 如果涉及视觉，先收敛 token 和 theme vars，再做组件样式。

## 9. 近期优先级

### 已完成 ✅

1. 质量门禁三件套：ESLint/Prettier + Husky 提交门禁 + CI 测试集成
2. `workspace-service.ts` 按 `tabs / sessions / transfers` 拆子模块
3. `App.tsx` 拆分：7 个 hooks + ModalPortalManager + ErrorBoundary（3898 → 1698 行）
4. SSH 与 FTP controller 物理分离
5. 共享类型收敛到 `packages/core`
6. 系统信息采集多平台化：Linux / BusyBox / Windows collector + parser 归一化 + CRLF 加固
7. Windows 终端 POSIX 注入门控 + PowerShell 采集多级 fallback
8. as any 清理（ssh-session-controller 零命中）+ renderer :any 清理（零命中）
9. SSH/FTP controller 直接测试：生命周期、Windows/POSIX 注入门控、FTP 重连与操作串行化

### 当前重点 🔜

1. 继续稳定主题系统，避免颜色、阴影、圆角散落在业务组件里
2. 评估 Zustand 状态管理（App.tsx 已拆分 56%，hooks 方案已足够，非必须迁移）
3. 继续扩展 Rust SSH/FTP service 异常与协议边界测试（基础生命周期覆盖已落地）

### 可接受债务 📋

- 敏感信息明文存储 profile（safeStorage 暂缓，见硬性边界）
- 无 store（hooks 方案已满足当前需要，Zustand 按需推进）

## 10. 发版操作规范

发版任务的完整流程、Release Notes 模板、流水线监督和失败处理见：

- `.agents/skills/fileterm-release/SKILL.md`：智能体执行流程与验收清单。
- `docs/quality/git-branch-release-convention.md`：项目正式分支、tag 和 Release Notes 规范。
- `.github/workflows/release.yml`：当前实际的构建与发布行为。

必须保持的硬性约束：

- 版本号只修改根目录 `package.json` 的 `version` 字段，随后立即运行 `npm run sync:version`；禁止手动修改 workspace 版本或内部依赖版本。
- 日常改动和版本说明先通过 Pull Request 合入 `main`；`release/<version>` 只从最新 `main` 创建，作为不可变发布快照，不接收常规开发改动。
- tag 必须使用 `v<version>`，并与根版本号、workspace 同步版本和 Release Notes 文件名一致；tag 必须指向 `origin/release/*` 中的提交，否则发布工作流会拒绝构建。
- 发布说明只维护自定义正文；发布工作流必须同时保留 `--notes` 和 `--generate-notes`，禁止手写 `Contributors`、`What's Changed` 或 `Full Changelog`。

## 11. 文档维护规则

- `AGENTS.md` 只放入口地图和硬约束，保持短小。
- 稳定架构事实放 `docs/architecture.md`。
- 设计规范放 `docs/design.md`。
- 阶段目标放 `docs/roadmap.md`。
- 跨文件或跨层任务放 `docs/plans/active/`，完成后移到 `docs/plans/completed/`。
- 已确认的架构选择放 `docs/decisions/`。
- 质量、测试、发布和安全检查放 `docs/quality/`。
- 历史 UI 优化记录放 `docs/quality/`（如 `MODAL_OPTIMIZATION_SUMMARY.md`）。
- 已隐藏但代码保留的 UI 功能记录在 `docs/hidden-features.md`。
- `.agents/` 只放协作草案和扩展设计，不放生产运行代码。
- 项目内技能统一放在 `.agents/skills/`，不要再写回 `.codex/`。

一句话结论：FileTerm 已完成质量防线建设与核心解耦，当前从"骨架搭建"进入"精细化稳定"阶段——边推进功能边守住 `protocol / service / UI / type / theme` 的边界。
