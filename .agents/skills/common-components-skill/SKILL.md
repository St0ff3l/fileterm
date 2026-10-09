---
name: common-components-skill
description: 修改 FileTerm renderer 的 React/TSX、组件 CSS、主题 token 或公共控件时使用；维护组件复用、语义颜色和 CSS contract。
---

# FileTerm 通用组件与主题

目标：复用公共交互，让内置和自定义主题通过同一条变量链路生效。路径以当前源码为准。

## 实现边界

- 新公共视觉组件放 `apps/tauri/src/renderer/components/common/<name>/`，组件 CSS 共址；业务组件放 `features/<feature>/`。
- `features/common/` 保留行为工具、辅助组件和兼容 re-export；`styles/features/` 是迁移区，不新增组件皮肤。
- 具体颜色只进入主题 `--ref-*`；`styles/tokens/semantic.css` 映射语义变量；组件只消费语义变量，不写 hex/rgb、`--ref-*` 或旧兼容别名。布局、尺寸与排版数值可在组件 CSS 定义。
- 新颜色补齐四个内置主题与 `app/theme-config.ts` 的运行时映射；`--focus-outline` 只用于描边/光环，主按钮用 `--action-primary-*`。
- 表单用 `DropdownSelect`，箭头随实际控件高度缩放；破坏性操作用 `ConfirmActionDialog`，禁止业务原生 select / `window.confirm()`。
- 新增纵向滚动默认复用 `features/common/vertical-scrollbar.tsx` 并隐藏原生纵向滚动条；横向、第三方编辑器内部及协议组件自带滚动可保留专用实现。
- UI 图标使用本地 SVG / `AppIcon` 和 currentColor，按功能域归档；应用身份资产与 UI 图标隔离，不引入 WebFont 或 CDN。
- 同组按钮高度、圆角和内边距一致，异步操作防止重复提交并保留错误反馈。

## 按需细节

- 修改 token、自定义主题、颜色迁移、首屏主题或编辑器主题时，读 [主题实现与迁移](references/theme-contract.md) 对应章节。
- 修改具体控件、图标、选择状态或系统指标时，读 [公共控件与资产约束](references/controls.md) 对应条目。
- 原生窗口、平台差异与快捷键另读 [跨平台 skill](../multiplatform-desktop-skill/SKILL.md)。

## 验证与完成

组件/CSS 改动前后运行（仓库根目录）：

```bash
bash .agents/skills/common-components-skill/scripts/check-css-contract.sh
```

对比已有债务与本次新增问题，不为通过检查顺手重写无关样式。检查范围是 semantic.css、公共组件、feature CSS、四个主题文件；feature TSX 行内颜色、原生 select/confirm 和自定义主题映射需另外检查。

运行改动文件的 Prettier 检查及 AGENTS.md 要求的代码门禁。验证受影响主题、自定义 accent、控件状态和平台分支；报告结果、历史债务与未验证平台。
