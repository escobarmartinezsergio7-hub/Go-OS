use crate::usermode;
use crate::spinlock::SpinLock;
use core::arch::global_asm;
use core::sync::atomic::{AtomicU32, Ordering};

pub const MAX_PROCESSES: usize = 4;
pub const MAX_THREADS: usize = 8;
pub const NAME_MAX: usize = 16;
const MAX_CORES: usize = crate::per_core::MAX_CORES;
const PRIORITY_LEVELS: usize = 4;
const STARVATION_RELIEF_BASE_TICKS: u64 = 12;
const CORE_BALANCE_INTERVAL_TICKS: u64 = 8;
const KTHREAD_STACK_SIZE: usize = 16 * 1024;
// Context switch asm path is kept in-tree but disabled by default until
// IRQ-mode reentrancy is fully hardened on real hardware.
const ENABLE_KTHREAD_CONTEXT_SWITCH: bool = true;

pub type ThreadEntry = fn(tid: usize, tick: u64);

#[repr(C)]
#[derive(Clone, Copy)]
struct SwitchContext {
    rsp: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    rip: u64,
}

impl SwitchContext {
    const fn empty() -> Self {
        Self {
            rsp: 0,
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rip: 0,
        }
    }
}

#[repr(align(16))]
#[derive(Clone, Copy)]
struct KernelThreadStack([u8; KTHREAD_STACK_SIZE]);

unsafe extern "C" {
    fn process_switch_context(prev: *mut SwitchContext, next: *const SwitchContext);
}

