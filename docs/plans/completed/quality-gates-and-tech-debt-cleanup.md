# 质量门禁与技术债清理计划 (Completed)

## 背景

本计划源于 2026-07-09 的项目质量评审（架构师只读评审 + 主理人交叉验证，仓库零变更）。评审结论：FileTerm 总体 7/10，定位"架构扎实、文档优秀的准生产级 MVP"；架构质量（8/10）与文档质量（8.5/10）远超工程执行（5/10）。

核心矛盾：**架构边界和文档纪律已达到准生产级，但质量门禁自动化（lint/test/CI/hooks）和部分代码质量专项问题仍是推向正式发布前的两道必跨门槛。** 所有债务方向正确、只欠执行。

本计划只收录评审发现的、**未被现有 active plan 覆盖**的工作项。`App.tsx` 拆分、`workspace-service` 解耦、TransferService 等已有专项计划，见末尾「与现有 plan 的关系」。

## 目标

1. 建立自动化质量防线，把"人肉把关"升级为"CI + 提交门禁"把关。
2. 清理评审发现的代码质量反模式（as any 私有字段、渲染端 prop 懒类型）。
3. 补齐最脆弱模块的单测覆盖。
4. 收敛根目录散落文档。

## 验收标准（整体）

- CI 在每次 PR 上自动执行 typecheck + lint + 单元测试，任一失败即阻断合并。
- 提交代码时 pre-commit 自动 prettier + eslint --fix，pre-push 自动 typecheck。
- `ssh-session-controller.ts` 中 7 处 `(this as any)` 全部消除，对应字段声明为 `private` 真实类型。
- 渲染端 6 处 `any` prop 替换为具体类型。
- `file-profile-repository` 的 group/parentId 自愈逻辑、`workspace-session-runtime` 的终端合并/事件转发有单测覆盖。

---

## P0：质量门禁三件套（发布前必须完成）

### P0-1. 引入 ESLint + Prettier

**问题**：全仓库无 ESLint/Prettier 配置，代码风格无强制约束，潜在 bug（未用变量、隐式 any、危险相等、可疑条件）无静态兜底。贡献者增多后风格漂移和低级错误会失控。

**证据**：

- 根目录与所有 workspace 均无 `.eslintrc*` / `eslint.config.*` / `.prettierrc*`。
- `apps/desktop/package.json` devDependencies 无 eslint / prettier（仅 typescript/vite/concurrently 等）。

**影响**：质量全靠人肉把关，CI 的 typecheck 只能抓类型错误，抓不到代码风格和常见坏味道。

**具体改法**：

1. 在根 `package.json` devDependencies 引入：
   - `eslint`、`@typescript-eslint/parser`、`@typescript-eslint/eslint-plugin`
   - `prettier`、`eslint-config-prettier`、`eslint-plugin-prettier`
   - `eslint-plugin-react`、`eslint-plugin-react-hooks`（renderer 侧 React 规则）
2. 在仓库根新建 `eslint.config.js`（flat config，匹配当前 ESLint 9+ 生态）：
   - 基础规则：`@typescript-eslint` recommended，关闭与现有代码冲突的激进规则（如 `no-non-null-assertion`），先求"有兜底"再求"严格"。
   - renderer 文件（`apps/desktop/src/renderer/**/*.{ts,tsx}`）启用 `eslint-plugin-react` + `eslint-plugin-react-hooks`（`rules-of-hooks` 必须 error）。
   - main/preload 文件禁用 `no-console`（main 有合法日志）。
   - 配合 `eslint-config-prettier` 关闭与 prettier 冲突的格式规则。
3. 在仓库根新建 `.prettierrc.json`，与现有代码风格对齐（先用 prettier 跑一遍 `--check` 看差异，再定配置，避免大规模格式改动）：
   - 建议起始：`{ "semi": false, "singleQuote": true, "trailingComma": "none", "printWidth": 120 }`（先用 `--check` 确认与现有代码差异最小，再定稿）。
