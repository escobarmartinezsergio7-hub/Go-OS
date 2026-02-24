use crate::usermode;

pub const MAX_PROCESSES: usize = 4;
pub const MAX_THREADS: usize = 8;
pub const NAME_MAX: usize = 16;

pub type ThreadEntry = fn(tid: usize, tick: u64);

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
    state: ThreadState,
    active: bool,
    runs: u64,
    name: [u8; NAME_MAX],
    name_len: u8,
    entry: Option<ThreadEntry>,
}

impl Thread {
    const fn empty() -> Self {
        Self {
            tid: 0,
            pid: 0,
            ring: RingLevel::Kernel,
            state: ThreadState::Dead,
            active: false,
            runs: 0,
            name: [0; NAME_MAX],
            name_len: 0,
            entry: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThreadInfo {
    pub tid: u16,
    pub pid: u16,
    pub ring: RingLevel,
    pub state: ThreadState,
    pub runs: u64,
    pub name: [u8; NAME_MAX],
    pub name_len: u8,
}

struct ProcessManager {
    processes: [Process; MAX_PROCESSES],
    process_count: usize,
    threads: [Thread; MAX_THREADS],
    thread_count: usize,
    cursor: usize,
    dispatches: u64,
}

impl ProcessManager {
    const fn new() -> Self {
        Self {
            processes: [Process::empty(); MAX_PROCESSES],
            process_count: 0,
            threads: [Thread::empty(); MAX_THREADS],
            thread_count: 0,
            cursor: 0,
            dispatches: 0,
        }
    }

    fn reset(&mut self) {
        self.processes = [Process::empty(); MAX_PROCESSES];
        self.process_count = 0;
        self.threads = [Thread::empty(); MAX_THREADS];
        self.thread_count = 0;
        self.cursor = 0;
        self.dispatches = 0;
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
        entry: ThreadEntry,
    ) -> Option<u16> {
        if self.thread_count >= MAX_THREADS || !self.has_process(pid) {
            return None;
        }

        let tid = (self.thread_count + 1) as u16;
        let (name_buf, name_len) = Self::copy_name(name);
        self.threads[self.thread_count] = Thread {
            tid,
            pid,
            ring,
            state: ThreadState::Ready,
            active: true,
            runs: 0,
            name: name_buf,
            name_len,
            entry: Some(entry),
        };
        self.thread_count += 1;
        Some(tid)
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

        let _ = self.add_thread(shell_pid, "shell.main", RingLevel::User, usermode::shell_thread_main);
        let _ = self.add_thread(apps_pid, "apps.idle", RingLevel::User, usermode::app_idle_thread_main);
    }

    fn on_tick(&mut self, tick: u64) {
        if self.thread_count == 0 {
            return;
        }

        let mut scanned = 0usize;
        while scanned < self.thread_count {
            let idx = self.cursor % self.thread_count;
            self.cursor = (self.cursor + 1) % self.thread_count;

            let thread = &mut self.threads[idx];
            if !thread.active || thread.state != ThreadState::Ready {
                scanned += 1;
                continue;
            }

            thread.state = ThreadState::Running;
            thread.runs = thread.runs.saturating_add(1);
            self.dispatches = self.dispatches.saturating_add(1);

            if let Some(entry) = thread.entry {
                entry(idx, tick);
            }

            if thread.active && thread.state == ThreadState::Running {
                thread.state = ThreadState::Ready;
            }
            scanned += 1;
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
            state: t.state,
            runs: t.runs,
            name: t.name,
            name_len: t.name_len,
        })
    }

    fn thread_count(&self) -> usize {
        self.thread_count
    }

    fn dispatches(&self) -> u64 {
        self.dispatches
    }
}

static mut PM: ProcessManager = ProcessManager::new();

pub fn init_user_space() {
    unsafe { PM.init_user_space() };
}

pub fn on_tick(tick: u64) {
    unsafe { PM.on_tick(tick) };
}

pub fn ring_of_thread(thread_index: usize) -> Option<RingLevel> {
    unsafe { PM.ring_of_thread(thread_index) }
}

pub fn thread_info(index: usize) -> Option<ThreadInfo> {
    unsafe { PM.thread_info(index) }
}

pub fn thread_count() -> usize {
    unsafe { PM.thread_count() }
}

pub fn dispatches() -> u64 {
    unsafe { PM.dispatches() }
}
