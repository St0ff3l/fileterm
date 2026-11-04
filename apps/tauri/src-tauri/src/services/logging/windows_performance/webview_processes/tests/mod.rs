use super::*;

#[test]
fn closing_an_old_window_cannot_remove_its_replacement_and_closed_slots_are_released() {
    let label = "webview-registration-lifecycle-test";
    let old = Arc::new(Mutex::new(Registration::Pending));
    let replacement = Arc::new(Mutex::new(Registration::Pending));
    let weak = Arc::downgrade(&replacement);
    WINDOWS
        .lock()
        .unwrap()
        .insert(label.into(), Arc::clone(&replacement));
    forget(label, &old);
    assert!(WINDOWS.lock().unwrap().contains_key(label));
    forget(label, &replacement);
    assert!(!WINDOWS.lock().unwrap().contains_key(label));
    drop(replacement);
    assert!(weak.upgrade().is_none());
}
