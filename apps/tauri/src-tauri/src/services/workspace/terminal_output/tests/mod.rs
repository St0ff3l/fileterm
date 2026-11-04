use super::*;

fn full_frame(tab: usize, sequence: usize) -> String {
    let prefix = format!("\x1b[32m{tab}:{sequence}中🔒\x1b[0m\r\n");
    format!("{prefix}{}", "x".repeat(BATCH_BYTES - prefix.len()))
}

#[test]
fn ten_busy_tabs_keep_independent_pending_capacity() {
    let mut output = OutputQueue::default();
    for sequence in 0..8 {
        for tab in 0..10 {
            let prefix = format!("{tab}:{sequence}\r\n");
            let chunk = format!("{prefix}{}", "x".repeat(BATCH_BYTES - prefix.len()));
            assert!(output.enqueue(&tab.to_string(), &chunk));
        }
    }
    assert_eq!(output.batches.len(), 80);
    assert!(output.skipped_bytes.is_empty());
}

#[test]
fn one_saturated_tab_cannot_fill_another_tabs_pending_capacity() {
    let mut output = OutputQueue::default();
    let full = "x".repeat(BATCH_BYTES);
    for _ in 0..8 {
        assert!(output.enqueue("hot", &full));
    }
    assert!(!output.enqueue("hot", "overflow"));
    assert!(output.enqueue("quiet", "prompt> "));
    assert!(!output.skipped_bytes.contains_key("quiet"));
}

#[test]
fn round_robin_keeps_each_tabs_order_and_serves_a_quiet_tab_before_the_hot_backlog() {
    let mut output = OutputQueue::default();
    for sequence in 0..3 {
        assert!(output.enqueue("a", &full_frame(0, sequence)));
    }
    for sequence in 0..2 {
        assert!(output.enqueue("b", &full_frame(1, sequence)));
    }
    assert!(output.enqueue("quiet", "prompt> "));
    for (tab, sequence) in [
        ("a", 0),
        ("b", 0),
        ("quiet", 0),
        ("a", 1),
        ("b", 1),
        ("a", 2),
    ] {
        let batch = output.pop_next().unwrap();
        assert_eq!(batch.tab_id, tab);
        if tab != "quiet" {
            let index = if tab == "a" { 0 } else { 1 };
            assert_eq!(batch.chunk, full_frame(index, sequence));
        } else {
            assert_eq!(batch.chunk, "prompt> ");
        }
    }
    assert!(output.pop_next().is_none());
    assert!(output.ready_tabs.is_empty());
}

#[test]
fn round_robin_tracks_tabs_that_drain_close_and_rejoin() {
    let mut output = OutputQueue::default();
    assert!(output.enqueue("a", &full_frame(0, 0)));
    assert!(output.enqueue("a", &full_frame(0, 1)));
    assert!(output.enqueue("b", &full_frame(1, 0)));
    assert!(output.enqueue("c", "before"));
    assert_eq!(output.pop_next().unwrap().chunk, full_frame(0, 0));

    output.forget_tab("b");
    assert!(output.enqueue("b", "rejoined"));
    assert!(output.enqueue("c", " tail"));
    assert_eq!(output.pop_next().unwrap().chunk, "before tail");
    assert!(output.enqueue("c", "next turn"));
    assert_eq!(output.pop_next().unwrap().chunk, full_frame(0, 1));
    assert!(output.enqueue("a", "new backlog"));
    output.forget_tab("c");
    assert!(output.enqueue("c", "after close"));

    for (tab, chunk) in [
        ("b", "rejoined"),
        ("a", "new backlog"),
        ("c", "after close"),
    ] {
        let batch = output.pop_next().unwrap();
        assert_eq!(batch.tab_id, tab);
        assert_eq!(batch.chunk, chunk);
    }
    assert!(output.pop_next().is_none());
    assert!(output.batches.is_empty());
    assert!(output.ready_tabs.is_empty());
}

