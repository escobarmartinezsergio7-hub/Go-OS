//! Per-Core state and scheduler infrastructure for SMP.
//!
//! Each CPU core has a `CoreState` containing:
//!   - A local work queue (jobs to execute)
//!   - Statistics (jobs dispatched, idle ticks)
//!   - Current CPU index mapping
//!
//! The `GlobalWorkQueue` is a SpinLock-protected FIFO that any core can push to.
//! Each core pulls work from the global queue when its local queue is empty.

extern crate alloc;

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicBool, Ordering};
use crate::spinlock::SpinLock;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Max cores supported
pub const MAX_CORES: usize = 128;

/// Max jobs in a core's local queue
const LOCAL_QUEUE_SIZE: usize = 16;

/// Max jobs in the global shared queue
const GLOBAL_QUEUE_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Job definition
// ---------------------------------------------------------------------------

/// A unit of work that can be dispatched to any core.
pub type JobFn = fn(arg: u64);

#[derive(Clone, Copy)]
pub struct Job {
    pub func: JobFn,
    pub arg: u64,
    pub priority: u8,      // 0=highest, 3=lowest
    pub affinity: i32,     // -1=any core, >=0=specific core
}

impl Job {
    pub const fn empty() -> Self {
        Self { func: noop_job, arg: 0, priority: 2, affinity: -1 }
    }
}

fn noop_job(_arg: u64) {}

// ---------------------------------------------------------------------------
// Per-core local queue (no lock needed — only accessed by owning core)
// ---------------------------------------------------------------------------

