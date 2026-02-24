const MAX_TASKS: usize = 8;

#[derive(Clone, Copy)]
struct Task {
    name: &'static str,
    period_ticks: u64,
    max_runs: u64, // 0 => unlimited
    runs: u64,
    active: bool,
}

impl Task {
    const fn empty() -> Self {
        Self {
            name: "",
            period_ticks: 1,
            max_runs: 0,
            runs: 0,
            active: false,
        }
    }

    const fn demo(name: &'static str, period_ticks: u64, max_runs: u64) -> Self {
        Self {
            name,
            period_ticks,
            max_runs,
            runs: 0,
            active: true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct TaskView {
    pub name: &'static str,
    pub period_ticks: u64,
    pub max_runs: u64,
    pub runs: u64,
    pub active: bool,
}

impl TaskView {
    const fn empty() -> Self {
        Self {
            name: "",
            period_ticks: 1,
            max_runs: 0,
            runs: 0,
            active: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SchedulerSnapshot {
    pub tick: u64,
    pub dispatches: u64,
    pub task_count: usize,
    pub cursor: usize,
    pub tasks: [TaskView; MAX_TASKS],
}

impl SchedulerSnapshot {
    const fn empty() -> Self {
        Self {
            tick: 0,
            dispatches: 0,
            task_count: 0,
            cursor: 0,
            tasks: [TaskView::empty(); MAX_TASKS],
        }
    }
}

struct Scheduler {
    tasks: [Task; MAX_TASKS],
    task_count: usize,
    cursor: usize,
    tick: u64,
    dispatches: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            tasks: [Task::empty(); MAX_TASKS],
            task_count: 0,
            cursor: 0,
            tick: 0,
            dispatches: 0,
        }
    }

    fn reset_demo(&mut self) {
        self.tasks = [Task::empty(); MAX_TASKS];
        self.task_count = 0;
        self.cursor = 0;
        self.tick = 0;
        self.dispatches = 0;

        self.add(Task::demo("idle", 1, 0));
        self.add(Task::demo("input", 2, 0));
        self.add(Task::demo("render", 3, 0));
        self.add(Task::demo("net", 5, 300));
        self.add(Task::demo("audio", 7, 180));
    }

    fn add(&mut self, t: Task) {
        if self.task_count < MAX_TASKS {
            self.tasks[self.task_count] = t;
            self.task_count += 1;
        }
    }

    fn on_tick(&mut self, current_tick: u64) {
        self.tick = current_tick;
        if self.task_count == 0 {
            return;
        }

        for _ in 0..self.task_count {
            let idx = self.cursor % self.task_count;
            self.cursor = (self.cursor + 1) % self.task_count;

            let task = &mut self.tasks[idx];
            if !task.active {
                continue;
            }

            if current_tick % task.period_ticks != 0 {
                continue;
            }

            if task.max_runs != 0 && task.runs >= task.max_runs {
                task.active = false;
                continue;
            }

            task.runs = task.runs.saturating_add(1);
            self.dispatches = self.dispatches.saturating_add(1);

            if task.max_runs != 0 && task.runs >= task.max_runs {
                task.active = false;
            }

            break;
        }
    }

    fn snapshot(&self) -> SchedulerSnapshot {
        let mut snap = SchedulerSnapshot::empty();
        snap.tick = self.tick;
        snap.dispatches = self.dispatches;
        snap.task_count = self.task_count;
        snap.cursor = self.cursor;

        let mut i = 0;
        while i < self.task_count {
            let t = self.tasks[i];
            snap.tasks[i] = TaskView {
                name: t.name,
                period_ticks: t.period_ticks,
                max_runs: t.max_runs,
                runs: t.runs,
                active: t.active,
            };
            i += 1;
        }

        snap
    }
}

static mut SCHEDULER: Scheduler = Scheduler::new();

pub fn init_demo() {
    unsafe { SCHEDULER.reset_demo() };
}

pub fn on_tick(current_tick: u64) {
    unsafe { SCHEDULER.on_tick(current_tick) };
}

pub fn snapshot() -> SchedulerSnapshot {
    unsafe { SCHEDULER.snapshot() }
}
