//! Aggregates only observed scheduler transitions; blocked time is never ready-queue time.
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) enum Transition {
    Ready(u32),
    SwappedOut(u32),
    Switch {
        incoming: u32,
        outgoing: u32,
        state: u8,
    },
}

pub(super) fn decode(opcode: u8, version: u8, data: &[u8]) -> Option<Transition> {
    if version != 2 {
        return None;
    }
    let tid = |start| {
        data.get(start..start + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
    };
    match opcode {
        36 if data.len() >= 24 => Some(Transition::Switch {
            incoming: tid(0)?,
            outgoing: tid(4)?,
            state: data[14],
        }),
        50 if data.len() >= 8 => Some(if data[6] & 0x6 != 0 {
            Transition::SwappedOut(tid(0)?)
        } else {
            Transition::Ready(tid(0)?)
        }),
        _ => None,
    }
}

#[derive(Clone)]
pub(super) struct ThreadIdentity {
    pub pid: u32,
    pub tid: u32,
    pub role: &'static str,
}

struct Thread {
    identity: ThreadIdentity,
    ready_since: Option<u64>,
    runtime: BTreeMap<u16, u64>,
    ready_total: u64,
    ready_max: u64,
    dispatches: u64,
    slow_dispatches: u64,
}

pub(super) struct SchedulerModel {
    threads: BTreeMap<u32, Thread>,
    running: BTreeMap<u16, (u32, u64)>,
    processor_busy: BTreeMap<u16, u64>,
    processor_since: BTreeMap<u16, u64>,
    classes: BTreeMap<u16, u8>,
    pub discontinuities: u64,
    pub tracked_transitions: u64,
}

impl SchedulerModel {
    pub fn new(threads: Vec<ThreadIdentity>, classes: BTreeMap<u16, u8>) -> Self {
        Self {
            threads: threads
                .into_iter()
                .map(|identity| {
                    (
                        identity.tid,
                        Thread {
                            identity,
                            ready_since: None,
                            runtime: BTreeMap::new(),
                            ready_total: 0,
                            ready_max: 0,
                            dispatches: 0,
                            slow_dispatches: 0,
                        },
                    )
                })
                .collect(),
            running: BTreeMap::new(),
            processor_busy: BTreeMap::new(),
            processor_since: BTreeMap::new(),
            classes,
            discontinuities: 0,
            tracked_transitions: 0,
        }
    }

    pub fn ready(&mut self, tid: u32, at: u64) {
        if !self.threads.contains_key(&tid) {
            return;
        }
        if let Some((_, since)) = self.running.values().find(|(running, _)| *running == tid) {
            // Cross-CPU delivery can put a same-timestamp Ready after its dispatch.
            if at > *since {
                self.discontinuities += 1;
            }
            return;
        }
        if let Some(thread) = self.threads.get_mut(&tid) {
            self.tracked_transitions += 1;
            // Duplicate ReadyThread notifications must not shorten an observed wait.
            thread.ready_since.get_or_insert(at);
        }
    }

    pub fn swapped_out(&mut self, tid: u32) {
        if let Some(thread) = self.threads.get_mut(&tid) {
            // A second ReadyThread event follows after the stack/address space is paged in.
            thread.ready_since = None;
        }
    }

    pub fn switch(&mut self, cpu: u16, at: u64, incoming: u32, outgoing: u32, state: u8) {
        self.processor_since.entry(cpu).or_insert(at);
        if self.threads.contains_key(&incoming) || self.threads.contains_key(&outgoing) {
            self.tracked_transitions += 1;
        }
        if let Some((tid, since)) = self.running.remove(&cpu) {
            if tid == outgoing && at >= since {
                if tid != 0 {
                    *self.processor_busy.entry(cpu).or_default() += at - since;
                }
                if let Some(thread) = self.threads.get_mut(&tid) {
                    *thread.runtime.entry(cpu).or_default() += at - since;
                }
            } else {
                self.discontinuities += 1;
            }
        }
        if let Some(thread) = self.threads.get_mut(&outgoing) {
            // KTHREAD_STATE Ready=1: preempted runnable work rejoins the ready queue.
            thread.ready_since = (state == 1).then_some(at);
        }
        if let Some(thread) = self.threads.get_mut(&incoming) {
            if let Some(since) = thread.ready_since.take() {
                if let Some(wait) = at.checked_sub(since) {
                    thread.ready_total += wait;
                    thread.ready_max = thread.ready_max.max(wait);
                    thread.dispatches += 1;
                    thread.slow_dispatches += u64::from(wait >= 160_000);
                } else {
                    self.discontinuities += 1;
                }
            }
        }
        self.running.insert(cpu, (incoming, at));
    }

    pub fn report(&self, end: u64) -> Value {
        let mut threads: Vec<_> = self.threads.values().collect();
        // Preserve named UI/render threads even when stalled and consuming no CPU.
        threads.sort_by_key(|thread| {
            (
                match thread.identity.role {
                    "fileterm-ui" => 0,
                    "renderer-main" => 1,
                    "browser-main" | "gpu-main" => 2,
                    "other" => 4,
                    _ => 3,
                },
                std::cmp::Reverse(
                    thread.runtime.values().sum::<u64>()
                        + thread.ready_total
                        + thread
                            .ready_since
                            .map(|at| end.saturating_sub(at))
                            .unwrap_or(0)
                        + self
                            .running
                            .values()
                            .filter(|(tid, _)| *tid == thread.identity.tid)
                            .map(|(_, at)| end.saturating_sub(*at))
                            .sum::<u64>(),
                ),
                thread.identity.tid,
            )
        });
        let sampled = threads.len();
        let threads: Vec<_> = threads.into_iter().take(12).map(|thread| {
            let mut by_class: BTreeMap<u8, u64> = BTreeMap::new();
            let mut unknown = 0;
            for (cpu, time) in &thread.runtime {
                if let Some(class) = self.classes.get(cpu) {
                    *by_class.entry(*class).or_default() += time;
                } else { unknown += time; }
            }
            json!({
                "pid": thread.identity.pid, "tid": thread.identity.tid, "role": thread.identity.role,
                "runtimeByEfficiencyClassMs": by_class.into_iter().map(|(class, time)|
                    json!({"class": class, "ms": time as f64 / 10_000.0})).collect::<Vec<_>>(),
                "runtimeByProcessorMs": thread.runtime.iter().take(8).map(|(cpu, time)|
                    json!({"processor": cpu, "ms": *time as f64 / 10_000.0})).collect::<Vec<_>>(),
                "processorsTruncated": thread.runtime.len() > 8,
                "unknownProcessorMs": unknown as f64 / 10_000.0,
                "runningAtWindowEnd": self.running.iter().filter(|(_, (tid, _))| *tid == thread.identity.tid)
                    .map(|(cpu, (_, since))| json!({"processor": cpu, "class": self.classes.get(cpu),
                        "observedMsLowerBound": end.saturating_sub(*since) as f64 / 10_000.0})).collect::<Vec<_>>(),
                "readyWaitTotalMs": thread.ready_total as f64 / 10_000.0,
                "readyWaitMaxMs": thread.ready_max as f64 / 10_000.0,
                "observedReadyDispatches": thread.dispatches,
                "readyWaitOver16ms": thread.slow_dispatches,
                "pendingReadyMsLowerBound": thread.ready_since.and_then(|at| end.checked_sub(at))
                    .map(|time| time as f64 / 10_000.0),
            })
        }).collect();
        json!({
            "threads": threads, "trackedThreads": sampled,
            "threadsTruncated": sampled > 12, "discontinuities": self.discontinuities,
            "openRunningSlices": self.running.values().filter(|(tid, _)| self.threads.contains_key(tid)).count(),
            "processorNonIdleScheduledMs": self.processor_since.iter().map(|(cpu, since)| json!({
                "processor": cpu, "class": self.classes.get(cpu),
                "coveredMs": end.saturating_sub(*since) as f64 / 10_000.0,
                "ms": (self.processor_busy.get(cpu).copied().unwrap_or(0)
                    + self.running.get(cpu).filter(|(tid, _)| *tid != 0)
                        .map(|(_, at)| end.saturating_sub(*at)).unwrap_or(0)) as f64 / 10_000.0
            })).collect::<Vec<_>>(),
            "trackedTransitions": self.tracked_transitions,
            "interruptTimeAccounted": false,
            "availableCpuCapacityKnown": false,
            "runtimeInterpretation": "scheduled-residency-includes-interrupts",
        })
    }
}

#[cfg(test)]
mod tests;
