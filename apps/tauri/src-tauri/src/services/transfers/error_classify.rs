// 目录传输的瞬时错误重试策略与错误分类。
//
// 目标是把“一个文件失败炸掉整个任务”的失败半径缩小到单文件：
// - 瞬时错误（网络抖动、超时、连接重置）：按退避自动重试，重试期间
//   复用既有断点探测（`prepare_remote_upload`）从实际偏移续传；
// - 永久错误（权限、路径、源文件变化）：标记该文件失败并跳过，
//   任务继续处理其余文件，结束后进入可续传的 paused 状态供用户重试失败项。

/// 单个文件/目录操作的瞬时错误最大尝试次数（1 次初始 + 2 次重试）。
const TRANSIENT_MAX_ATTEMPTS: u32 = 3;
/// 重试退避基准延迟，按 4 的幂递增（1s、4s）。
const TRANSIENT_RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

/// 判断传输错误是否为永久性错误（重试不可能成功）。
///
/// 分类依据是错误消息子串（russh/suppaftp/io 层透传的字符串）。
/// 未知错误一律按瞬时处理：重试次数有界，误判“永久为瞬时”的代价只是
/// 几次额外尝试；反向误判会静默跳过本可恢复的文件，代价更高。
fn is_permanent_transfer_error(message: &str) -> bool {
    // 小写化以兼容 `Permission denied` / `permission denied` 等大小写差异；
    // 中文标记不受影响。
    let lowered = message.to_lowercase();
    const PERMANENT_MARKERS: &[&str] = &[
        // 权限类
        "permission denied",
        "access denied",
        "operation not permitted",
        "权限",
        // 空间类
        "no space",
        "disk quota",
        "quota exceeded",
        "磁盘已满",
        // 路径/源文件类
        "no such file",
        "not found",
        "does not exist",
        "不存在",
        "read-only",
        "readonly file",
        "只读",
        "is a directory",
        "not a directory",
        "name too long",
        // FileTerm 自产错误中的永久语义
        "已发生变化",
        "大于源文件",
        "不是普通文件",
    ];
    PERMANENT_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// 第 attempt 次失败后的退避延迟（attempt 从 1 开始计）。
fn transient_retry_backoff(attempt: u32) -> Duration {
    TRANSIENT_RETRY_BASE_DELAY
        .saturating_mul(4u32.saturating_pow(attempt.saturating_sub(1)))
}

/// 可被取消的退避等待：取消令牌触发时立即返回。
async fn sleep_transient_backoff(cancel: &CancellationToken, delay: Duration) {
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = tokio::time::sleep(delay) => {}
    }
}