4. 在根 `package.json` scripts 增加：
   - `"lint": "eslint ."`
   - `"lint:fix": "eslint . --fix"`
   - `"format": "prettier --write ."`
   - `"format:check": "prettier --check ."`
5. 首次落地时跑一次 `npm run format` 全量格式化，单独提交一个 `style: apply prettier formatting` commit，**不与逻辑改动混在同一 commit**。

**验收标准**：

- [x] `npm run lint` 在 main 上零 error（warning 可暂留，记录到本 plan 进度）。
- [x] `npm run format:check` 通过。
- [x] CI 中加入 lint step（见 P0-3）。

**注意事项**：首次启用 eslint 可能暴露大量 warning，建议先以 `error` 只保留"会引发 bug"的规则（如 `no-unused-vars`、`@typescript-eslint/no-explicit-any`），风格类规则降为 `warn`，分批收敛。不要一上来全 error 导致无法提交。

---

### P0-2. CI 执行单元测试

**问题**：已写好的 9 个测试文件（约 1240 行）形同摆设——CI 从未调用测试脚本。破坏可恢复传输逻辑、profile 自愈逻辑的改动可静默合入 main。

**证据**：

- `.github/workflows/ci.yml` steps 仅有：Checkout → Setup Node → Install dependencies → Build packages → Typecheck → Build，**无任何 test step**。
- `apps/desktop/package.json` 存在可用脚本：
  - `"test:transfers": "node --test --experimental-strip-types test/transfers/*.test.ts"`
  - `"test:transfers:protocol": "npm --prefix ../.. run build -w @fileterm/core && npm run build:main && node --test test/protocol/*.test.mjs"`

**影响**：测试写了不跑等于没写。回归完全靠人肉，且最复杂的传输/协议逻辑恰恰是回归风险最高的部分。

**具体改法**：

1. 在 `.github/workflows/ci.yml` 的 `validate` job 中，于 "Typecheck" step 之后、"Build" step 之前，新增测试 step：
   ```yaml
   - name: Run unit tests
     run: |
       npm run test:transfers -w @fileterm/desktop
   ```
2. `test:transfers:protocol` 需要先 build core + main，耗时较长且依赖编译产物。建议：
   - 单独作为一个 step 放在 "Build" 之后（因为 `test:transfers:protocol` 内部已 `npm run build:main`，但要确保 `@fileterm/core` 已 build）。
   - 或拆成独立 job `test-protocol`，与 `validate` 并行，避免拖慢主流程。
   - 推荐先只接 `test:transfers`（纯逻辑、快），`test:transfers:protocol` 标注为后续接入。
3. CI 失败时上传测试日志 artifact，方便排查：
   ```yaml
   - name: Upload test logs on failure
     if: failure()
     uses: actions/upload-artifact@v4
     with:
       name: test-logs
       path: apps/desktop/test/**/*.log
   ```

**验收标准**：

- [x] CI 在每次 push/PR 上执行 `test:transfers`。
- [x] 测试失败时 CI 标红阻断。
- [x] `test:transfers:protocol` 至少在 CI 中有占位 step（即使暂标注 `continue-on-error` 或单独 job）。

---

### P0-3. 建立 pre-commit / pre-push 提交门禁

**问题**：`.githooks/` 仅有 `prepare-commit-msg`（Codex co-author 注入），无 pre-commit / pre-push。类型/lint 错误、未跑测试的代码可直接 commit/push，CI 是唯一防线但 CI 不跑测试（P0-2）且无 lint（P0-1）。

**证据**：

- `.githooks/` 目录仅 `prepare-commit-msg` 一个文件。
- 根 `package.json` 无 `husky` / `lint-staged` 依赖。

**影响**：三重防线全缺（无 lint、CI 不跑测试、无提交门禁），低级错误可在本地无阻拦地进入远端。

**具体改法**：

