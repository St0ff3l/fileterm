use super::*;

#[test]
fn second_session_follows_until_the_leader_releases() {
    let leases = Arc::new(MonitoringLeases::default());
    let changed = leases.subscribe();
    let leader = leases.try_acquire("profile-a", "tab-1").unwrap();
    assert_eq!(
        leases.try_acquire("profile-a", "tab-2").err().as_deref(),
        Some("tab-1")
    );
    // Other machines are independent.
    let other = leases.try_acquire("profile-b", "tab-2").unwrap();

    drop(leader);
    assert!(changed.has_changed().unwrap());
    let next = leases.try_acquire("profile-a", "tab-2").unwrap();
    assert_eq!(
        leases.try_acquire("profile-a", "tab-1").err().as_deref(),
        Some("tab-2")
    );
    drop(next);
    drop(other);
}

#[test]
fn a_reconnected_tab_waits_for_its_previous_collector() {
    let leases = Arc::new(MonitoringLeases::default());
    let previous = leases.try_acquire("profile-a", "tab-1").unwrap();
    assert_eq!(
        leases.try_acquire("profile-a", "tab-1").err().as_deref(),
        Some("tab-1")
    );
    drop(previous);
    assert!(leases.try_acquire("profile-a", "tab-1").is_ok());
}
