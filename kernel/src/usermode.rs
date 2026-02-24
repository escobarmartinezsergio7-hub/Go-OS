use crate::syscall;

struct ShellState {
    initialized: bool,
}

impl ShellState {
    const fn new() -> Self {
        Self { initialized: false }
    }
}

static mut SHELL_STATE: ShellState = ShellState::new();
static mut APP_INIT: bool = false;

#[inline]
fn sys_write_line(tid: usize, bytes: &[u8]) {
    let _ = syscall::invoke(
        tid,
        syscall::SYS_WRITE_LINE,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
        0,
    );
}

#[inline]
fn sys_clear_lines(tid: usize) {
    let _ = syscall::invoke(tid, syscall::SYS_CLEAR_LINES, 0, 0, 0, 0);
}

#[inline]
fn sys_get_tick(tid: usize) -> u64 {
    syscall::invoke(tid, syscall::SYS_GET_TICK, 0, 0, 0, 0)
}

#[inline]
fn sys_get_runtime_flags(tid: usize) -> u64 {
    syscall::invoke(tid, syscall::SYS_GET_RUNTIME_FLAGS, 0, 0, 0, 0)
}

#[inline]
fn sys_recv_command(tid: usize, out: &mut [u8]) -> usize {
    syscall::invoke(
        tid,
        syscall::SYS_RECV_COMMAND,
        out.as_mut_ptr() as u64,
        out.len() as u64,
        0,
        0,
    ) as usize
}

#[inline]
fn sys_thread_info(tid: usize, index: usize, out: &mut syscall::SysThreadInfo) -> bool {
    syscall::invoke(
        tid,
        syscall::SYS_THREAD_INFO,
        index as u64,
        out as *mut syscall::SysThreadInfo as u64,
        0,
        0,
    ) != 0
}

#[inline]
fn sys_syscall_count(tid: usize, syscall_id: usize) -> u64 {
    syscall::invoke(
        tid,
        syscall::SYS_SYSCALL_COUNT,
        syscall_id as u64,
        0,
        0,
        0,
    )
}

#[inline]
fn sys_priv_status(tid: usize) -> u64 {
    syscall::invoke(tid, syscall::SYS_PRIV_STATUS, 0, 0, 0, 0)
}

#[inline]
fn sys_priv_next(tid: usize) -> u64 {
    syscall::invoke(tid, syscall::SYS_PRIV_NEXT_PHASE, 0, 0, 0, 0)
}

#[inline]
fn sys_priv_unsafe_test(tid: usize) -> u64 {
    syscall::invoke(tid, syscall::SYS_PRIV_UNSAFE_TEST, 0, 0, 0, 0)
}

fn to_upper_byte(b: u8) -> u8 {
    if b.is_ascii_lowercase() {
        b - 32
    } else {
        b
    }
}

fn trim_bounds(buf: &[u8]) -> (usize, usize) {
    let mut start = 0usize;
    while start < buf.len() && buf[start] == b' ' {
        start += 1;
    }

    let mut end = buf.len();
    while end > start && buf[end - 1] == b' ' {
        end -= 1;
    }

    (start, end)
}