1. 在根 `package.json` devDependencies 引入 `husky` + `lint-staged`。
2. 初始化 husky：`npx husky init`（生成 `.husky/` 目录）。注意与现有 `.githooks/` 的关系——现有 `prepare-commit-msg` 在 `.githooks/`，需迁移到 `.husky/prepare-commit-msg` 并保留其 co-author 注入逻辑，或配置 `core.hooksPath`。**建议统一到 `.husky/`**，删除 `.githooks/`，避免两套 hook 体系并存。
3. 新建 `.husky/pre-commit`：
   ```sh
   npx lint-staged
   ```
4. 新建 `.husky/pre-push`：
   ```sh
   npm run typecheck
   ```
5. 在根 `package.json` 增加 `lint-staged` 配置：
   ```json
   "lint-staged": {
     "*.{ts,tsx,cts}": ["prettier --write", "eslint --fix"],
     "*.{json,md,css}": ["prettier --write"]
   }
   ```
6. 在根 `package.json` scripts 增加 `"prepare": "husky"`（确保 clone 后自动安装 hooks）。

**验收标准**：

- [x] `git commit` 时自动对暂存文件执行 prettier + eslint --fix。
- [x] `git push` 时自动执行 typecheck，失败则阻断。
- [x] 现有 `prepare-commit-msg` 的 co-author 注入逻辑迁移到 `.husky/` 后仍正常工作。
- [x] `.githooks/` 已清理，不再有两套 hook 体系。

**注意事项**：pre-push 跑全量 typecheck 可能耗时，若太慢可降级为只 typecheck 受影响 workspace。但首版建议先跑全量，确认基线。

---

## P1：代码质量专项（中期必须完成）

### P1-1. 清理 ssh-session-controller 的 7 处 `(this as any)` 私有字段

**问题**：`LiveSshSessionController` 用 `(this as any)._xxx` 访问未在类中声明的动态实例属性，绕过 TypeScript 类型系统。这些字段不在类型视野内，重构时极易遗漏，也无法被 IDE 重命名/跳转追踪。

**证据**（已 grep 核实，行号准确）：

- `apps/desktop/src/main/services/sessions/ssh-session-controller.ts:235` `(this as any)._lastInjectTime || 0`
- `:240` `(this as any)._lastInjectTime = now`
- `:2325` `(this as any)._sudoWindow || ''`
- `:2327` `(this as any)._sudoWindow = newWindow`
- `:2338` `(this as any)._recentKeystrokes || ''`
- `:2373` `(this as any)._recentKeystrokes || ''`
- `:2378` `(this as any)._recentKeystrokes = recentKeys`

涉及 3 个动态字段：`_lastInjectTime`（number，shell 注入节流时间戳）、`_sudoWindow`（string，sudo 窗口标记）、`_recentKeystrokes`（string，近期按键缓冲，用于 sudo 检测）。

**影响**：类型安全在类内部被绕开，这三个状态完全游离于类型系统之外；后续若有人重命名或删除相关逻辑，TS 不会提示。

**具体改法**：

1. 在 `LiveSshSessionController` 类的私有字段声明区（当前约 39-83 行附近，已有 `private shellCwd?` 等声明）补充：
   ```ts
   private _lastInjectTime = 0
   private _sudoWindow = ''
   private _recentKeystrokes = ''
   ```
   （字段名建议去掉前导下划线，改为 `lastInjectTime` / `sudoWindow` / `recentKeystrokes`，与现有命名风格如 `shellCwd` 一致；同时更新所有引用点。）
2. 将 7 处 `(this as any)._xxx` 替换为 `this.xxx`。
3. 确认这些字段的初始值与原 `|| 0` / `|| ''` 回退语义一致（声明时给默认值即可）。
4. 跑 `npm run typecheck -w @fileterm/desktop` 确认无新类型错误。

**验收标准**：

- [x] `grep -rn "as any" apps/desktop/src/main/services/sessions/ssh-session-controller.ts` 零命中。
- [x] 三个字段在类声明区有 `private` 类型化声明。
- [x] typecheck 通过。

