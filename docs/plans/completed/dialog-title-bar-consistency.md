# 紧凑弹窗标题栏一致性

## 范围

- 路径收藏标题复用本地 star SVG，尺寸 16px，保留原有标题语义及关闭行为。
- 表单弹窗、外部操作审批/调用详情、路径收藏共享 foundations 中的 44px 标题栏尺寸。
- 旧式编辑器与独立窗口的标题栏基线保持各自约束。

## 验证

浏览器夹具确认三类标题栏均为 44 CSS px，浅色表单高度同样为 44px。路径收藏星标为 16px。临时夹具已移除。类型、lint、Prettier、247 项 renderer 测试、Rust/CLI/contract 测试、Clippy 与 CSS contract 均通过。既有 SSH OTP 交互式夹具保持 ignored，原生桌面和 Windows/Linux 未手工验收。

公共 Dialog 的 title 属性已去除原生 HTML 字符串属性的类型交叉，使既有 ReactNode 标题接口可接收图标与文字组合。
