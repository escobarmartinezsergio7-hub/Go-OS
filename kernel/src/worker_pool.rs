use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::timer;

const WORKER_QUEUE_MAX: usize = 256;
const WORKER_SLOTS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkerJobKind {
    FsIo = 0,
    Install = 1,
    LinuxAbi = 2,
    Gui = 3,
    Net = 4,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum WorkerPriority {
    Interactive = 0,
    Normal = 1,
    Background = 2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkerAffinity {
    Any,
    Core(u8),
}

impl WorkerAffinity {
    fn to_raw(self) -> u8 {
        match self {
            WorkerAffinity::Any => u8::MAX,
            WorkerAffinity::Core(core) => core,
        }
    }

    fn from_raw(raw: u8) -> Self {
        if raw == u8::MAX {
            WorkerAffinity::Any
        } else {
            WorkerAffinity::Core(raw)
        }
    }
}

#[derive(Clone, Copy)]
pub struct WorkerJobRequest {
    pub kind: WorkerJobKind,
    pub priority: WorkerPriority,
    pub affinity: WorkerAffinity,
    pub owner_tag: u32,
    pub payload_id: u64,
    pub deadline_tick: u64, // 0 => no deadline
}

#[derive(Clone, Copy)]
struct WorkerQueuedJob {
    id: u64,
    kind: WorkerJobKind,
    priority: WorkerPriority,
    affinity_raw: u8,
    owner_tag: u32,
    payload_id: u64,
    enqueue_tick: u64,
    deadline_tick: u64,
}

impl WorkerQueuedJob {
    fn affinity(&self) -> WorkerAffinity {
        WorkerAffinity::from_raw(self.affinity_raw)
    }
}

#[derive(Clone, Copy)]
struct WorkerRunningJob {
    id: u64,
    kind: WorkerJobKind,
    priority: WorkerPriority,
    affinity_raw: u8,
    owner_tag: u32,
    payload_id: u64,
    started_tick: u64,
    deadline_tick: u64,
    cancel_requested: bool,
}

impl WorkerRunningJob {
    fn from_queued(job: WorkerQueuedJob, now: u64) -> Self {
        Self {
            id: job.id,
            kind: job.kind,
            priority: job.priority,
            affinity_raw: job.affinity_raw,
            owner_tag: job.owner_tag,
            payload_id: job.payload_id,
            started_tick: now,
            deadline_tick: job.deadline_tick,
            cancel_requested: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkerJobStatus {
    Completed,
    Failed,
    Cancelled,
    Dropped,
}

#[derive(Clone, Copy)]
pub struct WorkerLease {
    pub worker_slot: usize,
    pub job_id: u64,
    pub kind: WorkerJobKind,
    pub priority: WorkerPriority,
    pub affinity: WorkerAffinity,
    pub owner_tag: u32,
    pub payload_id: u64,
    pub started_tick: u64,
    pub deadline_tick: u64,
}

#[derive(Clone, Copy)]
pub struct WorkerPoolSnapshot {
    pub workers: usize,
    pub queued_total: usize,
    pub running_total: usize,
    pub queued_fs_io: usize,
    pub running_fs_io: usize,
    pub enqueued_total: u64,
    pub completed_total: u64,
    pub failed_total: u64,
    pub cancelled_total: u64,
    pub dropped_total: u64,
    pub last_finish_tick: u64,
}

impl WorkerPoolSnapshot {
    const fn empty() -> Self {
        Self {
            workers: WORKER_SLOTS,
            queued_total: 0,
            running_total: 0,
            queued_fs_io: 0,
            running_fs_io: 0,
            enqueued_total: 0,
            completed_total: 0,
            failed_total: 0,
            cancelled_total: 0,
            dropped_total: 0,
            last_finish_tick: 0,
        }
    }
}

struct WorkerPoolState {
    next_id: u64,
    queue: Vec<WorkerQueuedJob>,
    running: [Option<WorkerRunningJob>; WORKER_SLOTS],
    enqueued_total: u64,
    completed_total: u64,
    failed_total: u64,
    cancelled_total: u64,
    dropped_total: u64,
    last_finish_tick: u64,
}

impl WorkerPoolState {
    fn new() -> Self {
        Self {
            next_id: 1,
            queue: Vec::new(),
            running: [None; WORKER_SLOTS],
            enqueued_total: 0,
            completed_total: 0,
            failed_total: 0,
            cancelled_total: 0,
            dropped_total: 0,
            last_finish_tick: 0,
        }
    }

    fn next_job_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn sort_queue(&mut self) {
        self.queue.sort_by(|a, b| match a.priority.cmp(&b.priority) {
            Ordering::Equal => {
                let a_deadline = if a.deadline_tick == 0 {
                    u64::MAX
                } else {
                    a.deadline_tick
                };
                let b_deadline = if b.deadline_tick == 0 {
                    u64::MAX
                } else {
                    b.deadline_tick
                };
                match a_deadline.cmp(&b_deadline) {
                    Ordering::Equal => match a.enqueue_tick.cmp(&b.enqueue_tick) {
                        Ordering::Equal => a.id.cmp(&b.id),
                        ord => ord,
                    },
                    ord => ord,
                }
            }
            ord => ord,
        });
    }

    fn free_worker_slot(&self) -> Option<usize> {
        let mut idx = 0usize;
        while idx < self.running.len() {
            if self.running[idx].is_none() {
                return Some(idx);
            }
            idx += 1;
        }
        None
    }

    fn finish_running(&mut self, job_id: u64, status: WorkerJobStatus) -> bool {
        let mut idx = 0usize;
        while idx < self.running.len() {
            let Some(running) = self.running[idx] else {
                idx += 1;
                continue;
            };
            if running.id != job_id {
                idx += 1;
                continue;
            }
            self.running[idx] = None;
            self.last_finish_tick = timer::ticks();
            match status {
                WorkerJobStatus::Completed => {
                    self.completed_total = self.completed_total.saturating_add(1);
                }
                WorkerJobStatus::Failed => {
                    self.failed_total = self.failed_total.saturating_add(1);
                }
                WorkerJobStatus::Cancelled => {
                    self.cancelled_total = self.cancelled_total.saturating_add(1);
                }
                WorkerJobStatus::Dropped => {
                    self.dropped_total = self.dropped_total.saturating_add(1);
                }
            }
            return true;
        }
        false
    }

    fn cancel_running(&mut self, job_id: u64) -> bool {
        let mut idx = 0usize;
        while idx < self.running.len() {
            if let Some(mut running) = self.running[idx] {
                if running.id == job_id {
                    running.cancel_requested = true;
                    self.running[idx] = Some(running);
                    return true;
                }
            }
            idx += 1;
        }
        false
    }

    fn cancel_running_payload(
        &mut self,
        kind: WorkerJobKind,
        payload_id: u64,
    ) -> bool {
        let mut idx = 0usize;
        while idx < self.running.len() {
            if let Some(mut running) = self.running[idx] {
                if running.kind == kind && running.payload_id == payload_id {
                    running.cancel_requested = true;
                    self.running[idx] = Some(running);
                    return true;
                }
            }
            idx += 1;
        }
        false
    }
}

static mut WORKER_POOL: Option<WorkerPoolState> = None;

fn state_mut() -> &'static mut WorkerPoolState {
    unsafe {
        if WORKER_POOL.is_none() {
            WORKER_POOL = Some(WorkerPoolState::new());
        }
        match WORKER_POOL.as_mut() {
            Some(state) => state,
            None => unreachable!(),
        }
    }
}

fn state_ref() -> &'static WorkerPoolState {
    unsafe {
        if WORKER_POOL.is_none() {
            WORKER_POOL = Some(WorkerPoolState::new());
        }
        match WORKER_POOL.as_ref() {
            Some(state) => state,
            None => unreachable!(),
        }
    }
}

pub fn init() {
    let _ = state_mut();
}

pub fn enqueue(req: WorkerJobRequest) -> Result<u64, &'static str> {
    let state = state_mut();
    if state.queue.len() >= WORKER_QUEUE_MAX {
        return Err("worker queue full");
    }

    let now = timer::ticks();
    let id = state.next_job_id();
    state.queue.push(WorkerQueuedJob {
        id,
        kind: req.kind,
        priority: req.priority,
        affinity_raw: req.affinity.to_raw(),
        owner_tag: req.owner_tag,
        payload_id: req.payload_id,
        enqueue_tick: now,
        deadline_tick: req.deadline_tick,
    });
    state.sort_queue();
    state.enqueued_total = state.enqueued_total.saturating_add(1);
    Ok(id)
}

pub fn claim_next_for_kind(kind: WorkerJobKind) -> Option<WorkerLease> {
    let state = state_mut();
    let worker_slot = state.free_worker_slot()?;
    let now = timer::ticks();

    let mut idx = 0usize;
    while idx < state.queue.len() {
        let queued = state.queue[idx];
        if queued.kind != kind {
            idx += 1;
            continue;
        }
        if queued.deadline_tick != 0 && now > queued.deadline_tick {
            let _ = state.queue.remove(idx);
            state.dropped_total = state.dropped_total.saturating_add(1);
            continue;
        }

        let queued = state.queue.remove(idx);
        let running = WorkerRunningJob::from_queued(queued, now);
        state.running[worker_slot] = Some(running);
        return Some(WorkerLease {
            worker_slot,
            job_id: running.id,
            kind: running.kind,
            priority: running.priority,
            affinity: WorkerAffinity::from_raw(running.affinity_raw),
            owner_tag: running.owner_tag,
            payload_id: running.payload_id,
            started_tick: running.started_tick,
            deadline_tick: running.deadline_tick,
        });
    }
    None
}

pub fn finish(job_id: u64, status: WorkerJobStatus) -> bool {
    let state = state_mut();
    state.finish_running(job_id, status)
}

pub fn cancel(job_id: u64) -> bool {
    let state = state_mut();
    if let Some(idx) = state.queue.iter().position(|job| job.id == job_id) {
        let _ = state.queue.remove(idx);
        state.cancelled_total = state.cancelled_total.saturating_add(1);
        return true;
    }
    state.cancel_running(job_id)
}

pub fn cancel_kind(kind: WorkerJobKind) -> usize {
    let state = state_mut();
    let mut removed = 0usize;
    let mut i = 0usize;
    while i < state.queue.len() {
        if state.queue[i].kind == kind {
            let _ = state.queue.remove(i);
            removed = removed.saturating_add(1);
            state.cancelled_total = state.cancelled_total.saturating_add(1);
            continue;
        }
        i += 1;
    }
    removed
}

pub fn cancel_by_payload(kind: WorkerJobKind, payload_id: u64) -> bool {
    let state = state_mut();
    if let Some(idx) = state
        .queue
        .iter()
        .position(|job| job.kind == kind && job.payload_id == payload_id)
    {
        let _ = state.queue.remove(idx);
        state.cancelled_total = state.cancelled_total.saturating_add(1);
        return true;
    }
    state.cancel_running_payload(kind, payload_id)
}

pub fn is_cancel_requested(job_id: u64) -> bool {
    let state = state_ref();
    let mut idx = 0usize;
    while idx < state.running.len() {
        if let Some(running) = state.running[idx] {
            if running.id == job_id {
                return running.cancel_requested;
            }
        }
        idx += 1;
    }
    false
}

pub fn snapshot() -> WorkerPoolSnapshot {
    let state = state_ref();
    let mut snap = WorkerPoolSnapshot::empty();
    snap.enqueued_total = state.enqueued_total;
    snap.completed_total = state.completed_total;
    snap.failed_total = state.failed_total;
    snap.cancelled_total = state.cancelled_total;
    snap.dropped_total = state.dropped_total;
    snap.last_finish_tick = state.last_finish_tick;

    snap.queued_total = state.queue.len();
    for job in state.queue.iter() {
        if job.kind == WorkerJobKind::FsIo {
            snap.queued_fs_io = snap.queued_fs_io.saturating_add(1);
        }
    }

    let mut idx = 0usize;
    while idx < state.running.len() {
        if let Some(running) = state.running[idx] {
            snap.running_total = snap.running_total.saturating_add(1);
            if running.kind == WorkerJobKind::FsIo {
                snap.running_fs_io = snap.running_fs_io.saturating_add(1);
            }
        }
        idx += 1;
    }
    snap
}