**进阶（可选，独立工作项）**：把 CWD 跟踪和 sudo 检测逻辑抽成独立 collaborator（类似已有的 `shell-cwd-integration.ts` 的 `ShellCwdTracker`），可让 controller 瘦身约 400 行。此项非紧急，可单独排期。

---

### P1-2. 修复渲染端 prop 懒类型（6 处 `any`）

**问题**：渲染端组件接口用 `any` 接收 props，削弱了 `packages/core` 定义的类型契约向 UI 层的传导——core 定义了 `CommandTemplateInput` 等类型，但组件边界用 `any` 接收，类型安全在 UI 边界断链。

**证据**：

- `apps/desktop/src/renderer/features/workspace/WorkspaceStage.tsx` 约 156-168 行，命令创建回调 prop 用 `input: any`。
- `apps/desktop/src/renderer/features/workspace/HomeWorkspace.tsx` 约 55-66 行，`tabBarProps: any` 等。

**影响**：组件入参失去类型保护，调用方传错结构不会报错；重构 core 类型时 UI 调用点不会被 TS 追踪到。

**具体改法**：

1. `WorkspaceStage.tsx`：用 `@fileterm/core` 的 `CommandTemplateInput`（或对应类型）替换 `input: any`；若该回调签名复杂，为该 prop 定义独立接口。
2. `HomeWorkspace.tsx`：为 `tabBarProps` 定义具体接口（可复用 `features/layout/TabBar.tsx` 已导出的 `OrderedTabEntry` / `TabContextTarget` 等类型组合）。
3. 全仓库 grep `: any` 在 renderer 下的命中，逐个评估是否可用具体类型替换（评审统计 renderer 共约 6 处 any，集中在上述两文件）。

**验收标准**：

- [x] `grep -rn ": any" apps/desktop/src/renderer` 在 props/参数位置零命中（局部 `any` 如事件对象可酌情保留并加注释）。
- [x] typecheck 通过。

---

### P1-3. 补齐最脆弱模块的单测覆盖

**问题**：测试覆盖严重偏科。9 个测试文件集中在 `test/transfers/`（7 个）+ `test/protocol/`（2 个 resume）+ `test/system-metrics/parser`（1 个）。**零覆盖**：所有 session controller、`workspace-service`、`file-profile-repository`、全部 IPC handler、全部 renderer 组件、`packages/core`。

最复杂、最易回归的领域（SSH 控制器、工作区编排、profile 自愈）反而无测试保护。

**影响**：核心逻辑无回归保护，重构（如 P1-1、现有 core-decoupling-plan）风险高。

**具体改法（按优先级，先做纯逻辑、易测、高回归风险的）**：

**第一批（最高优先，纯逻辑易测）**：

1. `apps/desktop/test/profiles/file-profile-repository.test.ts`
   - 覆盖 `file-profile-repository.ts` 的 group/parentId 双向自愈：
     - create profile 时按 folder.name 反查 parentId（:88-92）
     - folder 重命名级联更新子 profile 的 group（:196）
     - folder 删除时子 profile parentId/group 回退上级（:206-226）
     - 用临时目录 mock 文件存储，验证 CRUD 后的 parentId/group 一致性。
2. `apps/desktop/test/workspace/terminal-merge.test.ts`
   - 覆盖 `workspace-session-runtime.ts` 的终端 16ms 合并逻辑（`TERMINAL_OUTPUT_FLUSH_INTERVAL_MS=16`，`terminalOutputBuffers` + 定时 flush）。
   - 验证：高频写入被合并、flush 后缓冲清空、定时器生命周期。

**第二批（中优先，需 mock）**：3. `packages/core` 的 `mergeSystemMetricsHistory` 和 `createTabLayout` 纯函数补单测（core 目前零测试，但有两个纯函数可低成本覆盖）。4. `workspace-session-runtime` 的事件转发机制（CWD 变更、Root 状态提权）用 mock controller 做集成测试。

**第三批（controller 层，成本较高，单独排期）**：5. SSH/FTP controller 用接口 mock（`FileSessionController` 接口）做集成测试，验证传输 offset 续传、降级路径。