struct LocalQueue {
    jobs: [Job; LOCAL_QUEUE_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl LocalQueue {
    const fn new() -> Self {
        Self {
            jobs: [Job::empty(); LOCAL_QUEUE_SIZE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn push(&mut self, job: Job) -> bool {
        if self.count >= LOCAL_QUEUE_SIZE { return false; }
        self.jobs[self.tail] = job;
        self.tail = (self.tail + 1) % LOCAL_QUEUE_SIZE;
        self.count += 1;
        true
    }

    fn pop(&mut self) -> Option<Job> {
        if self.count == 0 { return None; }
        let job = self.jobs[self.head];
        self.head = (self.head + 1) % LOCAL_QUEUE_SIZE;
        self.count -= 1;
        Some(job)
    }

    fn len(&self) -> usize { self.count }
    fn is_empty(&self) -> bool { self.count == 0 }
}

// ---------------------------------------------------------------------------
// Per-core state
// ---------------------------------------------------------------------------

struct CoreState {
    /// Which core index this is (0 = BSP)
    core_index: u32,
    /// APIC ID of this core
    apic_id: u32,
    /// Whether this core is active and running the scheduler loop
    active: bool,
    /// Local work queue (only this core touches it)
    local_queue: LocalQueue,
    /// Stats
    jobs_dispatched: u64,
    idle_ticks: u64,
}

impl CoreState {
    const fn new() -> Self {
        Self {
            core_index: 0,
            apic_id: 0,
            active: false,
            local_queue: LocalQueue::new(),
            jobs_dispatched: 0,
            idle_ticks: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Global shared work queue
// ---------------------------------------------------------------------------

struct SharedQueue {
    jobs: [Job; GLOBAL_QUEUE_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl SharedQueue {
    const fn new() -> Self {
        Self {
            jobs: [Job::empty(); GLOBAL_QUEUE_SIZE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn push(&mut self, job: Job) -> bool {
        if self.count >= GLOBAL_QUEUE_SIZE { return false; }
        self.jobs[self.tail] = job;
        self.tail = (self.tail + 1) % GLOBAL_QUEUE_SIZE;
        self.count += 1;
        true
    }

    fn pop(&mut self) -> Option<Job> {
        if self.count == 0 { return None; }
        let job = self.jobs[self.head];
        self.head = (self.head + 1) % GLOBAL_QUEUE_SIZE;
        self.count -= 1;
        Some(job)
    }

    fn len(&self) -> usize { self.count }
}

// ---------------------------------------------------------------------------
// Statics
// ---------------------------------------------------------------------------

/// Per-core state array. Each core only writes to its own entry.
/// Safe because each core index is unique. Reads from other cores are
/// for stats only (races acceptable for counters).
static mut CORES: [CoreState; MAX_CORES] = {
    const INIT: CoreState = CoreState::new();
    [INIT; MAX_CORES]
};

/// Number of cores initialized
static CORE_COUNT: AtomicU32 = AtomicU32::new(0);

/// Global shared work queue (SpinLock-protected for cross-core access)
static GLOBAL_QUEUE: SpinLock<SharedQueue> = SpinLock::new(SharedQueue::new());

/// Total jobs enqueued across all time
static TOTAL_JOBS_ENQUEUED: AtomicU64 = AtomicU64::new(0);
/// Total jobs completed across all time  
static TOTAL_JOBS_COMPLETED: AtomicU64 = AtomicU64::new(0);

/// Whether per-core scheduling is initialized
static INITIALIZED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Core registration
// ---------------------------------------------------------------------------

/// Register a core. Called during SMP init. Returns the core index.
pub fn register_core(apic_id: u32, is_bsp: bool) -> usize {
    let idx = CORE_COUNT.fetch_add(1, Ordering::SeqCst) as usize;
    if idx >= MAX_CORES { return idx; }
    unsafe {
        CORES[idx].core_index = idx as u32;
        CORES[idx].apic_id = apic_id;
        CORES[idx].active = is_bsp; // BSP starts active, APs activate later
    }
    idx
}

/// Initialize per-core scheduling. Call after SMP discovery.
pub fn init() {
    if INITIALIZED.load(Ordering::SeqCst) { return; }

    let cpu_count = crate::smp::cpu_count() as usize;
    for i in 0..cpu_count {
        if let Some(cpu) = crate::smp::cpu_info(i) {
            register_core(cpu.apic_id, cpu.is_bsp);
        }
    }

    INITIALIZED.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Job submission API
// ---------------------------------------------------------------------------

/// Enqueue a job. If affinity >= 0, goes directly to that core's local queue.
/// Otherwise goes to the global shared queue for any core to pick up.
pub fn enqueue(job: Job) -> bool {
    if !INITIALIZED.load(Ordering::SeqCst) { init(); }

    TOTAL_JOBS_ENQUEUED.fetch_add(1, Ordering::Relaxed);

    if job.affinity >= 0 {
        let core_idx = job.affinity as usize;
        if core_idx < CORE_COUNT.load(Ordering::SeqCst) as usize {
            unsafe {
                return CORES[core_idx].local_queue.push(job);
            }
        }
    }

    // Push to global queue
    let mut gq = GLOBAL_QUEUE.lock();
    gq.push(job)
}

/// Convenience: enqueue a simple job with no affinity.
pub fn enqueue_simple(func: JobFn, arg: u64, priority: u8) -> bool {
    enqueue(Job { func, arg, priority, affinity: -1 })
}

// ---------------------------------------------------------------------------
// Core scheduler tick (called per-core from the runtime loop)
// ---------------------------------------------------------------------------

/// Process one job on the current core. Returns true if a job was dispatched.
/// Called from the BSP's main loop or from an AP's scheduler loop.
pub fn tick(core_index: usize) -> bool {
    if core_index >= MAX_CORES { return false; }

    // Try local queue first
    let job = unsafe { CORES[core_index].local_queue.pop() };

    let job = match job {
        Some(j) => j,
        None => {
            // Try to steal from global queue
            match GLOBAL_QUEUE.try_lock() {
                Some(mut gq) => {
                    match gq.pop() {
                        Some(j) => j,
                        None => {
                            unsafe { CORES[core_index].idle_ticks += 1; }
                            return false;
                        }
                    }
                }
                None => {
                    // Lock contended — skip this tick
                    unsafe { CORES[core_index].idle_ticks += 1; }
                    return false;
                }
            }
        }
    };

    // Execute the job
    (job.func)(job.arg);

    unsafe { CORES[core_index].jobs_dispatched += 1; }
    TOTAL_JOBS_COMPLETED.fetch_add(1, Ordering::Relaxed);
    true
}

/// Mark a core as active (it's running its scheduler loop).
pub fn activate_core(core_index: usize) {
    if core_index < MAX_CORES {
        unsafe { CORES[core_index].active = true; }
    }
}

// ---------------------------------------------------------------------------
// Status / diagnostics
// ---------------------------------------------------------------------------

pub fn core_count() -> u32 { CORE_COUNT.load(Ordering::SeqCst) }
pub fn total_enqueued() -> u64 { TOTAL_JOBS_ENQUEUED.load(Ordering::Relaxed) }
pub fn total_completed() -> u64 { TOTAL_JOBS_COMPLETED.load(Ordering::Relaxed) }
pub fn global_queue_len() -> usize { GLOBAL_QUEUE.lock().len() }

pub fn core_jobs_dispatched(core_index: usize) -> u64 {
    if core_index >= MAX_CORES { return 0; }
    unsafe { CORES[core_index].jobs_dispatched }
}

pub fn core_idle_ticks(core_index: usize) -> u64 {
    if core_index >= MAX_CORES { return 0; }
    unsafe { CORES[core_index].idle_ticks }
}

pub fn core_local_queue_len(core_index: usize) -> usize {
    if core_index >= MAX_CORES { return 0; }
    unsafe { CORES[core_index].local_queue.len() }
}

pub fn core_is_active(core_index: usize) -> bool {
    if core_index >= MAX_CORES { return false; }
    unsafe { CORES[core_index].active }
}

pub fn core_apic_id(core_index: usize) -> u32 {
    if core_index >= MAX_CORES { return 0; }
    unsafe { CORES[core_index].apic_id }
}

/// Return a formatted status string for the per-core scheduler.
pub fn status_string() -> alloc::string::String {
    use alloc::format;
    use alloc::string::String;

    if !INITIALIZED.load(Ordering::SeqCst) {
        return String::from("Per-core scheduler: not initialized.");
    }

    let cc = core_count() as usize;
    let mut s = format!(
        "Per-core scheduler: {} cores, {} jobs enqueued, {} completed, {} in global queue\n",
        cc, total_enqueued(), total_completed(), global_queue_len()
    );

    for i in 0..cc.min(MAX_CORES) {
        let active = if core_is_active(i) { "active" } else { "idle" };
        s.push_str(&format!(
            "  Core[{}] APIC={}: {} dispatched={} idle={} local_q={}\n",
            i, core_apic_id(i), active,
            core_jobs_dispatched(i), core_idle_ticks(i), core_local_queue_len(i),
        ));
    }
    s
}