global_asm!(
    r#"
.global process_switch_context
process_switch_context:
    mov [rdi + 0x00], rsp
    mov [rdi + 0x08], r15
    mov [rdi + 0x10], r14
    mov [rdi + 0x18], r13
    mov [rdi + 0x20], r12
    mov [rdi + 0x28], rbx
    mov [rdi + 0x30], rbp
    lea rax, [rip + .Lprocess_switch_resume]
    mov [rdi + 0x38], rax

    mov rsp, [rsi + 0x00]
    mov r15, [rsi + 0x08]
    mov r14, [rsi + 0x10]
    mov r13, [rsi + 0x18]
    mov r12, [rsi + 0x20]
    mov rbx, [rsi + 0x28]
    mov rbp, [rsi + 0x30]
    jmp qword ptr [rsi + 0x38]

.Lprocess_switch_resume:
    ret
"#
);

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RingLevel {
    Kernel = 0,
    User = 3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ThreadState {
    Ready = 0,
    Running = 1,
    Blocked = 2,
    Dead = 3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ThreadPriority {
    Realtime = 0,
    High = 1,
    Normal = 2,
    Background = 3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SchedulerProfile {
    LowLatency = 0,
    Balanced = 1,
    Throughput = 2,
}

impl SchedulerProfile {
    pub const fn name(self) -> &'static str {
        match self {
            SchedulerProfile::LowLatency => "low-latency",
            SchedulerProfile::Balanced => "balanced",
            SchedulerProfile::Throughput => "throughput",
        }
    }

    const fn quantum_ticks(self, priority: ThreadPriority) -> u8 {
        match self {
            SchedulerProfile::LowLatency => match priority {
                ThreadPriority::Realtime => 2,
                ThreadPriority::High => 2,
                ThreadPriority::Normal => 1,
                ThreadPriority::Background => 1,
            },
            SchedulerProfile::Balanced => match priority {
                ThreadPriority::Realtime => 4,
                ThreadPriority::High => 3,
                ThreadPriority::Normal => 2,
                ThreadPriority::Background => 1,
            },
            SchedulerProfile::Throughput => match priority {
                ThreadPriority::Realtime => 6,
                ThreadPriority::High => 4,
                ThreadPriority::Normal => 3,
                ThreadPriority::Background => 2,
            },
        }
    }
}

impl ThreadPriority {
    const fn quantum_ticks(self) -> u8 {
        match self {
            ThreadPriority::Realtime => 4,
            ThreadPriority::High => 3,
            ThreadPriority::Normal => 2,
            ThreadPriority::Background => 1,
        }
    }

    const fn queue_index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy)]
struct Process {
    pid: u16,
    ring: RingLevel,
    active: bool,
    name: [u8; NAME_MAX],
    name_len: u8,
}

impl Process {
    const fn empty() -> Self {
        Self {
            pid: 0,
            ring: RingLevel::Kernel,
            active: false,
            name: [0; NAME_MAX],
            name_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Thread {
    tid: u16,
    pid: u16,
    ring: RingLevel,
    priority: ThreadPriority,
    state: ThreadState,
    active: bool,
    in_runqueue: bool,
    runs: u64,
    quantum_default: u8,
    quantum_left: u8,
    core_id: u8,
    core_affinity: i8, // -1 = any core
    name: [u8; NAME_MAX],
    name_len: u8,
    entry: Option<ThreadEntry>,
    context: SwitchContext,
    stack_base: u64,
    stack_top: u64,
}

impl Thread {
    const fn empty() -> Self {
        Self {
            tid: 0,
            pid: 0,
            ring: RingLevel::Kernel,
            priority: ThreadPriority::Normal,
            state: ThreadState::Dead,
            active: false,
            in_runqueue: false,
            runs: 0,
            quantum_default: 0,
            quantum_left: 0,
            core_id: 0,
            core_affinity: -1,
            name: [0; NAME_MAX],
            name_len: 0,
            entry: None,
            context: SwitchContext::empty(),
            stack_base: 0,
            stack_top: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThreadInfo {
    pub tid: u16,
    pub pid: u16,
    pub ring: RingLevel,
    pub priority: ThreadPriority,
    pub state: ThreadState,
    pub runs: u64,
    pub quantum_default: u8,
    pub quantum_left: u8,
    pub name: [u8; NAME_MAX],
    pub name_len: u8,
}

#[derive(Clone, Copy)]
struct RunQueue {
    entries: [u8; MAX_THREADS],
    head: usize,
    tail: usize,
    count: usize,
}

impl RunQueue {
    const fn new() -> Self {
        Self {
            entries: [0; MAX_THREADS],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn clear(&mut self) {
        self.entries = [0; MAX_THREADS];
        self.head = 0;
        self.tail = 0;
        self.count = 0;
    }

    fn push(&mut self, idx: usize) -> bool {
        if self.count >= MAX_THREADS {
            return false;
        }
        self.entries[self.tail] = idx as u8;
        self.tail = (self.tail + 1) % MAX_THREADS;
        self.count += 1;
        true
    }

    fn pop(&mut self) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let idx = self.entries[self.head] as usize;
        self.head = (self.head + 1) % MAX_THREADS;
        self.count -= 1;
        Some(idx)
    }
}

#[derive(Clone, Copy)]
struct CoreScheduler {
    runqueues: [RunQueue; PRIORITY_LEVELS],
    current_thread: Option<usize>,
    last_accounted_tick: u64,
    dispatches: u64,
    dispatches_by_priority: [u64; PRIORITY_LEVELS],
    last_dispatch_tick_by_priority: [u64; PRIORITY_LEVELS],
    starvation_boosts: u64,
    preemptions: u64,
    forced_preempt_pending: u32,
    resched_pending: u8,
    irq_preempt_injections: u64,
    scheduler_context: SwitchContext,
}

impl CoreScheduler {
    const fn new() -> Self {
        Self {
            runqueues: [RunQueue::new(); PRIORITY_LEVELS],
            current_thread: None,
            last_accounted_tick: 0,
            dispatches: 0,
            dispatches_by_priority: [0; PRIORITY_LEVELS],
            last_dispatch_tick_by_priority: [0; PRIORITY_LEVELS],
            starvation_boosts: 0,
            preemptions: 0,
            forced_preempt_pending: 0,
            resched_pending: 0,
            irq_preempt_injections: 0,
            scheduler_context: SwitchContext::empty(),
        }
    }

    fn reset(&mut self) {
        let mut i = 0usize;
        while i < PRIORITY_LEVELS {
            self.runqueues[i].clear();
            i += 1;
        }
        self.current_thread = None;
        self.last_accounted_tick = 0;
        self.dispatches = 0;
        self.dispatches_by_priority = [0; PRIORITY_LEVELS];
        self.last_dispatch_tick_by_priority = [0; PRIORITY_LEVELS];
        self.starvation_boosts = 0;
        self.preemptions = 0;
        self.forced_preempt_pending = 0;
        self.resched_pending = 0;
        self.irq_preempt_injections = 0;
        self.scheduler_context = SwitchContext::empty();
    }

    fn runqueue_len(&self) -> usize {
        let mut total = 0usize;
        let mut i = 0usize;
        while i < PRIORITY_LEVELS {
            total = total.saturating_add(self.runqueues[i].count);
            i += 1;
        }
        total
    }
}

#[derive(Clone, Copy)]
struct DispatchDecision {
    thread_index: usize,
    entry: Option<ThreadEntry>,
    tick_advanced: bool,
}

struct ProcessManager {
    processes: [Process; MAX_PROCESSES],
    process_count: usize,
    threads: [Thread; MAX_THREADS],
    thread_count: usize,
    profile: SchedulerProfile,
    core_schedulers: [CoreScheduler; MAX_CORES],
    last_balance_tick: u64,
    next_core_hint: usize,
}

impl ProcessManager {
    const fn new() -> Self {
        Self {
            processes: [Process::empty(); MAX_PROCESSES],
            process_count: 0,
            threads: [Thread::empty(); MAX_THREADS],
            thread_count: 0,
            profile: SchedulerProfile::Balanced,
            core_schedulers: [CoreScheduler::new(); MAX_CORES],
            last_balance_tick: 0,
            next_core_hint: 0,
        }
    }

    fn reset(&mut self) {
        self.processes = [Process::empty(); MAX_PROCESSES];
        self.process_count = 0;
        self.threads = [Thread::empty(); MAX_THREADS];
        self.thread_count = 0;
        let mut i = 0usize;
        while i < MAX_CORES {
            self.core_schedulers[i].reset();
            i += 1;
        }
        self.last_balance_tick = 0;
        self.next_core_hint = 0;
        unsafe {
            let mut i = 0usize;
            while i < MAX_CORES {
                PROCESS_ACTIVE_THREAD_INDEX[i] = usize::MAX;
                PROCESS_ACTIVE_TICK[i] = 0;
                KERNEL_PREEMPT_RESUME_RIP[i] = 0;
                KERNEL_PREEMPT_ARMED[i] = 0;
                i += 1;
            }
        }
    }

    fn copy_name(name: &str) -> ([u8; NAME_MAX], u8) {
        let mut out = [0u8; NAME_MAX];
        let bytes = name.as_bytes();
        let n = bytes.len().min(NAME_MAX);
        let mut i = 0usize;
        while i < n {
            out[i] = bytes[i];
            i += 1;
        }
        (out, n as u8)
    }

    fn add_process(&mut self, name: &str, ring: RingLevel) -> Option<u16> {
        if self.process_count >= MAX_PROCESSES {
            return None;
        }
        let pid = (self.process_count + 1) as u16;
        let (name_buf, name_len) = Self::copy_name(name);
        self.processes[self.process_count] = Process {
            pid,
            ring,
            active: true,
            name: name_buf,
            name_len,
        };
        self.process_count += 1;
        Some(pid)
    }

    fn has_process(&self, pid: u16) -> bool {
        let mut i = 0usize;
        while i < self.process_count {
            if self.processes[i].active && self.processes[i].pid == pid {
                return true;
            }
            i += 1;
        }
        false
    }

    fn add_thread(
        &mut self,
        pid: u16,
        name: &str,
        ring: RingLevel,
        priority: ThreadPriority,
        entry: ThreadEntry,
    ) -> Option<u16> {
        if self.thread_count >= MAX_THREADS || !self.has_process(pid) {
            return None;
        }

        let tid = (self.thread_count + 1) as u16;
        let thread_index = self.thread_count;
        let (name_buf, name_len) = Self::copy_name(name);
        let quantum = self.profile.quantum_ticks(priority);
        let (stack_base, stack_top) = Self::thread_stack_bounds(thread_index);
        self.threads[thread_index] = Thread {
            tid,
            pid,
            ring,
            priority,
            state: ThreadState::Ready,
            active: true,
            in_runqueue: false,
            runs: 0,
            quantum_default: quantum,
            quantum_left: quantum,
            core_id: 0,
            core_affinity: -1,
            name: name_buf,
            name_len,
            entry: Some(entry),
            context: Self::seed_thread_context(stack_top),
            stack_base,
            stack_top,
        };
        let idx = thread_index;
        self.thread_count += 1;
        self.enqueue_thread(idx);
        Some(tid)
    }

    fn thread_stack_bounds(thread_index: usize) -> (u64, u64) {
        unsafe {
            let slot = core::ptr::addr_of_mut!(THREAD_STACKS[thread_index]);
            let base = slot as u64;
            let top = base.saturating_add(KTHREAD_STACK_SIZE as u64);
            (base, top)
        }
    }

    fn seed_thread_context(stack_top: u64) -> SwitchContext {
        // x86_64 SysV: function entry expects RSP % 16 == 8.
        let mut rsp = stack_top & !0x0Fu64;
        rsp = rsp.saturating_sub(8);
        SwitchContext {
            rsp,
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rip: process_thread_bootstrap as *const () as usize as u64,
        }
    }

    fn init_user_space(&mut self) {
        self.reset();

        let shell_pid = match self.add_process("shell", RingLevel::User) {
            Some(pid) => pid,
            None => return,
        };

        let apps_pid = match self.add_process("apps", RingLevel::User) {
            Some(pid) => pid,
            None => return,
        };

        let _ = self.add_thread(
            shell_pid,
            "shell.main",
            RingLevel::User,
            ThreadPriority::High,
            usermode::shell_thread_main,
        );
        let _ = self.add_thread(
            apps_pid,
            "apps.idle",
            RingLevel::User,
            ThreadPriority::Background,
            usermode::app_idle_thread_main,
        );
    }

    fn available_core_count(&self) -> usize {
        let mut cores = crate::smp::cpu_count() as usize;
        if cores == 0 {
            cores = 1;
        }
        if cores > MAX_CORES {
            cores = MAX_CORES;
        }
        cores
    }

    fn core_is_schedulable(&self, core_index: usize) -> bool {
        if core_index == 0 {
            return true;
        }
        crate::per_core::core_is_active(core_index)
    }

    fn runqueue_load_for_core(&self, core_index: usize) -> usize {
        let core = &self.core_schedulers[core_index];
        core.runqueue_len()
            .saturating_add(if core.current_thread.is_some() { 1 } else { 0 })
    }

    fn choose_core_for_thread(&mut self, idx: usize) -> usize {
        let cores = self.available_core_count();
        if cores <= 1 {
            return 0;
        }

        let affinity = self.threads[idx].core_affinity;
        if affinity >= 0 {
            let target = affinity as usize;
            if target < cores && self.core_is_schedulable(target) {
                return target;
            }
        }

        let mut best_core = 0usize;
        let mut best_load = usize::MAX;
        let mut i = 0usize;
        while i < cores {
            if !self.core_is_schedulable(i) {
                i += 1;
                continue;
            }
            let load = self.runqueue_load_for_core(i);
            if load < best_load {
                best_load = load;
                best_core = i;
                if best_load == 0 {
                    break;
                }
            }
            i += 1;
        }

        if best_load == usize::MAX {
            // Fallback to round-robin if no active cores detected.
            let core = self.next_core_hint % cores;
            self.next_core_hint = self.next_core_hint.saturating_add(1);
            return core;
        }

        best_core
    }

    fn note_resched(&mut self, core_index: usize) {
        if core_index >= MAX_CORES {
            return;
        }
        let current = crate::smp::current_cpu_index().min(MAX_CORES.saturating_sub(1));
        if core_index == current {
            return;
        }
        if !self.core_is_schedulable(core_index) {
            return;
        }
        let core = &mut self.core_schedulers[core_index];
        if core.resched_pending != 0 {
            return;
        }
        core.resched_pending = 1;
        if core_index != current {
            crate::smp::send_resched_ipi(core_index);
        }
    }

    fn enqueue_thread_on_core(&mut self, idx: usize, core_index: usize) {
        if idx >= self.thread_count || core_index >= MAX_CORES {
            return;
        }

        let thread = &mut self.threads[idx];
        if !thread.active || thread.state != ThreadState::Ready || thread.in_runqueue {
            return;
        }

        let rq = thread.priority.queue_index();
        if rq >= PRIORITY_LEVELS {
            return;
        }

        if self.core_schedulers[core_index].runqueues[rq].push(idx) {
            thread.in_runqueue = true;
            thread.core_id = core_index as u8;
            self.note_resched(core_index);
        }
    }

    fn enqueue_thread(&mut self, idx: usize) {
        if idx >= self.thread_count {
            return;
        }
        let core = self.choose_core_for_thread(idx);
        self.enqueue_thread_on_core(idx, core);
    }

    fn steal_thread_from_core(&mut self, src_core: usize, dst_core: usize) -> bool {
        if src_core >= MAX_CORES || dst_core >= MAX_CORES || src_core == dst_core {
            return false;
        }

        let mut q = PRIORITY_LEVELS;
        while q > 0 {
            q -= 1;
            let mut remaining = self.core_schedulers[src_core].runqueues[q].count;
            while remaining > 0 {
                let idx = match self.core_schedulers[src_core].runqueues[q].pop() {
                    Some(v) => v,
                    None => break,
                };
                self.threads[idx].in_runqueue = false;
                remaining = remaining.saturating_sub(1);

                if idx >= self.thread_count {
                    continue;
                }
                let (active, state, affinity, priority) = {
                    let t = &self.threads[idx];
                    (t.active, t.state, t.core_affinity, t.priority)
                };
                if !active || state != ThreadState::Ready {
                    continue;
                }
                if affinity >= 0 && affinity as usize != dst_core {
                    let _ = self.core_schedulers[src_core].runqueues[q].push(idx);
                    self.threads[idx].in_runqueue = true;
                    continue;
                }

                let dst_queue = priority.queue_index().min(PRIORITY_LEVELS.saturating_sub(1));
                if self.core_schedulers[dst_core].runqueues[dst_queue].push(idx) {
                    let t = &mut self.threads[idx];
                    t.in_runqueue = true;
                    t.core_id = dst_core as u8;
                    self.note_resched(dst_core);
                    return true;
                }

                let _ = self.core_schedulers[src_core].runqueues[q].push(idx);
                self.threads[idx].in_runqueue = true;
            }
        }
        false
    }

    fn maybe_balance_queues(&mut self, tick: u64) {
        if tick.saturating_sub(self.last_balance_tick) < CORE_BALANCE_INTERVAL_TICKS {
            return;
        }
        self.last_balance_tick = tick;

        let cores = self.available_core_count();
        if cores <= 1 {
            return;
        }

        let mut busiest = 0usize;
        let mut least = 0usize;
        let mut max_load = 0usize;
        let mut min_load = usize::MAX;

        let mut i = 0usize;
        while i < cores {
            if !self.core_is_schedulable(i) {
                i += 1;
                continue;
            }
            let load = self.runqueue_load_for_core(i);
            if load > max_load {
                max_load = load;
                busiest = i;
            }
            if load < min_load {
                min_load = load;
                least = i;
            }
            i += 1;
        }

        if max_load <= min_load.saturating_add(1) {
            return;
        }

        let _ = self.steal_thread_from_core(busiest, least);
    }

    fn pop_ready_from_queue(&mut self, core_index: usize, queue_index: usize) -> Option<usize> {
        if core_index >= MAX_CORES || queue_index >= PRIORITY_LEVELS {
            return None;
        }

        let mut remaining = self.core_schedulers[core_index].runqueues[queue_index].count;
        while remaining > 0 {
            let idx = match self.core_schedulers[core_index].runqueues[queue_index].pop() {
                Some(v) => v,
                None => break,
            };
            if idx >= self.thread_count {
                remaining -= 1;
                continue;
            }

            self.threads[idx].in_runqueue = false;
            if !self.threads[idx].active || self.threads[idx].state != ThreadState::Ready {
                remaining -= 1;
                continue;
            }

            if self.threads[idx].quantum_left == 0 {
                self.threads[idx].quantum_left = self.threads[idx].quantum_default;
            }
            self.threads[idx].state = ThreadState::Running;
            self.threads[idx].core_id = core_index as u8;
            return Some(idx);
        }

        None
    }

    fn pick_next_thread(&mut self, core_index: usize, tick: u64) -> Option<usize> {
        if core_index >= MAX_CORES {
            return None;
        }

        // Anti-starvation: if lower-priority queues have been waiting too long,
        // allow one dispatch from them before strict-priority scan.
        let mut q = 1usize;
        while q < PRIORITY_LEVELS {
            if self.core_schedulers[core_index].runqueues[q].count > 0 {
                let last = self.core_schedulers[core_index].last_dispatch_tick_by_priority[q];
                let grace = STARVATION_RELIEF_BASE_TICKS.saturating_mul((q as u64) + 1);
                let overdue = last == 0 || tick.saturating_sub(last) >= grace;
                if overdue {
                    if let Some(idx) = self.pop_ready_from_queue(core_index, q) {
                        self.core_schedulers[core_index].starvation_boosts =
                            self.core_schedulers[core_index]
                                .starvation_boosts
                                .saturating_add(1);
                        return Some(idx);
                    }
                }
            }
            q += 1;
        }

        let mut q = 0usize;
        while q < PRIORITY_LEVELS {
            if let Some(idx) = self.pop_ready_from_queue(core_index, q) {
                return Some(idx);
            }
            q += 1;
        }
        None
    }

    fn sanitize_current(&mut self, core_index: usize) {
        if core_index >= MAX_CORES {
            return;
        }
        let idx = match self.core_schedulers[core_index].current_thread {
            Some(v) => v,
            None => return,
        };
        if idx >= self.thread_count || !self.threads[idx].active {
            self.core_schedulers[core_index].current_thread = None;
            return;
        }

        match self.threads[idx].state {
            ThreadState::Dead | ThreadState::Blocked => {
                self.core_schedulers[core_index].current_thread = None;
            }
            ThreadState::Ready => {
                self.core_schedulers[core_index].current_thread = None;
                self.enqueue_thread_on_core(idx, core_index);
            }
            ThreadState::Running => {}
        }
    }

    fn clear_preempt_hints(&mut self) {
        let mut i = 0usize;
        while i < MAX_CORES {
            self.core_schedulers[i].forced_preempt_pending = 0;
            self.core_schedulers[i].resched_pending = 0;
            i += 1;
        }
        IRQ_PREEMPT_HINTS.store(0, Ordering::SeqCst);
    }

    fn pull_irq_preempt_hints(&mut self, core_index: usize) -> u32 {
        if core_index >= MAX_CORES {
            return 0;
        }
        let pending = IRQ_PREEMPT_HINTS.swap(0, Ordering::SeqCst);
        if pending > 0 {
            let core = &mut self.core_schedulers[core_index];
            // Coalesce IRQ hints to avoid preempt-storm backlog when IRQ frequency is
            // higher than scheduler dispatch cadence.
            core.forced_preempt_pending = core.forced_preempt_pending.saturating_add(1).min(2);
            core.irq_preempt_injections = core.irq_preempt_injections.saturating_add(1);
        }
        pending.min(1)
    }

    fn pull_resched_pending(&mut self, core_index: usize) -> u32 {
        if core_index >= MAX_CORES {
            return 0;
        }
        let core = &mut self.core_schedulers[core_index];
        if core.resched_pending == 0 {
            return 0;
        }
        core.resched_pending = 0;
        core.forced_preempt_pending = core.forced_preempt_pending.saturating_add(1).min(2);
        1
    }

    fn pending_preempt_hints(&self) -> u32 {
        let mut total = IRQ_PREEMPT_HINTS.load(Ordering::SeqCst);
        let mut i = 0usize;
        while i < MAX_CORES {
            total = total.saturating_add(self.core_schedulers[i].forced_preempt_pending);
            total = total.saturating_add(self.core_schedulers[i].resched_pending as u32);
            i += 1;
        }
        total
    }

    fn apply_forced_preempt_once(&mut self, core_index: usize) {
        if core_index >= MAX_CORES {
            return;
        }
        if self.core_schedulers[core_index].forced_preempt_pending == 0 {
            return;
        }

        let Some(idx) = self.core_schedulers[core_index].current_thread else {
            return;
        };
        if idx >= self.thread_count {
            return;
        }

        {
            let thread = &mut self.threads[idx];
            if !thread.active || thread.state != ThreadState::Running {
                return;
            }
            thread.state = ThreadState::Ready;
            thread.quantum_left = thread.quantum_default;
        }

        self.core_schedulers[core_index].current_thread = None;
        self.enqueue_thread_on_core(idx, core_index);
        self.core_schedulers[core_index].forced_preempt_pending =
            self.core_schedulers[core_index].forced_preempt_pending.saturating_sub(1);
        self.core_schedulers[core_index].preemptions =
            self.core_schedulers[core_index]
                .preemptions
                .saturating_add(1);
    }

    fn on_tick_prepare(&mut self, core_index: usize, tick: u64) -> Option<DispatchDecision> {
        if self.thread_count == 0 || core_index >= MAX_CORES {
            return None;
        }
        if !self.core_is_schedulable(core_index) {
            return None;
        }
        if core_index == 0 {
            self.maybe_balance_queues(tick);
        }

        self.sanitize_current(core_index);
        self.pull_resched_pending(core_index);
        self.apply_forced_preempt_once(core_index);
        if self.core_schedulers[core_index].current_thread.is_none() {
            self.core_schedulers[core_index].current_thread =
                self.pick_next_thread(core_index, tick);
        }

        let idx = match self.core_schedulers[core_index].current_thread {
            Some(v) => v,
            None => return None,
        };
        if idx >= self.thread_count {
            self.core_schedulers[core_index].current_thread = None;
            return None;
        }

        let priority_idx;
        {
            let thread = &mut self.threads[idx];
            if !thread.active || thread.state == ThreadState::Dead {
                self.core_schedulers[core_index].current_thread = None;
                return None;
            }
            priority_idx = thread.priority.queue_index().min(PRIORITY_LEVELS.saturating_sub(1));
            thread.state = ThreadState::Running;
            thread.runs = thread.runs.saturating_add(1);
            thread.core_id = core_index as u8;
        }
        let core = &mut self.core_schedulers[core_index];
        core.dispatches = core.dispatches.saturating_add(1);
        core.dispatches_by_priority[priority_idx] =
            core.dispatches_by_priority[priority_idx].saturating_add(1);
        core.last_dispatch_tick_by_priority[priority_idx] = tick;

        let tick_advanced = tick != core.last_accounted_tick;
        let entry = self.threads[idx].entry;
        Some(DispatchDecision {
            thread_index: idx,
            entry,
            tick_advanced,
        })
    }

    fn on_tick_finish(&mut self, core_index: usize, tick: u64, decision: DispatchDecision) {
        if core_index >= MAX_CORES {
            return;
        }
        let idx = decision.thread_index;
        if idx >= self.thread_count {
            self.core_schedulers[core_index].current_thread = None;
            return;
        }

        let mut requeue = false;
        let mut deschedule = false;

        {
            let thread = &mut self.threads[idx];
            match thread.state {
                ThreadState::Dead => {
                    thread.active = false;
                    thread.in_runqueue = false;
                    deschedule = true;
                }
                ThreadState::Blocked => {
                    deschedule = true;
                }
                ThreadState::Ready => {
                    requeue = true;
                    deschedule = true;
                }
                ThreadState::Running => {
                    if decision.tick_advanced && thread.quantum_left > 0 {
                        thread.quantum_left -= 1;
                    }
                    if decision.tick_advanced && thread.quantum_left == 0 {
                        thread.quantum_left = thread.quantum_default;
                        thread.state = ThreadState::Ready;
                        requeue = true;
                        deschedule = true;
                        self.core_schedulers[core_index].preemptions =
                            self.core_schedulers[core_index]
                                .preemptions
                                .saturating_add(1);
                    }
                }
            }
        }

        if decision.tick_advanced {
            self.core_schedulers[core_index].last_accounted_tick = tick;
        }

        if deschedule {
            if self.core_schedulers[core_index].current_thread == Some(idx) {
                self.core_schedulers[core_index].current_thread = None;
            }
            if requeue {
                self.enqueue_thread_on_core(idx, core_index);
            }
        }
    }

    fn ring_of_thread(&self, thread_index: usize) -> Option<RingLevel> {
        if thread_index >= self.thread_count {
            return None;
        }
        Some(self.threads[thread_index].ring)
    }

    fn thread_info(&self, index: usize) -> Option<ThreadInfo> {
        if index >= self.thread_count {
            return None;
        }
        let t = self.threads[index];
        Some(ThreadInfo {
            tid: t.tid,
            pid: t.pid,
            ring: t.ring,
            priority: t.priority,
            state: t.state,
            runs: t.runs,
            quantum_default: t.quantum_default,
            quantum_left: t.quantum_left,
            name: t.name,
            name_len: t.name_len,
        })
    }

    fn thread_count(&self) -> usize {
        self.thread_count
    }

    fn dispatches(&self) -> u64 {
        let mut total = 0u64;
        let mut i = 0usize;
        while i < MAX_CORES {
            total = total.saturating_add(self.core_schedulers[i].dispatches);
            i += 1;
        }
        total
    }

    fn preemptions(&self) -> u64 {
        let mut total = 0u64;
        let mut i = 0usize;
        while i < MAX_CORES {
            total = total.saturating_add(self.core_schedulers[i].preemptions);
            i += 1;
        }
        total
    }

    fn set_profile(&mut self, profile: SchedulerProfile) {
        self.profile = profile;
        let mut i = 0usize;
        while i < self.thread_count {
            if self.threads[i].active {
                let next_q = self.profile.quantum_ticks(self.threads[i].priority).max(1);
                self.threads[i].quantum_default = next_q;
                if self.threads[i].quantum_left == 0 {
                    self.threads[i].quantum_left = next_q;
                } else if self.threads[i].quantum_left > next_q {
                    self.threads[i].quantum_left = next_q;
                }
            }
            i += 1;
        }
    }

    fn profile(&self) -> SchedulerProfile {
        self.profile
    }

    fn starvation_boosts(&self) -> u64 {
        let mut total = 0u64;
        let mut i = 0usize;
        while i < MAX_CORES {
            total = total.saturating_add(self.core_schedulers[i].starvation_boosts);
            i += 1;
        }
        total
    }

    fn dispatches_for_priority(&self, priority: ThreadPriority) -> u64 {
        let idx = priority.queue_index().min(PRIORITY_LEVELS.saturating_sub(1));
        let mut total = 0u64;
        let mut i = 0usize;
        while i < MAX_CORES {
            total = total.saturating_add(self.core_schedulers[i].dispatches_by_priority[idx]);
            i += 1;
        }
        total
    }

    fn irq_preempt_injections(&self) -> u64 {
        let mut total = 0u64;
        let mut i = 0usize;
        while i < MAX_CORES {
            total = total.saturating_add(self.core_schedulers[i].irq_preempt_injections);
            i += 1;
        }
        total
    }

    fn dispatch_thread_once(&mut self, core_index: usize, idx: usize, tick: u64) {
        if core_index >= MAX_CORES || idx >= self.thread_count {
            return;
        }
        let Some(entry) = self.threads[idx].entry else {
            return;
        };

        if !ENABLE_KTHREAD_CONTEXT_SWITCH {
            entry(idx, tick);
            return;
        }
        unsafe {
            PROCESS_ACTIVE_THREAD_INDEX[core_index] = idx;
            PROCESS_ACTIVE_TICK[core_index] = tick;
            process_switch_context(
                &mut self.core_schedulers[core_index].scheduler_context as *mut SwitchContext,
                &self.threads[idx].context as *const SwitchContext,
            );
            PROCESS_ACTIVE_THREAD_INDEX[core_index] = usize::MAX;
        }
    }

    fn context_ptrs(&mut self, core_index: usize, idx: usize) -> Option<(*mut SwitchContext, *const SwitchContext)> {
        if core_index >= MAX_CORES || idx >= self.thread_count {
            return None;
        }
        let prev = &mut self.core_schedulers[core_index].scheduler_context as *mut SwitchContext;
        let next = &self.threads[idx].context as *const SwitchContext;
        Some((prev, next))
    }
}

static PM_LOCK: SpinLock<()> = SpinLock::new(());
static mut PM: ProcessManager = ProcessManager::new();
static IRQ_PREEMPT_HINTS: AtomicU32 = AtomicU32::new(0);
static mut THREAD_STACKS: [KernelThreadStack; MAX_THREADS] =
    [KernelThreadStack([0; KTHREAD_STACK_SIZE]); MAX_THREADS];
static mut PROCESS_ACTIVE_THREAD_INDEX: [usize; MAX_CORES] = [usize::MAX; MAX_CORES];
static mut PROCESS_ACTIVE_TICK: [u64; MAX_CORES] = [0; MAX_CORES];
static mut KERNEL_PREEMPT_RESUME_RIP: [u64; MAX_CORES] = [0; MAX_CORES];
static mut KERNEL_PREEMPT_ARMED: [u8; MAX_CORES] = [0; MAX_CORES];

#[unsafe(no_mangle)]
extern "C" fn process_thread_yield() {
    unsafe {
        let core_index = crate::smp::current_cpu_index().min(MAX_CORES.saturating_sub(1));
        let idx = PROCESS_ACTIVE_THREAD_INDEX[core_index];
        if idx >= PM.thread_count {
            return;
        }
        process_switch_context(
            &mut PM.threads[idx].context as *mut SwitchContext,
            &PM.core_schedulers[core_index].scheduler_context as *const SwitchContext,
        );
    }
}

#[unsafe(no_mangle)]
extern "C" fn process_thread_bootstrap() -> ! {
    loop {
        unsafe {
            let core_index = crate::smp::current_cpu_index().min(MAX_CORES.saturating_sub(1));
            let idx = PROCESS_ACTIVE_THREAD_INDEX[core_index];
            if idx < PM.thread_count {
                if let Some(entry) = PM.threads[idx].entry {
                    entry(idx, PROCESS_ACTIVE_TICK[core_index]);
                }
            }
        }
        process_thread_yield();
    }
}

pub fn init_user_space() {
    {
        let _guard = PM_LOCK.lock();
        unsafe { PM.init_user_space() };
    }
    reset_irq_preempt_hints();
}

pub fn on_tick_core(core_index: usize, tick: u64) {
    let decision = {
        let _guard = PM_LOCK.lock();
        unsafe { PM.on_tick_prepare(core_index, tick) }
    };

    if let Some(decision) = decision {
        if ENABLE_KTHREAD_CONTEXT_SWITCH {
            if decision.entry.is_some() {
                let ctx_pair = {
                    let _guard = PM_LOCK.lock();
                    unsafe { PM.context_ptrs(core_index, decision.thread_index) }
                };
                if let Some((prev_ctx, next_ctx)) = ctx_pair {
                    unsafe {
                        PROCESS_ACTIVE_THREAD_INDEX[core_index] = decision.thread_index;
                        PROCESS_ACTIVE_TICK[core_index] = tick;
                        process_switch_context(prev_ctx, next_ctx);
                        PROCESS_ACTIVE_THREAD_INDEX[core_index] = usize::MAX;
                    }
                }
            }
        } else if let Some(entry) = decision.entry {
            entry(decision.thread_index, tick);
        }
        let _guard = PM_LOCK.lock();
        unsafe { PM.on_tick_finish(core_index, tick, decision) };
    }
}

pub fn on_tick(tick: u64) {
    on_tick_core(0, tick);
}

pub fn ring_of_thread(thread_index: usize) -> Option<RingLevel> {
    let _guard = PM_LOCK.lock();
    unsafe { PM.ring_of_thread(thread_index) }
}

pub fn thread_info(index: usize) -> Option<ThreadInfo> {
    let _guard = PM_LOCK.lock();
    unsafe { PM.thread_info(index) }
}

pub fn thread_count() -> usize {
    let _guard = PM_LOCK.lock();
    unsafe { PM.thread_count() }
}

pub fn dispatches() -> u64 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.dispatches() }
}

pub fn preemptions() -> u64 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.preemptions() }
}

pub fn set_scheduler_profile(profile: SchedulerProfile) {
    let _guard = PM_LOCK.lock();
    unsafe { PM.set_profile(profile) };
}

pub fn scheduler_profile() -> SchedulerProfile {
    let _guard = PM_LOCK.lock();
    unsafe { PM.profile() }
}

pub fn scheduler_profile_name() -> &'static str {
    scheduler_profile().name()
}

pub fn scheduler_starvation_boosts() -> u64 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.starvation_boosts() }
}

pub fn scheduler_dispatches_for_priority(priority: ThreadPriority) -> u64 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.dispatches_for_priority(priority) }
}

