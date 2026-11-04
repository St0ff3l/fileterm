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
