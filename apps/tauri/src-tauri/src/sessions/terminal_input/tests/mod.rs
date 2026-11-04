use super::*;

#[test]
fn budget_follows_input_across_merge_and_conversion_and_releases_on_drop() {
    let budget = Arc::new(Semaphore::new(128));
    let controls = Arc::new(Semaphore::new(64));
    let mut input = TerminalInput::with_budget("中".into(), &budget, &controls).unwrap();
    input.append(TerminalInput::with_budget("\r".into(), &budget, &controls).unwrap());
    assert!(TerminalInput::with_budget("x".into(), &budget, &controls).is_err());
    let bytes = input.into_bytes();
    assert_eq!(bytes.data, "中\r".as_bytes());
    assert_eq!(budget.available_permits(), 0);
    let interrupt = TerminalInput::with_budget("\x03".into(), &budget, &controls).unwrap();
    assert!(TerminalInput::with_budget("\x03".into(), &budget, &controls).is_err());
    drop(interrupt);
    assert_eq!(controls.available_permits(), 64);
    drop(bytes);
    assert_eq!(budget.available_permits(), 128);
}

#[test]
fn disconnect_releases_queued_input_and_utf8_is_charged_in_bytes() {
    let budget = Arc::new(Semaphore::new(300));
    let controls = Arc::new(Semaphore::new(64));
    let input = TerminalInput::with_budget("中".repeat(100), &budget, &controls).unwrap();
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    assert!(sender.send(input.into_bytes()).is_ok());
    assert_eq!(budget.available_permits(), 0);
    drop(receiver);
    assert_eq!(budget.available_permits(), 300);
    let input = TerminalInput::with_budget("retry".into(), &budget, &controls).unwrap();
    assert!(sender.send(input.into_bytes()).is_err());
    assert_eq!(budget.available_permits(), 300);
}

#[test]
fn oversized_paste_is_rejected_whole_without_consuming_budget() {
    let budget = Arc::new(Semaphore::new(TOTAL_INPUT_BYTES));
    let controls = Arc::new(Semaphore::new(64));
    assert!(
        TerminalInput::with_budget("x".repeat(MAX_INPUT_BYTES + 1), &budget, &controls).is_err()
    );
    assert_eq!(budget.available_permits(), TOTAL_INPUT_BYTES);
}
