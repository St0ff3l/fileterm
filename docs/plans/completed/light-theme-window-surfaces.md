# 浅色主题窗口与表面层次

完成日期：2026-10-02。基于 main `08dd1624`，分支 `fix/light-theme-window-surfaces`。

## 问题与修改

浅色独立窗口使用从画布到 hover 色的渐变，Codex 运行时 hover 色含透明度，而 macOS/Linux 根背景透明，导致窗口底部透出桌面。共享独立窗口 frame 增加语义画布底板，浅色窗口及文件编辑器改为实色画布；外层透明继续承担原生圆角裁切。

连接表单的侧栏、内容与 fieldset 原先统一使用 hover 色，层次不清。相关浅色规则迁移至连接组件共址 CSS，改为 section / panel 表面，补足分组边界、圆角、提示文字对比与操作区分隔线。

Catppuccin、Dracula、Gruvbox、Nord、Solarized、Tokyo Night 六个 iTerm2 浅色预设的 panel / card / modal 提亮，section 保留主题浅底，input 再提亮。保留各主题色温与终端配色。所有修改继续通过 ref → semantic → component 链路。

顶部栏复查：非默认主题皮肤曾把标题栏、标签条及窗口菜单统一绑定到 elevated，导致浅色控件提亮后出现白带。修正为已有 surface-titlebar 语义变量；六套 iTerm2 浅色预设该值已与画布一致，不新增变量或具体色值，不新增 !important。

## 验证

- 类型检查、Lint、全项目 Prettier 检查通过。
- Rust unit/integration/contract 测试与 all-targets/all-features Clippy 通过。
- CSS contract 通过；历史 11 处 canonical !important 与约 1 处 legacy 直接颜色值未增加。
- 用实际 ConnectionModal、StandaloneWindowFrame、主题运行时和完整 CSS 的临时浏览器夹具检查八套主题 × 三个平台属性，共 24 组：frame 不透明、窗口无渐变、614px 窄视口无横向溢出；macOS 使用原生 DropdownSelect 外壳，Windows/Linux 使用自绘触发器。
- 桌面开发进程从当前工作树启动，开发配置为 com.fileterm.desktop.dev，复用当前工作树 Vite 5188。Computer Use 无法发现该 bundle ID，故未进行原生桌面窗口目测；Windows/Linux 检查为浏览器平台分支验证，并非真实系统手测。
- 顶部栏追加验证：六套 iTerm2 浅色 × 三个平台属性，共 18 组、90 个区域，标签栏、品牌区、标签内容区、窗口菜单和独立标题栏的实际背景均与主题画布一致；包含首页 home-tabs-bar 的历史优先级覆盖。
- 临时浏览器夹具验证后删除。未修改超过 1000 行的非豁免源码文件，无需拆分。
