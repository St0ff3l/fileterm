use super::*;

#[test]
fn name_and_marker_must_both_match_before_reclaiming_a_trace() {
    let owner = Owner {
        pid: 123,
        created: 456,
    };
    let mut properties = Properties::for_owner(owner);
    assert_eq!(Owner::from_properties(&properties).unwrap().pid, 123);
    properties.header.Wnode.Guid.data1 ^= 1;
    assert!(Owner::from_properties(&properties).is_none());
    properties.header.Wnode.Guid = owner.guid();
    properties.logger_name[0] ^= 1;
    assert!(Owner::from_properties(&properties).is_none());
}

#[test]
fn process_creation_distinguishes_live_owners_and_reused_ids() {
    let current = Owner::for_process(std::process::id()).unwrap();
    assert_eq!(
        process_creation(current.pid).unwrap(),
        Some(current.created)
    );
    assert_ne!(
        process_creation(current.pid).unwrap(),
        Some(current.created + 1)
    );
    assert_eq!(process_creation(u32::MAX).unwrap(), None);
}

#[test]
fn matching_name_with_wrong_guid_is_not_stopped_even_by_parent_fallback() {
    let owner = Owner::for_process(std::process::id()).unwrap();
    let mut properties = Properties::for_owner(owner);
    properties.header.Wnode.Guid.data1 ^= 1;
    properties.header.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    properties.header.LogFileNameOffset = 0;
    properties.header.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
    properties.header.BufferSize = 4;
    properties.header.MinimumBuffers = 2;
    properties.header.MaximumBuffers = 2;
    let mut foreign = Session {
        handle: CONTROLTRACE_HANDLE::default(),
        properties,
        stopped: true,
    };
    let error = unsafe {
        StartTraceW(
            &mut foreign.handle,
            foreign.properties.logger_name.as_ptr(),
            &mut foreign.properties.header,
        )
    };
    if error != 0 {
        assert_eq!(error, 5);
        return;
    }
    foreign.stopped = false;
    let fallback = cleanup_current(owner);
    let recovery = recover_orphans();
    let still_running = foreign.stop();
    assert_eq!(fallback, ERROR_INVALID_DATA);
    assert_eq!(recovery, Err(ERROR_INVALID_DATA));
    assert_eq!(
        still_running, 0,
        "neither cleanup path may stop the foreign session"
    );
}

#[test]
#[ignore = "used only as a child process by the orphan recovery regression"]
fn recovery_owner_child() {
    use std::io::Read;
    let _ = std::io::stdin().read_exact(&mut [0u8]);
}

#[test]
fn killed_owner_is_reclaimed_and_live_owner_is_left_alone() {
    use std::process::{Command, Stdio};
    if let Err(error) = super::super::scheduler::enable_profile_privilege() {
        assert!([5, 1300, 1314].contains(&error));
        return;
    }
    // Reuse the Rust test binary so the fixture needs no external executable or desktop DLL.
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "recovery_owner_child", "--nocapture"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let owner = Owner::for_process(child.id()).unwrap();
    let mut session = match Session::start(owner) {
        Ok(session) => session,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            assert!([5, 1314].contains(&error));
            return;
        }
    };
    let before = recover_orphans();
    // Always end the child and trace even when the assertion below fails.
    let mut query = Properties::for_owner(owner);
    let live_query = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            query.logger_name.as_ptr(),
            &mut query.header,
            EVENT_TRACE_CONTROL_QUERY,
        )
    };
    child.kill().unwrap();
    child.wait().unwrap();
    let after = recover_orphans();
    let mut query = Properties::for_owner(owner);
    let after_query = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            query.logger_name.as_ptr(),
            &mut query.header,
            EVENT_TRACE_CONTROL_QUERY,
        )
    };
    session.stop();
    assert!(before.is_ok(), "{before:?}");
    assert_eq!(live_query, 0, "recovery must not stop a live owner's trace");
    assert!(after.unwrap() >= 1);
    assert_eq!(after_query, ERROR_WMI_INSTANCE_NOT_FOUND);
}