pub fn irq_preempt_signal() {
    IRQ_PREEMPT_HINTS.store(1, Ordering::SeqCst);
}

pub fn sync_irq_preempt_hints() -> u32 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.pull_irq_preempt_hints(0) }
}

pub fn pending_irq_preempt_hints() -> u32 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.pending_preempt_hints() }
}

pub fn reset_irq_preempt_hints() {
    let _guard = PM_LOCK.lock();
    unsafe { PM.clear_preempt_hints() };
}

pub fn irq_preempt_injections() -> u64 {
    let _guard = PM_LOCK.lock();
    unsafe { PM.irq_preempt_injections() }
}

#[unsafe(no_mangle)]
extern "C" fn process_irq_preempt_arm(irq_rsp: u64) -> u8 {
    if !ENABLE_KTHREAD_CONTEXT_SWITCH {
        return 0;
    }
    if !crate::syscall::runtime_irq_mode_active() {
        return 0;
    }
    let core_index = crate::smp::current_cpu_index().min(MAX_CORES.saturating_sub(1));
    unsafe {
        if KERNEL_PREEMPT_ARMED[core_index] != 0 {
            return 0;
        }
    }

    let guard = match PM_LOCK.try_lock() {
        Some(g) => g,
        None => return 0,
    };

    let current = unsafe { PM.core_schedulers[core_index].current_thread };
    let idx = match current {
        Some(v) => v,
        None => return 0,
    };
    if idx >= unsafe { PM.thread_count } {
        return 0;
    }
    unsafe {
        let thread = &mut PM.threads[idx];
        if !thread.active || thread.state != ThreadState::Running {
            return 0;
        }
        // Save original RIP from IRQ frame (offset 120 bytes).
        let rip_ptr = (irq_rsp as *const u64).add(15);
        KERNEL_PREEMPT_RESUME_RIP[core_index] = *rip_ptr;
        KERNEL_PREEMPT_ARMED[core_index] = 1;
        thread.state = ThreadState::Ready;
        thread.quantum_left = thread.quantum_default;
        PM.core_schedulers[core_index].preemptions =
            PM.core_schedulers[core_index]
                .preemptions
                .saturating_add(1);
    }
    core::mem::drop(guard);
    1
}

#[unsafe(no_mangle)]
extern "C" fn process_irq_preempt_resume_rip() -> u64 {
    let core_index = crate::smp::current_cpu_index().min(MAX_CORES.saturating_sub(1));
    unsafe {
        let rip = KERNEL_PREEMPT_RESUME_RIP[core_index];
        KERNEL_PREEMPT_RESUME_RIP[core_index] = 0;
        KERNEL_PREEMPT_ARMED[core_index] = 0;
        rip
    }
}
