use super::*;

fn model() -> SchedulerModel {
    SchedulerModel::new(
        vec![ThreadIdentity {
            pid: 9,
            tid: 10,
            role: "renderer-main",
        }],
        BTreeMap::from([(0, 0), (1, 8)]),
    )
}

#[test]
fn decodes_only_known_complete_kernel_payloads() {
    let mut data = [0u8; 24];
    data[..4].copy_from_slice(&10u32.to_le_bytes());
    data[4..8].copy_from_slice(&99u32.to_le_bytes());
    data[14] = 1;
    assert!(matches!(
        decode(36, 2, &data),
        Some(Transition::Switch {
            incoming: 10,
            outgoing: 99,
            state: 1
        })
    ));
    assert!(matches!(
        decode(50, 2, &data[..8]),
        Some(Transition::Ready(10))
    ));
    data[6] = 2;
    assert!(matches!(
        decode(50, 2, &data[..8]),
        Some(Transition::SwappedOut(10))
    ));
    assert!(decode(36, 3, &data).is_none());
    assert!(decode(36, 2, &data[..23]).is_none());
    assert!(decode(50, 2, &data[..7]).is_none());
}

#[test]
fn measures_actual_core_runtime_and_ready_wait_without_counting_blocked_time() {
    let mut model = model();
    model.ready(10, 10_000);
    model.ready(10, 20_000);
    model.switch(0, 210_000, 10, 99, 5);
    model.switch(0, 310_000, 99, 10, 5);
    // Blocked on IO for 1 second, then ready for 1ms on the faster class.
    model.ready(10, 10_310_000);
    model.switch(1, 10_320_000, 10, 99, 5);
    model.switch(1, 10_520_000, 99, 10, 1);
    let report = model.report(10_530_000);
    let thread = &report["threads"][0];
    assert_eq!(
        thread["runtimeByEfficiencyClassMs"],
        json!([
        {"class": 0, "ms": 10.0}, {"class": 8, "ms": 20.0}])
    );
    assert_eq!(thread["readyWaitTotalMs"], 21.0);
    assert_eq!(thread["readyWaitMaxMs"], 20.0);
    assert_eq!(thread["readyWaitOver16ms"], 1);
    assert_eq!(thread["pendingReadyMsLowerBound"], 1.0);
}

#[test]
fn excludes_partial_intervals_and_marks_broken_event_sequences() {
    let mut model = model();
    model.switch(0, 10, 99, 10, 5);
    model.switch(0, 20, 10, 99, 5);
    model.switch(0, 30, 99, 88, 5);
    model.ready(10, 50);
    model.switch(0, 40, 10, 99, 5);
    let report = model.report(60);
    assert_eq!(report["discontinuities"], 2);
    assert_eq!(report["threads"][0]["observedReadyDispatches"], 0);
    assert_eq!(
        report["threads"][0]["runtimeByEfficiencyClassMs"],
        json!([])
    );
    assert_eq!(report["openRunningSlices"], 1);
}

#[test]
fn preserves_stalled_named_threads_and_bounds_report_size() {
    let threads = (1..=1024)
        .map(|tid| ThreadIdentity {
            pid: 9,
            tid,
            role: if tid == 1024 {
                "renderer-main"
            } else {
                "other"
            },
        })
        .collect();
    let mut model = SchedulerModel::new(threads, BTreeMap::new());
    model.ready(1024, 1);
    let report = model.report(10_000_001);
    assert_eq!(report["threads"].as_array().unwrap().len(), 12);
    assert_eq!(report["threads"][0]["tid"], 1024);
    assert_eq!(report["threads"][0]["pendingReadyMsLowerBound"], 1000.0);
    assert_eq!(report["threadsTruncated"], true);
}

#[test]
fn preempted_threads_wait_until_the_next_dispatch_and_unknown_cpus_stay_unknown() {
    let mut model = model();
    model.switch(0, 0, 10, 99, 5);
    model.switch(0, 10_000, 99, 10, 1);
    model.switch(3, 210_000, 10, 99, 5);
    model.switch(3, 310_000, 99, 10, 5);
    let report = model.report(400_000);
    let thread = &report["threads"][0];
    assert_eq!(thread["readyWaitTotalMs"], 20.0);
    assert_eq!(thread["unknownProcessorMs"], 10.0);
    assert_eq!(report["discontinuities"], 0);
}

#[test]
fn processor_pressure_counts_other_work_without_logging_other_threads_or_idle_time() {
    let mut model = model();
    model.switch(1, 0, 999, 0, 5);
    model.switch(1, 100_000, 0, 999, 5);
    model.switch(1, 900_000, 888, 0, 5);
    model.switch(1, 1_000_000, 0, 888, 5);
    let report = model.report(1_000_000);
    assert_eq!(
        report["processorNonIdleScheduledMs"],
        json!([{"processor": 1, "class": 8, "coveredMs": 100.0, "ms": 20.0}])
    );
    assert_eq!(report["trackedTransitions"], 0);
    assert!(!report.to_string().contains("999"));
    assert!(!report.to_string().contains("888"));
}

#[test]
fn swapped_out_stack_is_not_counted_as_scheduler_ready_wait() {
    let mut model = model();
    model.ready(10, 1);
    model.swapped_out(10);
    model.ready(10, 10_000_000);
    model.switch(0, 10_010_000, 10, 99, 5);
    let report = model.report(10_020_000);
    assert_eq!(report["threads"][0]["readyWaitTotalMs"], 1.0);
    assert_eq!(
        report["threads"][0]["runningAtWindowEnd"][0]["observedMsLowerBound"],
        1.0
    );
}

#[test]
fn cross_cpu_ready_after_same_timestamp_dispatch_is_not_a_new_wait() {
    let mut model = model();
    model.switch(0, 1000, 10, 99, 5);
    model.ready(10, 1000);
    let report = model.report(10_001_000);
    assert!(report["threads"][0]["pendingReadyMsLowerBound"].is_null());
    assert_eq!(
        report["threads"][0]["runningAtWindowEnd"][0]["observedMsLowerBound"],
        1000.0
    );
    assert_eq!(report["discontinuities"], 0);
    model.ready(10, 1001);
    assert_eq!(
        model.discontinuities, 1,
        "contradictory states must invalidate evidence"
    );
}

#[test]
fn busy_compositors_cannot_evict_pending_ready_renderer_or_ui() {
    let mut threads: Vec<_> = (1..=12)
        .map(|tid| ThreadIdentity {
            pid: 9,
            tid,
            role: "compositor",
        })
        .collect();
    threads.push(ThreadIdentity {
        pid: 9,
        tid: 99,
        role: "renderer-main",
    });
    threads.push(ThreadIdentity {
        pid: 9,
        tid: 100,
        role: "fileterm-ui",
    });
    let mut model = SchedulerModel::new(threads, BTreeMap::from([(0, 0)]));
    model.ready(99, 0);
    for tid in 1..=12 {
        let at = u64::from(tid) * 20_000;
        model.switch(0, at, tid, 0, 5);
        model.switch(0, at + 10_000, 0, tid, 5);
    }
    let report = model.report(10_000_000);
    assert_eq!(report["threads"][0]["role"], "fileterm-ui");
    assert_eq!(report["threads"][1]["role"], "renderer-main");
    assert_eq!(report["threads"][1]["pendingReadyMsLowerBound"], 1000.0);
    assert_eq!(report["threadsTruncated"], true);
    assert_eq!(report["availableCpuCapacityKnown"], false);
    assert_eq!(report["interruptTimeAccounted"], false);
    assert!(report.get("processorBusyMs").is_none());
}
