use serde_json::Value;

const DEFAULT_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DEFAULT_MAX_BATCH_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const DEFAULT_MAX_BATCH_FILES: u64 = 128;
const MIN_MAX_FILE_BYTES: u64 = 1024 * 1024;
const MIN_MAX_BATCH_BYTES: u64 = 1024 * 1024;
const MAX_MAX_BATCH_FILES: u64 = 4096;

/// Resource limits protect receive mode from a malformed or hostile sender.
/// The fields are optional profile values so old profiles keep the established
/// behavior while deployments that need larger images can opt in explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SerialTransferLimits {
    pub(super) max_file_bytes: u64,
    pub(super) max_batch_bytes: u64,
    pub(super) max_batch_files: u64,
}

impl Default for SerialTransferLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_batch_bytes: DEFAULT_MAX_BATCH_BYTES,
            max_batch_files: DEFAULT_MAX_BATCH_FILES,
        }
    }
}

impl SerialTransferLimits {
    pub(super) fn from_profile(profile: &Value) -> Result<Self, String> {
        let defaults = Self::default();
        let max_file_bytes = profile
            .get("serialTransferMaxFileBytes")
            .and_then(Value::as_u64)
            .unwrap_or(defaults.max_file_bytes);
        let max_batch_bytes = profile
            .get("serialTransferMaxBatchBytes")
            .and_then(Value::as_u64)
            .unwrap_or(defaults.max_batch_bytes);
        let max_batch_files = profile
            .get("serialTransferMaxFiles")
            .and_then(Value::as_u64)
            .unwrap_or(defaults.max_batch_files);

        if max_file_bytes < MIN_MAX_FILE_BYTES {
            return Err(format!(
                "串口单文件传输上限不能小于 {} 字节",
                MIN_MAX_FILE_BYTES
            ));
        }
        if max_batch_bytes < MIN_MAX_BATCH_BYTES || max_batch_bytes < max_file_bytes {
            return Err(format!(
                "串口批量传输上限必须至少为单文件上限，且不能小于 {} 字节",
                MIN_MAX_BATCH_BYTES
            ));
        }
        if max_batch_files == 0 || max_batch_files > MAX_MAX_BATCH_FILES {
            return Err(format!(
                "串口批量文件数量必须在 1 到 {} 之间",
                MAX_MAX_BATCH_FILES
            ));
        }

        Ok(Self {
            max_file_bytes,
            max_batch_bytes,
            max_batch_files,
        })
    }
}

/// Tracks a transfer batch without buffering file contents in memory.
#[derive(Debug)]
pub(super) struct TransferBudget {
    limits: SerialTransferLimits,
    files: u64,
    reserved_bytes: u64,
}

impl TransferBudget {
    pub(super) fn new(limits: SerialTransferLimits) -> Self {
        Self {
            limits,
            files: 0,
            reserved_bytes: 0,
        }
    }

    /// Start a file. A declared size reserves batch space immediately; raw
    /// and Kermit-style streams use `None` and account bytes as they arrive.
    pub(super) fn begin_file(&mut self, declared_size: Option<u64>) -> Result<(), String> {
        if self.files >= self.limits.max_batch_files {
            return Err(format!(
                "串口文件传输文件数量超过上限（{} 个）",
                self.limits.max_batch_files
            ));
        }
        if let Some(size) = declared_size {
            if size > self.limits.max_file_bytes {
                return Err(format!(
                    "串口单个文件超过接收上限（{} 字节）",
                    self.limits.max_file_bytes
                ));
            }
            let next = self
                .reserved_bytes
                .checked_add(size)
                .ok_or_else(|| "串口文件传输总大小超出支持范围".to_string())?;
            if next > self.limits.max_batch_bytes {
                return Err(format!(
                    "串口批量传输超过总大小上限（{} 字节）",
                    self.limits.max_batch_bytes
                ));
            }
            self.reserved_bytes = next;
        }
        self.files += 1;
        Ok(())
    }

    pub(super) fn account_unknown_bytes(&mut self, count: u64) -> Result<(), String> {
        let next = self
            .reserved_bytes
            .checked_add(count)
            .ok_or_else(|| "串口文件传输总大小超出支持范围".to_string())?;
        if next > self.limits.max_batch_bytes {
            return Err(format!(
                "串口批量传输超过总大小上限（{} 字节）",
                self.limits.max_batch_bytes
            ));
        }
        self.reserved_bytes = next;
        Ok(())
    }

    pub(super) fn max_file_bytes(&self) -> u64 {
        self.limits.max_file_bytes
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SerialTransferLimits, TransferBudget};

    #[test]
    fn parses_bounded_profile_limits() {
        let limits = SerialTransferLimits::from_profile(&json!({
            "serialTransferMaxFileBytes": 2 * 1024 * 1024,
            "serialTransferMaxBatchBytes": 4 * 1024 * 1024,
            "serialTransferMaxFiles": 2
        }))
        .unwrap();
        assert_eq!(limits.max_batch_files, 2);
        assert!(SerialTransferLimits::from_profile(&json!({
            "serialTransferMaxFileBytes": 4 * 1024 * 1024,
            "serialTransferMaxBatchBytes": 2 * 1024 * 1024
        }))
        .is_err());
    }

    #[test]
    fn limits_known_and_unknown_batch_sizes() {
        let limits = SerialTransferLimits {
            max_file_bytes: 1024,
            max_batch_bytes: 2048,
            max_batch_files: 2,
        };
        let mut budget = TransferBudget::new(limits);
        budget.begin_file(Some(1024)).unwrap();
        budget.begin_file(None).unwrap();
        assert!(budget.account_unknown_bytes(1025).is_err());
        assert!(budget.begin_file(None).is_err());
    }
}