fn eq_upper(buf: &[u8], cmd: &[u8]) -> bool {
    if buf.len() != cmd.len() {
        return false;
    }

    let mut i = 0usize;
    while i < cmd.len() {
        if to_upper_byte(buf[i]) != cmd[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn starts_with_upper(buf: &[u8], prefix: &[u8]) -> bool {
    if buf.len() < prefix.len() {
        return false;
    }

    let mut i = 0usize;
    while i < prefix.len() {
        if to_upper_byte(buf[i]) != prefix[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn append_u64(buf: &mut [u8], mut n: usize, mut value: u64) -> usize {
    if n >= buf.len() {
        return n;
    }

    if value == 0 {
        buf[n] = b'0';
        return n + 1;
    }

    let mut tmp = [0u8; 20];
    let mut t = 0usize;
    while value > 0 && t < tmp.len() {
        tmp[t] = b'0' + (value % 10) as u8;
        value /= 10;
        t += 1;
    }

    while t > 0 && n < buf.len() {
        t -= 1;
        buf[n] = tmp[t];
        n += 1;
    }

    n
}

fn append_bytes(buf: &mut [u8], mut n: usize, bytes: &[u8]) -> usize {
    let mut i = 0usize;
    while i < bytes.len() && n < buf.len() {
        buf[n] = bytes[i];
        n += 1;
        i += 1;
    }
    n
}

fn print_status(tid: usize) {
    let flags = sys_get_runtime_flags(tid);
    let running = (flags & 1) != 0;
    let irq_mode = (flags & (1 << 1)) != 0;
    let tick = sys_get_tick(tid);

    if running {
        sys_write_line(tid, b"STATE: RUNNING");
    } else {
        sys_write_line(tid, b"STATE: PAUSED");
    }

    if irq_mode {
        sys_write_line(tid, b"MODE: IRQ");
    } else {
        sys_write_line(tid, b"MODE: POLLING");
    }

    let mut line = [0u8; 64];
    let mut n = 0usize;
    n = append_bytes(&mut line, n, b"TICK: ");
    n = append_u64(&mut line, n, tick);
    sys_write_line(tid, &line[..n]);
}

fn print_ps(tid: usize) {
    sys_write_line(tid, b"PID/TID RING STATE RUNS NAME");

    let mut index = 0usize;
    while index < 32 {
        let mut info = syscall::SysThreadInfo::empty();
        if !sys_thread_info(tid, index, &mut info) {
            break;
        }

        let mut line = [0u8; 96];
        let mut n = 0usize;

        n = append_u64(&mut line, n, info.pid as u64);
        n = append_bytes(&mut line, n, b"/");
        n = append_u64(&mut line, n, info.tid as u64);

        n = append_bytes(&mut line, n, b" R");
        n = append_u64(&mut line, n, info.ring as u64);

        n = append_bytes(&mut line, n, b" S");
        n = append_u64(&mut line, n, info.state as u64);

        n = append_bytes(&mut line, n, b" RUNS ");
        n = append_u64(&mut line, n, info.runs);

        n = append_bytes(&mut line, n, b" ");
        let name_len = (info.name_len as usize).min(info.name.len());
        n = append_bytes(&mut line, n, &info.name[..name_len]);

        sys_write_line(tid, &line[..n]);
        index += 1;
    }
}

fn print_syscalls(tid: usize) {
    let mut id = 0usize;
    while id < syscall::SYS_COUNT {
        let count = sys_syscall_count(tid, id);
        let mut line = [0u8; 48];
        let mut n = 0usize;
        n = append_bytes(&mut line, n, b"SYS[");
        n = append_u64(&mut line, n, id as u64);
        n = append_bytes(&mut line, n, b"] = ");
        n = append_u64(&mut line, n, count);
        sys_write_line(tid, &line[..n]);
        id += 1;
    }
}

fn print_priv_status(tid: usize) {
    let word = sys_priv_status(tid);
    let phase = (word & 0xFF) as u64;
    let test = ((word >> 8) & 0xFF) as u64;

    let mut line = [0u8; 64];
    let mut n = 0usize;
    n = append_bytes(&mut line, n, b"PRIV PHASE: ");
    n = append_u64(&mut line, n, phase);
    sys_write_line(tid, &line[..n]);

    let mut line2 = [0u8; 64];
    let mut n2 = 0usize;
    n2 = append_bytes(&mut line2, n2, b"CPL3 TEST: ");
    n2 = append_u64(&mut line2, n2, test);
    n2 = append_bytes(&mut line2, n2, b" (0=UNK 1=PASS 2=FAIL 3=SAFE)");
    sys_write_line(tid, &line2[..n2]);
}

fn handle_shell_command(tid: usize, cmd: &[u8]) {
    let (start, end) = trim_bounds(cmd);
    if end <= start {
        return;
    }

    let text = &cmd[start..end];

    if eq_upper(text, b"HELP") {
        sys_write_line(tid, b"CMDS: HELP CLEAR ABOUT STATUS ECHO <TXT>");
        sys_write_line(tid, b"CMDS: PS SYSCALLS PRIV PRIV NEXT");
        sys_write_line(tid, b"CMDS: PRIV UNSAFE");
        return;
    }

    if eq_upper(text, b"CLEAR") {
        sys_clear_lines(tid);
        return;
    }

    if eq_upper(text, b"ABOUT") {
        sys_write_line(tid, b"USER SHELL RUNNING OUTSIDE KERNEL LOGIC");
        sys_write_line(tid, b"KERNEL EXPOSES SYSCALL TABLE + PROCESS MODEL");
        return;
    }

    if eq_upper(text, b"STATUS") {
        print_status(tid);
        return;
    }

    if eq_upper(text, b"PS") {
        print_ps(tid);
        return;
    }

    if eq_upper(text, b"SYSCALLS") {
        print_syscalls(tid);
        return;
    }

    if eq_upper(text, b"PRIV") {
        print_priv_status(tid);
        return;
    }

    if eq_upper(text, b"PRIV NEXT") {
        let _ = sys_priv_next(tid);
        print_priv_status(tid);
        return;
    }

    if eq_upper(text, b"PRIV UNSAFE") {
        sys_write_line(tid, b"RUNNING UNSAFE CPL3 TEST...");
        let _ = sys_priv_unsafe_test(tid);
        print_priv_status(tid);
        return;
    }

    if starts_with_upper(text, b"ECHO ") {
        if text.len() > 5 {
            sys_write_line(tid, &text[5..]);
        } else {
            sys_write_line(tid, b"");
        }
        return;
    }

    sys_write_line(tid, b"UNKNOWN COMMAND");
}

pub fn shell_thread_main(tid: usize, _tick: u64) {
    unsafe {
        if !SHELL_STATE.initialized {
            SHELL_STATE.initialized = true;
            sys_write_line(tid, b"RING3 SHELL ONLINE");
            sys_write_line(tid, b"TYPE: HELP");
        }
    }

    let mut processed = 0usize;
    while processed < 4 {
        let mut cmd = [0u8; crate::ui::TERM_MAX_INPUT];
        let n = sys_recv_command(tid, &mut cmd);
        if n == 0 {
            break;
        }
        handle_shell_command(tid, &cmd[..n]);
        processed += 1;
    }
}

pub fn app_idle_thread_main(tid: usize, tick: u64) {
    unsafe {
        if !APP_INIT {
            APP_INIT = true;
            sys_write_line(tid, b"APP THREAD ONLINE");
        }
    }

    if tick != 0 && (tick % 1200) == 0 {
        sys_write_line(tid, b"APP HEARTBEAT");
    }
}