#[test]
fn window_capacity_bounds_tiny_streams_and_reuses_a_slot_after_drain() {
    let mut output = OutputQueue::default();
    for tab in 0..PENDING_BATCHES {
        assert!(output.enqueue(&tab.to_string(), "x"));
    }
    assert!(!output.enqueue("overflow", "lost"));
    assert!(output.enqueue("1", "still fits"));
    assert_eq!(output.batches.len(), PENDING_BATCHES);
    assert_eq!(output.ready_tabs.len(), PENDING_BATCHES);
    output.pop_next();
    assert!(output.enqueue("overflow", "recovered"));
    assert_eq!(output.batches.back().unwrap().skipped_bytes_before, 4);
    assert_eq!(output.batches.len(), PENDING_BATCHES);
}

#[test]
fn interleaved_small_chunks_share_bounded_batches_without_changing_tab_order() {
    let mut output = OutputQueue::default();
    let mut expected = HashMap::<String, String>::new();
    for index in 0..1000 {
        for tab in 0..6 {
            let tab_id = tab.to_string();
            let chunk = format!("\x1b[32m{index}中\x1b[0m");
            assert!(output.enqueue(&tab_id, &chunk));
            expected.entry(tab_id).or_default().push_str(&chunk);
        }
    }
    assert_eq!(output.batches.len(), 6);
    assert!(output.skipped_bytes.is_empty());
    for batch in output.batches {
        assert!(batch.chunk.len() <= BATCH_BYTES);
        assert_eq!(batch.skipped_bytes_before, 0);
        assert_eq!(batch.chunk, expected[&batch.tab_id]);
    }
}

#[test]
fn coalescing_never_appends_to_an_older_batch_before_a_full_one() {
    let mut output = OutputQueue::default();
    let full = "b".repeat(BATCH_BYTES);
    for chunk in ["a", &full, "中", "🔒"] {
        assert!(output.enqueue("tab", chunk));
    }
    assert_eq!(output.batches.len(), 3);
    assert_eq!(output.batches[0].chunk, "a");
    assert_eq!(output.batches[1].chunk, full);
    assert_eq!(output.batches[2].chunk, "中🔒");
}

#[test]
fn saturation_stays_bounded_and_never_merges_across_a_display_gap() {
    let mut output = OutputQueue::default();
    let full = "x".repeat(BATCH_BYTES);
    for _ in 0..PENDING_BATCHES_PER_TAB - 1 {
        assert!(output.enqueue("tab", &full));
    }
    assert!(output.enqueue("tab", "head"));
    assert!(!output.enqueue("tab", &full));
    assert!(!output.enqueue("tab", "tail"));
    assert_eq!(output.batches.len(), PENDING_BATCHES_PER_TAB);
    assert_eq!(output.batches.back().unwrap().chunk, "head");
    assert_eq!(output.skipped_bytes["tab"], BATCH_BYTES + "tail".len());

    output.pop_next();
    assert!(output.enqueue("tab", "later"));
    assert!(output.enqueue("tab", "\r\n"));
    assert!(output.skipped_bytes.is_empty());
    let recovered = output.batches.back().unwrap();
    assert_eq!(recovered.chunk, "later\r\n");
    assert_eq!(recovered.skipped_bytes_before, BATCH_BYTES + "tail".len());
    assert!(output
        .batches
        .iter()
        .all(|batch| batch.chunk.len() <= BATCH_BYTES));
}

