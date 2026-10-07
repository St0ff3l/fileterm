# Linux 桌面窗口外框适配

## 问题与范围

原实现对所有 Linux 主窗口和独立窗口统一裁切 `15px` 圆角。在 LXQt 的直角窗口及缺少透明合成的远程桌面会话中，四角会露出黑色背景。

## 实现

- Core 定义 `LinuxWindowCornerStyle`；Rust command 读取本机桌面与会话环境，不读取远端 SSH 平台。
- 明确的 GNOME Wayland 沿用现有圆角策略，其余桌面、X11 和未知环境回退直角。这是保守的产品策略，不是对系统窗口主题或透明合成能力的准确探测。
- Bridge 获取元数据，renderer 在 React 挂载前设置 `data-linux-window-corners`；基础 token 统一控制主窗口、独立表单和最大化窗口的外框。
- 取消 reset/global 两处无条件 `15px` 覆盖；内部控件的圆角独立保留。

## 验证边界

Rust 用例覆盖 LXQt、混合桌面标识、KDE、Xfce、未知桌面、GNOME X11 和 GNOME Wayland 的策略分支。本机为 Windows，LXQt/X11 与 GNOME Wayland 的打包窗口视觉验收仍需在对应 Linux 会话执行。

- Tauri typecheck、本次 TS 文件 ESLint、本次文件 Prettier、CSS contract 和 diff whitespace 检查通过。
- `npm run test:tauri` 通过：642 项单元测试、10 项 CLI 测试、21 项 contract 测试。
- 全库 lint/Prettier 被未改动文件的既有 CRLF 换行问题阻断（Prettier 报告 344 个文件）；未扩大本次修改范围。
- `cargo clippy --locked --all-targets --all-features -- -D warnings` 在 `aws-lc-sys` 构建阶段因本机缺少 NASM 失败，未完成全功能静态检查。
- Bridge 的运行时元数据职责提取到 `runtime-metadata.ts`（25 行），`tauri-api.ts` 从 798 行降为 782 行；不属于超过 1000 行的强制拆分，类型聚合文件仍属于豁免类别。
