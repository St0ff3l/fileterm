# FileTerm UI 图标资源

应用自身的静态 UI 图形统一存储为本目录中的独立 SVG，按 actions、navigation、files、network、security、system 分类。动态图表由数据生成，不属于静态图标。Dock、托盘、安装包等应用身份资产继续使用各平台的独立资源目录。

## 两组现有造型

- `app-*.svg`：从原 `AppIcon` 组件逐项提取的 SVG，以及原自绘窗口控件。保留原路径、嵌套 SVG、坐标变换、填充和描边。`brand` 与 `terminal` 共用 `network/app-terminal.svg`。
- `material-*.svg`：原本地 Material Symbols Outlined 400 字重字体中的 58 个字形保持原轮廓与 `viewBox="0 -960 960 960"` 字体基线；`material-fit_screen.svg` 是从 `@material-design-icons/svg` 0.10.5 单独加入的适配画板轮廓图标。来源摘要见 `tests/fixtures/material-icon-geometry.json`；Apache 2.0 许可与版权见应用的 `licenses/material-symbols/LICENSE.txt` 和根目录 `THIRD_PARTY_NOTICES.md`。
- 未带前缀的现有素材保留原造型。其中部分用于 CSS mask；同名但不同路径的素材不直接合并，避免改变既有外观。

## 使用方式

```tsx
<AppIcon name="shield" size={20} strokeWidth={1.8} />
<MaterialIcon name="settings" className="material-symbols-outlined" />
```

组件从 SVG 文件通过 Vite `?raw` 静态导入图形，再生成 SVG DOM，因此不需要等待图标字体或逐个请求图片文件。图形数据的唯一来源是资源文件；不要在业务 TSX 中复制路径。

`AppIcon` 保留原 `name`、`size`、`strokeWidth`、`className` API。素材中的 `--app-icon-stroke-width` 与 `--app-icon-scaled-stroke-width` 占位值由组件按参数替换成原有数字属性，确保生成的 SVG DOM 与原组件一致；不要把它们改成固定值。固定描边（如拖拽点、信息圆点）也保持原值。

`MaterialIcon` 保留原 span 包装和 CSS class，以保持尺寸、颜色、动画与布局选择器的行为。

## 编辑与验证

所有单色图标使用 `currentColor` 跟随主题、状态和文件夹颜色，不引入 CDN。修改素材时检查不同尺寸、深浅主题、描边参数、折叠侧栏、自绘窗口控件及拖拽预览。

`tests/svg-icon-assets.test.mjs` 对照迁移前的图形摘要验证 AppIcon 的 57 个原名称与 7 组描边，并验证 Material 字形轮廓、资源安全性和图标来源。窗口控件保留 10×10 坐标；其余 AppIcon 保留 16×16 坐标。