#[test]
fn another_tab_cannot_consume_a_gap_or_reopen_the_old_batch() {
    let mut output = OutputQueue::default();
    let full = "x".repeat(BATCH_BYTES);
    for index in 0..PENDING_BATCHES - 1 {
        assert!(output.enqueue(&format!("filler-{index}"), &full));
    }
    assert!(output.enqueue("a", "before"));
    assert!(!output.enqueue("a", &full));
    output.pop_next();
    assert!(output.enqueue("b", "unrelated"));
    assert_eq!(output.batches.back().unwrap().skipped_bytes_before, 0);
    assert!(!output.enqueue("a", "also lost"));
    assert_eq!(
        output
            .batches
            .iter()
            .find(|b| b.tab_id == "a")
            .unwrap()
            .chunk,
        "before"
    );

    output.pop_next();
    assert!(output.enqueue("a", "after"));
    let recovered = output.batches.back().unwrap();
    assert_eq!(recovered.tab_id, "a");
    assert_eq!(recovered.chunk, "after");
    assert_eq!(
        recovered.skipped_bytes_before,
        BATCH_BYTES + "also lost".len()
    );

    assert!(!output.enqueue("b", &full));
    assert!(!output.enqueue("a", &full));
    output.pop_next();
    assert!(output.enqueue("b", "b after"));
    assert_eq!(
        output.batches.back().unwrap().skipped_bytes_before,
        BATCH_BYTES
    );
    assert!(output.enqueue("b", "tail"));
    assert!(!output.enqueue("a", "lost again"));
    output.pop_next();
    assert!(output.enqueue("a", "a after"));
    assert_eq!(
        output.batches.back().unwrap().skipped_bytes_before,
        BATCH_BYTES + "lost again".len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_tabs_recover_small_output_bursts_without_skipping_or_reordering() {
    let state = WorkspaceState::default();
    let messages = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received = Arc::clone(&messages);
    let channel = Channel::new(move |body| {
        received.lock().unwrap().push(body.deserialize().unwrap());
        Ok(())
    });
    let channel_id = channel.id();
    state.register_terminal_output_channel("main", channel);
    let mut expected = HashMap::<String, String>::new();
    for index in 0..IN_FLIGHT_BATCHES {
        state.publish_terminal_output("warmup", "initial");
        expected
            .entry("warmup".to_owned())
            .or_default()
            .push_str("initial");
        tokio::time::timeout(Duration::from_secs(1), async {
            while messages.lock().unwrap().len() <= index {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    for tab in 0..10 {
        expected.insert(
            tab.to_string(),
            (0..PENDING_BATCHES_PER_TAB)
                .map(|index| full_frame(tab, index))
                .collect(),
        );
    }
    let start = std::sync::Barrier::new(11);
    std::thread::scope(|scope| {
        for tab in 0..10 {
            let state = &state;
            let start = &start;
            scope.spawn(move || {
                start.wait();
                let tab_id = tab.to_string();
                for index in 0..PENDING_BATCHES_PER_TAB {
                    for chunk in full_frame(tab, index).as_bytes().chunks(256) {
                        state.publish_terminal_output(&tab_id, std::str::from_utf8(chunk).unwrap());
                    }
                }
            });
        }
        start.wait();
    });
    let expected_bytes = expected.values().map(String::len).sum::<usize>();
    assert_eq!(messages.lock().unwrap().len(), IN_FLIGHT_BATCHES);
    let health = state.terminal_output_health().unwrap();
    assert_eq!(health["queuedBatches"], 10 * PENDING_BATCHES_PER_TAB);
    assert_eq!(
        health["queuedBytes"],
        10 * PENDING_BATCHES_PER_TAB * BATCH_BYTES
    );
    assert_eq!(health["queuedTabs"], 10);
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut acknowledged = 0;
        loop {
            let delivered = messages.lock().unwrap().clone();
            for message in &delivered[acknowledged..] {
                state.ack_terminal_output(
                    "main",
                    channel_id,
                    message["deliveryId"].as_u64().unwrap(),
                );
            }
            acknowledged = delivered.len();
            if delivered
                .iter()
                .map(|m| m["chunk"].as_str().unwrap().len())
                .sum::<usize>()
                == expected_bytes
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let messages = messages.lock().unwrap();
    let mut actual = HashMap::<String, String>::new();
    for message in messages.iter() {
        actual
            .entry(message["tabId"].as_str().unwrap().to_owned())
            .or_default()
            .push_str(message["chunk"].as_str().unwrap());
    }
    assert_eq!(actual, expected);
    let replayed = &messages[IN_FLIGHT_BATCHES..];
    assert_eq!(replayed.len(), 10 * PENDING_BATCHES_PER_TAB);
    for round in replayed.chunks(10) {
        let tabs = round
            .iter()
            .map(|message| message["tabId"].as_str().unwrap())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            tabs.len(),
            10,
            "each ready tab gets a turn before the next round"
        );
    }
    assert!(messages.iter().all(|m| m["skippedBytesBefore"] == 0));
    assert!(messages
        .iter()
        .all(|m| m["chunk"].as_str().unwrap().len() <= BATCH_BYTES));
    for subscription in state.terminal_output_channels.lock().unwrap().values() {
        subscription.close();
    }
}

#[test]
fn synchronous_window_command_can_register_without_tokio_context() {
    let state = WorkspaceState::default();
    state.register_terminal_output_channel("main", Channel::new(|_| Ok(())));
    for subscription in state.terminal_output_channels.lock().unwrap().values() {
        subscription.close();
    }
}

#[test]
fn health_sampling_never_waits_on_output_queue() {
    let state = WorkspaceState::default();
    state.register_terminal_output_channel("main", Channel::new(|_| Ok(())));
    let subscription = state
        .terminal_output_channels
        .lock()
        .unwrap()
        .values()
        .next()
        .cloned()
        .unwrap();
    let busy = subscription.output.lock().unwrap();
    let health = state.terminal_output_health().unwrap();
    assert!(health["queuedBatches"].is_null());
    assert!(health["queuedBytes"].is_null());
    assert!(health["queuedTabs"].is_null());
    drop(busy);
    assert_eq!(state.terminal_output_health().unwrap()["queuedBatches"], 0);
    subscription.close();
}

#[test]
fn forgetting_a_tab_clears_its_output_and_gap_in_every_window() {
    let state = WorkspaceState::default();
    for window in ["main", "secondary"] {
        state.register_terminal_output_channel(window, Channel::new(|_| Ok(())));
    }
    let subscriptions = state
        .terminal_output_channels
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for subscription in &subscriptions {
        let mut output = subscription.output.lock().unwrap();
        for _ in 0..PENDING_BATCHES_PER_TAB {
            assert!(output.enqueue("closed", &"x".repeat(BATCH_BYTES)));
        }
        assert!(output.enqueue("other", "keep"));
        assert!(!output.enqueue("closed", "lost"));
    }
    state.forget_terminal_output_tab("closed");
    for subscription in subscriptions {
        let mut output = subscription.output.lock().unwrap();
        assert!(output.skipped_bytes.is_empty());
        assert_eq!(output.batches.len(), 1);
        assert_eq!(output.batches[0].tab_id, "other");
        assert_eq!(output.batches[0].chunk, "keep");
        assert_eq!(output.ready_tabs.len(), 1);
        assert_eq!(output.pop_next().unwrap().tab_id, "other");
        assert!(output.pop_next().is_none());
        drop(output);
        subscription.close();
    }
}

#[tokio::test]
async fn slow_window_does_not_block_other_window() {
    let state = Arc::new(WorkspaceState::default());
    let slow_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let slow_receiver = Arc::clone(&slow_count);
    state.register_terminal_output_channel(
        "slow",
        Channel::new(move |_| {
            slow_receiver.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
    );
    let healthy_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let healthy_receiver = Arc::clone(&healthy_count);
    let receiver_state = Arc::clone(&state);
    let channel_id = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let receiver_id = Arc::clone(&channel_id);
    let channel = Channel::new(move |body| {
        let payload: serde_json::Value = body.deserialize().unwrap();
        receiver_state.ack_terminal_output(
            "main",
            receiver_id.load(Ordering::Relaxed),
            payload["deliveryId"].as_u64().unwrap(),
        );
        healthy_receiver.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    channel_id.store(channel.id(), Ordering::Relaxed);
    state.register_terminal_output_channel("main", channel);
    tokio::time::timeout(Duration::from_secs(5), async {
        for index in 0..2000 {
            state.publish_terminal_output("tab", "output");
            while healthy_count.load(Ordering::Relaxed) <= index {
                tokio::task::yield_now().await;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(healthy_count.load(Ordering::Relaxed), 2000);
    assert_eq!(slow_count.load(Ordering::Relaxed), IN_FLIGHT_BATCHES);
    let health = state.terminal_output_health().unwrap();
    assert_eq!(health["windows"], 2);
    assert_eq!(health["inFlight"], IN_FLIGHT_BATCHES);
    assert!(
        health["queuedBatches"].is_null()
            || health["queuedBatches"].as_u64().unwrap() <= (PENDING_BATCHES * 2) as u64
    );
    let mut channels = state.terminal_output_channels.lock().unwrap();
    assert!(
        state.terminal_output_health().is_none(),
        "diagnostics must never wait on a busy queue"
    );
    for subscription in channels.values() {
        subscription.close();
    }
    channels.clear();
}

#[tokio::test]
async fn shared_device_workers_do_not_wait_for_display_credit() {
    for protocol in ["serial", "telnet"] {
        let state = WorkspaceState::default();
        let messages = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received = Arc::clone(&messages);
        let channel = Channel::new(move |body| {
            received.lock().unwrap().push(body.deserialize().unwrap());
            Ok(())
        });
        let channel_id = channel.id();
        state.register_terminal_output_channel("main", channel);
        let chunk = "x".repeat(BATCH_BYTES);
        for _ in 0..100 {
            state.publish_terminal_output(protocol, &chunk);
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while messages.lock().unwrap().len() < IN_FLIGHT_BATCHES {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(messages.lock().unwrap().len(), IN_FLIGHT_BATCHES);
        let health = state.terminal_output_health().unwrap();
        assert_eq!(health["inFlight"], IN_FLIGHT_BATCHES);
        assert!(health["queuedBatches"].as_u64().unwrap() <= PENDING_BATCHES_PER_TAB as u64);
        assert!(
            health["queuedBytes"].as_u64().unwrap()
                <= (PENDING_BATCHES_PER_TAB * BATCH_BYTES) as u64
        );

        state.ack_terminal_output("main", channel_id, 0);
        tokio::time::timeout(Duration::from_secs(2), async {
            while state.terminal_output_health().unwrap()["queuedBatches"]
                .as_u64()
                .unwrap_or(PENDING_BATCHES as u64)
                >= PENDING_BATCHES_PER_TAB as u64
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        state.publish_terminal_output(protocol, "latest response");
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut acknowledged = 0;
            loop {
                let deliveries = messages.lock().unwrap().clone();
                for payload in &deliveries[acknowledged..] {
                    state.ack_terminal_output(
                        "main",
                        channel_id,
                        payload["deliveryId"].as_u64().unwrap(),
                    );
                }
                acknowledged = deliveries.len();
                if let Some(latest) = deliveries
                    .last()
                    .filter(|p| p["chunk"] == "latest response")
                {
                    assert!(latest["skippedBytesBefore"].as_u64().unwrap() > 0);
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        for subscription in state.terminal_output_channels.lock().unwrap().values() {
            subscription.close();
        }
    }
}

#[tokio::test]
async fn slow_renderer_limits_in_flight_utf8_output_and_rejects_foreign_ack() {
    let state = Arc::new(WorkspaceState::default());
    let messages = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received = Arc::clone(&messages);
    let channel = Channel::new(move |body| {
        received.lock().unwrap().push(body.deserialize().unwrap());
        Ok(())
    });
    let channel_id = channel.id();
    state.register_terminal_output_channel("main", channel);
    let sender = Arc::clone(&state);
    let chunk = "中文🔒".repeat(BATCH_BYTES / 2);
    let expected = chunk.clone();
    let task = tokio::spawn(async move { sender.publish_terminal_output("tab", &chunk) });
    while messages.lock().unwrap().len() < IN_FLIGHT_BATCHES {
        tokio::task::yield_now().await;
    }
    tokio::task::yield_now().await;
    assert!(task.is_finished());
    assert_eq!(messages.lock().unwrap().len(), IN_FLIGHT_BATCHES);
    state.ack_terminal_output("other", channel_id, 0);
    tokio::task::yield_now().await;
    assert_eq!(messages.lock().unwrap().len(), IN_FLIGHT_BATCHES);
    let mut acknowledged = 0;
    while messages
        .lock()
        .unwrap()
        .iter()
        .map(|p| p["chunk"].as_str().unwrap().len())
        .sum::<usize>()
        < expected.len()
    {
        let length = messages.lock().unwrap().len();
        while acknowledged < length {
            state.ack_terminal_output("main", channel_id, acknowledged as u64);
            acknowledged += 1;
        }
        tokio::task::yield_now().await;
    }
    task.await.unwrap();
    let messages = messages.lock().unwrap();
    let actual: String = messages
        .iter()
        .map(|p| p["chunk"].as_str().unwrap())
        .collect();
    assert_eq!(actual, expected);
    assert!(messages
        .iter()
        .all(|p| p["chunk"].as_str().unwrap().len() <= BATCH_BYTES));
}

#[tokio::test]
async fn only_matching_acknowledgements_record_renderer_progress() {
    let state = WorkspaceState::default();
    let channel = Channel::new(|_| Ok(()));
    let channel_id = channel.id();
    state.register_terminal_output_channel("main", channel);
    let subscription = state
        .terminal_output_channels
        .lock()
        .unwrap()
        .get(&channel_id)
        .cloned()
        .unwrap();
    assert_eq!(subscription.last_ack_age_ms(), None);

    state.publish_terminal_output("tab", "output");
    tokio::time::timeout(Duration::from_secs(1), async {
        while subscription.pending.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let delivery_id = *subscription.pending.lock().unwrap().keys().next().unwrap();

    let progress = subscription.progress();
    assert_eq!(progress["issuedBatches"], 1);
    assert_eq!(progress["inFlight"], 1);
    assert_eq!(progress["oldestPendingDeliveryId"], delivery_id);
    assert!(progress["lastAcknowledgedDeliveryId"].is_null());
    state.ack_terminal_output("other", channel_id, delivery_id);
    state.ack_terminal_output("main", channel_id, delivery_id + 1);
    assert_eq!(
        subscription.last_ack_age_ms(),
        None,
        "foreign windows and unknown deliveries must not look like renderer progress"
    );

    state.ack_terminal_output("main", channel_id, delivery_id);
    assert!(subscription.last_ack_age_ms().is_some_and(|age| age < 1000));
    let progress = subscription.progress();
    assert_eq!(progress["lastAcknowledgedDeliveryId"], delivery_id);
    assert_eq!(progress["inFlight"], 0);

    subscription.close();
    state.terminal_output_channels.lock().unwrap().clear();
}

#[tokio::test]
async fn reload_releases_waiters_and_replaces_old_subscription() {
    let state = WorkspaceState::default();
    let channel = Channel::new(|_| Ok(()));
    let id = channel.id();
    state.register_terminal_output_channel("main", channel);
    let previous = Arc::downgrade(
        state
            .terminal_output_channels
            .lock()
            .unwrap()
            .get(&id)
            .unwrap(),
    );
    state.publish_terminal_output("tab", &"x".repeat(BATCH_BYTES * 8));
    loop {
        let pending = previous.upgrade().unwrap().pending.lock().unwrap().len();
        if pending == IN_FLIGHT_BATCHES {
            break;
        }
        tokio::task::yield_now().await;
    }
    state.register_terminal_output_channel("main", Channel::new(|_| Ok(())));
    tokio::time::timeout(Duration::from_secs(1), async {
        while previous.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state.terminal_output_channels.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn reload_discards_old_output_and_rejects_old_acknowledgements() {
    let state = WorkspaceState::default();
    let old_messages = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let received = Arc::clone(&old_messages);
    let old_channel = Channel::new(move |_| {
        received.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let old_id = old_channel.id();
    state.register_terminal_output_channel("main", old_channel);
    let old = Arc::clone(
        state
            .terminal_output_channels
            .lock()
            .unwrap()
            .get(&old_id)
            .unwrap(),
    );
    state.publish_terminal_output("old", &"x".repeat(BATCH_BYTES * PENDING_BATCHES_PER_TAB));
    tokio::time::timeout(Duration::from_secs(1), async {
        while old_messages.load(Ordering::Relaxed) < IN_FLIGHT_BATCHES {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!old.output.lock().unwrap().batches.is_empty());

    let messages = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received = Arc::clone(&messages);
    let channel = Channel::new(move |body| {
        received.lock().unwrap().push(body.deserialize().unwrap());
        Ok(())
    });
    let channel_id = channel.id();
    let start = Arc::new(std::sync::Barrier::new(2));
    let producer_start = Arc::clone(&start);
    let producer_subscription = Arc::clone(&old);
    let producer = std::thread::spawn(move || {
        producer_start.wait();
        for _ in 0..1000 {
            producer_subscription.enqueue("old", "racing output");
        }
    });
    start.wait();
    state.register_terminal_output_channel("main", channel);
    producer.join().unwrap();
    assert!(old.closed.is_cancelled());
    assert!(old.pending.lock().unwrap().is_empty());
    assert!(old.output.lock().unwrap().batches.is_empty());
    assert!(old.output.lock().unwrap().skipped_bytes.is_empty());
    assert_eq!(old_messages.load(Ordering::Relaxed), IN_FLIGHT_BATCHES);
    old.enqueue("old", "late output");
    assert!(old.output.lock().unwrap().batches.is_empty());

    state.publish_terminal_output("new", "fresh output");
    tokio::time::timeout(Duration::from_secs(1), async {
        while messages.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let subscription = Arc::clone(
        state
            .terminal_output_channels
            .lock()
            .unwrap()
            .get(&channel_id)
            .unwrap(),
    );
    assert_eq!(messages.lock().unwrap()[0]["deliveryId"], 0);
    state.ack_terminal_output("main", old_id, 0);
    state.ack_terminal_output("other", channel_id, 0);
    assert_eq!(subscription.pending.lock().unwrap().len(), 1);
    assert_eq!(
        subscription.credit.available_permits(),
        IN_FLIGHT_BATCHES - 1
    );
    state.ack_terminal_output("main", channel_id, 0);
    state.ack_terminal_output("main", channel_id, 0);
    assert!(subscription.pending.lock().unwrap().is_empty());
    assert_eq!(subscription.credit.available_permits(), IN_FLIGHT_BATCHES);
    assert_eq!(messages.lock().unwrap()[0]["chunk"], "fresh output");
    subscription.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reload_while_old_send_is_running_keeps_new_credit_isolated() {
    let state = Arc::new(WorkspaceState::default());
    let entered = Arc::new(Notify::new());
    let sending = Arc::clone(&entered);
    let finished = Arc::new(Notify::new());
    let sent = Arc::clone(&finished);
    let (resume, paused) = std::sync::mpsc::channel();
    let paused = Mutex::new(paused);
    let old_id = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let receiver_id = Arc::clone(&old_id);
    let receiver_state = Arc::clone(&state);
    let old_channel = Channel::new(move |body| {
        let payload: serde_json::Value = body.deserialize().unwrap();
        sending.notify_one();
        paused
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        receiver_state.ack_terminal_output(
            "main",
            receiver_id.load(Ordering::Relaxed),
            payload["deliveryId"].as_u64().unwrap(),
        );
        sent.notify_one();
        Ok(())
    });
    old_id.store(old_channel.id(), Ordering::Relaxed);
    state.register_terminal_output_channel("main", old_channel);
    let previous = Arc::downgrade(
        state
            .terminal_output_channels
            .lock()
            .unwrap()
            .get(&old_id.load(Ordering::Relaxed))
            .unwrap(),
    );
    state.publish_terminal_output("old", &"x".repeat(BATCH_BYTES * PENDING_BATCHES_PER_TAB));
    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .unwrap();

    let messages = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received = Arc::clone(&messages);
    let channel = Channel::new(move |body| {
        received.lock().unwrap().push(body.deserialize().unwrap());
        Ok(())
    });
    let channel_id = channel.id();
    state.register_terminal_output_channel("main", channel);
    {
        let old = previous.upgrade().unwrap();
        assert!(old.credit.is_closed());
        assert!(old.pending.lock().unwrap().is_empty());
        assert!(old.output.lock().unwrap().batches.is_empty());
        assert!(old.output.lock().unwrap().ready_tabs.is_empty());
    }
    state.publish_terminal_output("new", "fresh output");
    tokio::time::timeout(Duration::from_secs(1), async {
        while messages.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    resume.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), finished.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while previous.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let subscription = Arc::clone(
        state
            .terminal_output_channels
            .lock()
            .unwrap()
            .get(&channel_id)
            .unwrap(),
    );
    assert_eq!(messages.lock().unwrap().len(), 1);
    assert_eq!(subscription.pending.lock().unwrap().len(), 1);
    assert_eq!(
        subscription.credit.available_permits(),
        IN_FLIGHT_BATCHES - 1
    );
    state.ack_terminal_output("main", channel_id, 0);
    assert!(subscription.pending.lock().unwrap().is_empty());
    assert_eq!(subscription.credit.available_permits(), IN_FLIGHT_BATCHES);
    subscription.close();
}