**验收标准**：

- [x] 第一批两个测试文件创建并通过。
- [x] `test:transfers` 脚本扩展为 `test`（聚合 transfers + profiles + workspace），或在 CI 分别调用。
- [x] controller 层测试至少有占位 TODO。

---

## P2：可接受债务（择机处理）

### P2-1. workspace-service.ts 1697 行

已是 façade 薄委托（委托给 `WorkspaceTabsState` / `WorkspaceTransfersState` / `WorkspaceTabLifecycleService` / `WorkspaceSessionRuntime` / `TransferJournal` / `ProfileRepository`），多数方法 3-10 行。风险可控，非上帝对象。`TransferService` 的抽取记录见 `docs/plans/completed/fileterm-core-decoupling-plan.md`，不在此重复。

### P2-2. 根目录散落文档归档

- `MODAL_OPTIMIZATION_SUMMARY.md` → 移至 `docs/quality/` 或删除（若内容已过时）。
- `design.md` → 评估是否归入 `docs/` 或保留为设计草稿。

### P2-3. 无 store 的状态管理

`architecture.md` 已明确“等 App.tsx 拆分后再评估 Zustand”，属于有意为之的债。当前靠 React state + snapshot + IPC 广播，短期可持续。App.tsx hook 化拆分已完成，后续是否引入 store 仍按真实复杂度单独评估，不因重构完成而自动推进。

---

## 与现有 plan 的关系

本计划**不重复**以下已有计划的工作，仅做交叉引用：

- `docs/plans/completed/structural-boundaries.md`：IPC 拆分、协议控制器物理隔离、tab lifecycle、transfer runtime 与 App.tsx hooks 拆分均已完成。
- `docs/plans/completed/fileterm-core-decoupling-plan.md`：TransferService 抽取、会话 runtime 事件转发器重构、App.tsx 状态托管 hook 化均已完成；本计划建立的质量门禁为这些重构提供了安全网。
- `docs/plans/completed/multiplatform-system-observability.md`：多平台系统信息采集，与本计划无重叠。

---

## 改进优先级与建议执行顺序

| 顺序 | 工作项                                     | 预估投入 | 依赖                    |
| ---- | ------------------------------------------ | -------- | ----------------------- |
| 1    | P0-1 ESLint + Prettier                     | ~1 天    | 无                      |
| 2    | P0-3 提交门禁（依赖 P0-1 的 lint）         | ~0.5 天  | P0-1                    |
| 3    | P0-2 CI 跑测试                             | ~0.5 天  | 无                      |
| 4    | P1-1 清理 as any 私有字段                  | ~0.5 天  | P0-1（lint 会标记 any） |
| 5    | P1-2 修复渲染端 prop any                   | ~0.5 天  | 无                      |
| 6    | P1-3 第一批单测（profile 自愈 + 终端合并） | ~2 天    | P0-2（CI 能跑测试）     |

**关键路径**：P0-1（lint）是 P0-3（门禁）和 P1-1（as any 清理）的前置；P0-2（CI 测试）是 P1-3（补测）价值兑现的前提。建议先集中完成 P0 全部三项（约 2 天），再推进 P1。

---

## 进度记录

- 2026-07-09：基于质量评审建立本计划，收录 P0 三项质量门禁、P1 三项代码质量专项、P2 三项可接受债务。所有问题证据已核实（as any 7 处行号经 grep 确认，CI step 与测试脚本经读源确认）。
- 2026-07-10：P0 质量门禁三件套全部落地——ESLint+Prettier 配置完成（`--max-warnings=0` 全绿）、CI 接入 `npm test` + `test:transfers:protocol`、husky pre-commit(lint-staged)/pre-push(typecheck) 门禁已建。P1 技术债清理全部完成——ssh-session-controller 7 处 `as any` 零命中、renderer 6 处 `: any` 零命中、profiles 自愈 + 终端 16ms 合并 + platform-probe + windows-collector 单测已补齐（31/31 通过）。四项门禁全绿验证通过。
