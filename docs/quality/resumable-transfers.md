# 可恢复传输回归矩阵

## 自动化命令

```bash
npm run test:transfers -w @fileterm/desktop
npm run test:transfers:protocol -w @fileterm/desktop
npm run typecheck
npm run build
git diff --check
```

## 协议矩阵

| 场景             | SFTP     | root SFTP      | FTP      | 显式 FTPS | 隐式 FTPS |
| ---------------- | -------- | -------------- | -------- | --------- | --------- |
| 单文件上传/下载  | 必测     | 必测           | 必测     | 必测      | 必测      |
| 暂停后继续       | 必测     | 必测           | 必测     | 必测      | 必测      |
| 连接中断后重试   | 必测     | 必测           | 必测     | 必测      | 必测      |
| 应用重启后继续   | 必测     | 重新授权后必测 | 必测     | 必测      | 必测      |
| 目录 manifest    | 必测     | 必测           | 必测     | 必测      | 必测      |
| 断点大于源文件   | 必须拒绝 | 必须拒绝       | 必须拒绝 | 必须拒绝  | 必须拒绝  |
| 最终 rename 失败 | 保留断点 | 保留断点       | 保留断点 | 保留断点  | 保留断点  |

真实 socket 协议夹具位于 `apps/desktop/test/protocol/`，包含本地 OpenSSH SFTP、FTP、显式 FTPS 和隐式 FTPS。受限环境缺少 sshd/openssl 或禁止监听 localhost 时相应用例显示 skipped；普通 macOS/Linux 开发机或 CI 中必须执行为 pass。
