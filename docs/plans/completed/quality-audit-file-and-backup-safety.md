# 文件与备份质量检查

## 范围

从 main `c2354028` 开始，分两轮检查传输路径、本地文件操作、SMB 清理、备份恢复、前端剪贴板行为及 CI 门禁。

## 实施

- 在 Rust 传输层校验目标名称与目录映射。
- 将下载限额收敛为 WebDAV/S3 共用流式读取，完善恢复前校验和串口身份。
- 保护本地新建/复制/跨磁盘移动与 SMB 清理的数据安全。
- 阻止跨端剪切在传输入队后立即删除源文件，补充 renderer 行为回归并接入 CI。

## 验收记录

见 [传输路径检查](../../quality/transfer-path-quality-audit.md) 与 [扩展质量检查](../../quality/comprehensive-quality-audit.md)。实现与本机验证完成；三平台实机/真实服务验收未包含在本轮执行中。

第三轮后端专项复核覆盖日志恢复、历史保留、WebDAV ETag 并发及传输取消传播，见 [后端专项报告](../../quality/backend-final-audit.md)。
