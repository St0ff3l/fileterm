# 源码规模治理清单（完成状态）

> 依据 `AGENTS.md` §3「文件规模边界」：
>
> - 业务代码硬目标 **≤ 800 行**；
> - 改动中发现文件 **≥ 1000 行**，须先停下提醒用户是否拆分；
> - **豁免类**（不计入硬目标）：CSS、i18n 字典、类型/常量聚合（`packages/core/src/index.ts`）、生成代码、测试、文档。
>
> 本清单为单一总表，不再区分历史「非 SSH / SSH」两个 plan。所有数字来自本次重新扫描（已排除 `target/`、`vendor/`、`node_modules/`、生成物）。
> 扫描时间：2026-09-01。

## 一、当前所有 > 800 行的文件（一张总表，按行数降序）

| 行数 | 文件                                            | 层级     | 豁免         | 备注 / 建议                                                      |
| ---: | ----------------------------------------------- | -------- | ------------ | ---------------------------------------------------------------- |
| 4051 | `renderer/styles/features/session.css`          | CSS      | 是           | 数据中心型，不拆                                                 |
| 4051 | `renderer/styles/features/component-skins.css`  | CSS      | 是           | 数据中心型，不拆                                                 |
| 3940 | `renderer/i18n.ts`                              | Renderer | 是(i18n)     | 字典型，不拆                                                     |
| 3744 | `renderer/styles/features/ai-copilot.css`       | CSS      | 是           | 数据中心型，不拆                                                 |
| 3119 | `renderer/styles/features/modal-components.css` | CSS      | 是           | 数据中心型，不拆                                                 |
| 2643 | `packages/core/src/index.ts`                    | core     | 是(类型聚合) | 领域 SSoT，保持入口不变                                          |
| 2454 | `renderer/styles/features/commands.css`         | CSS      | 是           | 数据中心型，不拆                                                 |
| 1715 | `renderer/styles/features/modals.css`           | CSS      | 是           | 数据中心型，不拆                                                 |
| 1674 | `renderer/styles/features/workstation-skin.css` | CSS      | 是           | 数据中心型，不拆                                                 |
| 1355 | `renderer/styles/features/shell.css`            | CSS      | 是           | 数据中心型，不拆                                                 |
| 1175 | `renderer/styles/features/home.css`             | CSS      | 是           | 数据中心型，不拆                                                 |
|  902 | `src-tauri/src/services/connections.rs`         | Rust     | 部分豁免     | `#[cfg(test)]` 从约 687 行开始，生产代码未超 800；按原备注暂保留 |

> 说明：`tests.rs` 类文件（如 `sessions/ssh/tests.rs`、各 `*tests.rs`）行数远超 800，但属测试豁免类，未计入上表。除 `services/connections.rs` 的测试密集例外外，当前业务源文件最大为 794 行。

## 二、本轮已完成的拆分

- **Renderer 入口与页面**：`App.tsx` → `app.tsx`（178 行）及 `app-*` hooks/views；`ConnectionModal.tsx` → `connection-modal.tsx`（401 行）及表单区块；`FileManager.tsx` → `file-manager.tsx`（787 行）及 pane、toolbar、同步 Hook；`AiCopilotPanel.tsx` → `ai-copilot-panel.tsx`（566 行）及消息/输入区块；`SystemSidebar.tsx`、`SshKeyManagerPage.tsx`、`CommandCenter.tsx`、`SecuritySettingsPanel.tsx` 均已按职责拆分。
- **Renderer hooks**：`useTerminalLifecycle.ts`、`useWorkspaceTabs.ts`、`useFileOperations.ts`、`useTerminalView.ts`、`useAiCopilot.ts`、`useWorkspaceIpcSync.ts` 均已拆为职责模块，facade 保留稳定 Hook 导出。
- **Rust SSH worker**：`loop.rs` 改为 `event_loop.rs` 并配合 `SshSessionContext`、startup/dispatch 子模块；`dispatch.rs` 按 terminal、transfer、files 分组委托。
- **Rust storage**：`storage/migration.rs` 已整理为 `storage/migration/mod.rs`、`portable.rs`、`legacy.rs`、`staging.rs`。
- **命名统一**：Renderer 业务 `.ts/.tsx` 文件名全部统一为小写 `kebab-case`；Rust 文件名和模块名统一为 `snake_case`。`vite.config.ts` 等工具链约定文件保留标准命名，声明文件仍使用 `.d.ts` 约定。导出的 React 类型/组件仍使用 PascalCase，Hook/函数仍使用 camelCase。

## 三、当前结论

- 原清单中的 Renderer 与 SSH worker 业务大文件已完成治理，当前没有需要继续拆分的非豁免业务源文件。
- `services/connections.rs` 保留为测试密集例外；如后续继续治理，应只抽离其 inline tests，不改变连接导入/导出 facade。
- i18n、CSS、`packages/core/src/index.ts` 和测试文件继续按豁免规则维护。

## 四、执行纪律

每改一个文件前，先比对上表行数：若仍 ≥ 1000 行，停止并询问是否顺带拆分；禁止默认往大文件继续堆代码。拆分后目标：业务代码 ≤ 800 行。

### 4.1 首选拆分机制：同名目录 + 薄 re-export facade

超标文件默认拆成「**同名目录 + 薄 re-export facade**」，保证对外 import 表面不变、零调用方改动：

- **Rust**：`foo.rs` → `foo/mod.rs` + 若干子模块（文件夹名 == 原模块名）。已落地的 `sessions/ssh`、`commands`、`services/ai` 等均是此模式。
- **TypeScript / React**：按职责拆为兄弟模块或同名目录，facade 保持稳定导出；文件名统一使用小写 `kebab-case`，例如 `app.tsx`、`use-workspace-tabs.ts`、`connection-modal.tsx`。

facade 只保留 `pub use` / `export * from` 再导出与类型 re-export，不承载业务逻辑；逻辑下沉到子模块。

### 4.2 例外与变通

- **Rust 关键字文件名不能转同名目录**：`sessions/ssh/worker/loop.rs` 的 `loop` 是保留字，不能建 `worker/loop/` 目录并以 `worker::loop` 引用。该文件须先改名（如 `event_loop.rs`）再拆，或直接拆为 `worker/` 下的兄弟子模块，不追求同名。
- **允许按区块抽兄弟文件**：Renderer 那批表单/组件类（`App.tsx`、`ConnectionModal.tsx`、`useWorkspaceTabs.ts` 等）按区块或 tab 抽成兄弟子组件/子 hook 往往比套同名目录更自然，不强制同名目录。
- **类型聚合文件**（`packages/core/src/index.ts`）：若未来切分，同样用 `index.ts` 仅做 `export * from './xxx'` 的 facade，对外 `@fileterm/core` 入口不变。
- **Rust 命名**：文件名/模块名使用 `snake_case`（如 `event_loop.rs`、`sftp_startup.rs`）；Rust 类型名和公开组件名不因此改变。

### 4.3 验收

拆完后：① 文件 ≤ 800 行；② 原调用方 import 路径不变（或仅改 facade 内部再导出）；③ 通过 `npm run typecheck -w @fileterm/tauri`、`npm run lint`、Prettier、`npm run test:tauri`、Rustfmt 和 Clippy。
