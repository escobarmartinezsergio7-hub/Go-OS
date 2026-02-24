use alloc::alloc::{alloc, dealloc};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::ptr;

use crate::{framebuffer, privilege, process, timer, ui};

pub const SYS_WRITE_LINE: usize = 0;
pub const SYS_CLEAR_LINES: usize = 1;
pub const SYS_GET_TICK: usize = 2;
pub const SYS_GET_RUNTIME_FLAGS: usize = 3;
pub const SYS_RECV_COMMAND: usize = 4;
pub const SYS_THREAD_INFO: usize = 5;
pub const SYS_SYSCALL_COUNT: usize = 6;
pub const SYS_PRIV_STATUS: usize = 7;
pub const SYS_PRIV_NEXT_PHASE: usize = 8;
pub const SYS_PRIV_UNSAFE_TEST: usize = 9;

pub const SYS_COUNT: usize = 10;

pub const SYS_ERR_BAD_SYSCALL: u64 = u64::MAX - 1;
pub const SYS_ERR_BAD_THREAD: u64 = u64::MAX - 2;
pub const SYS_ERR_PERMISSION: u64 = u64::MAX - 3;

const CMD_QUEUE_CAP: usize = 16;
const LINUX_MAX_MMAPS: usize = 64;
const LINUX_MAX_RUNTIME_FILES: usize = 160;
const LINUX_MAX_OPEN_FILES: usize = 48;
const LINUX_MAX_THREADS: usize = 32;
const LINUX_MAX_PROCESSES: usize = 32;
const LINUX_EXITED_QUEUE_CAP: usize = 32;
const LINUX_PAGE_SIZE: u64 = 4096;
const LINUX_BRK_REGION_BYTES: u64 = 64 * 1024 * 1024;
const LINUX_MMAP_BASE: u64 = 0x0000_0007_0000_0000;
const LINUX_MMAP_LIMIT: u64 = 0x0000_000f_0000_0000;
const LINUX_PATH_MAX: usize = 192;
const LINUX_EXECVE_MAX_ARG_ITEMS: usize = 256;
const LINUX_EXECVE_MAX_ENV_ITEMS: usize = 256;
const LINUX_EXECVE_MAX_ITEM_LEN: usize = 4096;
const LINUX_FD_BASE: i32 = 3;
const LINUX_AT_FDCWD: i64 = -100;
const LINUX_RUNTIME_BLOB_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const LINUX_SHIM_WATCHDOG_MAX_CALLS: u64 = 200_000;
const LINUX_SHIM_WATCHDOG_MAX_TICKS: u64 = 12_000;
const LINUX_ERRNO_ETIMEDOUT: i64 = 110;
const LINUX_GFX_MAX_WIDTH: usize = 640;
const LINUX_GFX_MAX_HEIGHT: usize = 360;
const LINUX_GFX_MAX_PIXELS: usize = LINUX_GFX_MAX_WIDTH * LINUX_GFX_MAX_HEIGHT;
const LINUX_GFX_STATUS_MAX: usize = 96;
const LINUX_GFX_EVENT_CAP: usize = 64;
// In real-transfer mode interrupts can be disabled (CLI), so timer ticks may stop.
// Keep direct-present unthrottled to avoid freezing on the first rendered frame.
const LINUX_GFX_DIRECT_PRESENT_MIN_TICKS: u64 = 0;
const LINUX_STAT_MODE_REG: u32 = 0o100644;
const LINUX_STAT_MODE_DIR: u32 = 0o040755;
const LINUX_STAT_MODE_SOCK: u32 = 0o140777;
const LINUX_MAX_EVENTFDS: usize = 32;
const LINUX_MAX_PIPES: usize = 32;
const LINUX_MAX_EPOLLS: usize = 16;
const LINUX_MAX_EPOLL_WATCHES: usize = 64;
const LINUX_MAX_SOCKETS: usize = 48;
const LINUX_SOCKET_RX_BUF: usize = 32768;
const LINUX_MEMFD_PREFIX: &[u8] = b"/memfd/";
const LINUX_X11_MAX_WINDOWS: usize = 96;
const LINUX_X11_MAX_PROPERTIES: usize = 256;
const LINUX_X11_MAX_SELECTIONS: usize = 32;
const LINUX_X11_MAX_PIXMAPS: usize = 12;
const LINUX_X11_MAX_GCS: usize = 128;
const LINUX_X11_PROPERTY_DATA_MAX: usize = 1024;
const LINUX_X11_PIXMAP_SLOT_PIXELS: usize = LINUX_GFX_MAX_PIXELS;
const LINUX_X11_DEFAULT_COLORMAP: u32 = 0x0000_0200;
const LINUX_X11_MAX_SHM_SEGMENTS: usize = 16;

#[derive(Clone, Copy)]
struct LinuxX11ShmSlot {
    active: bool,
    shmseg: u32,
    shmid: u32,
    read_only: bool,
}

impl LinuxX11ShmSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            shmseg: 0,
            shmid: 0,
            read_only: false,
        }
    }
}

const LINUX_SYS_READ: u64 = 0;
const LINUX_SYS_PREAD64: u64 = 17;
const LINUX_SYS_READV: u64 = 19;
const LINUX_SYS_POLL: u64 = 7;
const LINUX_SYS_IOCTL: u64 = 16;
const LINUX_SYS_WRITEV: u64 = 20;
const LINUX_SYS_ACCESS: u64 = 21;
const LINUX_SYS_PIPE: u64 = 22;
const LINUX_SYS_MREMAP: u64 = 25;
const LINUX_SYS_SHMGET: u64 = 29;
const LINUX_SYS_SHMAT: u64 = 30;
const LINUX_SYS_SHMCTL: u64 = 31;
const LINUX_SYS_SCHED_YIELD: u64 = 24;
const LINUX_SYS_DUP: u64 = 32;
const LINUX_SYS_DUP2: u64 = 33;
const LINUX_SYS_MADVISE: u64 = 28;
const LINUX_SYS_NANOSLEEP: u64 = 35;
const LINUX_SYS_FSTAT: u64 = 5;
const LINUX_SYS_LSEEK: u64 = 8;
const LINUX_SYS_MMAP: u64 = 9;
const LINUX_SYS_MPROTECT: u64 = 10;
const LINUX_SYS_CLOSE: u64 = 3;
const LINUX_SYS_MUNMAP: u64 = 11;
const LINUX_SYS_BRK: u64 = 12;
const LINUX_SYS_CLONE: u64 = 56;
const LINUX_SYS_FORK: u64 = 57;
const LINUX_SYS_VFORK: u64 = 58;
const LINUX_SYS_WAIT4: u64 = 61;
const LINUX_SYS_WAITID: u64 = 247;
const LINUX_SYS_CLONE3: u64 = 435;
const LINUX_SYS_SHMDT: u64 = 67;
const LINUX_SYS_MSYNC: u64 = 26;
const LINUX_SYS_MINCORE: u64 = 27;
const LINUX_SYS_RT_SIGACTION: u64 = 13;
const LINUX_SYS_RT_SIGPROCMASK: u64 = 14;
const LINUX_SYS_RT_SIGRETURN: u64 = 15;
const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_SOCKET: u64 = 41;
const LINUX_SYS_CONNECT: u64 = 42;
const LINUX_SYS_ACCEPT: u64 = 43;
const LINUX_SYS_SENDTO: u64 = 44;
const LINUX_SYS_RECVFROM: u64 = 45;
const LINUX_SYS_SENDMSG: u64 = 46;
const LINUX_SYS_RECVMSG: u64 = 47;
const LINUX_SYS_SHUTDOWN: u64 = 48;
const LINUX_SYS_BIND: u64 = 49;
const LINUX_SYS_LISTEN: u64 = 50;
const LINUX_SYS_GETSOCKNAME: u64 = 51;
const LINUX_SYS_GETPEERNAME: u64 = 52;
const LINUX_SYS_SOCKETPAIR: u64 = 53;
const LINUX_SYS_SETSOCKOPT: u64 = 54;
const LINUX_SYS_GETSOCKOPT: u64 = 55;
const LINUX_SYS_GETPID: u64 = 39;
const LINUX_SYS_KILL: u64 = 62;
const LINUX_SYS_UNAME: u64 = 63;
const LINUX_SYS_GETUID: u64 = 102;
const LINUX_SYS_GETGID: u64 = 104;
const LINUX_SYS_SETUID: u64 = 105;
const LINUX_SYS_SETGID: u64 = 106;
const LINUX_SYS_SETPGID: u64 = 109;
const LINUX_SYS_GETPPID: u64 = 110;
const LINUX_SYS_SETRESUID: u64 = 117;
const LINUX_SYS_GETRESUID: u64 = 118;
const LINUX_SYS_SETRESGID: u64 = 119;
const LINUX_SYS_GETRESGID: u64 = 120;
const LINUX_SYS_GETPGID: u64 = 121;
const LINUX_SYS_GETSID: u64 = 124;
const LINUX_SYS_RT_SIGPENDING: u64 = 127;
const LINUX_SYS_RT_SIGSUSPEND: u64 = 130;
const LINUX_SYS_SIGALTSTACK: u64 = 131;
const LINUX_SYS_GETEUID: u64 = 107;
const LINUX_SYS_GETEGID: u64 = 108;
const LINUX_SYS_GETCWD: u64 = 79;
const LINUX_SYS_READLINK: u64 = 89;
const LINUX_SYS_GETTIMEOFDAY: u64 = 96;
const LINUX_SYS_GETRLIMIT: u64 = 97;
const LINUX_SYS_GETRUSAGE: u64 = 98;
const LINUX_SYS_SYSINFO: u64 = 99;
const LINUX_SYS_TIMES: u64 = 100;
const LINUX_SYS_FCNTL: u64 = 72;
const LINUX_SYS_GETDENTS64: u64 = 217;
const LINUX_SYS_PRCTL: u64 = 157;
const LINUX_SYS_SETRLIMIT: u64 = 160;
const LINUX_SYS_ARCH_PRCTL: u64 = 158;
const LINUX_SYS_MLOCK: u64 = 149;
const LINUX_SYS_MUNLOCK: u64 = 150;
const LINUX_SYS_MLOCKALL: u64 = 151;
const LINUX_SYS_MUNLOCKALL: u64 = 152;
const LINUX_SYS_GETTID: u64 = 186;
const LINUX_SYS_SCHED_SETAFFINITY: u64 = 203;
const LINUX_SYS_SCHED_GETAFFINITY: u64 = 204;
const LINUX_SYS_EPOLL_CREATE: u64 = 213;
const LINUX_SYS_SET_TID_ADDRESS: u64 = 218;
const LINUX_SYS_RESTART_SYSCALL: u64 = 219;
const LINUX_SYS_EXIT: u64 = 60;
const LINUX_SYS_FUTEX: u64 = 202;
const LINUX_SYS_EPOLL_CTL: u64 = 233;
const LINUX_SYS_TGKILL: u64 = 234;
const LINUX_SYS_CLOCK_GETTIME: u64 = 228;
const LINUX_SYS_CLOCK_GETRES: u64 = 229;
const LINUX_SYS_CLOCK_NANOSLEEP: u64 = 230;
const LINUX_SYS_EXIT_GROUP: u64 = 231;
const LINUX_SYS_EPOLL_WAIT: u64 = 232;
const LINUX_SYS_EPOLL_PWAIT: u64 = 281;
const LINUX_SYS_EPOLL_PWAIT2: u64 = 441;
const LINUX_SYS_EVENTFD: u64 = 284;
const LINUX_SYS_TIMERFD_CREATE: u64 = 283;
const LINUX_SYS_TIMERFD_SETTIME: u64 = 286;
const LINUX_SYS_TIMERFD_GETTIME: u64 = 287;
const LINUX_SYS_ACCEPT4: u64 = 288;
const LINUX_SYS_EVENTFD2: u64 = 290;
const LINUX_SYS_EPOLL_CREATE1: u64 = 291;
const LINUX_SYS_DUP3: u64 = 292;
const LINUX_SYS_PIPE2: u64 = 293;
const LINUX_SYS_OPENAT: u64 = 257;
const LINUX_SYS_READLINKAT: u64 = 267;
const LINUX_SYS_NEWFSTATAT: u64 = 262;
const LINUX_SYS_FACCESSAT: u64 = 269;
const LINUX_SYS_PPOLL: u64 = 271;
const LINUX_SYS_SET_ROBUST_LIST: u64 = 273;
const LINUX_SYS_GET_ROBUST_LIST: u64 = 274;
const LINUX_SYS_PRLIMIT64: u64 = 302;
const LINUX_SYS_GETCPU: u64 = 309;
const LINUX_SYS_GETRANDOM: u64 = 318;
const LINUX_SYS_MEMFD_CREATE: u64 = 319;
const LINUX_SYS_STATX: u64 = 332;
const LINUX_SYS_RSEQ: u64 = 334;
const LINUX_SYS_EXECVEAT: u64 = 322;
const LINUX_SYS_MEMBARRIER: u64 = 324;
const LINUX_SYS_PIDFD_SEND_SIGNAL: u64 = 424;
const LINUX_SYS_CLOSE_RANGE: u64 = 436;
const LINUX_SYS_OPENAT2: u64 = 437;
const LINUX_SYS_FACCESSAT2: u64 = 439;
const LINUX_SYS_FUTEX_WAITV: u64 = 449;

const LINUX_MAP_SHARED: u64 = 0x01;
const LINUX_MAP_PRIVATE: u64 = 0x02;
const LINUX_MAP_FIXED: u64 = 0x10;
const LINUX_MAP_ANONYMOUS: u64 = 0x20;
const LINUX_MREMAP_MAYMOVE: u64 = 0x1;
const LINUX_MREMAP_FIXED: u64 = 0x2;
const LINUX_IPC_RMID: u64 = 0;
const LINUX_MS_ASYNC: u64 = 0x1;
const LINUX_MS_INVALIDATE: u64 = 0x2;
const LINUX_MS_SYNC: u64 = 0x4;

const LINUX_FUTEX_WAIT: u64 = 0;
const LINUX_FUTEX_WAKE: u64 = 1;
const LINUX_FUTEX_REQUEUE: u64 = 3;
const LINUX_FUTEX_CMP_REQUEUE: u64 = 4;
const LINUX_FUTEX_WAKE_OP: u64 = 5;
const LINUX_FUTEX_LOCK_PI: u64 = 6;
const LINUX_FUTEX_UNLOCK_PI: u64 = 7;
const LINUX_FUTEX_TRYLOCK_PI: u64 = 8;
const LINUX_FUTEX_WAIT_BITSET: u64 = 9;
const LINUX_FUTEX_WAKE_BITSET: u64 = 10;
const LINUX_FUTEX_WAIT_REQUEUE_PI: u64 = 11;
const LINUX_FUTEX_CMP_REQUEUE_PI: u64 = 12;
const LINUX_FUTEX_LOCK_PI2: u64 = 13;
const LINUX_FUTEX_PRIVATE_FLAG: u64 = 128;
const LINUX_FUTEX_CLOCK_REALTIME: u64 = 256;
const LINUX_FUTEX_BITSET_MATCH_ANY: u32 = 0xFFFF_FFFF;
const LINUX_FUTEX_32: u32 = 0x2;
const LINUX_FUTEX_TID_MASK: u32 = 0x3FFF_FFFF;
const LINUX_FUTEX_OWNER_DIED: u32 = 0x4000_0000;
const LINUX_FUTEX_WAITERS: u32 = 0x8000_0000;
const LINUX_FUTEX_WAITV_MAX: usize = 128;
const LINUX_FUTEX_OP_SET: u32 = 0;
const LINUX_FUTEX_OP_ADD: u32 = 1;
const LINUX_FUTEX_OP_OR: u32 = 2;
const LINUX_FUTEX_OP_ANDN: u32 = 3;
const LINUX_FUTEX_OP_XOR: u32 = 4;
const LINUX_FUTEX_OP_ARG_SHIFT: u32 = 8;
const LINUX_FUTEX_OP_CMP_EQ: u32 = 0;
const LINUX_FUTEX_OP_CMP_NE: u32 = 1;
const LINUX_FUTEX_OP_CMP_LT: u32 = 2;
const LINUX_FUTEX_OP_CMP_LE: u32 = 3;
const LINUX_FUTEX_OP_CMP_GT: u32 = 4;
const LINUX_FUTEX_OP_CMP_GE: u32 = 5;
const LINUX_SEEK_SET: u64 = 0;
const LINUX_SEEK_CUR: u64 = 1;
const LINUX_SEEK_END: u64 = 2;

const LINUX_ARCH_SET_FS: u64 = 0x1002;
const LINUX_ARCH_GET_FS: u64 = 0x1003;
const LINUX_CLONE_VM: u64 = 0x0000_0100;
const LINUX_CLONE_FS: u64 = 0x0000_0200;
const LINUX_CLONE_FILES: u64 = 0x0000_0400;
const LINUX_CLONE_SIGHAND: u64 = 0x0000_0800;
const LINUX_CLONE_PTRACE: u64 = 0x0000_2000;
const LINUX_CLONE_VFORK: u64 = 0x0000_4000;
const LINUX_CLONE_PARENT: u64 = 0x0000_8000;
const LINUX_CLONE_THREAD: u64 = 0x0001_0000;
const LINUX_CLONE_NEWNS: u64 = 0x0002_0000;
const LINUX_CLONE_SYSVSEM: u64 = 0x0004_0000;
const LINUX_CLONE_SETTLS: u64 = 0x0008_0000;
const LINUX_CLONE_PARENT_SETTID: u64 = 0x0010_0000;
const LINUX_CLONE_CHILD_CLEARTID: u64 = 0x0020_0000;
const LINUX_CLONE_DETACHED: u64 = 0x0040_0000;
const LINUX_CLONE_UNTRACED: u64 = 0x0080_0000;
const LINUX_CLONE_CHILD_SETTID: u64 = 0x0100_0000;
const LINUX_CLONE_NEWCGROUP: u64 = 0x0200_0000;
const LINUX_CLONE_NEWUTS: u64 = 0x0400_0000;
const LINUX_CLONE_NEWIPC: u64 = 0x0800_0000;
const LINUX_CLONE_NEWUSER: u64 = 0x1000_0000;
const LINUX_CLONE_NEWPID: u64 = 0x2000_0000;
const LINUX_CLONE_NEWNET: u64 = 0x4000_0000;
const LINUX_CLONE_IO: u64 = 0x8000_0000;
const LINUX_CLONE_CLEAR_SIGHAND: u64 = 0x0000_0001_0000_0000;
const LINUX_CLONE_INTO_CGROUP: u64 = 0x0000_0002_0000_0000;
const LINUX_CLONE_PIDFD: u64 = 0x0000_1000;
const LINUX_CLONE_SIGNAL_MASK: u64 = 0xff;

const LINUX_SYS_EXECVE: u64 = 59;
const LINUX_SYS_PIDFD_OPEN: u64 = 434;

const LINUX_CLOCK_REALTIME: u64 = 0;
const LINUX_CLOCK_MONOTONIC: u64 = 1;

const LINUX_THREAD_RUNNABLE: u8 = 1;
const LINUX_THREAD_BLOCKED_FUTEX: u8 = 2;
const LINUX_THREAD_STOPPED: u8 = 3;

const LINUX_ROBUST_LIST_HEAD_LEN_MIN: u64 = 24;
const LINUX_ROBUST_LIST_MAX_NODES: usize = 128;

const LINUX_AF_UNIX: u16 = 1;
const LINUX_AF_INET: u16 = 2;
const LINUX_AF_INET6: u16 = 10;
const LINUX_SOCK_STREAM: u16 = 1;
const LINUX_SOCK_DGRAM: u16 = 2;
const LINUX_SOCK_SEQPACKET: u16 = 5;
const LINUX_SOCK_TYPE_MASK: u64 = 0x0f;
const LINUX_SOCK_NONBLOCK: u64 = 0x0000_0800;
const LINUX_SOCK_CLOEXEC: u64 = 0x0008_0000;
const LINUX_SOCK_FLAGS_MASK: u64 = LINUX_SOCK_NONBLOCK | LINUX_SOCK_CLOEXEC;
const LINUX_SOL_SOCKET: u64 = 1;
const LINUX_SO_TYPE: u64 = 3;
const LINUX_SO_ERROR: u64 = 4;
const LINUX_SO_RCVBUF: u64 = 8;
const LINUX_SO_SNDBUF: u64 = 7;
const LINUX_X11_TCP_PORT_BASE: u16 = 6000;
const LINUX_X11_TCP_PORT_MAX: u16 = 6063;

const LINUX_GETRANDOM_MAX: usize = 256;
const LINUX_UTS_FIELD_LEN: usize = 65;
const LINUX_STDIO_CAPTURE_LIMIT: usize = 4096;
const LINUX_TIOCGWINSZ: u64 = 0x5413;

const LINUX_POLLIN: i16 = 0x0001;
const LINUX_POLLOUT: i16 = 0x0004;
const LINUX_POLLERR: i16 = 0x0008;
const LINUX_POLLHUP: i16 = 0x0010;
const LINUX_POLLNVAL: i16 = 0x0020;
const LINUX_O_NONBLOCK: u64 = 0x0000_0800;
const LINUX_EFD_SEMAPHORE: u64 = 0x0000_0001;
const LINUX_EFD_NONBLOCK: u64 = 0x0000_0800;
const LINUX_EFD_CLOEXEC: u64 = 0x0008_0000;
const LINUX_TFD_TIMER_ABSTIME: u64 = 0x1;
const LINUX_EPOLL_CLOEXEC: u64 = 0x0008_0000;
const LINUX_EPOLL_CTL_ADD: u64 = 1;
const LINUX_EPOLL_CTL_DEL: u64 = 2;
const LINUX_EPOLL_CTL_MOD: u64 = 3;
const LINUX_EPOLLIN: u32 = 0x0000_0001;
const LINUX_EPOLLOUT: u32 = 0x0000_0004;
const LINUX_EPOLLERR: u32 = 0x0000_0008;
const LINUX_EPOLLHUP: u32 = 0x0000_0010;
const LINUX_DUP3_CLOEXEC: u64 = 0x0008_0000;
const LINUX_F_DUPFD: u64 = 0;
const LINUX_F_DUPFD_CLOEXEC: u64 = 1030;
const LINUX_F_GETFD: u64 = 1;
const LINUX_F_SETFD: u64 = 2;
const LINUX_F_GETFL: u64 = 3;
const LINUX_F_SETFL: u64 = 4;
const LINUX_MFD_CLOEXEC: u64 = 0x0001;
const LINUX_SIG_BLOCK: u64 = 0;
const LINUX_SIG_UNBLOCK: u64 = 1;
const LINUX_SIG_SETMASK: u64 = 2;
const LINUX_SIGKILL: u64 = 9;
const LINUX_SIGTERM: u64 = 15;
const LINUX_SIGCONT: u64 = 18;
const LINUX_SIGSTOP: u64 = 19;
const LINUX_SIGTSTP: u64 = 20;
const LINUX_SIGTTIN: u64 = 21;
const LINUX_SIGTTOU: u64 = 22;
const LINUX_SIGCHLD: u64 = 17;
const LINUX_CLD_EXITED: i32 = 1;
const LINUX_CLD_STOPPED: i32 = 5;
const LINUX_CLD_CONTINUED: i32 = 6;
const LINUX_SS_DISABLE: i32 = 2;
const LINUX_MAX_SIGNAL_NUM: usize = 64;
const LINUX_WNOHANG: u64 = 1;
const LINUX_WSTOPPED: u64 = 0x0000_0002;
const LINUX_WEXITED: u64 = 0x0000_0004;
const LINUX_WCONTINUED: u64 = 0x0000_0008;
const LINUX_WNOWAIT: u64 = 0x0100_0000;
const LINUX_P_ALL: u64 = 0;
const LINUX_P_PID: u64 = 1;
const LINUX_P_PGID: u64 = 2;
const LINUX_CHILD_EVENT_EXITED: u8 = 1;
const LINUX_CHILD_EVENT_STOPPED: u8 = 2;
const LINUX_CHILD_EVENT_CONTINUED: u8 = 3;

pub fn linux_syscall_name(sysno: u64) -> &'static str {
    match sysno {
        LINUX_SYS_READ => "read",
        LINUX_SYS_MSYNC => "msync",
        LINUX_SYS_MINCORE => "mincore",
        LINUX_SYS_WRITE => "write",
        LINUX_SYS_CLOSE => "close",
        LINUX_SYS_POLL => "poll",
        LINUX_SYS_LSEEK => "lseek",
        LINUX_SYS_MMAP => "mmap",
        LINUX_SYS_MPROTECT => "mprotect",
        LINUX_SYS_MUNMAP => "munmap",
        LINUX_SYS_BRK => "brk",
        LINUX_SYS_IOCTL => "ioctl",
        LINUX_SYS_READV => "readv",
        LINUX_SYS_WRITEV => "writev",
        LINUX_SYS_NANOSLEEP => "nanosleep",
        LINUX_SYS_CLOCK_GETRES => "clock_getres",
        LINUX_SYS_DUP => "dup",
        LINUX_SYS_DUP2 => "dup2",
        LINUX_SYS_GETPID => "getpid",
        LINUX_SYS_GETPGID => "getpgid",
        LINUX_SYS_GETSID => "getsid",
        LINUX_SYS_GETPPID => "getppid",
        LINUX_SYS_SETUID => "setuid",
        LINUX_SYS_SETGID => "setgid",
        LINUX_SYS_SETPGID => "setpgid",
        LINUX_SYS_SETRESUID => "setresuid",
        LINUX_SYS_SETRESGID => "setresgid",
        LINUX_SYS_FCNTL => "fcntl",
        LINUX_SYS_GETCWD => "getcwd",
        LINUX_SYS_READLINK => "readlink",
        LINUX_SYS_SYSINFO => "sysinfo",
        LINUX_SYS_GETRUSAGE => "getrusage",
        LINUX_SYS_TIMES => "times",
        LINUX_SYS_OPENAT => "openat",
        LINUX_SYS_OPENAT2 => "openat2",
        LINUX_SYS_NEWFSTATAT => "newfstatat",
        LINUX_SYS_FACCESSAT => "faccessat",
        LINUX_SYS_FACCESSAT2 => "faccessat2",
        LINUX_SYS_EXECVEAT => "execveat",
        LINUX_SYS_CLOSE_RANGE => "close_range",
        LINUX_SYS_PIDFD_SEND_SIGNAL => "pidfd_send_signal",
        LINUX_SYS_MEMBARRIER => "membarrier",
        LINUX_SYS_PPOLL => "ppoll",
        LINUX_SYS_GETRANDOM => "getrandom",
        LINUX_SYS_EPOLL_WAIT => "epoll_wait",
        LINUX_SYS_EPOLL_PWAIT => "epoll_pwait",
        LINUX_SYS_EPOLL_PWAIT2 => "epoll_pwait2",
        LINUX_SYS_EPOLL_CTL => "epoll_ctl",
        LINUX_SYS_EVENTFD => "eventfd",
        LINUX_SYS_EVENTFD2 => "eventfd2",
        LINUX_SYS_TIMERFD_CREATE => "timerfd_create",
        LINUX_SYS_TIMERFD_SETTIME => "timerfd_settime",
        LINUX_SYS_TIMERFD_GETTIME => "timerfd_gettime",
        LINUX_SYS_SOCKET => "socket",
        LINUX_SYS_CONNECT => "connect",
        LINUX_SYS_ACCEPT => "accept",
        LINUX_SYS_ACCEPT4 => "accept4",
        LINUX_SYS_BIND => "bind",
        LINUX_SYS_LISTEN => "listen",
        LINUX_SYS_SENDTO => "sendto",
        LINUX_SYS_RECVFROM => "recvfrom",
        LINUX_SYS_SENDMSG => "sendmsg",
        LINUX_SYS_RECVMSG => "recvmsg",
        LINUX_SYS_SHUTDOWN => "shutdown",
        LINUX_SYS_CLONE => "clone",
        LINUX_SYS_CLONE3 => "clone3",
        LINUX_SYS_SETRLIMIT => "setrlimit",
        LINUX_SYS_MLOCK => "mlock",
        LINUX_SYS_MUNLOCK => "munlock",
        LINUX_SYS_MLOCKALL => "mlockall",
        LINUX_SYS_MUNLOCKALL => "munlockall",
        LINUX_SYS_SET_TID_ADDRESS => "set_tid_address",
        LINUX_SYS_FUTEX => "futex",
        LINUX_SYS_FUTEX_WAITV => "futex_waitv",
        LINUX_SYS_TGKILL => "tgkill",
        LINUX_SYS_EXIT => "exit",
        LINUX_SYS_EXIT_GROUP => "exit_group",
        LINUX_SYS_STATX => "statx",
        _ => "unknown",
    }
}

pub fn linux_errno_name(errno: i64) -> &'static str {
    match errno {
        0 => "OK",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        9 => "EBADF",
        10 => "ECHILD",
        11 => "EAGAIN",
        12 => "ENOMEM",
        14 => "EFAULT",
        22 => "EINVAL",
        24 => "EMFILE",
        25 => "ENOTTY",
        29 => "ESPIPE",
        32 => "EPIPE",
        34 => "ERANGE",
        36 => "ENAMETOOLONG",
        38 => "ENOSYS",
        88 => "ENOTSOCK",
        95 => "EOPNOTSUPP",
        97 => "EAFNOSUPPORT",
        101 => "ENETUNREACH",
        106 => "EISCONN",
        107 => "ENOTCONN",
        110 => "ETIMEDOUT",
        _ => "EUNKNOWN",
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SysThreadInfo {
    pub tid: u16,
    pub pid: u16,
    pub ring: u8,
    pub state: u8,
    pub name_len: u8,
    pub _pad: [u8; 3],
    pub runs: u64,
    pub name: [u8; process::NAME_MAX],
}

impl SysThreadInfo {
    pub const fn empty() -> Self {
        Self {
            tid: 0,
            pid: 0,
            ring: 0,
            state: 0,
            name_len: 0,
            _pad: [0; 3],
            runs: 0,
            name: [0; process::NAME_MAX],
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeState {
    tick: u64,
    running: bool,
    irq_mode: bool,
}

impl RuntimeState {
    const fn empty() -> Self {
        Self {
            tick: 0,
            running: true,
            irq_mode: false,
        }
    }
}

#[derive(Clone, Copy)]
struct CommandQueue {
    items: [[u8; ui::TERM_MAX_INPUT]; CMD_QUEUE_CAP],
    lens: [u8; CMD_QUEUE_CAP],
    head: usize,
    tail: usize,
    count: usize,
}

impl CommandQueue {
    const fn new() -> Self {
        Self {
            items: [[0; ui::TERM_MAX_INPUT]; CMD_QUEUE_CAP],
            lens: [0; CMD_QUEUE_CAP],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn push(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let n = bytes.len().min(ui::TERM_MAX_INPUT);
        if self.count == CMD_QUEUE_CAP {
            // Drop oldest to keep latency low.
            self.head = (self.head + 1) % CMD_QUEUE_CAP;
            self.count -= 1;
        }

        let idx = self.tail;
        let mut i = 0usize;
        while i < n {
            self.items[idx][i] = bytes[i];
            i += 1;
        }
        self.lens[idx] = n as u8;
        self.tail = (self.tail + 1) % CMD_QUEUE_CAP;
        self.count += 1;
    }

    fn pop_into(&mut self, out: &mut [u8]) -> usize {
        if self.count == 0 || out.is_empty() {
            return 0;
        }

        let idx = self.head;
        let n = (self.lens[idx] as usize).min(out.len());

        let mut i = 0usize;
        while i < n {
            out[i] = self.items[idx][i];
            i += 1;
        }

        self.head = (self.head + 1) % CMD_QUEUE_CAP;
        self.count -= 1;
        n
    }
}

#[derive(Clone, Copy)]
struct LinuxMmapSlot {
    active: bool,
    process_pid: u32,
    addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    backing_ptr: u64,
    backing_len: u64,
}

impl LinuxMmapSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            process_pid: 0,
            addr: 0,
            len: 0,
            prot: 0,
            flags: 0,
            backing_ptr: 0,
            backing_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxProcessSlot {
    active: bool,
    pid: u32,
    parent_pid: u32,
    leader_tid: u32,
    brk_base: u64,
    brk_current: u64,
    brk_limit: u64,
    mmap_cursor: u64,
    mmap_count: usize,
}

impl LinuxProcessSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            pid: 0,
            parent_pid: 0,
            leader_tid: 0,
            brk_base: 0,
            brk_current: 0,
            brk_limit: 0,
            mmap_cursor: LINUX_MMAP_BASE,
            mmap_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxRuntimeFileSlot {
    active: bool,
    size: u64,
    path_len: u16,
    path: [u8; LINUX_PATH_MAX],
    data_ptr: u64,
    data_len: u64,
}

impl LinuxRuntimeFileSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            size: 0,
            path_len: 0,
            path: [0; LINUX_PATH_MAX],
            data_ptr: 0,
            data_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxDirSlot {
    active: bool,
    path_len: u16,
    path: [u8; LINUX_PATH_MAX],
}

impl LinuxDirSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            path_len: 0,
            path: [0; LINUX_PATH_MAX],
        }
    }
}

const LINUX_OPEN_KIND_RUNTIME: u8 = 1;
const LINUX_OPEN_KIND_EVENTFD: u8 = 2;
const LINUX_OPEN_KIND_PIPE_READ: u8 = 3;
const LINUX_OPEN_KIND_PIPE_WRITE: u8 = 4;
const LINUX_OPEN_KIND_EPOLL: u8 = 5;
const LINUX_OPEN_KIND_STDIO_DUP: u8 = 6;
const LINUX_OPEN_KIND_SOCKET: u8 = 7;
const LINUX_OPEN_KIND_PIDFD: u8 = 8;
const LINUX_OPEN_KIND_DIR: u8 = 9;
const LINUX_OPEN_AUX_TIMERFD: u64 = 0x5446_4D52; // "TFMR"

const LINUX_O_CREAT: u64 = 0x0000_0040;
const LINUX_O_EXCL: u64 = 0x0000_0080;
const LINUX_O_DIRECTORY: u64 = 0x0001_0000;
const LINUX_O_CLOEXEC: u64 = 0x0008_0000;
const LINUX_AT_EMPTY_PATH: u64 = 0x1000;
const LINUX_CLOSE_RANGE_UNSHARE: u64 = 0x2;
const LINUX_CLOSE_RANGE_CLOEXEC: u64 = 0x4;
const LINUX_MEMBARRIER_CMD_QUERY: u64 = 0;

const LINUX_DT_UNKNOWN: u8 = 0;
const LINUX_DT_DIR: u8 = 4;
const LINUX_DT_REG: u8 = 8;
const LINUX_DT_SOCK: u8 = 12;

const LINUX_MAX_DIR_SLOTS: usize = 96;

const LINUX_SOCKET_ENDPOINT_NONE: u8 = 0;
const LINUX_SOCKET_ENDPOINT_X11: u8 = 1;
const LINUX_SOCKET_ENDPOINT_PAIR: u8 = 2;
const LINUX_SOCKET_ENDPOINT_UNIX_PATH: u8 = 3;
const LINUX_SOCKET_ENDPOINT_DBUS: u8 = 4;
const LINUX_DBUS_STATE_AUTH_WAIT: u8 = 0;
const LINUX_DBUS_STATE_AUTH_OK: u8 = 1;
const LINUX_DBUS_STATE_RUNNING: u8 = 2;
const LINUX_DBUS_AUTH_OK_REPLY: &[u8] = b"OK 0123456789abcdef0123456789abcdef\r\n";
const LINUX_DBUS_AUTH_UNIX_FD_REPLY: &[u8] = b"AGREE_UNIX_FD\r\n";
const LINUX_X11_STATE_HANDSHAKE: u8 = 0;
const LINUX_X11_STATE_READY: u8 = 1;
const LINUX_X11_EXT_MIT_SHM: u8 = 130;
const LINUX_X11_EXT_BIGREQ: u8 = 131;
const LINUX_X11_EXT_RANDR: u8 = 132;
const LINUX_X11_EXT_RENDER: u8 = 133;
const LINUX_X11_EXT_XFIXES: u8 = 134;
const LINUX_X11_EXT_SHAPE: u8 = 135;
const LINUX_X11_EXT_SYNC: u8 = 136;
const LINUX_X11_EXT_XTEST: u8 = 137;
const LINUX_X11_EXT_XINPUT: u8 = 138;
const LINUX_X11_ROOT_WINDOW: u32 = 0x0000_0100;
const LINUX_X11_VISUAL_TRUECOLOR: u32 = 0x0000_0021;
const LINUX_X11_ATOM_WM_PROTOCOLS: u32 = 68;
const LINUX_X11_ATOM_WM_DELETE_WINDOW: u32 = 69;
const LINUX_X11_ATOM_WM_NAME: u32 = 39;
const LINUX_X11_ATOM_STRING: u32 = 31;
const LINUX_X11_ATOM_UTF8_STRING: u32 = 0x0100_0001;
const LINUX_X11_ATOM_NET_WM_NAME: u32 = 0x0100_0002;
const LINUX_X11_ATOM_CLIPBOARD: u32 = 0x0100_0003;
const LINUX_X11_ATOM_TARGETS: u32 = 0x0100_0004;
const LINUX_X11_ATOM_ATOM: u32 = 4;
const LINUX_X11_ATOM_WINDOW: u32 = 33;
const LINUX_X11_ATOM_CARDINAL: u32 = 6;
const LINUX_X11_ATOM_PRIMARY: u32 = 1;
const LINUX_X11_ATOM_SECONDARY: u32 = 2;
const LINUX_X11_ATOM_WM_CLASS: u32 = 67;
const LINUX_X11_ATOM_WM_STATE: u32 = 0x0100_0010;
const LINUX_X11_ATOM_NET_SUPPORTED: u32 = 0x0100_0011;
const LINUX_X11_ATOM_NET_SUPPORTING_WM_CHECK: u32 = 0x0100_0012;
const LINUX_X11_ATOM_NET_ACTIVE_WINDOW: u32 = 0x0100_0013;
const LINUX_X11_ATOM_NET_WM_PID: u32 = 0x0100_0014;
const LINUX_X11_ATOM_NET_WM_STATE: u32 = 0x0100_0015;
const LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_VERT: u32 = 0x0100_0016;
const LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_HORZ: u32 = 0x0100_0017;
const LINUX_X11_ATOM_NET_WM_WINDOW_TYPE: u32 = 0x0100_0018;
const LINUX_X11_ATOM_NET_WM_WINDOW_TYPE_NORMAL: u32 = 0x0100_0019;
const LINUX_X11_ATOM_NET_CURRENT_DESKTOP: u32 = 0x0100_001A;
const LINUX_X11_ATOM_NET_NUMBER_OF_DESKTOPS: u32 = 0x0100_001B;
const LINUX_X11_ATOM_NET_DESKTOP_NAMES: u32 = 0x0100_001C;
const LINUX_X11_ATOM_NET_CLIENT_LIST: u32 = 0x0100_001D;
const LINUX_X11_ATOM_MOTIF_WM_HINTS: u32 = 0x0100_001E;
const LINUX_X11_EVENT_CLIENT_MESSAGE: u8 = 33;
const LINUX_X11_EVENT_KEY_PRESS: u8 = 2;
const LINUX_X11_EVENT_KEY_RELEASE: u8 = 3;
const LINUX_X11_EVENT_BUTTON_PRESS: u8 = 4;
const LINUX_X11_EVENT_BUTTON_RELEASE: u8 = 5;
const LINUX_X11_EVENT_MOTION_NOTIFY: u8 = 6;
const LINUX_X11_EVENT_EXPOSE: u8 = 12;
const LINUX_X11_EVENT_DESTROY_NOTIFY: u8 = 17;
const LINUX_X11_EVENT_UNMAP_NOTIFY: u8 = 18;
const LINUX_X11_EVENT_MAP_NOTIFY: u8 = 19;
const LINUX_X11_EVENT_CONFIGURE_NOTIFY: u8 = 22;
const LINUX_X11_EVENT_PROPERTY_NOTIFY: u8 = 28;
const LINUX_X11_EVENT_SELECTION_NOTIFY: u8 = 31;
const LINUX_X11_EVENT_MASK_KEY_PRESS: u32 = 1 << 0;
const LINUX_X11_EVENT_MASK_KEY_RELEASE: u32 = 1 << 1;
const LINUX_X11_EVENT_MASK_BUTTON_PRESS: u32 = 1 << 2;
const LINUX_X11_EVENT_MASK_BUTTON_RELEASE: u32 = 1 << 3;
const LINUX_X11_EVENT_MASK_POINTER_MOTION: u32 = 1 << 6;
const LINUX_X11_EVENT_MASK_EXPOSURE: u32 = 1 << 15;
const LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY: u32 = 1 << 17;
const LINUX_X11_EVENT_MASK_PROPERTY_CHANGE: u32 = 1 << 22;
const LINUX_X11_CW_OVERRIDE_REDIRECT: u32 = 1 << 9;
const LINUX_X11_CW_EVENT_MASK: u32 = 1 << 11;

#[derive(Clone, Copy)]
struct LinuxOpenFileSlot {
    active: bool,
    fd: i32,
    kind: u8,
    _pad_kind: [u8; 3],
    object_index: usize,
    cursor: u64,
    flags: u64,
    aux: u64,
}

impl LinuxOpenFileSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            fd: 0,
            kind: 0,
            _pad_kind: [0; 3],
            object_index: 0,
            cursor: 0,
            flags: 0,
            aux: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxEventFdSlot {
    active: bool,
    semaphore: bool,
    counter: u64,
}

impl LinuxEventFdSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            semaphore: false,
            counter: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxPipeSlot {
    active: bool,
    pending_bytes: u64,
    read_open: bool,
    write_open: bool,
}

impl LinuxPipeSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            pending_bytes: 0,
            read_open: false,
            write_open: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxEpollEvent {
    events: u32,
    _pad: u32,
    data: u64,
}

#[derive(Clone, Copy)]
struct LinuxEpollWatchSlot {
    active: bool,
    target_fd: i32,
    events: u32,
    data: u64,
}

impl LinuxEpollWatchSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            target_fd: 0,
            events: 0,
            data: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxEpollSlot {
    active: bool,
    watches: [LinuxEpollWatchSlot; LINUX_MAX_EPOLL_WATCHES],
}

impl LinuxEpollSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            watches: [LinuxEpollWatchSlot::empty(); LINUX_MAX_EPOLL_WATCHES],
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxSocketSlot {
    active: bool,
    domain: u16,
    sock_type: u16,
    protocol: i32,
    nonblock: bool,
    cloexec: bool,
    connected: bool,
    bound: bool,
    listening: bool,
    endpoint: u8,
    _pad0: [u8; 2],
    peer_index: i32,
    pending_accept_index: i32,
    last_error: i32,
    path_len: u16,
    x11_seq: u16,
    x11_state: u8,
    x11_byte_order: u8,
    x11_bigreq: bool,
    _pad1: [u8; 1],
    rx_len: usize,
    rx_cursor: usize,
    path: [u8; LINUX_PATH_MAX],
    rx_buf: [u8; LINUX_SOCKET_RX_BUF],
}

impl LinuxSocketSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            domain: 0,
            sock_type: 0,
            protocol: 0,
            nonblock: false,
            cloexec: false,
            connected: false,
            bound: false,
            listening: false,
            endpoint: LINUX_SOCKET_ENDPOINT_NONE,
            _pad0: [0; 2],
            peer_index: -1,
            pending_accept_index: -1,
            last_error: 0,
            path_len: 0,
            x11_seq: 0,
            x11_state: LINUX_X11_STATE_HANDSHAKE,
            x11_byte_order: b'l',
            x11_bigreq: false,
            _pad1: [0; 1],
            rx_len: 0,
            rx_cursor: 0,
            path: [0; LINUX_PATH_MAX],
            rx_buf: [0; LINUX_SOCKET_RX_BUF],
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxX11WindowSlot {
    active: bool,
    id: u32,
    parent: u32,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    border: u16,
    class_hint: u16,
    mapped: bool,
    override_redirect: bool,
    _pad0: [u8; 2],
    visual: u32,
    event_mask: u32,
}

impl LinuxX11WindowSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            id: 0,
            parent: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            border: 0,
            class_hint: 1,
            mapped: false,
            override_redirect: false,
            _pad0: [0; 2],
            visual: LINUX_X11_VISUAL_TRUECOLOR,
            event_mask: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxX11PropertySlot {
    active: bool,
    window: u32,
    atom: u32,
    prop_type: u32,
    format: u8,
    _pad0: [u8; 3],
    data_len: usize,
    data: [u8; LINUX_X11_PROPERTY_DATA_MAX],
}

impl LinuxX11PropertySlot {
    const fn empty() -> Self {
        Self {
            active: false,
            window: 0,
            atom: 0,
            prop_type: 0,
            format: 0,
            _pad0: [0; 3],
            data_len: 0,
            data: [0; LINUX_X11_PROPERTY_DATA_MAX],
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxX11SelectionSlot {
    active: bool,
    selection_atom: u32,
    owner_window: u32,
}

impl LinuxX11SelectionSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            selection_atom: 0,
            owner_window: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxX11PixmapSlot {
    active: bool,
    id: u32,
    drawable: u32,
    width: u16,
    height: u16,
    depth: u8,
    _pad0: [u8; 3],
}

impl LinuxX11PixmapSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            id: 0,
            drawable: 0,
            width: 0,
            height: 0,
            depth: 24,
            _pad0: [0; 3],
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxX11GcSlot {
    active: bool,
    id: u32,
    drawable: u32,
    function: u8,
    fill_style: u8,
    _pad0: [u8; 2],
    foreground: u32,
    background: u32,
    line_width: u16,
    _pad1: [u8; 2],
}

impl LinuxX11GcSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            id: 0,
            drawable: 0,
            function: 3,
            fill_style: 0,
            _pad0: [0; 2],
            foreground: 0x00E6_E6E6,
            background: 0x0010_1018,
            line_width: 1,
            _pad1: [0; 2],
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxThreadContext {
    valid: bool,
    rax: u64,
    rcx: u64,
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    r10: u64,
    r11: u64,
    r8: u64,
    r9: u64,
    rsp: u64,
    rip: u64,
    rflags: u64,
}

impl LinuxThreadContext {
    const fn empty() -> Self {
        Self {
            valid: false,
            rax: 0,
            rcx: 0,
            rbx: 0,
            rbp: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            r10: 0,
            r11: 0,
            r8: 0,
            r9: 0,
            rsp: 0,
            rip: 0,
            rflags: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxThreadSlot {
    active: bool,
    tid: u32,
    process_pid: u32,
    parent_tid: u32,
    exit_signal: u8,
    state: u8,
    _pad0: [u8; 2],
    fs_base: u64,
    tid_addr: u64,
    robust_list_head: u64,
    robust_list_len: u64,
    futex_wait_addr: u64,
    futex_wait_mask: u32,
    futex_timeout_errno: i32,
    futex_timeout_deadline: u64,
    futex_requeue_pi_target: u64,
    futex_waitv_count: u16,
    _pad_waitv: [u8; 6],
    futex_waitv_uaddrs: [u64; LINUX_FUTEX_WAITV_MAX],
    clone_flags: u64,
    signal_mask: u64,
    pending_signals: u64,
}

impl LinuxThreadSlot {
    const fn empty() -> Self {
        Self {
            active: false,
            tid: 0,
            process_pid: 0,
            parent_tid: 0,
            exit_signal: 0,
            state: 0,
            _pad0: [0; 2],
            fs_base: 0,
            tid_addr: 0,
            robust_list_head: 0,
            robust_list_len: 0,
            futex_wait_addr: 0,
            futex_wait_mask: LINUX_FUTEX_BITSET_MATCH_ANY,
            futex_timeout_errno: 0,
            futex_timeout_deadline: 0,
            futex_requeue_pi_target: 0,
            futex_waitv_count: 0,
            _pad_waitv: [0; 6],
            futex_waitv_uaddrs: [0; LINUX_FUTEX_WAITV_MAX],
            clone_flags: 0,
            signal_mask: 0,
            pending_signals: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxShimState {
    active: bool,
    session_id: u64,
    main_entry: u64,
    interp_entry: u64,
    stack_ptr: u64,
    brk_base: u64,
    brk_current: u64,
    brk_limit: u64,
    mmap_cursor: u64,
    mmap_count: usize,
    write_calls: u64,
    fs_base: u64,
    tid_value: u32,
    tid_addr: u64,
    current_tid: u32,
    current_pid: u32,
    next_tid: u32,
    next_pid: u32,
    pending_switch_tid: u32,
    thread_count: usize,
    process_count: usize,
    robust_list_head: u64,
    robust_list_len: u64,
    signal_mask: u64,
    pending_signals: u64,
    exited_tids: [u32; LINUX_EXITED_QUEUE_CAP],
    exited_parent_tids: [u32; LINUX_EXITED_QUEUE_CAP],
    exited_status: [i32; LINUX_EXITED_QUEUE_CAP],
    exited_kinds: [u8; LINUX_EXITED_QUEUE_CAP],
    exited_count: usize,
    runtime_file_count: usize,
    runtime_blob_bytes: u64,
    runtime_blob_files: usize,
    open_file_count: usize,
    next_fd: i32,
    shm_next_id: i32,
    shm_size_hint: u64,
    exit_code: i32,
    start_tick: u64,
    syscall_count: u64,
    last_sysno: u64,
    last_result: i64,
    last_errno: i64,
    last_path_len: u16,
    last_path: [u8; LINUX_PATH_MAX],
    last_path_errno: i64,
    last_path_sysno: u64,
    last_path_runtime_hit: bool,
    last_unix_connect_len: u16,
    _pad_unix_connect: [u8; 2],
    last_unix_connect_errno: i32,
    last_unix_connect_path: [u8; LINUX_PATH_MAX],
    watchdog_triggered: bool,
    exec_transition_pending: bool,
    stdio_line: [u8; ui::TERM_MAX_INPUT],
    stdio_line_len: usize,
    maps: [LinuxMmapSlot; LINUX_MAX_MMAPS],
    runtime_files: [LinuxRuntimeFileSlot; LINUX_MAX_RUNTIME_FILES],
    dirs: [LinuxDirSlot; LINUX_MAX_DIR_SLOTS],
    open_files: [LinuxOpenFileSlot; LINUX_MAX_OPEN_FILES],
    eventfds: [LinuxEventFdSlot; LINUX_MAX_EVENTFDS],
    pipes: [LinuxPipeSlot; LINUX_MAX_PIPES],
    epolls: [LinuxEpollSlot; LINUX_MAX_EPOLLS],
    sockets: [LinuxSocketSlot; LINUX_MAX_SOCKETS],
    x11_windows: [LinuxX11WindowSlot; LINUX_X11_MAX_WINDOWS],
    x11_properties: [LinuxX11PropertySlot; LINUX_X11_MAX_PROPERTIES],
    x11_selections: [LinuxX11SelectionSlot; LINUX_X11_MAX_SELECTIONS],
    x11_pixmaps: [LinuxX11PixmapSlot; LINUX_X11_MAX_PIXMAPS],
    x11_gcs: [LinuxX11GcSlot; LINUX_X11_MAX_GCS],
    x11_shm_segments: [LinuxX11ShmSlot; LINUX_X11_MAX_SHM_SEGMENTS],
    x11_focus_window: u32,
    x11_pointer_x: i16,
    x11_pointer_y: i16,
    x11_pointer_buttons: u8,
    x11_last_keycode: u8,
    x11_last_button: u8,
    _x11_pad: u8,
    processes: [LinuxProcessSlot; LINUX_MAX_PROCESSES],
    threads: [LinuxThreadSlot; LINUX_MAX_THREADS],
    thread_contexts: [LinuxThreadContext; LINUX_MAX_THREADS],
    sigactions: [LinuxKernelSigAction; LINUX_MAX_SIGNAL_NUM + 1],
}

impl LinuxShimState {
    const fn empty() -> Self {
        Self {
            active: false,
            session_id: 0,
            main_entry: 0,
            interp_entry: 0,
            stack_ptr: 0,
            brk_base: 0,
            brk_current: 0,
            brk_limit: 0,
            mmap_cursor: LINUX_MMAP_BASE,
            mmap_count: 0,
            write_calls: 0,
            fs_base: 0,
            tid_value: 0,
            tid_addr: 0,
            current_tid: 0,
            current_pid: 0,
            next_tid: 0,
            next_pid: 0,
            pending_switch_tid: 0,
            thread_count: 0,
            process_count: 0,
            robust_list_head: 0,
            robust_list_len: 0,
            signal_mask: 0,
            pending_signals: 0,
            exited_tids: [0; LINUX_EXITED_QUEUE_CAP],
            exited_parent_tids: [0; LINUX_EXITED_QUEUE_CAP],
            exited_status: [0; LINUX_EXITED_QUEUE_CAP],
            exited_kinds: [0; LINUX_EXITED_QUEUE_CAP],
            exited_count: 0,
            runtime_file_count: 0,
            runtime_blob_bytes: 0,
            runtime_blob_files: 0,
            open_file_count: 0,
            next_fd: LINUX_FD_BASE,
            shm_next_id: 1,
            shm_size_hint: 0,
            exit_code: 0,
            start_tick: 0,
            syscall_count: 0,
            last_sysno: 0,
            last_result: 0,
            last_errno: 0,
            last_path_len: 0,
            last_path: [0; LINUX_PATH_MAX],
            last_path_errno: 0,
            last_path_sysno: 0,
            last_path_runtime_hit: false,
            last_unix_connect_len: 0,
            _pad_unix_connect: [0; 2],
            last_unix_connect_errno: 0,
            last_unix_connect_path: [0; LINUX_PATH_MAX],
            watchdog_triggered: false,
            exec_transition_pending: false,
            stdio_line: [0; ui::TERM_MAX_INPUT],
            stdio_line_len: 0,
            maps: [LinuxMmapSlot::empty(); LINUX_MAX_MMAPS],
            runtime_files: [LinuxRuntimeFileSlot::empty(); LINUX_MAX_RUNTIME_FILES],
            dirs: [LinuxDirSlot::empty(); LINUX_MAX_DIR_SLOTS],
            open_files: [LinuxOpenFileSlot::empty(); LINUX_MAX_OPEN_FILES],
            eventfds: [LinuxEventFdSlot::empty(); LINUX_MAX_EVENTFDS],
            pipes: [LinuxPipeSlot::empty(); LINUX_MAX_PIPES],
            epolls: [LinuxEpollSlot::empty(); LINUX_MAX_EPOLLS],
            sockets: [LinuxSocketSlot::empty(); LINUX_MAX_SOCKETS],
            x11_windows: [LinuxX11WindowSlot::empty(); LINUX_X11_MAX_WINDOWS],
            x11_properties: [LinuxX11PropertySlot::empty(); LINUX_X11_MAX_PROPERTIES],
            x11_selections: [LinuxX11SelectionSlot::empty(); LINUX_X11_MAX_SELECTIONS],
            x11_pixmaps: [LinuxX11PixmapSlot::empty(); LINUX_X11_MAX_PIXMAPS],
            x11_gcs: [LinuxX11GcSlot::empty(); LINUX_X11_MAX_GCS],
            x11_shm_segments: [LinuxX11ShmSlot::empty(); LINUX_X11_MAX_SHM_SEGMENTS],
            x11_focus_window: LINUX_X11_ROOT_WINDOW,
            x11_pointer_x: 0,
            x11_pointer_y: 0,
            x11_pointer_buttons: 0,
            x11_last_keycode: 0,
            x11_last_button: 0,
            _x11_pad: 0,
            processes: [LinuxProcessSlot::empty(); LINUX_MAX_PROCESSES],
            threads: [LinuxThreadSlot::empty(); LINUX_MAX_THREADS],
            thread_contexts: [LinuxThreadContext::empty(); LINUX_MAX_THREADS],
            sigactions: [LinuxKernelSigAction::empty(); LINUX_MAX_SIGNAL_NUM + 1],
        }
    }
}

#[derive(Clone, Copy)]
pub struct LinuxShimStatus {
    pub active: bool,
    pub session_id: u64,
    pub main_entry: u64,
    pub interp_entry: u64,
    pub stack_ptr: u64,
    pub brk_current: u64,
    pub brk_limit: u64,
    pub mmap_count: usize,
    pub mmap_cursor: u64,
    pub fs_base: u64,
    pub tid_value: u32,
    pub current_tid: u32,
    pub current_pid: u32,
    pub thread_count: usize,
    pub process_count: usize,
    pub runnable_threads: usize,
    pub signal_mask: u64,
    pub pending_signals: u64,
    pub runtime_file_count: usize,
    pub runtime_blob_bytes: u64,
    pub runtime_blob_files: usize,
    pub open_file_count: usize,
    pub next_fd: i32,
    pub exit_code: i32,
    pub syscall_count: u64,
    pub last_sysno: u64,
    pub last_result: i64,
    pub last_errno: i64,
    pub last_path_len: usize,
    pub last_path: [u8; LINUX_PATH_MAX],
    pub last_path_errno: i64,
    pub last_path_sysno: u64,
    pub last_path_runtime_hit: bool,
    pub watchdog_triggered: bool,
}

#[derive(Clone, Copy)]
pub struct LinuxShimProbeSummary {
    pub attempted: u32,
    pub ok: u32,
    pub unsupported: u32,
    pub failed: u32,
    pub first_errno: i64,
    pub brk_before: i64,
    pub brk_after: i64,
    pub mmap_res: i64,
    pub mprotect_res: i64,
    pub futex_res: i64,
    pub clock_res: i64,
    pub random_res: i64,
    pub uname_res: i64,
    pub openat_res: i64,
    pub fstat_res: i64,
    pub lseek_res: i64,
    pub read_res: i64,
    pub close_res: i64,
}

#[derive(Clone, Copy)]
pub struct LinuxX11SocketStatus {
    pub endpoint_count: usize,
    pub connected_count: usize,
    pub ready_count: usize,
    pub handshake_count: usize,
    pub last_error: i32,
    pub last_path_len: usize,
    pub last_path: [u8; LINUX_PATH_MAX],
    pub last_unix_connect_errno: i32,
    pub last_unix_connect_len: usize,
    pub last_unix_connect_path: [u8; LINUX_PATH_MAX],
}

impl LinuxShimProbeSummary {
    const fn empty() -> Self {
        Self {
            attempted: 0,
            ok: 0,
            unsupported: 0,
            failed: 0,
            first_errno: 0,
            brk_before: 0,
            brk_after: 0,
            mmap_res: 0,
            mprotect_res: 0,
            futex_res: 0,
            clock_res: 0,
            random_res: 0,
            uname_res: 0,
            openat_res: 0,
            fstat_res: 0,
            lseek_res: 0,
            read_res: 0,
            close_res: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct LinuxShimSliceSummary {
    pub active: bool,
    pub completed_calls: u32,
    pub ok: u32,
    pub unsupported: u32,
    pub failed: u32,
    pub first_errno: i64,
    pub watchdog_triggered: bool,
    pub exit_code: i32,
    pub last_sysno: u64,
    pub last_result: i64,
}

impl LinuxShimSliceSummary {
    const fn empty() -> Self {
        Self {
            active: false,
            completed_calls: 0,
            ok: 0,
            unsupported: 0,
            failed: 0,
            first_errno: 0,
            watchdog_triggered: false,
            exit_code: 0,
            last_sysno: 0,
            last_result: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct LinuxGfxInputEvent {
    pub kind: u8,
    pub down: u8,
    pub x: i32,
    pub y: i32,
    pub code: u32,
}

impl LinuxGfxInputEvent {
    const fn empty() -> Self {
        Self {
            kind: 0,
            down: 0,
            x: 0,
            y: 0,
            code: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct LinuxGfxBridgeState {
    active: bool,
    width: u32,
    height: u32,
    frame_seq: u64,
    status_len: usize,
    status: [u8; LINUX_GFX_STATUS_MAX],
    dirty: bool,
    event_head: usize,
    event_tail: usize,
    event_count: usize,
    event_dropped: u64,
    event_seq: u64,
    last_input_tick: u64,
    direct_present: bool,
    direct_last_present_tick: u64,
    events: [LinuxGfxInputEvent; LINUX_GFX_EVENT_CAP],
}

impl LinuxGfxBridgeState {
    const fn empty() -> Self {
        Self {
            active: false,
            width: 0,
            height: 0,
            frame_seq: 0,
            status_len: 0,
            status: [0; LINUX_GFX_STATUS_MAX],
            dirty: false,
            event_head: 0,
            event_tail: 0,
            event_count: 0,
            event_dropped: 0,
            event_seq: 0,
            last_input_tick: 0,
            direct_present: false,
            direct_last_present_tick: 0,
            events: [LinuxGfxInputEvent::empty(); LINUX_GFX_EVENT_CAP],
        }
    }
}

#[derive(Clone, Copy)]
pub struct LinuxGfxBridgeStatus {
    pub active: bool,
    pub width: u32,
    pub height: u32,
    pub frame_seq: u64,
    pub status_len: usize,
    pub status: [u8; LINUX_GFX_STATUS_MAX],
    pub dirty: bool,
    pub event_count: usize,
    pub event_dropped: u64,
    pub event_seq: u64,
    pub last_input_tick: u64,
    pub direct_present: bool,
}

type SysHandler = fn(thread_index: usize, a0: u64, a1: u64, a2: u64, a3: u64) -> u64;

fn handle_write_line(_thread_index: usize, a0: u64, a1: u64, _a2: u64, _a3: u64) -> u64 {
    if a0 == 0 || a1 == 0 {
        return 0;
    }

    let requested = (a1 as usize).min(ui::TERM_MAX_INPUT);
    let mut buf = [0u8; ui::TERM_MAX_INPUT];

    unsafe {
        let src = a0 as *const u8;
        let mut i = 0usize;
        while i < requested {
            let b = ptr::read(src.add(i));
            buf[i] = if b.is_ascii() && (b >= 0x20 || b == b'\t') { b } else { b'?' };
            i += 1;
        }
    }

    ui::terminal_system_message_bytes(&buf[..requested]);
    requested as u64
}

fn handle_clear_lines(_thread_index: usize, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    ui::terminal_clear_lines();
    0
}

fn handle_get_tick(_thread_index: usize, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    timer::ticks()
}

fn handle_get_runtime_flags(_thread_index: usize, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    unsafe {
        let mut flags = 0u64;
        if RUNTIME_STATE.running {
            flags |= 1;
        }
        if RUNTIME_STATE.irq_mode {
            flags |= 1 << 1;
        }
        flags | (RUNTIME_STATE.tick << 8)
    }
}

fn handle_recv_command(_thread_index: usize, a0: u64, a1: u64, _a2: u64, _a3: u64) -> u64 {
    if a0 == 0 || a1 == 0 {
        return 0;
    }

    let cap = (a1 as usize).min(ui::TERM_MAX_INPUT);
    let mut local = [0u8; ui::TERM_MAX_INPUT];
    let n = unsafe { CMD_QUEUE.pop_into(&mut local) };
    if n == 0 {
        return 0;
    }

    let copy = n.min(cap);
    unsafe {
        let dst = a0 as *mut u8;
        let mut i = 0usize;
        while i < copy {
            ptr::write(dst.add(i), local[i]);
            i += 1;
        }
    }

    copy as u64
}

fn handle_thread_info(_thread_index: usize, a0: u64, a1: u64, _a2: u64, _a3: u64) -> u64 {
    if a1 == 0 {
        return 0;
    }

    let index = a0 as usize;
    let info = match process::thread_info(index) {
        Some(i) => i,
        None => return 0,
    };

    let out = SysThreadInfo {
        tid: info.tid,
        pid: info.pid,
        ring: info.ring as u8,
        state: info.state as u8,
        name_len: info.name_len,
        _pad: [0; 3],
        runs: info.runs,
        name: info.name,
    };

    unsafe {
        let dst = a1 as *mut SysThreadInfo;
        ptr::write(dst, out);
    }

    1
}

fn handle_syscall_count(_thread_index: usize, a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let id = a0 as usize;
    if id >= SYS_COUNT {
        return 0;
    }
    unsafe { SYSCALL_COUNTS[id] }
}

fn handle_priv_status(_thread_index: usize, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    privilege::status_word()
}

fn handle_priv_next(_thread_index: usize, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    privilege::advance_phase() as u64
}

fn handle_priv_unsafe_test(_thread_index: usize, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if privilege::run_cpl3_test_unsafe_now() {
        1
    } else {
        0
    }
}

fn linux_align_up(value: u64, align: u64) -> Option<u64> {
    if align == 0 {
        return Some(value);
    }
    value
        .checked_add(align.saturating_sub(1))
        .map(|v| v & !(align.saturating_sub(1)))
}

#[inline]
fn linux_neg_errno(errno: i64) -> i64 {
    -errno
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxTimespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxRobustListHead {
    list_next: u64,
    futex_offset: i64,
    list_op_pending: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxItimerSpec {
    it_interval: LinuxTimespec,
    it_value: LinuxTimespec,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxCloneArgs {
    flags: u64,
    pidfd: u64,
    child_tid: u64,
    parent_tid: u64,
    exit_signal: u64,
    stack: u64,
    stack_size: u64,
    tls: u64,
    set_tid: u64,
    set_tid_size: u64,
    cgroup: u64,
}

impl LinuxCloneArgs {
    const fn empty() -> Self {
        Self {
            flags: 0,
            pidfd: 0,
            child_tid: 0,
            parent_tid: 0,
            exit_signal: 0,
            stack: 0,
            stack_size: 0,
            tls: 0,
            set_tid: 0,
            set_tid_size: 0,
            cgroup: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxFutexWaitV {
    val: u64,
    uaddr: u64,
    flags: u32,
    _reserved: u32,
}

impl LinuxFutexWaitV {
    const fn empty() -> Self {
        Self {
            val: 0,
            uaddr: 0,
            flags: 0,
            _reserved: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxOpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

impl LinuxOpenHow {
    const fn empty() -> Self {
        Self {
            flags: 0,
            mode: 0,
            resolve: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxTimeval {
    tv_sec: i64,
    tv_usec: i64,
}

#[repr(C)]
struct LinuxTimezone {
    tz_minuteswest: i32,
    tz_dsttime: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxPollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
struct LinuxRlimit {
    rlim_cur: u64,
    rlim_max: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxRusage {
    ru_utime: LinuxTimeval,
    ru_stime: LinuxTimeval,
    ru_maxrss: i64,
    ru_ixrss: i64,
    ru_idrss: i64,
    ru_isrss: i64,
    ru_minflt: i64,
    ru_majflt: i64,
    ru_nswap: i64,
    ru_inblock: i64,
    ru_oublock: i64,
    ru_msgsnd: i64,
    ru_msgrcv: i64,
    ru_nsignals: i64,
    ru_nvcsw: i64,
    ru_nivcsw: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxSysinfo {
    uptime: i64,
    loads: [u64; 3],
    totalram: u64,
    freeram: u64,
    sharedram: u64,
    bufferram: u64,
    totalswap: u64,
    freeswap: u64,
    procs: u16,
    _pad: u16,
    totalhigh: u64,
    freehigh: u64,
    mem_unit: u32,
    _f: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxTms {
    tms_utime: i64,
    tms_stime: i64,
    tms_cutime: i64,
    tms_cstime: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxKernelSigAction {
    handler: u64,
    flags: u64,
    restorer: u64,
    mask: u64,
}

impl LinuxKernelSigAction {
    const fn empty() -> Self {
        Self {
            handler: 0,
            flags: 0,
            restorer: 0,
            mask: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxStackT {
    sp: u64,
    flags: i32,
    _pad: i32,
    size: u64,
}

#[repr(C)]
struct LinuxStat64 {
    st_dev: u64,
    st_ino: u64,
    st_nlink: u64,
    st_mode: u32,
    st_uid: u32,
    st_gid: u32,
    __pad0: u32,
    st_rdev: u64,
    st_size: i64,
    st_blksize: i64,
    st_blocks: i64,
    st_atime: i64,
    st_atime_nsec: i64,
    st_mtime: i64,
    st_mtime_nsec: i64,
    st_ctime: i64,
    st_ctime_nsec: i64,
    __unused: [i64; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxIovec {
    base: u64,
    len: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxSockAddr {
    family: u16,
    data: [u8; 14],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxSockAddrUn {
    family: u16,
    path: [u8; 108],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxMsgHdr {
    msg_name: u64,
    msg_namelen: u32,
    _pad1: u32,
    msg_iov: u64,
    msg_iovlen: u64,
    msg_control: u64,
    msg_controllen: u64,
    msg_flags: u32,
    _pad2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxWinsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxStatxTimestamp {
    tv_sec: i64,
    tv_nsec: u32,
    __reserved: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxStatx {
    stx_mask: u32,
    stx_blksize: u32,
    stx_attributes: u64,
    stx_nlink: u32,
    stx_uid: u32,
    stx_gid: u32,
    stx_mode: u16,
    __spare0: u16,
    stx_ino: u64,
    stx_size: u64,
    stx_blocks: u64,
    stx_attributes_mask: u64,
    stx_atime: LinuxStatxTimestamp,
    stx_btime: LinuxStatxTimestamp,
    stx_ctime: LinuxStatxTimestamp,
    stx_mtime: LinuxStatxTimestamp,
    stx_rdev_major: u32,
    stx_rdev_minor: u32,
    stx_dev_major: u32,
    stx_dev_minor: u32,
    stx_mnt_id: u64,
    stx_dio_mem_align: u32,
    stx_dio_offset_align: u32,
    __spare3: [u64; 12],
}

fn linux_probe_mark(summary: &mut LinuxShimProbeSummary, result: i64) {
    summary.attempted = summary.attempted.saturating_add(1);
    if result >= 0 {
        summary.ok = summary.ok.saturating_add(1);
        return;
    }

    if result == linux_neg_errno(38) {
        summary.unsupported = summary.unsupported.saturating_add(1);
        return;
    }

    summary.failed = summary.failed.saturating_add(1);
    if summary.first_errno == 0 {
        summary.first_errno = result;
    }
}

fn linux_slice_mark(summary: &mut LinuxShimSliceSummary, result: i64) {
    if result >= 0 {
        summary.ok = summary.ok.saturating_add(1);
        return;
    }

    if result == linux_neg_errno(38) {
        summary.unsupported = summary.unsupported.saturating_add(1);
        return;
    }

    summary.failed = summary.failed.saturating_add(1);
    if summary.first_errno == 0 {
        summary.first_errno = result;
    }
}

fn linux_fill_ascii_field(dst: &mut [u8], text: &str) {
    if dst.is_empty() {
        return;
    }
    let max_copy = dst.len().saturating_sub(1);
    let src = text.as_bytes();
    let mut i = 0usize;
    while i < src.len() && i < max_copy {
        dst[i] = src[i];
        i += 1;
    }
    while i < dst.len() {
        dst[i] = 0;
        i += 1;
    }
}

fn linux_basename_start(path: &[u8], len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let mut idx = len;
    while idx > 0 {
        if path[idx - 1] == b'/' {
            return idx;
        }
        idx -= 1;
    }
    0
}

fn linux_normalize_path_bytes(dst: &mut [u8; LINUX_PATH_MAX], src: &[u8]) -> usize {
    let mut out = 0usize;
    let mut prev_slash = false;
    let mut i = 0usize;
    while i < src.len() && out < dst.len() {
        let mut b = src[i];
        if b == b'\\' {
            b = b'/';
        }
        if b.is_ascii_uppercase() {
            b = b.to_ascii_lowercase();
        }
        if b == 0 {
            break;
        }
        if b == b'/' {
            if out == 0 {
                dst[out] = b;
                out += 1;
                prev_slash = true;
                i += 1;
                continue;
            }
            if prev_slash {
                i += 1;
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        dst[out] = b;
        out += 1;
        i += 1;
    }
    while out > 1 && dst[out - 1] == b'/' {
        out -= 1;
    }
    out
}

fn linux_normalize_path_str(dst: &mut [u8; LINUX_PATH_MAX], text: &str) -> usize {
    linux_normalize_path_bytes(dst, text.as_bytes())
}

fn linux_paths_match_slot(slot: &LinuxRuntimeFileSlot, query: &[u8], query_len: usize) -> bool {
    let slot_len = (slot.path_len as usize).min(slot.path.len());
    if slot_len == 0 || query_len == 0 {
        return false;
    }
    if slot_len == query_len && slot.path[..slot_len] == query[..query_len] {
        return true;
    }
    let slot_base = linux_basename_start(&slot.path, slot_len);
    let query_base = linux_basename_start(query, query_len);
    let slot_base_len = slot_len.saturating_sub(slot_base);
    let query_base_len = query_len.saturating_sub(query_base);
    slot_base_len > 0
        && query_base_len > 0
        && slot_base_len == query_base_len
        && slot.path[slot_base..slot_len] == query[query_base..query_len]
}

fn linux_read_c_string(path_ptr: u64, out: &mut [u8; LINUX_PATH_MAX]) -> Result<usize, i64> {
    if path_ptr == 0 {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    let mut raw = [0u8; LINUX_PATH_MAX];
    let mut n = 0usize;
    unsafe {
        let src = path_ptr as *const u8;
        while n < raw.len() {
            let b = ptr::read(src.add(n));
            if b == 0 {
                break;
            }
            raw[n] = b;
            n += 1;
        }
    }
    if n == raw.len() {
        return Err(linux_neg_errno(36)); // ENAMETOOLONG
    }
    let normalized = linux_normalize_path_bytes(out, &raw[..n]);
    if normalized == 0 {
        return Err(linux_neg_errno(2)); // ENOENT
    }
    Ok(normalized)
}

fn linux_read_raw_c_string(ptr_raw: u64, out: &mut [u8]) -> Result<usize, i64> {
    if ptr_raw == 0 {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    let mut n = 0usize;
    unsafe {
        let src = ptr_raw as *const u8;
        while n < out.len() {
            let b = ptr::read(src.add(n));
            if b == 0 {
                break;
            }
            out[n] = b;
            n += 1;
        }
    }
    if n == out.len() {
        return Err(linux_neg_errno(36)); // ENAMETOOLONG
    }
    Ok(n)
}

fn linux_find_runtime_index(state: &LinuxShimState, path: &[u8], path_len: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if slot.active && linux_paths_match_slot(slot, path, path_len) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_runtime_slot_abs_path(slot: &LinuxRuntimeFileSlot, out: &mut [u8; LINUX_PATH_MAX]) -> usize {
    let slot_len = (slot.path_len as usize).min(LINUX_PATH_MAX);
    if slot_len == 0 {
        return 0;
    }
    if slot.path[0] == b'/' {
        let mut tmp = [0u8; LINUX_PATH_MAX];
        let mut i = 0usize;
        while i < slot_len {
            tmp[i] = slot.path[i];
            i += 1;
        }
        return linux_normalize_path_bytes(out, &tmp[..slot_len]);
    }
    let mut tmp = [0u8; LINUX_PATH_MAX];
    let mut n = 0usize;
    tmp[n] = b'/';
    n += 1;
    let copy_len = slot_len.min(LINUX_PATH_MAX.saturating_sub(n));
    let mut i = 0usize;
    while i < copy_len {
        tmp[n + i] = slot.path[i];
        i += 1;
    }
    n = n.saturating_add(copy_len);
    linux_normalize_path_bytes(out, &tmp[..n])
}

fn linux_path_is_absolute(path: &[u8], path_len: usize) -> bool {
    path_len > 0 && path[0] == b'/'
}

fn linux_path_equals_slices(a: &[u8], a_len: usize, b: &[u8], b_len: usize) -> bool {
    if a_len != b_len {
        return false;
    }
    let mut i = 0usize;
    while i < a_len {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn linux_path_prefix_of(base: &[u8], base_len: usize, path: &[u8], path_len: usize) -> bool {
    if base_len == 0 || path_len == 0 {
        return false;
    }
    if base_len == 1 && base[0] == b'/' {
        return path_len > 1 && path[0] == b'/';
    }
    if base_len >= path_len {
        return false;
    }
    let mut i = 0usize;
    while i < base_len {
        if base[i] != path[i] {
            return false;
        }
        i += 1;
    }
    path[base_len] == b'/'
}

fn linux_vfs_pick_runtime_exe_index(state: &LinuxShimState) -> Option<usize> {
    let mut fallback: Option<usize> = None;
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if !slot.active || slot.path_len == 0 {
            i += 1;
            continue;
        }
        if fallback.is_none() {
            fallback = Some(i);
        }
        let len = (slot.path_len as usize).min(slot.path.len());
        let base = linux_basename_start(&slot.path, len);
        let base_slice = &slot.path[base..len];
        let ends_so = base_slice.len() >= 3
            && (base_slice[base_slice.len() - 3..] == *b".so"
                || (base_slice.len() >= 6 && base_slice[base_slice.len() - 6..].starts_with(b".so.")));
        if !ends_so {
            return Some(i);
        }
        i += 1;
    }
    fallback
}

fn linux_vfs_find_exact_runtime_file(state: &LinuxShimState, path: &[u8], path_len: usize) -> Option<usize> {
    if linux_path_equals(path, path_len, "/proc/self/exe") {
        return linux_vfs_pick_runtime_exe_index(state);
    }
    let mut abs_query = [0u8; LINUX_PATH_MAX];
    let abs_query_len = if linux_path_is_absolute(path, path_len) {
        let mut i = 0usize;
        while i < path_len.min(LINUX_PATH_MAX) {
            abs_query[i] = path[i];
            i += 1;
        }
        i
    } else {
        let mut tmp = [0u8; LINUX_PATH_MAX];
        let mut n = 0usize;
        if n < tmp.len() {
            tmp[n] = b'/';
            n += 1;
        }
        let copy = path_len.min(tmp.len().saturating_sub(n));
        let mut i = 0usize;
        while i < copy {
            tmp[n + i] = path[i];
            i += 1;
        }
        n = n.saturating_add(copy);
        linux_normalize_path_bytes(&mut abs_query, &tmp[..n])
    };
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if !slot.active || slot.path_len == 0 {
            i += 1;
            continue;
        }
        let mut abs_slot = [0u8; LINUX_PATH_MAX];
        let abs_slot_len = linux_runtime_slot_abs_path(slot, &mut abs_slot);
        if abs_slot_len > 0
            && linux_path_equals_slices(
                &abs_slot,
                abs_slot_len,
                &abs_query,
                abs_query_len,
            )
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_vfs_directory_exists(state: &LinuxShimState, path: &[u8], path_len: usize) -> bool {
    if path_len == 0 {
        return false;
    }
    if linux_path_equals(path, path_len, "/")
        || linux_path_equals(path, path_len, "/proc")
        || linux_path_equals(path, path_len, "/proc/self")
        || linux_path_equals(path, path_len, "/tmp")
        || linux_path_equals(path, path_len, "/tmp/.x11-unix")
        || linux_path_is_virtual_dbus_dir(path, path_len)
    {
        return true;
    }
    if linux_path_equals(path, path_len, "/proc/self/cwd") {
        return true;
    }
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if !slot.active || slot.path_len == 0 {
            i += 1;
            continue;
        }
        let mut abs_slot = [0u8; LINUX_PATH_MAX];
        let abs_slot_len = linux_runtime_slot_abs_path(slot, &mut abs_slot);
        if abs_slot_len > 0 && linux_path_prefix_of(path, path_len, &abs_slot, abs_slot_len) {
            return true;
        }
        i += 1;
    }
    false
}

fn linux_vfs_lookup_path(state: &LinuxShimState, path: &[u8], path_len: usize) -> (bool, bool, Option<usize>, u32) {
    if linux_path_is_virtual_x11_socket(path, path_len) {
        return (true, false, None, LINUX_STAT_MODE_SOCK);
    }
    if linux_path_is_virtual_dbus_socket(path, path_len) {
        return (true, false, None, LINUX_STAT_MODE_SOCK);
    }
    if let Some(runtime_idx) = linux_vfs_find_exact_runtime_file(state, path, path_len) {
        return (true, true, Some(runtime_idx), LINUX_STAT_MODE_REG);
    }
    if linux_vfs_directory_exists(state, path, path_len) || linux_path_is_virtual_x11_dir(path, path_len) {
        return (true, false, None, LINUX_STAT_MODE_DIR);
    }
    (false, false, None, 0)
}

fn linux_lookup_open_slot(state: &LinuxShimState, fd: i32) -> Result<LinuxOpenFileSlot, i64> {
    if fd <= 2 {
        return Err(linux_neg_errno(9)); // EBADF for dirfd use.
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd) else {
        return Err(linux_neg_errno(9));
    };
    Ok(state.open_files[open_idx])
}

fn linux_get_dir_slot_path(state: &LinuxShimState, dir_idx: usize, out: &mut [u8; LINUX_PATH_MAX]) -> Option<usize> {
    if dir_idx >= LINUX_MAX_DIR_SLOTS || !state.dirs[dir_idx].active {
        return None;
    }
    let len = (state.dirs[dir_idx].path_len as usize).min(LINUX_PATH_MAX);
    let mut i = 0usize;
    while i < len {
        out[i] = state.dirs[dir_idx].path[i];
        i += 1;
    }
    Some(len)
}

fn linux_resolve_dirfd_base_path(
    state: &LinuxShimState,
    dirfd: i64,
    out: &mut [u8; LINUX_PATH_MAX],
) -> Result<usize, i64> {
    if dirfd == LINUX_AT_FDCWD {
        out[0] = b'/';
        return Ok(1);
    }
    let slot = linux_lookup_open_slot(state, dirfd as i32)?;
    if slot.kind != LINUX_OPEN_KIND_DIR {
        return Err(linux_neg_errno(20)); // ENOTDIR
    }
    linux_get_dir_slot_path(state, slot.object_index, out).ok_or_else(|| linux_neg_errno(9))
}

fn linux_resolve_open_path(
    state: &LinuxShimState,
    dirfd: i64,
    input: &[u8; LINUX_PATH_MAX],
    input_len: usize,
    out: &mut [u8; LINUX_PATH_MAX],
) -> Result<usize, i64> {
    if input_len == 0 {
        return Err(linux_neg_errno(2));
    }
    if linux_path_is_absolute(input, input_len) {
        let mut tmp = [0u8; LINUX_PATH_MAX];
        let mut i = 0usize;
        while i < input_len.min(LINUX_PATH_MAX) {
            tmp[i] = input[i];
            i += 1;
        }
        let normalized = linux_normalize_path_bytes(out, &tmp[..i]);
        if normalized == 0 {
            return Err(linux_neg_errno(2));
        }
        return Ok(normalized);
    }
    let mut base = [0u8; LINUX_PATH_MAX];
    let base_len = linux_resolve_dirfd_base_path(state, dirfd, &mut base)?;
    let mut tmp = [0u8; LINUX_PATH_MAX];
    let mut n = 0usize;
    let mut i = 0usize;
    while i < base_len && n < tmp.len() {
        tmp[n] = base[i];
        i += 1;
        n += 1;
    }
    if n == 0 {
        tmp[n] = b'/';
        n += 1;
    }
    if n > 0 && tmp[n - 1] != b'/' {
        if n >= tmp.len() {
            return Err(linux_neg_errno(36)); // ENAMETOOLONG
        }
        tmp[n] = b'/';
        n += 1;
    }
    i = 0;
    while i < input_len && n < tmp.len() {
        tmp[n] = input[i];
        i += 1;
        n += 1;
    }
    if i < input_len {
        return Err(linux_neg_errno(36));
    }
    let normalized = linux_normalize_path_bytes(out, &tmp[..n]);
    if normalized == 0 {
        return Err(linux_neg_errno(2));
    }
    Ok(normalized)
}

fn linux_allocate_dir_slot(state: &mut LinuxShimState, path: &[u8; LINUX_PATH_MAX], path_len: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_DIR_SLOTS {
        if state.dirs[i].active
            && (state.dirs[i].path_len as usize) == path_len
            && linux_path_equals_slices(&state.dirs[i].path, path_len, path, path_len)
        {
            return Some(i);
        }
        i += 1;
    }
    i = 0;
    while i < LINUX_MAX_DIR_SLOTS {
        if !state.dirs[i].active {
            state.dirs[i].active = true;
            state.dirs[i].path_len = path_len as u16;
            let mut j = 0usize;
            while j < path_len && j < LINUX_PATH_MAX {
                state.dirs[i].path[j] = path[j];
                j += 1;
            }
            while j < LINUX_PATH_MAX {
                state.dirs[i].path[j] = 0;
                j += 1;
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_vfs_hash_name(name: &[u8]) -> u64 {
    let mut hash = 1469598103934665603u64;
    let mut i = 0usize;
    while i < name.len() {
        hash ^= name[i] as u64;
        hash = hash.wrapping_mul(1099511628211u64);
        i += 1;
    }
    hash
}

fn linux_vfs_emit_dirent64(
    dirp: u64,
    offset: usize,
    count: usize,
    ino: u64,
    next_off: u64,
    d_type: u8,
    name: &str,
) -> Option<usize> {
    let name_bytes = name.as_bytes();
    let base = 8 + 8 + 2 + 1;
    let total = base + name_bytes.len() + 1;
    let reclen = (total + 7) & !7;
    if offset.checked_add(reclen)? > count {
        return None;
    }
    unsafe {
        let ptr_base = (dirp as usize).checked_add(offset)? as *mut u8;
        ptr::write(ptr_base as *mut u64, ino);
        ptr::write(ptr_base.add(8) as *mut i64, next_off as i64);
        ptr::write(ptr_base.add(16) as *mut u16, reclen as u16);
        ptr::write(ptr_base.add(18), d_type);
        ptr::copy_nonoverlapping(name_bytes.as_ptr(), ptr_base.add(19), name_bytes.len());
        ptr::write(ptr_base.add(19 + name_bytes.len()), 0);
        let mut pad = 20 + name_bytes.len();
        while pad < reclen {
            ptr::write(ptr_base.add(pad), 0);
            pad += 1;
        }
    }
    Some(reclen)
}

fn linux_record_last_path_lookup(
    state: &mut LinuxShimState,
    sysno: u64,
    path: &[u8; LINUX_PATH_MAX],
    path_len: usize,
    result: i64,
    runtime_hit: bool,
) {
    let capped_len = path_len.min(LINUX_PATH_MAX);
    let mut i = 0usize;
    while i < capped_len {
        state.last_path[i] = path[i];
        i += 1;
    }
    while i < LINUX_PATH_MAX {
        state.last_path[i] = 0;
        i += 1;
    }
    state.last_path_len = capped_len as u16;
    state.last_path_sysno = sysno;
    state.last_path_errno = if result < 0 { (-result).min(i64::MAX) } else { 0 };
    state.last_path_runtime_hit = runtime_hit;
}

fn linux_find_open_slot_index(state: &LinuxShimState, fd: i32) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_OPEN_FILES {
        let slot = &state.open_files[i];
        if slot.active && slot.fd == fd {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_find_unused_fd(state: &LinuxShimState, start: i32) -> Option<i32> {
    let mut fd = start.max(LINUX_FD_BASE);
    let mut scans = 0usize;
    while scans < (LINUX_MAX_OPEN_FILES * 4) {
        if linux_find_open_slot_index(state, fd).is_none() {
            return Some(fd);
        }
        fd = fd.saturating_add(1);
        scans += 1;
    }
    None
}

fn linux_allocate_open_slot_for_fd(state: &mut LinuxShimState, fd: i32) -> Option<usize> {
    if fd < LINUX_FD_BASE {
        return None;
    }
    let idx = linux_allocate_open_slot(state)?;
    if fd >= state.next_fd {
        state.next_fd = fd.saturating_add(1);
    }
    Some(idx)
}

fn linux_allocate_eventfd_slot(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_EVENTFDS {
        if !state.eventfds[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_allocate_pipe_slot(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_PIPES {
        if !state.pipes[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_allocate_epoll_slot(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_EPOLLS {
        if !state.epolls[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_allocate_socket_slot(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_SOCKETS {
        if !state.sockets[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_socket_compact_rx(slot: &mut LinuxSocketSlot) {
    if slot.rx_cursor == 0 {
        return;
    }
    if slot.rx_cursor >= slot.rx_len {
        slot.rx_cursor = 0;
        slot.rx_len = 0;
        return;
    }
    let remaining = slot.rx_len.saturating_sub(slot.rx_cursor);
    let mut i = 0usize;
    while i < remaining {
        slot.rx_buf[i] = slot.rx_buf[slot.rx_cursor + i];
        i += 1;
    }
    slot.rx_cursor = 0;
    slot.rx_len = remaining;
}

fn linux_socket_rx_available(slot: &LinuxSocketSlot) -> usize {
    slot.rx_len.saturating_sub(slot.rx_cursor)
}

fn linux_socket_push_rx(slot: &mut LinuxSocketSlot, data: &[u8]) -> usize {
    linux_socket_compact_rx(slot);
    if slot.rx_len >= slot.rx_buf.len() || data.is_empty() {
        return 0;
    }
    let free = slot.rx_buf.len().saturating_sub(slot.rx_len);
    let write_len = free.min(data.len());
    let mut i = 0usize;
    while i < write_len {
        slot.rx_buf[slot.rx_len + i] = data[i];
        i += 1;
    }
    slot.rx_len = slot.rx_len.saturating_add(write_len);
    write_len
}

fn linux_ascii_contains_casefold(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    let mut i = 0usize;
    while i + needle.len() <= haystack.len() {
        let mut j = 0usize;
        while j < needle.len() {
            if haystack[i + j].to_ascii_lowercase() != needle[j].to_ascii_lowercase() {
                break;
            }
            j += 1;
        }
        if j == needle.len() {
            return true;
        }
        i += 1;
    }
    false
}

fn linux_dbus_consume_payload(slot: &mut LinuxSocketSlot, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    if slot.x11_state == LINUX_DBUS_STATE_RUNNING {
        // Once DBus auth reaches BEGIN, payload is binary DBus traffic.
        // Keep the transport alive and accept writes as no-op compat path.
        return;
    }

    if linux_ascii_contains_casefold(data, b"AUTH") {
        let _ = linux_socket_push_rx(slot, LINUX_DBUS_AUTH_OK_REPLY);
        slot.x11_state = LINUX_DBUS_STATE_AUTH_OK;
    }
    if linux_ascii_contains_casefold(data, b"NEGOTIATE_UNIX_FD") {
        let _ = linux_socket_push_rx(slot, LINUX_DBUS_AUTH_UNIX_FD_REPLY);
        if slot.x11_state < LINUX_DBUS_STATE_AUTH_OK {
            slot.x11_state = LINUX_DBUS_STATE_AUTH_OK;
        }
    }
    if linux_ascii_contains_casefold(data, b"BEGIN") {
        slot.x11_state = LINUX_DBUS_STATE_RUNNING;
    }
}

fn linux_socket_path_equals(slot: &LinuxSocketSlot, path: &[u8], path_len: usize) -> bool {
    let slot_len = (slot.path_len as usize).min(LINUX_PATH_MAX);
    linux_path_equals_slices(&slot.path, slot_len, path, path_len)
}

fn linux_find_unix_bound_socket_by_path(state: &LinuxShimState, path: &[u8], path_len: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_SOCKETS {
        let slot = &state.sockets[i];
        if slot.active
            && slot.domain == LINUX_AF_UNIX
            && slot.bound
            && slot.path_len > 0
            && linux_socket_path_equals(slot, path, path_len)
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_socket_has_reference(state: &LinuxShimState, sock_idx: usize) -> bool {
    if linux_is_open_kind_present(state, LINUX_OPEN_KIND_SOCKET, sock_idx) {
        return true;
    }
    let mut i = 0usize;
    while i < LINUX_MAX_SOCKETS {
        let slot = &state.sockets[i];
        if slot.active
            && (slot.peer_index == sock_idx as i32 || slot.pending_accept_index == sock_idx as i32)
        {
            return true;
        }
        i += 1;
    }
    false
}

fn linux_socket_queue_x11_fail(slot: &mut LinuxSocketSlot, reason: &str) {
    let reason_bytes = reason.as_bytes();
    let reason_len = reason_bytes.len().min(255);
    let padded = (reason_len + 3) & !3;
    let units = (padded / 4) as u16;
    let mut packet = [0u8; 8 + 256];
    packet[0] = 0; // Setup failed
    packet[1] = reason_len as u8;
    packet[2] = 11; // major protocol
    packet[3] = 0;
    packet[4] = 0;
    packet[5] = 0;
    packet[6] = (units & 0xff) as u8;
    packet[7] = ((units >> 8) & 0xff) as u8;
    let mut i = 0usize;
    while i < reason_len {
        packet[8 + i] = reason_bytes[i];
        i += 1;
    }
    let total = 8 + padded;
    let _ = linux_socket_push_rx(slot, &packet[..total]);
}

fn linux_read_u16_order(bytes: &[u8], off: usize, little: bool) -> u16 {
    if off + 2 > bytes.len() {
        return 0;
    }
    if little {
        u16::from_le_bytes([bytes[off], bytes[off + 1]])
    } else {
        u16::from_be_bytes([bytes[off], bytes[off + 1]])
    }
}

fn linux_read_u32_order(bytes: &[u8], off: usize, little: bool) -> u32 {
    if off + 4 > bytes.len() {
        return 0;
    }
    if little {
        u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    } else {
        u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    }
}

fn linux_write_u16_order(out: &mut [u8], off: usize, value: u16, little: bool) {
    if off + 2 > out.len() {
        return;
    }
    let bytes = if little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    out[off] = bytes[0];
    out[off + 1] = bytes[1];
}

fn linux_write_u32_order(out: &mut [u8], off: usize, value: u32, little: bool) {
    if off + 4 > out.len() {
        return;
    }
    let bytes = if little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    out[off] = bytes[0];
    out[off + 1] = bytes[1];
    out[off + 2] = bytes[2];
    out[off + 3] = bytes[3];
}

fn linux_ascii_eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    while i < a.len() {
        let mut ac = a[i];
        let mut bc = b[i];
        if (b'A'..=b'Z').contains(&ac) {
            ac = ac - b'A' + b'a';
        }
        if (b'A'..=b'Z').contains(&bc) {
            bc = bc - b'A' + b'a';
        }
        if ac != bc {
            return false;
        }
        i += 1;
    }
    true
}

fn linux_x11_little(slot: &LinuxSocketSlot) -> bool {
    slot.x11_byte_order != b'B'
}

fn linux_x11_extension_major(name: &[u8]) -> u8 {
    if linux_ascii_eq_ignore_case(name, b"MIT-SHM") {
        LINUX_X11_EXT_MIT_SHM
    } else if linux_ascii_eq_ignore_case(name, b"BIG-REQUESTS") {
        LINUX_X11_EXT_BIGREQ
    } else if linux_ascii_eq_ignore_case(name, b"RANDR") {
        LINUX_X11_EXT_RANDR
    } else if linux_ascii_eq_ignore_case(name, b"RENDER") {
        LINUX_X11_EXT_RENDER
    } else if linux_ascii_eq_ignore_case(name, b"XFIXES") {
        LINUX_X11_EXT_XFIXES
    } else if linux_ascii_eq_ignore_case(name, b"SHAPE") {
        LINUX_X11_EXT_SHAPE
    } else if linux_ascii_eq_ignore_case(name, b"SYNC") {
        LINUX_X11_EXT_SYNC
    } else if linux_ascii_eq_ignore_case(name, b"XTEST") {
        LINUX_X11_EXT_XTEST
    } else if linux_ascii_eq_ignore_case(name, b"XInputExtension") {
        LINUX_X11_EXT_XINPUT
    } else {
        0
    }
}

fn linux_x11_extension_name(major: u8) -> &'static [u8] {
    match major {
        LINUX_X11_EXT_MIT_SHM => b"MIT-SHM",
        LINUX_X11_EXT_BIGREQ => b"BIG-REQUESTS",
        LINUX_X11_EXT_RANDR => b"RANDR",
        LINUX_X11_EXT_RENDER => b"RENDER",
        LINUX_X11_EXT_XFIXES => b"XFIXES",
        LINUX_X11_EXT_SHAPE => b"SHAPE",
        LINUX_X11_EXT_SYNC => b"SYNC",
        LINUX_X11_EXT_XTEST => b"XTEST",
        LINUX_X11_EXT_XINPUT => b"XInputExtension",
        _ => b"",
    }
}

fn linux_x11_queue_reply(slot: &mut LinuxSocketSlot, data1: u8, head24: &[u8; 24], extra: &[u8]) {
    let little = linux_x11_little(slot);
    let extra_padded = (extra.len() + 3) & !3;
    let total = 32usize.saturating_add(extra_padded);
    if total > slot.rx_buf.len() {
        return;
    }

    linux_socket_compact_rx(slot);
    let free = slot.rx_buf.len().saturating_sub(slot.rx_len);
    if free < total {
        return;
    }

    let mut header = [0u8; 32];
    header[0] = 1; // Reply
    header[1] = data1;
    linux_write_u16_order(&mut header, 2, slot.x11_seq, little);
    linux_write_u32_order(&mut header, 4, (extra_padded / 4) as u32, little);
    let mut i = 0usize;
    while i < 24 {
        header[8 + i] = head24[i];
        i += 1;
    }
    if linux_socket_push_rx(slot, &header) != header.len() {
        return;
    }
    if !extra.is_empty() && linux_socket_push_rx(slot, extra) != extra.len() {
        return;
    }
    let pad = extra_padded.saturating_sub(extra.len());
    if pad > 0 {
        let zero = [0u8; 4];
        let _ = linux_socket_push_rx(slot, &zero[..pad.min(4)]);
    }
}

fn linux_x11_queue_reply32(slot: &mut LinuxSocketSlot, data1: u8, body: &[u8; 24]) {
    linux_x11_queue_reply(slot, data1, body, &[]);
}

fn linux_x11_hash_atom(name: &[u8]) -> u32 {
    let mut h = 2166136261u32;
    let mut i = 0usize;
    while i < name.len() {
        let mut b = name[i];
        if (b'A'..=b'Z').contains(&b) {
            b = b - b'A' + b'a';
        }
        h ^= b as u32;
        h = h.wrapping_mul(16777619u32);
        i += 1;
    }
    (h & 0x00FF_FFFF) | 0x0000_0100
}

fn linux_x11_queue_setup_success(slot: &mut LinuxSocketSlot) {
    let little = linux_x11_little(slot);
    let vendor = b"ReduxOS";
    let vendor_padded = (vendor.len() + 3) & !3;

    let setup_extra_len = 32usize  // connection setup
        .saturating_add(vendor_padded)
        .saturating_add(8)         // one pixmap format
        .saturating_add(72);       // one screen + one depth + one visual
    let setup_units = (setup_extra_len / 4) as u16;

    let mut packet = [0u8; 192];
    packet[0] = 1; // Success
    packet[1] = 0; // reason len
    linux_write_u16_order(&mut packet, 2, 11, little); // major
    linux_write_u16_order(&mut packet, 4, 0, little); // minor
    linux_write_u16_order(&mut packet, 6, setup_units, little);

    let mut off = 8usize;
    linux_write_u32_order(&mut packet, off + 0, 1, little); // release
    linux_write_u32_order(&mut packet, off + 4, 0x0020_0000, little); // resource base
    linux_write_u32_order(&mut packet, off + 8, 0x001F_FFFF, little); // resource mask
    linux_write_u32_order(&mut packet, off + 12, 0, little); // motion buffer
    linux_write_u16_order(&mut packet, off + 16, vendor.len() as u16, little);
    linux_write_u16_order(&mut packet, off + 18, 0xFFFF, little); // max request size
    packet[off + 20] = 1; // num roots
    packet[off + 21] = 1; // num formats
    packet[off + 22] = 0; // image byte order (LSBFirst)
    packet[off + 23] = 0; // bitmap bit order
    packet[off + 24] = 32; // scanline unit
    packet[off + 25] = 32; // scanline pad
    packet[off + 26] = 8; // min keycode
    packet[off + 27] = 255; // max keycode
    off = off.saturating_add(32);

    let mut i = 0usize;
    while i < vendor.len() {
        packet[off + i] = vendor[i];
        i += 1;
    }
    off = off.saturating_add(vendor_padded);

    // Pixmap format (depth=24, bpp=32)
    packet[off] = 24;
    packet[off + 1] = 32;
    packet[off + 2] = 32;
    off = off.saturating_add(8);

    // Root window info
    linux_write_u32_order(&mut packet, off + 0, 0x0000_0100, little); // root
    linux_write_u32_order(&mut packet, off + 4, LINUX_X11_DEFAULT_COLORMAP, little); // default colormap
    linux_write_u32_order(&mut packet, off + 8, 0x00FF_FFFF, little); // white pixel
    linux_write_u32_order(&mut packet, off + 12, 0x0000_0000, little); // black pixel
    linux_write_u32_order(&mut packet, off + 16, 0, little); // current input masks
    linux_write_u16_order(&mut packet, off + 20, LINUX_GFX_MAX_WIDTH as u16, little);
    linux_write_u16_order(&mut packet, off + 22, LINUX_GFX_MAX_HEIGHT as u16, little);
    linux_write_u16_order(&mut packet, off + 24, 169, little);
    linux_write_u16_order(&mut packet, off + 26, 95, little);
    linux_write_u16_order(&mut packet, off + 28, 1, little);
    linux_write_u16_order(&mut packet, off + 30, 1, little);
    linux_write_u32_order(&mut packet, off + 32, 0x0000_0021, little); // root visual
    packet[off + 36] = 0; // backing stores
    packet[off + 37] = 1; // save unders
    packet[off + 38] = 24; // root depth
    packet[off + 39] = 1; // nDepths
    off = off.saturating_add(40);

    // Depth record + one visual type
    packet[off] = 24;
    linux_write_u16_order(&mut packet, off + 2, 1, little); // nVisuals
    off = off.saturating_add(8);
    linux_write_u32_order(&mut packet, off + 0, 0x0000_0021, little); // visual id
    packet[off + 4] = 4; // TrueColor
    packet[off + 5] = 8; // bits per rgb
    linux_write_u16_order(&mut packet, off + 6, 256, little);
    linux_write_u32_order(&mut packet, off + 8, 0x00FF_0000, little);
    linux_write_u32_order(&mut packet, off + 12, 0x0000_FF00, little);
    linux_write_u32_order(&mut packet, off + 16, 0x0000_00FF, little);
    off = off.saturating_add(24);

    let total = 8usize.saturating_add(setup_extra_len).min(packet.len()).min(off);
    let _ = linux_socket_push_rx(slot, &packet[..total]);
}

fn linux_x11_extension_event_base(major: u8) -> u8 {
    match major {
        LINUX_X11_EXT_XINPUT => 64,
        LINUX_X11_EXT_XFIXES => 80,
        LINUX_X11_EXT_RANDR => 96,
        _ => 0,
    }
}

fn linux_x11_known_atom(name: &[u8]) -> u32 {
    if linux_ascii_eq_ignore_case(name, b"PRIMARY") {
        LINUX_X11_ATOM_PRIMARY
    } else if linux_ascii_eq_ignore_case(name, b"SECONDARY") {
        LINUX_X11_ATOM_SECONDARY
    } else if linux_ascii_eq_ignore_case(name, b"WM_PROTOCOLS") {
        LINUX_X11_ATOM_WM_PROTOCOLS
    } else if linux_ascii_eq_ignore_case(name, b"WM_DELETE_WINDOW") {
        LINUX_X11_ATOM_WM_DELETE_WINDOW
    } else if linux_ascii_eq_ignore_case(name, b"WM_NAME") {
        LINUX_X11_ATOM_WM_NAME
    } else if linux_ascii_eq_ignore_case(name, b"WM_CLASS") {
        LINUX_X11_ATOM_WM_CLASS
    } else if linux_ascii_eq_ignore_case(name, b"WM_STATE") {
        LINUX_X11_ATOM_WM_STATE
    } else if linux_ascii_eq_ignore_case(name, b"STRING") {
        LINUX_X11_ATOM_STRING
    } else if linux_ascii_eq_ignore_case(name, b"UTF8_STRING") {
        LINUX_X11_ATOM_UTF8_STRING
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_NAME") {
        LINUX_X11_ATOM_NET_WM_NAME
    } else if linux_ascii_eq_ignore_case(name, b"_NET_SUPPORTED") {
        LINUX_X11_ATOM_NET_SUPPORTED
    } else if linux_ascii_eq_ignore_case(name, b"_NET_SUPPORTING_WM_CHECK") {
        LINUX_X11_ATOM_NET_SUPPORTING_WM_CHECK
    } else if linux_ascii_eq_ignore_case(name, b"_NET_ACTIVE_WINDOW") {
        LINUX_X11_ATOM_NET_ACTIVE_WINDOW
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_PID") {
        LINUX_X11_ATOM_NET_WM_PID
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_STATE") {
        LINUX_X11_ATOM_NET_WM_STATE
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_STATE_MAXIMIZED_VERT") {
        LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_VERT
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_STATE_MAXIMIZED_HORZ") {
        LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_HORZ
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_WINDOW_TYPE") {
        LINUX_X11_ATOM_NET_WM_WINDOW_TYPE
    } else if linux_ascii_eq_ignore_case(name, b"_NET_WM_WINDOW_TYPE_NORMAL") {
        LINUX_X11_ATOM_NET_WM_WINDOW_TYPE_NORMAL
    } else if linux_ascii_eq_ignore_case(name, b"_NET_CURRENT_DESKTOP") {
        LINUX_X11_ATOM_NET_CURRENT_DESKTOP
    } else if linux_ascii_eq_ignore_case(name, b"_NET_NUMBER_OF_DESKTOPS") {
        LINUX_X11_ATOM_NET_NUMBER_OF_DESKTOPS
    } else if linux_ascii_eq_ignore_case(name, b"_NET_DESKTOP_NAMES") {
        LINUX_X11_ATOM_NET_DESKTOP_NAMES
    } else if linux_ascii_eq_ignore_case(name, b"_NET_CLIENT_LIST") {
        LINUX_X11_ATOM_NET_CLIENT_LIST
    } else if linux_ascii_eq_ignore_case(name, b"CLIPBOARD") {
        LINUX_X11_ATOM_CLIPBOARD
    } else if linux_ascii_eq_ignore_case(name, b"TARGETS") {
        LINUX_X11_ATOM_TARGETS
    } else if linux_ascii_eq_ignore_case(name, b"_MOTIF_WM_HINTS") {
        LINUX_X11_ATOM_MOTIF_WM_HINTS
    } else if linux_ascii_eq_ignore_case(name, b"ATOM") {
        LINUX_X11_ATOM_ATOM
    } else if linux_ascii_eq_ignore_case(name, b"WINDOW") {
        LINUX_X11_ATOM_WINDOW
    } else if linux_ascii_eq_ignore_case(name, b"CARDINAL") {
        LINUX_X11_ATOM_CARDINAL
    } else {
        0
    }
}

fn linux_x11_atom_from_name(name: &[u8], only_if_exists: bool) -> u32 {
    let known = linux_x11_known_atom(name);
    if known != 0 {
        return known;
    }
    if only_if_exists {
        return 0;
    }
    linux_x11_hash_atom(name)
}

fn linux_x11_atom_name_known(atom: u32) -> &'static [u8] {
    match atom {
        LINUX_X11_ATOM_PRIMARY => b"PRIMARY",
        LINUX_X11_ATOM_SECONDARY => b"SECONDARY",
        LINUX_X11_ATOM_ATOM => b"ATOM",
        LINUX_X11_ATOM_CARDINAL => b"CARDINAL",
        LINUX_X11_ATOM_STRING => b"STRING",
        LINUX_X11_ATOM_WINDOW => b"WINDOW",
        LINUX_X11_ATOM_WM_NAME => b"WM_NAME",
        LINUX_X11_ATOM_WM_CLASS => b"WM_CLASS",
        LINUX_X11_ATOM_WM_PROTOCOLS => b"WM_PROTOCOLS",
        LINUX_X11_ATOM_WM_DELETE_WINDOW => b"WM_DELETE_WINDOW",
        LINUX_X11_ATOM_WM_STATE => b"WM_STATE",
        LINUX_X11_ATOM_UTF8_STRING => b"UTF8_STRING",
        LINUX_X11_ATOM_NET_WM_NAME => b"_NET_WM_NAME",
        LINUX_X11_ATOM_NET_SUPPORTED => b"_NET_SUPPORTED",
        LINUX_X11_ATOM_NET_SUPPORTING_WM_CHECK => b"_NET_SUPPORTING_WM_CHECK",
        LINUX_X11_ATOM_NET_ACTIVE_WINDOW => b"_NET_ACTIVE_WINDOW",
        LINUX_X11_ATOM_NET_WM_PID => b"_NET_WM_PID",
        LINUX_X11_ATOM_NET_WM_STATE => b"_NET_WM_STATE",
        LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_VERT => b"_NET_WM_STATE_MAXIMIZED_VERT",
        LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_HORZ => b"_NET_WM_STATE_MAXIMIZED_HORZ",
        LINUX_X11_ATOM_NET_WM_WINDOW_TYPE => b"_NET_WM_WINDOW_TYPE",
        LINUX_X11_ATOM_NET_WM_WINDOW_TYPE_NORMAL => b"_NET_WM_WINDOW_TYPE_NORMAL",
        LINUX_X11_ATOM_NET_CURRENT_DESKTOP => b"_NET_CURRENT_DESKTOP",
        LINUX_X11_ATOM_NET_NUMBER_OF_DESKTOPS => b"_NET_NUMBER_OF_DESKTOPS",
        LINUX_X11_ATOM_NET_DESKTOP_NAMES => b"_NET_DESKTOP_NAMES",
        LINUX_X11_ATOM_NET_CLIENT_LIST => b"_NET_CLIENT_LIST",
        LINUX_X11_ATOM_CLIPBOARD => b"CLIPBOARD",
        LINUX_X11_ATOM_TARGETS => b"TARGETS",
        LINUX_X11_ATOM_MOTIF_WM_HINTS => b"_MOTIF_WM_HINTS",
        _ => b"",
    }
}

fn linux_x11_atom_name_bytes(atom: u32, out: &mut [u8; 32]) -> usize {
    let known = linux_x11_atom_name_known(atom);
    if !known.is_empty() {
        let len = known.len().min(out.len());
        let mut i = 0usize;
        while i < len {
            out[i] = known[i];
            i += 1;
        }
        return len;
    }
    let prefix = b"ATOM_";
    let mut off = 0usize;
    let mut i = 0usize;
    while i < prefix.len() && off < out.len() {
        out[off] = prefix[i];
        off += 1;
        i += 1;
    }
    let hex = b"0123456789ABCDEF";
    let mut shift = 28i32;
    while shift >= 0 && off < out.len() {
        let nibble = ((atom >> (shift as u32)) & 0xF) as usize;
        out[off] = hex[nibble];
        off += 1;
        shift -= 4;
    }
    off
}

fn linux_x11_find_window_index(state: &LinuxShimState, id: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_windows.len() {
        let win = state.x11_windows[i];
        if win.active && win.id == id {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_alloc_window_index(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_windows.len() {
        if !state.x11_windows[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_find_pixmap_index(state: &LinuxShimState, id: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_pixmaps.len() {
        let pm = state.x11_pixmaps[i];
        if pm.active && pm.id == id {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_alloc_pixmap_index(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_pixmaps.len() {
        if !state.x11_pixmaps[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_find_gc_index(state: &LinuxShimState, id: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_gcs.len() {
        let gc = state.x11_gcs[i];
        if gc.active && gc.id == id {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_alloc_gc_index(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_gcs.len() {
        if !state.x11_gcs[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_clear_pixmap_storage(slot_idx: usize) {
    if slot_idx >= LINUX_X11_MAX_PIXMAPS {
        return;
    }
    unsafe {
        let start = slot_idx.saturating_mul(LINUX_X11_PIXMAP_SLOT_PIXELS);
        let end = start
            .saturating_add(LINUX_X11_PIXMAP_SLOT_PIXELS)
            .min(LINUX_X11_PIXMAP_PIXELS.len());
        let mut i = start;
        while i < end {
            LINUX_X11_PIXMAP_PIXELS[i] = 0;
            i += 1;
        }
    }
}

fn linux_x11_window_origin(state: &LinuxShimState, window: u32) -> (i32, i32) {
    let mut x = 0i32;
    let mut y = 0i32;
    let mut current = window;
    let mut depth = 0usize;
    while depth < 16 {
        let Some(idx) = linux_x11_find_window_index(state, current) else {
            break;
        };
        let win = state.x11_windows[idx];
        x = x.saturating_add(win.x as i32);
        y = y.saturating_add(win.y as i32);
        if current == LINUX_X11_ROOT_WINDOW || win.parent == 0 || win.parent == current {
            break;
        }
        current = win.parent;
        depth += 1;
    }
    (x, y)
}

fn linux_x11_bridge_set_pixel(x: i32, y: i32, color: u32) -> bool {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active {
            return false;
        }
        let bw = state.width as i32;
        let bh = state.height as i32;
        if x < 0 || y < 0 || x >= bw || y >= bh {
            return false;
        }
        let idx = (y as usize)
            .saturating_mul(state.width as usize)
            .saturating_add(x as usize);
        if idx >= LINUX_GFX_PIXELS.len() {
            return false;
        }
        LINUX_GFX_PIXELS[idx] = color & 0x00FF_FFFF;
        true
    }
}

fn linux_x11_bridge_get_pixel(x: i32, y: i32) -> Option<u32> {
    unsafe {
        let state = &LINUX_GFX_BRIDGE;
        if !state.active {
            return None;
        }
        let bw = state.width as i32;
        let bh = state.height as i32;
        if x < 0 || y < 0 || x >= bw || y >= bh {
            return None;
        }
        let idx = (y as usize)
            .saturating_mul(state.width as usize)
            .saturating_add(x as usize);
        if idx >= LINUX_GFX_PIXELS.len() {
            return None;
        }
        Some(LINUX_GFX_PIXELS[idx])
    }
}

fn linux_x11_mark_bridge_dirty() {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active {
            return;
        }
        state.frame_seq = state.frame_seq.saturating_add(1);
        state.dirty = true;
        linux_gfx_bridge_present_direct_locked(state);
    }
}

fn linux_x11_drawable_get_pixel(state: &LinuxShimState, drawable: u32, x: i32, y: i32) -> Option<u32> {
    if let Some(win_idx) = linux_x11_find_window_index(state, drawable) {
        let win = state.x11_windows[win_idx];
        let (ox, oy) = linux_x11_window_origin(state, drawable);
        let local_x = x;
        let local_y = y;
        if local_x < 0
            || local_y < 0
            || local_x >= win.width as i32
            || local_y >= win.height as i32
        {
            return None;
        }
        return linux_x11_bridge_get_pixel(ox.saturating_add(local_x), oy.saturating_add(local_y));
    }
    if let Some(pm_idx) = linux_x11_find_pixmap_index(state, drawable) {
        let pm = state.x11_pixmaps[pm_idx];
        if x < 0 || y < 0 || x >= pm.width as i32 || y >= pm.height as i32 {
            return None;
        }
        let local = (y as usize)
            .saturating_mul(LINUX_GFX_MAX_WIDTH)
            .saturating_add(x as usize);
        let base = pm_idx.saturating_mul(LINUX_X11_PIXMAP_SLOT_PIXELS);
        let idx = base.saturating_add(local);
        unsafe {
            if idx < LINUX_X11_PIXMAP_PIXELS.len() {
                return Some(LINUX_X11_PIXMAP_PIXELS[idx]);
            }
        }
    }
    None
}

fn linux_x11_drawable_set_pixel(
    state: &mut LinuxShimState,
    drawable: u32,
    x: i32,
    y: i32,
    color: u32,
) -> bool {
    if let Some(win_idx) = linux_x11_find_window_index(state, drawable) {
        let win = state.x11_windows[win_idx];
        if x < 0 || y < 0 || x >= win.width as i32 || y >= win.height as i32 {
            return false;
        }
        let (ox, oy) = linux_x11_window_origin(state, drawable);
        return linux_x11_bridge_set_pixel(ox.saturating_add(x), oy.saturating_add(y), color);
    }
    if let Some(pm_idx) = linux_x11_find_pixmap_index(state, drawable) {
        let pm = state.x11_pixmaps[pm_idx];
        if x < 0 || y < 0 || x >= pm.width as i32 || y >= pm.height as i32 {
            return false;
        }
        let local = (y as usize)
            .saturating_mul(LINUX_GFX_MAX_WIDTH)
            .saturating_add(x as usize);
        let base = pm_idx.saturating_mul(LINUX_X11_PIXMAP_SLOT_PIXELS);
        let idx = base.saturating_add(local);
        unsafe {
            if idx < LINUX_X11_PIXMAP_PIXELS.len() {
                LINUX_X11_PIXMAP_PIXELS[idx] = color & 0x00FF_FFFF;
                return true;
            }
        }
    }
    false
}

fn linux_x11_fill_rect_drawable(
    state: &mut LinuxShimState,
    drawable: u32,
    x: i32,
    y: i32,
    w: u16,
    h: u16,
    color: u32,
) {
    if w == 0 || h == 0 {
        return;
    }
    if let Some(win_idx) = linux_x11_find_window_index(state, drawable) {
        let (ox, oy) = linux_x11_window_origin(state, drawable);
        let win = state.x11_windows[win_idx];
        let local_w = w.min(win.width);
        let local_h = h.min(win.height);
        linux_x11_fill_rect(
            ox.saturating_add(x),
            oy.saturating_add(y),
            local_w,
            local_h,
            color,
        );
        return;
    }
    if linux_x11_find_pixmap_index(state, drawable).is_some() {
        let mut yy = 0u16;
        while yy < h {
            let mut xx = 0u16;
            while xx < w {
                let _ = linux_x11_drawable_set_pixel(
                    state,
                    drawable,
                    x.saturating_add(xx as i32),
                    y.saturating_add(yy as i32),
                    color,
                );
                xx = xx.saturating_add(1);
            }
            yy = yy.saturating_add(1);
        }
    }
}

fn linux_x11_draw_line_drawable(
    state: &mut LinuxShimState,
    drawable: u32,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: u32,
) {
    let dx = (x1.saturating_sub(x0)).abs();
    let dy = (y1.saturating_sub(y0)).abs();
    let sx = if x0 <= x1 { 1 } else { -1 };
    let sy = if y0 <= y1 { 1 } else { -1 };
    let mut err = dx.saturating_sub(dy);

    loop {
        let _ = linux_x11_drawable_set_pixel(state, drawable, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err.saturating_mul(2);
        if e2 > -dy {
            err = err.saturating_sub(dy);
            x0 = x0.saturating_add(sx);
        }
        if e2 < dx {
            err = err.saturating_add(dx);
            y0 = y0.saturating_add(sy);
        }
    }
}

fn linux_x11_draw_rect_outline_drawable(
    state: &mut LinuxShimState,
    drawable: u32,
    x: i32,
    y: i32,
    w: u16,
    h: u16,
    color: u32,
) {
    if w == 0 || h == 0 {
        return;
    }
    let x1 = x.saturating_add((w as i32).saturating_sub(1));
    let y1 = y.saturating_add((h as i32).saturating_sub(1));
    linux_x11_draw_line_drawable(state, drawable, x, y, x1, y, color);
    linux_x11_draw_line_drawable(state, drawable, x1, y, x1, y1, color);
    linux_x11_draw_line_drawable(state, drawable, x1, y1, x, y1, color);
    linux_x11_draw_line_drawable(state, drawable, x, y1, x, y, color);
}

fn linux_x11_gc_color(state: &LinuxShimState, gc: u32, drawable: u32, background: bool) -> u32 {
    if let Some(gc_idx) = linux_x11_find_gc_index(state, gc) {
        let gc_slot = state.x11_gcs[gc_idx];
        return if background {
            gc_slot.background
        } else {
            gc_slot.foreground
        } & 0x00FF_FFFF;
    }
    if background {
        0x0010_1218
    } else {
        0x002D_7CF6 ^ drawable.wrapping_mul(2654435761u32)
    }
}

fn linux_x11_rgb16_to_pixel(red: u16, green: u16, blue: u16) -> u32 {
    (((red as u32) >> 8) << 16) | (((green as u32) >> 8) << 8) | ((blue as u32) >> 8)
}

fn linux_x11_pixel_to_rgb16(pixel: u32) -> (u16, u16, u16) {
    let r = ((pixel >> 16) & 0xFF) as u16;
    let g = ((pixel >> 8) & 0xFF) as u16;
    let b = (pixel & 0xFF) as u16;
    (
        ((r << 8) | r),
        ((g << 8) | g),
        ((b << 8) | b),
    )
}

fn linux_x11_apply_gc_values(
    gc: &mut LinuxX11GcSlot,
    value_mask: u32,
    req: &[u8],
    little: bool,
    mut value_off: usize,
) {
    let mut bit = 0u32;
    while bit < 23 {
        if (value_mask & (1u32 << bit)) != 0 {
            if value_off + 4 > req.len() {
                break;
            }
            let val = linux_read_u32_order(req, value_off, little);
            match bit {
                0 => gc.function = val as u8,
                2 => gc.foreground = val & 0x00FF_FFFF,
                3 => gc.background = val & 0x00FF_FFFF,
                4 => gc.line_width = (val as u16).max(1),
                8 => gc.fill_style = val as u8,
                _ => {}
            }
            value_off += 4;
        }
        bit += 1;
    }
}

fn linux_x11_ensure_root_window(state: &mut LinuxShimState) {
    if linux_x11_find_window_index(state, LINUX_X11_ROOT_WINDOW).is_some() {
        return;
    }
    let slot_idx = linux_x11_alloc_window_index(state).unwrap_or(0);
    state.x11_windows[slot_idx] = LinuxX11WindowSlot {
        active: true,
        id: LINUX_X11_ROOT_WINDOW,
        parent: 0,
        x: 0,
        y: 0,
        width: LINUX_GFX_MAX_WIDTH as u16,
        height: LINUX_GFX_MAX_HEIGHT as u16,
        border: 0,
        class_hint: 1,
        mapped: true,
        override_redirect: false,
        _pad0: [0; 2],
        visual: LINUX_X11_VISUAL_TRUECOLOR,
        event_mask: LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY
            | LINUX_X11_EVENT_MASK_PROPERTY_CHANGE
            | LINUX_X11_EVENT_MASK_POINTER_MOTION,
    };
    state.x11_focus_window = LINUX_X11_ROOT_WINDOW;
}

fn linux_x11_reset_server(state: &mut LinuxShimState) {
    state.x11_windows = [LinuxX11WindowSlot::empty(); LINUX_X11_MAX_WINDOWS];
    state.x11_properties = [LinuxX11PropertySlot::empty(); LINUX_X11_MAX_PROPERTIES];
    state.x11_selections = [LinuxX11SelectionSlot::empty(); LINUX_X11_MAX_SELECTIONS];
    state.x11_pixmaps = [LinuxX11PixmapSlot::empty(); LINUX_X11_MAX_PIXMAPS];
    state.x11_gcs = [LinuxX11GcSlot::empty(); LINUX_X11_MAX_GCS];
    state.x11_focus_window = LINUX_X11_ROOT_WINDOW;
    state.x11_pointer_x = (LINUX_GFX_MAX_WIDTH as i32 / 2) as i16;
    state.x11_pointer_y = (LINUX_GFX_MAX_HEIGHT as i32 / 2) as i16;
    state.x11_pointer_buttons = 0;
    state.x11_last_keycode = 0;
    state.x11_last_button = 0;
    // Keep init-shim responsive: pixmap storage is cleared lazily on CreatePixmap/FreePixmap.
    linux_x11_ensure_root_window(state);
    linux_x11_seed_ewmh_properties(state);
}

fn linux_x11_set_property_u32_list(
    state: &mut LinuxShimState,
    window: u32,
    atom: u32,
    prop_type: u32,
    values: &[u32],
) {
    let mut data = [0u8; LINUX_X11_PROPERTY_DATA_MAX];
    let mut off = 0usize;
    let mut i = 0usize;
    while i < values.len() {
        if off + 4 > data.len() {
            break;
        }
        let bytes = values[i].to_le_bytes();
        data[off] = bytes[0];
        data[off + 1] = bytes[1];
        data[off + 2] = bytes[2];
        data[off + 3] = bytes[3];
        off += 4;
        i += 1;
    }
    linux_x11_set_property(state, window, atom, prop_type, 32, 0, &data[..off]);
}

fn linux_x11_seed_ewmh_properties(state: &mut LinuxShimState) {
    linux_x11_ensure_root_window(state);
    let root = LINUX_X11_ROOT_WINDOW;
    let supported = [
        LINUX_X11_ATOM_NET_SUPPORTED,
        LINUX_X11_ATOM_NET_SUPPORTING_WM_CHECK,
        LINUX_X11_ATOM_NET_ACTIVE_WINDOW,
        LINUX_X11_ATOM_NET_WM_NAME,
        LINUX_X11_ATOM_NET_WM_PID,
        LINUX_X11_ATOM_NET_WM_STATE,
        LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_VERT,
        LINUX_X11_ATOM_NET_WM_STATE_MAXIMIZED_HORZ,
        LINUX_X11_ATOM_NET_WM_WINDOW_TYPE,
        LINUX_X11_ATOM_NET_WM_WINDOW_TYPE_NORMAL,
        LINUX_X11_ATOM_NET_CURRENT_DESKTOP,
        LINUX_X11_ATOM_NET_NUMBER_OF_DESKTOPS,
        LINUX_X11_ATOM_NET_DESKTOP_NAMES,
        LINUX_X11_ATOM_NET_CLIENT_LIST,
        LINUX_X11_ATOM_UTF8_STRING,
        LINUX_X11_ATOM_WM_PROTOCOLS,
        LINUX_X11_ATOM_WM_DELETE_WINDOW,
        LINUX_X11_ATOM_CLIPBOARD,
        LINUX_X11_ATOM_TARGETS,
    ];
    linux_x11_set_property_u32_list(
        state,
        root,
        LINUX_X11_ATOM_NET_SUPPORTED,
        LINUX_X11_ATOM_ATOM,
        &supported,
    );
    linux_x11_set_property_u32_list(
        state,
        root,
        LINUX_X11_ATOM_NET_SUPPORTING_WM_CHECK,
        LINUX_X11_ATOM_WINDOW,
        &[root],
    );
    linux_x11_set_property_u32_list(
        state,
        root,
        LINUX_X11_ATOM_NET_ACTIVE_WINDOW,
        LINUX_X11_ATOM_WINDOW,
        &[state.x11_focus_window],
    );
    linux_x11_set_property_u32_list(
        state,
        root,
        LINUX_X11_ATOM_NET_NUMBER_OF_DESKTOPS,
        LINUX_X11_ATOM_CARDINAL,
        &[1],
    );
    linux_x11_set_property_u32_list(
        state,
        root,
        LINUX_X11_ATOM_NET_CURRENT_DESKTOP,
        LINUX_X11_ATOM_CARDINAL,
        &[0],
    );
    linux_x11_set_property(
        state,
        root,
        LINUX_X11_ATOM_NET_DESKTOP_NAMES,
        LINUX_X11_ATOM_UTF8_STRING,
        8,
        0,
        b"ReduxOS\0",
    );
    linux_x11_set_property_u32_list(
        state,
        root,
        LINUX_X11_ATOM_NET_CLIENT_LIST,
        LINUX_X11_ATOM_WINDOW,
        &[],
    );
}

fn linux_x11_update_active_window_property(state: &mut LinuxShimState) {
    linux_x11_set_property_u32_list(
        state,
        LINUX_X11_ROOT_WINDOW,
        LINUX_X11_ATOM_NET_ACTIVE_WINDOW,
        LINUX_X11_ATOM_WINDOW,
        &[state.x11_focus_window],
    );
}

fn linux_x11_find_property_index(state: &LinuxShimState, window: u32, atom: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_properties.len() {
        let prop = state.x11_properties[i];
        if prop.active && prop.window == window && prop.atom == atom {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_alloc_property_index(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_properties.len() {
        if !state.x11_properties[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_remove_property(state: &mut LinuxShimState, window: u32, atom: u32) -> bool {
    let Some(idx) = linux_x11_find_property_index(state, window, atom) else {
        return false;
    };
    state.x11_properties[idx] = LinuxX11PropertySlot::empty();
    true
}

fn linux_x11_remove_window_properties(state: &mut LinuxShimState, window: u32) {
    let mut i = 0usize;
    while i < state.x11_properties.len() {
        if state.x11_properties[i].active && state.x11_properties[i].window == window {
            state.x11_properties[i] = LinuxX11PropertySlot::empty();
        }
        i += 1;
    }
}

fn linux_x11_property_bytes_per(format: u8) -> usize {
    match format {
        16 => 2,
        32 => 4,
        _ => 1,
    }
}

fn linux_x11_set_property(
    state: &mut LinuxShimState,
    window: u32,
    atom: u32,
    prop_type: u32,
    format: u8,
    mode: u8,
    data: &[u8],
) {
    let idx = if let Some(existing) = linux_x11_find_property_index(state, window, atom) {
        existing
    } else if let Some(new_idx) = linux_x11_alloc_property_index(state) {
        new_idx
    } else {
        return;
    };

    let mut slot = state.x11_properties[idx];
    if !slot.active || mode == 0 || slot.format != format || slot.prop_type != prop_type {
        slot = LinuxX11PropertySlot::empty();
        slot.active = true;
        slot.window = window;
        slot.atom = atom;
        slot.prop_type = prop_type;
        slot.format = format;
        let copy_len = data.len().min(LINUX_X11_PROPERTY_DATA_MAX);
        let mut i = 0usize;
        while i < copy_len {
            slot.data[i] = data[i];
            i += 1;
        }
        slot.data_len = copy_len;
        state.x11_properties[idx] = slot;
        return;
    }

    let incoming_len = data.len().min(LINUX_X11_PROPERTY_DATA_MAX);
    let mut merged = [0u8; LINUX_X11_PROPERTY_DATA_MAX];
    let mut merged_len = 0usize;
    if mode == 1 {
        // prepend
        let mut i = 0usize;
        while i < incoming_len {
            merged[i] = data[i];
            i += 1;
        }
        merged_len = incoming_len;
        let remain = LINUX_X11_PROPERTY_DATA_MAX.saturating_sub(merged_len);
        let copy_old = slot.data_len.min(remain);
        i = 0;
        while i < copy_old {
            merged[merged_len + i] = slot.data[i];
            i += 1;
        }
        merged_len = merged_len.saturating_add(copy_old);
    } else {
        // append
        let copy_old = slot.data_len.min(LINUX_X11_PROPERTY_DATA_MAX);
        let mut i = 0usize;
        while i < copy_old {
            merged[i] = slot.data[i];
            i += 1;
        }
        merged_len = copy_old;
        let remain = LINUX_X11_PROPERTY_DATA_MAX.saturating_sub(merged_len);
        let copy_new = incoming_len.min(remain);
        i = 0;
        while i < copy_new {
            merged[merged_len + i] = data[i];
            i += 1;
        }
        merged_len = merged_len.saturating_add(copy_new);
    }

    slot.data = merged;
    slot.data_len = merged_len;
    state.x11_properties[idx] = slot;
}

fn linux_x11_window_event_mask(state: &LinuxShimState, window: u32) -> u32 {
    if let Some(idx) = linux_x11_find_window_index(state, window) {
        state.x11_windows[idx].event_mask
    } else {
        0
    }
}

fn linux_x11_window_mapped(state: &LinuxShimState, window: u32) -> bool {
    if let Some(idx) = linux_x11_find_window_index(state, window) {
        state.x11_windows[idx].mapped
    } else {
        false
    }
}

fn linux_x11_queue_event(slot: &mut LinuxSocketSlot, event_type: u8, detail: u8, seq: u16, body: &[u8; 28]) {
    let little = linux_x11_little(slot);
    let mut packet = [0u8; 32];
    packet[0] = event_type;
    packet[1] = detail;
    linux_write_u16_order(&mut packet, 2, seq, little);
    let mut i = 0usize;
    while i < 28 {
        packet[4 + i] = body[i];
        i += 1;
    }
    let _ = linux_socket_push_rx(slot, &packet);
}

fn linux_x11_event_mask_for_type(event_type: u8) -> u32 {
    match event_type {
        LINUX_X11_EVENT_KEY_PRESS => LINUX_X11_EVENT_MASK_KEY_PRESS,
        LINUX_X11_EVENT_KEY_RELEASE => LINUX_X11_EVENT_MASK_KEY_RELEASE,
        LINUX_X11_EVENT_BUTTON_PRESS => LINUX_X11_EVENT_MASK_BUTTON_PRESS,
        LINUX_X11_EVENT_BUTTON_RELEASE => LINUX_X11_EVENT_MASK_BUTTON_RELEASE,
        LINUX_X11_EVENT_MOTION_NOTIFY => LINUX_X11_EVENT_MASK_POINTER_MOTION,
        LINUX_X11_EVENT_EXPOSE => LINUX_X11_EVENT_MASK_EXPOSURE,
        LINUX_X11_EVENT_DESTROY_NOTIFY
        | LINUX_X11_EVENT_UNMAP_NOTIFY
        | LINUX_X11_EVENT_MAP_NOTIFY
        | LINUX_X11_EVENT_CONFIGURE_NOTIFY => LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY,
        LINUX_X11_EVENT_PROPERTY_NOTIFY => LINUX_X11_EVENT_MASK_PROPERTY_CHANGE,
        _ => 0,
    }
}

fn linux_x11_queue_window_event(
    state: &mut LinuxShimState,
    sock_idx: usize,
    window: u32,
    event_type: u8,
    detail: u8,
    needed_mask: u32,
    body: &[u8; 28],
) {
    if sock_idx >= state.sockets.len() {
        return;
    }
    if !state.sockets[sock_idx].active || state.sockets[sock_idx].endpoint != LINUX_SOCKET_ENDPOINT_X11 {
        return;
    }
    if needed_mask != 0 {
        let mask = linux_x11_window_event_mask(state, window);
        if (mask & needed_mask) == 0 {
            return;
        }
    }
    let seq = state.sockets[sock_idx].x11_seq;
    linux_x11_queue_event(&mut state.sockets[sock_idx], event_type, detail, seq, body);
}

fn linux_x11_find_selection_index(state: &LinuxShimState, atom: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < state.x11_selections.len() {
        let sel = state.x11_selections[i];
        if sel.active && sel.selection_atom == atom {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_x11_set_selection_owner(state: &mut LinuxShimState, selection_atom: u32, owner_window: u32) {
    if owner_window == 0 {
        if let Some(idx) = linux_x11_find_selection_index(state, selection_atom) {
            state.x11_selections[idx] = LinuxX11SelectionSlot::empty();
        }
        return;
    }
    if let Some(idx) = linux_x11_find_selection_index(state, selection_atom) {
        state.x11_selections[idx].owner_window = owner_window;
        return;
    }
    let mut i = 0usize;
    while i < state.x11_selections.len() {
        if !state.x11_selections[i].active {
            state.x11_selections[i] = LinuxX11SelectionSlot {
                active: true,
                selection_atom,
                owner_window,
            };
            return;
        }
        i += 1;
    }
}

fn linux_x11_get_selection_owner(state: &LinuxShimState, selection_atom: u32) -> u32 {
    if let Some(idx) = linux_x11_find_selection_index(state, selection_atom) {
        state.x11_selections[idx].owner_window
    } else {
        0
    }
}

fn linux_x11_collect_children(state: &LinuxShimState, parent: u32, out: &mut [u32]) -> usize {
    let mut count = 0usize;
    let mut i = 0usize;
    while i < state.x11_windows.len() {
        let win = state.x11_windows[i];
        if win.active && win.parent == parent {
            if count < out.len() {
                out[count] = win.id;
            }
            count = count.saturating_add(1);
        }
        i += 1;
    }
    count.min(out.len())
}

fn linux_x11_refresh_client_list(state: &mut LinuxShimState) {
    let mut clients = [0u32; 96];
    let mut count = 0usize;
    let mut i = 0usize;
    while i < state.x11_windows.len() && count < clients.len() {
        let win = state.x11_windows[i];
        if win.active && win.id != LINUX_X11_ROOT_WINDOW {
            clients[count] = win.id;
            count += 1;
        }
        i += 1;
    }
    linux_x11_set_property_u32_list(
        state,
        LINUX_X11_ROOT_WINDOW,
        LINUX_X11_ATOM_NET_CLIENT_LIST,
        LINUX_X11_ATOM_WINDOW,
        &clients[..count],
    );
}

fn linux_x11_pick_input_window(state: &LinuxShimState) -> u32 {
    let focused = state.x11_focus_window;
    if focused != 0 && linux_x11_window_mapped(state, focused) {
        return focused;
    }
    let mut i = 0usize;
    while i < state.x11_windows.len() {
        let win = state.x11_windows[i];
        if win.active && win.id != LINUX_X11_ROOT_WINDOW && win.mapped {
            return win.id;
        }
        i += 1;
    }
    LINUX_X11_ROOT_WINDOW
}

fn linux_x11_keycode_from_char(code: u32) -> u8 {
    if code == 0 {
        return 38;
    }
    let mapped = ((code & 0x7F) as u8).saturating_add(8);
    mapped.max(8)
}

fn linux_x11_pointer_state_mask(buttons: u8) -> u16 {
    let mut mask = 0u16;
    if (buttons & 0x01) != 0 {
        mask |= 1 << 8;
    }
    if (buttons & 0x02) != 0 {
        mask |= 1 << 10;
    }
    mask
}

pub fn linux_gfx_bridge_input_pending() -> usize {
    unsafe {
        LINUX_GFX_BRIDGE.event_count
    }
}

fn linux_x11_pump_bridge_events(state: &mut LinuxShimState, sock_idx: usize) {
    if sock_idx >= state.sockets.len() {
        return;
    }
    if !state.sockets[sock_idx].active
        || state.sockets[sock_idx].endpoint != LINUX_SOCKET_ENDPOINT_X11
        || state.sockets[sock_idx].x11_state != LINUX_X11_STATE_READY
    {
        return;
    }
    linux_x11_ensure_root_window(state);
    let little = linux_x11_little(&state.sockets[sock_idx]);
    let mut pumped = 0usize;
    while pumped < 12 {
        let Some(ev) = linux_gfx_bridge_pop_input_event() else {
            break;
        };
        pumped += 1;
        let target = linux_x11_pick_input_window(state);
        let root_x = ev.x.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let root_y = ev.y.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        state.x11_pointer_x = root_x;
        state.x11_pointer_y = root_y;
        if ev.kind == 1 {
            let old_buttons = state.x11_pointer_buttons;
            let new_buttons = ev.down & 0x03;
            let old_state = linux_x11_pointer_state_mask(old_buttons);
            let new_state = linux_x11_pointer_state_mask(new_buttons);
            state.x11_pointer_buttons = new_buttons;

            let mut motion = [0u8; 28];
            linux_write_u32_order(&mut motion, 0, LINUX_X11_ROOT_WINDOW, little);
            linux_write_u32_order(&mut motion, 4, target, little);
            linux_write_u16_order(&mut motion, 12, root_x as u16, little);
            linux_write_u16_order(&mut motion, 14, root_y as u16, little);
            linux_write_u16_order(&mut motion, 16, root_x as u16, little);
            linux_write_u16_order(&mut motion, 18, root_y as u16, little);
            linux_write_u16_order(&mut motion, 20, new_state, little);
            motion[22] = 1;
            linux_x11_queue_window_event(
                state,
                sock_idx,
                target,
                LINUX_X11_EVENT_MOTION_NOTIFY,
                0,
                LINUX_X11_EVENT_MASK_POINTER_MOTION,
                &motion,
            );

            let left_changed = (old_buttons ^ new_buttons) & 0x01;
            if left_changed != 0 {
                let press = (new_buttons & 0x01) != 0;
                let mut body = [0u8; 28];
                linux_write_u32_order(&mut body, 0, LINUX_X11_ROOT_WINDOW, little);
                linux_write_u32_order(&mut body, 4, target, little);
                linux_write_u16_order(&mut body, 12, root_x as u16, little);
                linux_write_u16_order(&mut body, 14, root_y as u16, little);
                linux_write_u16_order(&mut body, 16, root_x as u16, little);
                linux_write_u16_order(&mut body, 18, root_y as u16, little);
                linux_write_u16_order(
                    &mut body,
                    20,
                    if press { old_state } else { new_state },
                    little,
                );
                body[22] = 1;
                linux_x11_queue_window_event(
                    state,
                    sock_idx,
                    target,
                    if press {
                        LINUX_X11_EVENT_BUTTON_PRESS
                    } else {
                        LINUX_X11_EVENT_BUTTON_RELEASE
                    },
                    1,
                    if press {
                        LINUX_X11_EVENT_MASK_BUTTON_PRESS
                    } else {
                        LINUX_X11_EVENT_MASK_BUTTON_RELEASE
                    },
                    &body,
                );
                state.x11_last_button = 1;
            }
            let right_changed = (old_buttons ^ new_buttons) & 0x02;
            if right_changed != 0 {
                let press = (new_buttons & 0x02) != 0;
                let mut body = [0u8; 28];
                linux_write_u32_order(&mut body, 0, LINUX_X11_ROOT_WINDOW, little);
                linux_write_u32_order(&mut body, 4, target, little);
                linux_write_u16_order(&mut body, 12, root_x as u16, little);
                linux_write_u16_order(&mut body, 14, root_y as u16, little);
                linux_write_u16_order(&mut body, 16, root_x as u16, little);
                linux_write_u16_order(&mut body, 18, root_y as u16, little);
                linux_write_u16_order(
                    &mut body,
                    20,
                    if press { old_state } else { new_state },
                    little,
                );
                body[22] = 1;
                linux_x11_queue_window_event(
                    state,
                    sock_idx,
                    target,
                    if press {
                        LINUX_X11_EVENT_BUTTON_PRESS
                    } else {
                        LINUX_X11_EVENT_BUTTON_RELEASE
                    },
                    3,
                    if press {
                        LINUX_X11_EVENT_MASK_BUTTON_PRESS
                    } else {
                        LINUX_X11_EVENT_MASK_BUTTON_RELEASE
                    },
                    &body,
                );
                state.x11_last_button = 3;
            }
        } else if ev.kind == 2 {
            let keycode = linux_x11_keycode_from_char(ev.code);
            state.x11_last_keycode = keycode;
            let mut body = [0u8; 28];
            linux_write_u32_order(&mut body, 0, LINUX_X11_ROOT_WINDOW, little);
            linux_write_u32_order(&mut body, 4, target, little);
            linux_write_u16_order(&mut body, 12, state.x11_pointer_x as u16, little);
            linux_write_u16_order(&mut body, 14, state.x11_pointer_y as u16, little);
            linux_write_u16_order(&mut body, 16, state.x11_pointer_x as u16, little);
            linux_write_u16_order(&mut body, 18, state.x11_pointer_y as u16, little);
            linux_write_u16_order(
                &mut body,
                20,
                linux_x11_pointer_state_mask(state.x11_pointer_buttons),
                little,
            );
            body[22] = 1;
            linux_x11_queue_window_event(
                state,
                sock_idx,
                target,
                if ev.down != 0 {
                    LINUX_X11_EVENT_KEY_PRESS
                } else {
                    LINUX_X11_EVENT_KEY_RELEASE
                },
                keycode,
                if ev.down != 0 {
                    LINUX_X11_EVENT_MASK_KEY_PRESS
                } else {
                    LINUX_X11_EVENT_MASK_KEY_RELEASE
                },
                &body,
            );
        }
    }
    if pumped > 0 {
        linux_gfx_bridge_set_status("X11 subset: input/eventos entregados al cliente.");
    }
}

fn linux_x11_fill_rect(x: i32, y: i32, w: u16, h: u16, color: u32) {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active {
            return;
        }
        let bw = (state.width as i32).max(1);
        let bh = (state.height as i32).max(1);
        let x0 = x.clamp(0, bw);
        let y0 = y.clamp(0, bh);
        let x1 = x.saturating_add(w as i32).clamp(0, bw);
        let y1 = y.saturating_add(h as i32).clamp(0, bh);
        let width = state.width as usize;
        let mut yy = y0;
        while yy < y1 {
            let row = yy as usize * width;
            let mut xx = x0;
            while xx < x1 {
                let idx = row + xx as usize;
                if idx < LINUX_GFX_PIXELS.len() {
                    LINUX_GFX_PIXELS[idx] = color;
                }
                xx += 1;
            }
            yy += 1;
        }
        state.frame_seq = state.frame_seq.saturating_add(1);
        state.dirty = true;
        linux_gfx_bridge_present_direct_locked(state);
    }
}

fn linux_x11_blit_put_image(state: &mut LinuxShimState, req: &[u8], little: bool) {
    if req.len() < 24 {
        return;
    }
    let drawable = linux_read_u32_order(req, 4, little);
    let width = linux_read_u16_order(req, 12, little) as usize;
    let height = linux_read_u16_order(req, 14, little) as usize;
    let dst_x = linux_read_u16_order(req, 16, little) as i16 as i32;
    let dst_y = linux_read_u16_order(req, 18, little) as i16 as i32;
    let depth = req[21];
    if width == 0 || height == 0 {
        return;
    }
    let data = &req[24..];
    let bpp = if depth >= 24 { 4usize } else { 1usize };
    let row_bytes = width.saturating_mul(bpp);
    if row_bytes == 0 {
        return;
    }

    let mut touched_bridge = false;
    let mut y = 0usize;
    while y < height {
        if y.saturating_mul(row_bytes) >= data.len() {
            break;
        }
        let row_start = y * row_bytes;
        let mut x = 0usize;
        while x < width {
            let src = row_start.saturating_add(x.saturating_mul(bpp));
            if src >= data.len() {
                break;
            }
            let color = if bpp >= 4 && src + 3 < data.len() {
                let b = data[src] as u32;
                let g = data[src + 1] as u32;
                let r = data[src + 2] as u32;
                (r << 16) | (g << 8) | b
            } else {
                let v = data[src] as u32;
                (v << 16) | (v << 8) | v
            };
            if linux_x11_drawable_set_pixel(
                state,
                drawable,
                dst_x.saturating_add(x as i32),
                dst_y.saturating_add(y as i32),
                color,
            ) {
                if linux_x11_find_window_index(state, drawable).is_some() {
                    touched_bridge = true;
                }
            }
            x += 1;
        }
        y += 1;
    }
    if touched_bridge {
        linux_x11_mark_bridge_dirty();
    }
}

fn linux_x11_copy_area(
    state: &mut LinuxShimState,
    src_drawable: u32,
    dst_drawable: u32,
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    width: u16,
    height: u16,
) {
    if width == 0 || height == 0 {
        return;
    }
    let copy_w = (width as usize).min(LINUX_GFX_MAX_WIDTH);
    let copy_h = (height as usize).min(LINUX_GFX_MAX_HEIGHT);
    if copy_w == 0 || copy_h == 0 {
        return;
    }

    let mut staging = Vec::new();
    staging.resize(copy_w.saturating_mul(copy_h), 0u32);

    let mut y = 0usize;
    while y < copy_h {
        let mut x = 0usize;
        while x < copy_w {
            let color = linux_x11_drawable_get_pixel(
                state,
                src_drawable,
                src_x.saturating_add(x as i32),
                src_y.saturating_add(y as i32),
            )
            .unwrap_or(0);
            let idx = y.saturating_mul(copy_w).saturating_add(x);
            if idx < staging.len() {
                staging[idx] = color;
            }
            x += 1;
        }
        y += 1;
    }

    let mut touched_bridge = false;
    y = 0;
    while y < copy_h {
        let mut x = 0usize;
        while x < copy_w {
            let idx = y.saturating_mul(copy_w).saturating_add(x);
            if idx < staging.len()
                && linux_x11_drawable_set_pixel(
                    state,
                    dst_drawable,
                    dst_x.saturating_add(x as i32),
                    dst_y.saturating_add(y as i32),
                    staging[idx],
                )
                && linux_x11_find_window_index(state, dst_drawable).is_some()
            {
                touched_bridge = true;
            }
            x += 1;
        }
        y += 1;
    }
    if touched_bridge {
        linux_x11_mark_bridge_dirty();
    }
}

fn linux_x11_handle_extension_request(
    state: &mut LinuxShimState,
    sock_idx: usize,
    major: u8,
    minor: u8,
    req: &[u8],
) {
    if sock_idx >= state.sockets.len() {
        return;
    }
    let little = linux_x11_little(&state.sockets[sock_idx]);
    let mut body = [0u8; 24];
    match major {
        LINUX_X11_EXT_MIT_SHM => match minor {
            0 => {
                linux_write_u16_order(&mut body, 0, 1, little);
                linux_write_u16_order(&mut body, 2, 2, little);
                linux_write_u16_order(&mut body, 4, 0, little);
                linux_write_u16_order(&mut body, 6, 0, little);
                body[8] = 0;
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
            1 => {
                // ShmAttach
                if req.len() >= 16 {
                    let shmseg = linux_read_u32_order(req, 4, little);
                    let shmid = linux_read_u32_order(req, 8, little);
                    let read_only = req[12] != 0;
                    let mut i = 0usize;
                    let mut free_slot = None;
                    while i < LINUX_X11_MAX_SHM_SEGMENTS {
                        if !state.x11_shm_segments[i].active {
                            if free_slot.is_none() {
                                free_slot = Some(i);
                            }
                        } else if state.x11_shm_segments[i].shmseg == shmseg {
                            free_slot = Some(i);
                            break;
                        }
                        i += 1;
                    }
                    if let Some(idx) = free_slot {
                        state.x11_shm_segments[idx] = LinuxX11ShmSlot {
                            active: true,
                            shmseg,
                            shmid,
                            read_only,
                        };
                    }
                }
            }
            2 => {
                // ShmDetach
                if req.len() >= 8 {
                    let shmseg = linux_read_u32_order(req, 4, little);
                    let mut i = 0usize;
                    while i < LINUX_X11_MAX_SHM_SEGMENTS {
                        if state.x11_shm_segments[i].active && state.x11_shm_segments[i].shmseg == shmseg {
                            state.x11_shm_segments[i] = LinuxX11ShmSlot::empty();
                            break;
                        }
                        i += 1;
                    }
                }
            }
            3 => {
                // ShmPutImage
                if req.len() >= 40 {
                    let drawable = linux_read_u32_order(req, 4, little);
                    let _gc = linux_read_u32_order(req, 8, little);
                    let total_w = linux_read_u16_order(req, 12, little) as usize;
                    let total_h = linux_read_u16_order(req, 14, little) as usize;
                    let src_x = linux_read_u16_order(req, 16, little) as u32 as usize;
                    let src_y = linux_read_u16_order(req, 18, little) as u32 as usize;
                    let src_w = linux_read_u16_order(req, 20, little) as usize;
                    let src_h = linux_read_u16_order(req, 22, little) as usize;
                    let dst_x = linux_read_u16_order(req, 24, little) as i16 as i32;
                    let dst_y = linux_read_u16_order(req, 26, little) as i16 as i32;
                    let _depth = req[28];
                    let _format = req[29];
                    let send_event = req[30] != 0;
                    let shmseg = linux_read_u32_order(req, 32, little);
                    let offset = linux_read_u32_order(req, 36, little) as usize;

                    let mut shm_ptr = 0u64;
                    let mut i = 0usize;
                    while i < LINUX_X11_MAX_SHM_SEGMENTS {
                        if state.x11_shm_segments[i].active && state.x11_shm_segments[i].shmseg == shmseg {
                            let shmid = state.x11_shm_segments[i].shmid;
                            // shmid usually corresponds to the ID mapped via sys_shmat
                            // we scan mmap slots for a match (sys_shmat maps it into the process)
                            let mut m = 0usize;
                            while m < LINUX_MAX_MMAPS {
                                if state.maps[m].active && state.maps[m].process_pid == state.current_pid {
                                    // Normally we would track shmid directly, but sys_shmat
                                    // maps anonymous shared memory for the current process.
                                    // For this shim, we will assume the largest MAP_SHARED
                                    // segment or just fallback to scanning.
                                    if (state.maps[m].flags & LINUX_MAP_SHARED) != 0 && state.maps[m].len > 0 {
                                        // A heuristic since shmat doesn't preserve shmid in maps directly
                                        if state.maps[m].len >= (total_w * total_h * 4) as u64 {
                                            shm_ptr = state.maps[m].addr;
                                            break;
                                        }
                                    }
                                }
                                m += 1;
                            }
                            break;
                        }
                        i += 1;
                    }

                    if shm_ptr != 0 && src_w > 0 && src_h > 0 {
                        let mut copy_y = 0usize;
                        while copy_y < src_h {
                            let sy = src_y.saturating_add(copy_y);
                            if sy >= total_h { break; }
                            
                            let mut copy_x = 0usize;
                            while copy_x < src_w {
                                let sx = src_x.saturating_add(copy_x);
                                if sx >= total_w { break; }

                                let pixel_offset = offset + (sy * total_w + sx) * 4;
                                let color = unsafe { 
                                    core::ptr::read_volatile((shm_ptr + pixel_offset as u64) as *const u32) 
                                };
                                
                                linux_x11_drawable_set_pixel(
                                    state,
                                    drawable,
                                    dst_x.saturating_add(copy_x as i32),
                                    dst_y.saturating_add(copy_y as i32),
                                    color,
                                );
                                copy_x += 1;
                            }
                            copy_y += 1;
                        }
                        
                        if linux_x11_find_window_index(state, drawable).is_some() {
                            linux_x11_mark_bridge_dirty();
                        }
                    }

                    if send_event {
                        let mut ev = [0u8; 28];
                        ev[0] = drawable as u8;
                        linux_write_u16_order(&mut ev, 2, state.sockets[sock_idx].x11_seq, little);
                        linux_write_u32_order(&mut ev, 4, drawable, little);
                        linux_write_u16_order(&mut ev, 8, minor as u16, little); // request=ShmPutImage
                        linux_write_u16_order(&mut ev, 10, major as u16, little);
                        linux_write_u32_order(&mut ev, 12, shmseg, little);
                        linux_write_u32_order(&mut ev, 16, offset as u32, little);
                        linux_x11_queue_window_event(state, sock_idx, drawable, LINUX_X11_EXT_MIT_SHM + 33, 0, 0, &ev); // ShmCompletion event
                    }
                }
            }
            4 => {
                // ShmCreatePixmap
                linux_gfx_bridge_set_status("X11 MIT-SHM subset: request procesado.");
            }
            _ => {}
        },
        LINUX_X11_EXT_BIGREQ => {
            if minor == 0 {
                linux_write_u32_order(&mut body, 0, 0x00FF_FFFF, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
                state.sockets[sock_idx].x11_bigreq = true;
            }
        }
        LINUX_X11_EXT_RANDR => match minor {
            0 => {
                let req_major = if req.len() >= 8 {
                    linux_read_u32_order(req, 4, little) as u16
                } else {
                    1
                };
                linux_write_u32_order(&mut body, 0, req_major.min(1) as u32, little);
                linux_write_u32_order(&mut body, 4, 6, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 1, &body);
            }
            1 | 4 | 8 | 20 | 21 | 26 | 40 => {
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
            _ => {}
        },
        LINUX_X11_EXT_RENDER => match minor {
            0 => {
                linux_write_u32_order(&mut body, 0, 0, little);
                linux_write_u32_order(&mut body, 4, 11, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
            1 | 28 => {
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
            _ => {}
        },
        LINUX_X11_EXT_XFIXES => {
            if minor == 0 {
                linux_write_u32_order(&mut body, 0, 5, little);
                linux_write_u32_order(&mut body, 4, 0, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        LINUX_X11_EXT_SHAPE => {
            if minor == 0 {
                linux_write_u16_order(&mut body, 0, 1, little);
                linux_write_u16_order(&mut body, 2, 1, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        LINUX_X11_EXT_SYNC => {
            if minor == 0 {
                linux_write_u32_order(&mut body, 0, 3, little);
                linux_write_u32_order(&mut body, 4, 1, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        LINUX_X11_EXT_XTEST => {
            if minor == 0 {
                linux_write_u16_order(&mut body, 0, 2, little);
                linux_write_u16_order(&mut body, 2, 2, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        LINUX_X11_EXT_XINPUT => {
            if minor == 47 {
                let mut req_major = 2u16;
                let mut req_minor = 0u16;
                if req.len() >= 8 {
                    req_major = linux_read_u16_order(req, 4, little);
                    req_minor = linux_read_u16_order(req, 6, little);
                }
                linux_write_u16_order(&mut body, 0, req_major.min(2), little);
                linux_write_u16_order(&mut body, 2, req_minor.min(3), little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        _ => {}
    }
}

fn linux_x11_handle_request(state: &mut LinuxShimState, sock_idx: usize, opcode: u8, req: &[u8]) {
    if sock_idx >= state.sockets.len() {
        return;
    }
    let little = linux_x11_little(&state.sockets[sock_idx]);
    if opcode >= 128 {
        let minor = if req.len() > 1 { req[1] } else { 0 };
        linux_x11_handle_extension_request(state, sock_idx, opcode, minor, req);
        return;
    }
    linux_x11_ensure_root_window(state);

    match opcode {
        1 => {
            // CreateWindow
            if req.len() >= 32 {
                let wid = linux_read_u32_order(req, 4, little);
                if wid != 0 {
                    let parent_raw = linux_read_u32_order(req, 8, little);
                    let parent = if linux_x11_find_window_index(state, parent_raw).is_some() {
                        parent_raw
                    } else {
                        LINUX_X11_ROOT_WINDOW
                    };
                    let x = linux_read_u16_order(req, 12, little) as i16;
                    let y = linux_read_u16_order(req, 14, little) as i16;
                    let width = linux_read_u16_order(req, 16, little).max(1);
                    let height = linux_read_u16_order(req, 18, little).max(1);
                    let border = linux_read_u16_order(req, 20, little);
                    let class_hint = linux_read_u16_order(req, 22, little);
                    let visual = linux_read_u32_order(req, 24, little);
                    let value_mask = linux_read_u32_order(req, 28, little);
                    let mut event_mask = 0u32;
                    let mut override_redirect = false;
                    let mut value_off = 32usize;
                    let mut bit = 0u32;
                    while bit < 32 {
                        if (value_mask & (1u32 << bit)) != 0 {
                            if value_off + 4 > req.len() {
                                break;
                            }
                            let val = linux_read_u32_order(req, value_off, little);
                            if bit == 11 {
                                event_mask = val;
                            } else if bit == 9 {
                                override_redirect = val != 0;
                            }
                            value_off += 4;
                        }
                        bit += 1;
                    }
                    let idx = linux_x11_find_window_index(state, wid)
                        .or_else(|| linux_x11_alloc_window_index(state));
                    if let Some(idx) = idx {
                        state.x11_windows[idx] = LinuxX11WindowSlot {
                            active: true,
                            id: wid,
                            parent,
                            x,
                            y,
                            width,
                            height,
                            border,
                            class_hint,
                            mapped: false,
                            override_redirect,
                            _pad0: [0; 2],
                            visual: if visual == 0 { LINUX_X11_VISUAL_TRUECOLOR } else { visual },
                            event_mask,
                        };
                        linux_x11_set_property_u32_list(
                            state,
                            wid,
                            LINUX_X11_ATOM_NET_WM_WINDOW_TYPE,
                            LINUX_X11_ATOM_ATOM,
                            &[LINUX_X11_ATOM_NET_WM_WINDOW_TYPE_NORMAL],
                        );
                        linux_x11_set_property_u32_list(
                            state,
                            wid,
                            LINUX_X11_ATOM_NET_WM_PID,
                            LINUX_X11_ATOM_CARDINAL,
                            &[state.tid_value as u32],
                        );
                        linux_x11_set_property_u32_list(
                            state,
                            wid,
                            LINUX_X11_ATOM_WM_PROTOCOLS,
                            LINUX_X11_ATOM_ATOM,
                            &[LINUX_X11_ATOM_WM_DELETE_WINDOW],
                        );
                        linux_x11_refresh_client_list(state);
                    }
                }
            }
        }
        2 => {
            // ChangeWindowAttributes
            if req.len() >= 12 {
                let wid = linux_read_u32_order(req, 4, little);
                if let Some(idx) = linux_x11_find_window_index(state, wid) {
                    let value_mask = linux_read_u32_order(req, 8, little);
                    let mut value_off = 12usize;
                    let mut bit = 0u32;
                    while bit < 32 {
                        if (value_mask & (1u32 << bit)) != 0 {
                            if value_off + 4 > req.len() {
                                break;
                            }
                            let val = linux_read_u32_order(req, value_off, little);
                            if bit == 11 {
                                state.x11_windows[idx].event_mask = val;
                            } else if bit == 9 {
                                state.x11_windows[idx].override_redirect = val != 0;
                            }
                            value_off += 4;
                        }
                        bit += 1;
                    }
                }
            }
        }
        3 => {
            // GetWindowAttributes
            let mut body = [0u8; 24];
            let mut extra = [0u8; 12];
            let mut win = LinuxX11WindowSlot::empty();
            if req.len() >= 8 {
                let wid = linux_read_u32_order(req, 4, little);
                if let Some(idx) = linux_x11_find_window_index(state, wid) {
                    win = state.x11_windows[idx];
                }
            }
            if !win.active {
                if let Some(root_idx) = linux_x11_find_window_index(state, LINUX_X11_ROOT_WINDOW) {
                    win = state.x11_windows[root_idx];
                }
            }
            linux_write_u32_order(&mut body, 0, win.visual, little);
            linux_write_u16_order(&mut body, 4, win.class_hint.max(1), little);
            body[16] = 0;
            body[17] = 1;
            body[18] = if win.mapped { 2 } else { 0 };
            body[19] = if win.override_redirect { 1 } else { 0 };
            linux_write_u32_order(&mut body, 20, 0, little);
            linux_write_u32_order(&mut extra, 0, 0xFFFF_FFFF, little);
            linux_write_u32_order(&mut extra, 4, win.event_mask, little);
            linux_write_u16_order(&mut extra, 8, 0, little);
            linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body, &extra);
        }
        4 => {
            // DestroyWindow
            if req.len() >= 8 {
                let wid = linux_read_u32_order(req, 4, little);
                if wid != LINUX_X11_ROOT_WINDOW {
                    if let Some(idx) = linux_x11_find_window_index(state, wid) {
                        state.x11_windows[idx] = LinuxX11WindowSlot::empty();
                        linux_x11_remove_window_properties(state, wid);
                        let mut p = 0usize;
                        while p < state.x11_pixmaps.len() {
                            if state.x11_pixmaps[p].active && state.x11_pixmaps[p].drawable == wid {
                                state.x11_pixmaps[p] = LinuxX11PixmapSlot::empty();
                                linux_x11_clear_pixmap_storage(p);
                            }
                            p += 1;
                        }
                        let mut g = 0usize;
                        while g < state.x11_gcs.len() {
                            if state.x11_gcs[g].active && state.x11_gcs[g].drawable == wid {
                                state.x11_gcs[g] = LinuxX11GcSlot::empty();
                            }
                            g += 1;
                        }
                        if state.x11_focus_window == wid {
                            state.x11_focus_window = LINUX_X11_ROOT_WINDOW;
                            linux_x11_update_active_window_property(state);
                        }
                        linux_x11_refresh_client_list(state);
                        let mut ev = [0u8; 28];
                        linux_write_u32_order(&mut ev, 0, wid, little);
                        linux_write_u32_order(&mut ev, 4, wid, little);
                        linux_x11_queue_window_event(
                            state,
                            sock_idx,
                            wid,
                            LINUX_X11_EVENT_DESTROY_NOTIFY,
                            0,
                            LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY,
                            &ev,
                        );
                    }
                }
            }
        }
        7 => {
            // ReparentWindow
            if req.len() >= 16 {
                let wid = linux_read_u32_order(req, 4, little);
                let parent = linux_read_u32_order(req, 8, little);
                let x = linux_read_u16_order(req, 12, little) as i16;
                let y = linux_read_u16_order(req, 14, little) as i16;
                if let Some(idx) = linux_x11_find_window_index(state, wid) {
                    state.x11_windows[idx].parent = if linux_x11_find_window_index(state, parent).is_some() {
                        parent
                    } else {
                        LINUX_X11_ROOT_WINDOW
                    };
                    state.x11_windows[idx].x = x;
                    state.x11_windows[idx].y = y;
                }
            }
        }
        8 => {
            // MapWindow
            if req.len() >= 8 {
                let wid = linux_read_u32_order(req, 4, little);
                if let Some(idx) = linux_x11_find_window_index(state, wid) {
                    state.x11_windows[idx].mapped = true;
                    state.x11_focus_window = wid;
                    linux_x11_update_active_window_property(state);
                    let mut map = [0u8; 28];
                    linux_write_u32_order(&mut map, 0, wid, little);
                    linux_write_u32_order(&mut map, 4, wid, little);
                    map[8] = if state.x11_windows[idx].override_redirect { 1 } else { 0 };
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        wid,
                        LINUX_X11_EVENT_MAP_NOTIFY,
                        0,
                        LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY,
                        &map,
                    );
                    let mut expose = [0u8; 28];
                    linux_write_u32_order(&mut expose, 0, wid, little);
                    linux_write_u16_order(&mut expose, 4, 0, little);
                    linux_write_u16_order(&mut expose, 6, 0, little);
                    linux_write_u16_order(&mut expose, 8, state.x11_windows[idx].width, little);
                    linux_write_u16_order(&mut expose, 10, state.x11_windows[idx].height, little);
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        wid,
                        LINUX_X11_EVENT_EXPOSE,
                        0,
                        LINUX_X11_EVENT_MASK_EXPOSURE,
                        &expose,
                    );
                }
            }
        }
        10 => {
            // UnmapWindow
            if req.len() >= 8 {
                let wid = linux_read_u32_order(req, 4, little);
                if let Some(idx) = linux_x11_find_window_index(state, wid) {
                    state.x11_windows[idx].mapped = false;
                    if state.x11_focus_window == wid {
                        state.x11_focus_window = LINUX_X11_ROOT_WINDOW;
                        linux_x11_update_active_window_property(state);
                    }
                    let mut ev = [0u8; 28];
                    linux_write_u32_order(&mut ev, 0, wid, little);
                    linux_write_u32_order(&mut ev, 4, wid, little);
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        wid,
                        LINUX_X11_EVENT_UNMAP_NOTIFY,
                        0,
                        LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY,
                        &ev,
                    );
                }
            }
        }
        12 => {
            // ConfigureWindow
            if req.len() >= 12 {
                let wid = linux_read_u32_order(req, 4, little);
                let value_mask = linux_read_u16_order(req, 8, little) as u32;
                if let Some(idx) = linux_x11_find_window_index(state, wid) {
                    let mut off = 12usize;
                    let mut bit = 0u32;
                    while bit <= 6 {
                        if (value_mask & (1u32 << bit)) != 0 {
                            if off + 4 > req.len() {
                                break;
                            }
                            let val = linux_read_u32_order(req, off, little);
                            match bit {
                                0 => state.x11_windows[idx].x = val as i32 as i16,
                                1 => state.x11_windows[idx].y = val as i32 as i16,
                                2 => state.x11_windows[idx].width = (val as u16).max(1),
                                3 => state.x11_windows[idx].height = (val as u16).max(1),
                                4 => state.x11_windows[idx].border = val as u16,
                                _ => {}
                            }
                            off += 4;
                        }
                        bit += 1;
                    }
                    let mut ev = [0u8; 28];
                    linux_write_u32_order(&mut ev, 0, wid, little);
                    linux_write_u32_order(&mut ev, 4, wid, little);
                    linux_write_u16_order(&mut ev, 8, state.x11_windows[idx].x as u16, little);
                    linux_write_u16_order(&mut ev, 10, state.x11_windows[idx].y as u16, little);
                    linux_write_u16_order(&mut ev, 12, state.x11_windows[idx].width, little);
                    linux_write_u16_order(&mut ev, 14, state.x11_windows[idx].height, little);
                    linux_write_u16_order(&mut ev, 16, state.x11_windows[idx].border, little);
                    ev[20] = if state.x11_windows[idx].override_redirect { 1 } else { 0 };
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        wid,
                        LINUX_X11_EVENT_CONFIGURE_NOTIFY,
                        0,
                        LINUX_X11_EVENT_MASK_STRUCTURE_NOTIFY,
                        &ev,
                    );
                }
            }
        }
        14 => {
            // GetGeometry
            let mut body = [0u8; 24];
            let mut target = LINUX_X11_ROOT_WINDOW;
            if req.len() >= 8 {
                target = linux_read_u32_order(req, 4, little);
            }
            let mut x = 0i16;
            let mut y = 0i16;
            let mut w = LINUX_GFX_MAX_WIDTH as u16;
            let mut h = LINUX_GFX_MAX_HEIGHT as u16;
            let mut border = 0u16;
            if let Some(idx) = linux_x11_find_window_index(state, target) {
                x = state.x11_windows[idx].x;
                y = state.x11_windows[idx].y;
                w = state.x11_windows[idx].width;
                h = state.x11_windows[idx].height;
                border = state.x11_windows[idx].border;
            }
            linux_write_u32_order(&mut body, 0, LINUX_X11_ROOT_WINDOW, little);
            linux_write_u16_order(&mut body, 8, x as u16, little);
            linux_write_u16_order(&mut body, 10, y as u16, little);
            linux_write_u16_order(&mut body, 12, w, little);
            linux_write_u16_order(&mut body, 14, h, little);
            linux_write_u16_order(&mut body, 16, border, little);
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 24, &body);
        }
        15 => {
            // QueryTree
            let mut body = [0u8; 24];
            let mut children = [0u32; 64];
            let win = if req.len() >= 8 {
                linux_read_u32_order(req, 4, little)
            } else {
                LINUX_X11_ROOT_WINDOW
            };
            let mut parent = 0u32;
            if let Some(idx) = linux_x11_find_window_index(state, win) {
                parent = state.x11_windows[idx].parent;
            }
            let child_count = linux_x11_collect_children(state, win, &mut children);
            linux_write_u32_order(&mut body, 0, LINUX_X11_ROOT_WINDOW, little);
            linux_write_u32_order(&mut body, 4, parent, little);
            linux_write_u16_order(&mut body, 8, child_count as u16, little);
            let mut extra = [0u8; 256];
            let mut i = 0usize;
            while i < child_count && (i * 4 + 4) <= extra.len() {
                linux_write_u32_order(&mut extra, i * 4, children[i], little);
                i += 1;
            }
            linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body, &extra[..i * 4]);
        }
        16 => {
            // InternAtom
            let mut body = [0u8; 24];
            if req.len() >= 8 {
                let only_if_exists = req[1] != 0;
                let name_len = linux_read_u16_order(req, 4, little) as usize;
                if req.len() >= 8 + name_len {
                    let atom = linux_x11_atom_from_name(&req[8..8 + name_len], only_if_exists);
                    linux_write_u32_order(&mut body, 0, atom, little);
                }
            }
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        17 => {
            // GetAtomName
            let mut body = [0u8; 24];
            let mut name_buf = [0u8; 32];
            if req.len() >= 8 {
                let atom = linux_read_u32_order(req, 4, little);
                let name_len = linux_x11_atom_name_bytes(atom, &mut name_buf);
                linux_write_u16_order(&mut body, 0, name_len as u16, little);
                linux_x11_queue_reply(
                    &mut state.sockets[sock_idx],
                    0,
                    &body,
                    &name_buf[..name_len],
                );
            } else {
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        18 => {
            // ChangeProperty
            if req.len() >= 24 {
                let mode = req[1];
                let window = linux_read_u32_order(req, 4, little);
                let property = linux_read_u32_order(req, 8, little);
                let prop_type = linux_read_u32_order(req, 12, little);
                let format = req[16];
                let data_len_units = linux_read_u32_order(req, 20, little) as usize;
                let bpp = linux_x11_property_bytes_per(format);
                let data_bytes = data_len_units.saturating_mul(bpp);
                if req.len() >= 24 + data_bytes && data_bytes <= LINUX_X11_PROPERTY_DATA_MAX {
                    linux_x11_set_property(
                        state,
                        window,
                        property,
                        prop_type,
                        format,
                        mode,
                        &req[24..24 + data_bytes],
                    );
                    let mut ev = [0u8; 28];
                    linux_write_u32_order(&mut ev, 0, window, little);
                    linux_write_u32_order(&mut ev, 4, property, little);
                    linux_write_u32_order(&mut ev, 8, timer::ticks() as u32, little);
                    ev[12] = 0;
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        window,
                        LINUX_X11_EVENT_PROPERTY_NOTIFY,
                        0,
                        LINUX_X11_EVENT_MASK_PROPERTY_CHANGE,
                        &ev,
                    );
                }
            }
        }
        19 => {
            // DeleteProperty
            if req.len() >= 12 {
                let window = linux_read_u32_order(req, 4, little);
                let property = linux_read_u32_order(req, 8, little);
                if linux_x11_remove_property(state, window, property) {
                    let mut ev = [0u8; 28];
                    linux_write_u32_order(&mut ev, 0, window, little);
                    linux_write_u32_order(&mut ev, 4, property, little);
                    linux_write_u32_order(&mut ev, 8, timer::ticks() as u32, little);
                    ev[12] = 1;
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        window,
                        LINUX_X11_EVENT_PROPERTY_NOTIFY,
                        0,
                        LINUX_X11_EVENT_MASK_PROPERTY_CHANGE,
                        &ev,
                    );
                }
            }
        }
        20 => {
            // GetProperty
            let mut body = [0u8; 24];
            if req.len() >= 24 {
                let delete = req[1] != 0;
                let window = linux_read_u32_order(req, 4, little);
                let property = linux_read_u32_order(req, 8, little);
                let req_type = linux_read_u32_order(req, 12, little);
                let long_offset = linux_read_u32_order(req, 16, little) as usize;
                let long_length = linux_read_u32_order(req, 20, little) as usize;
                if let Some(prop_idx) = linux_x11_find_property_index(state, window, property) {
                    let prop = state.x11_properties[prop_idx];
                    let bpp = linux_x11_property_bytes_per(prop.format).max(1);
                    let start = long_offset.saturating_mul(4).min(prop.data_len);
                    let req_bytes = long_length.saturating_mul(4);
                    let mut send_bytes = prop.data_len.saturating_sub(start).min(req_bytes);
                    if bpp > 1 {
                        send_bytes -= send_bytes % bpp;
                    }
                    if req_type != 0 && req_type != prop.prop_type {
                        send_bytes = 0;
                    }
                    let bytes_after = prop.data_len.saturating_sub(start + send_bytes);
                    let nitems = if bpp == 0 { 0 } else { send_bytes / bpp };
                    linux_write_u32_order(&mut body, 0, prop.prop_type, little);
                    linux_write_u32_order(&mut body, 4, bytes_after as u32, little);
                    linux_write_u32_order(&mut body, 8, nitems as u32, little);
                    let mut extra = [0u8; LINUX_X11_PROPERTY_DATA_MAX];
                    let mut i = 0usize;
                    while i < send_bytes && i < extra.len() {
                        extra[i] = prop.data[start + i];
                        i += 1;
                    }
                    linux_x11_queue_reply(
                        &mut state.sockets[sock_idx],
                        if send_bytes == 0 { 0 } else { prop.format },
                        &body,
                        &extra[..send_bytes.min(extra.len())],
                    );
                    if delete && bytes_after == 0 && send_bytes > 0 {
                        state.x11_properties[prop_idx] = LinuxX11PropertySlot::empty();
                    }
                } else {
                    linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
                }
            } else {
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        21 => {
            // ListProperties
            let mut body = [0u8; 24];
            let window = if req.len() >= 8 {
                linux_read_u32_order(req, 4, little)
            } else {
                0
            };
            let mut atoms = [0u32; 64];
            let mut count = 0usize;
            let mut i = 0usize;
            while i < state.x11_properties.len() && count < atoms.len() {
                let prop = state.x11_properties[i];
                if prop.active && prop.window == window {
                    atoms[count] = prop.atom;
                    count += 1;
                }
                i += 1;
            }
            linux_write_u16_order(&mut body, 0, count as u16, little);
            let mut extra = [0u8; 256];
            i = 0;
            while i < count && i * 4 + 4 <= extra.len() {
                linux_write_u32_order(&mut extra, i * 4, atoms[i], little);
                i += 1;
            }
            linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body, &extra[..i * 4]);
        }
        22 => {
            // SetSelectionOwner
            if req.len() >= 16 {
                let owner = linux_read_u32_order(req, 4, little);
                let selection = linux_read_u32_order(req, 8, little);
                linux_x11_set_selection_owner(state, selection, owner);
            }
        }
        23 => {
            // GetSelectionOwner
            let mut body = [0u8; 24];
            if req.len() >= 8 {
                let selection = linux_read_u32_order(req, 4, little);
                let owner = linux_x11_get_selection_owner(state, selection);
                linux_write_u32_order(&mut body, 0, owner, little);
            }
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        24 => {
            // ConvertSelection
            if req.len() >= 24 {
                let requestor = linux_read_u32_order(req, 4, little);
                let selection = linux_read_u32_order(req, 8, little);
                let target = linux_read_u32_order(req, 12, little);
                let property = linux_read_u32_order(req, 16, little);
                if property != 0 {
                    if target == LINUX_X11_ATOM_TARGETS {
                        let mut targets = [0u8; 12];
                        linux_write_u32_order(&mut targets, 0, LINUX_X11_ATOM_UTF8_STRING, little);
                        linux_write_u32_order(&mut targets, 4, LINUX_X11_ATOM_STRING, little);
                        linux_write_u32_order(&mut targets, 8, LINUX_X11_ATOM_TARGETS, little);
                        linux_x11_set_property(
                            state,
                            requestor,
                            property,
                            LINUX_X11_ATOM_ATOM,
                            32,
                            0,
                            &targets,
                        );
                    } else if target == LINUX_X11_ATOM_UTF8_STRING || target == LINUX_X11_ATOM_STRING {
                        linux_x11_set_property(
                            state,
                            requestor,
                            property,
                            target,
                            8,
                            0,
                            b"ReduxOS Linux bridge clipboard",
                        );
                    } else {
                        let owner = linux_x11_get_selection_owner(state, selection);
                        if owner == 0 {
                            linux_x11_remove_property(state, requestor, property);
                        }
                    }
                }
                let mut ev = [0u8; 28];
                linux_write_u32_order(&mut ev, 0, timer::ticks() as u32, little);
                linux_write_u32_order(&mut ev, 4, requestor, little);
                linux_write_u32_order(&mut ev, 8, selection, little);
                linux_write_u32_order(&mut ev, 12, target, little);
                linux_write_u32_order(&mut ev, 16, property, little);
                linux_x11_queue_window_event(
                    state,
                    sock_idx,
                    requestor,
                    LINUX_X11_EVENT_SELECTION_NOTIFY,
                    0,
                    0,
                    &ev,
                );
            }
        }
        25 => {
            // SendEvent
            if req.len() >= 44 {
                let destination = linux_read_u32_order(req, 4, little);
                let event_mask = linux_read_u32_order(req, 8, little);
                let mut target = destination;
                if target == 0 {
                    // PointerWindow
                    target = linux_x11_pick_input_window(state);
                } else if target == 1 {
                    // InputFocus
                    target = state.x11_focus_window;
                }
                let raw = &req[12..44];
                let event_type = raw[0] & 0x7F;
                let detail = raw[1];
                let mut body = [0u8; 28];
                let mut i = 0usize;
                while i < 28 {
                    body[i] = raw[4 + i];
                    i += 1;
                }
                let needed_mask = if event_mask != 0 {
                    event_mask
                } else {
                    linux_x11_event_mask_for_type(event_type)
                };
                linux_x11_queue_window_event(
                    state,
                    sock_idx,
                    target,
                    event_type,
                    detail,
                    needed_mask,
                    &body,
                );

                // Minimal WM behavior for common ClientMessage paths used by Electron/GTK.
                if event_type == LINUX_X11_EVENT_CLIENT_MESSAGE {
                    let window = linux_read_u32_order(&body, 0, little);
                    let message_type = linux_read_u32_order(&body, 4, little);
                    if message_type == LINUX_X11_ATOM_NET_ACTIVE_WINDOW {
                        if linux_x11_find_window_index(state, window).is_some() {
                            state.x11_focus_window = window;
                            linux_x11_update_active_window_property(state);
                        }
                    } else if message_type == LINUX_X11_ATOM_NET_WM_STATE {
                        let action = linux_read_u32_order(&body, 8, little);
                        let atom1 = linux_read_u32_order(&body, 12, little);
                        let atom2 = linux_read_u32_order(&body, 16, little);
                        let mut current = [0u32; 8];
                        let mut count = 0usize;
                        if let Some(prop_idx) =
                            linux_x11_find_property_index(state, window, LINUX_X11_ATOM_NET_WM_STATE)
                        {
                            let prop = state.x11_properties[prop_idx];
                            let max = (prop.data_len / 4).min(current.len());
                            let mut i = 0usize;
                            while i < max {
                                current[i] = linux_read_u32_order(prop.data.as_slice(), i * 4, true);
                                i += 1;
                            }
                            count = max;
                        }
                        let mut apply_atom = |atom: u32| {
                            if atom == 0 {
                                return;
                            }
                            let mut found = None;
                            let mut i = 0usize;
                            while i < count {
                                if current[i] == atom {
                                    found = Some(i);
                                    break;
                                }
                                i += 1;
                            }
                            let want_add = match action {
                                0 => false, // remove
                                1 => true,  // add
                                2 => found.is_none(), // toggle
                                _ => true,
                            };
                            if want_add {
                                if found.is_none() && count < current.len() {
                                    current[count] = atom;
                                    count += 1;
                                }
                            } else if let Some(idx) = found {
                                let mut j = idx;
                                while j + 1 < count {
                                    current[j] = current[j + 1];
                                    j += 1;
                                }
                                count -= 1;
                            }
                        };
                        apply_atom(atom1);
                        apply_atom(atom2);
                        if count > 0 {
                            linux_x11_set_property_u32_list(
                                state,
                                window,
                                LINUX_X11_ATOM_NET_WM_STATE,
                                LINUX_X11_ATOM_ATOM,
                                &current[..count],
                            );
                        } else {
                            let _ = linux_x11_remove_property(state, window, LINUX_X11_ATOM_NET_WM_STATE);
                        }
                    } else if message_type == LINUX_X11_ATOM_WM_PROTOCOLS {
                        let protocol = linux_read_u32_order(&body, 8, little);
                        if protocol == LINUX_X11_ATOM_WM_DELETE_WINDOW {
                            if let Some(idx) = linux_x11_find_window_index(state, window) {
                                state.x11_windows[idx].mapped = false;
                                if state.x11_focus_window == window {
                                    state.x11_focus_window = LINUX_X11_ROOT_WINDOW;
                                    linux_x11_update_active_window_property(state);
                                }
                            }
                        }
                    }
                }
            }
        }
        26 => {
            // GrabPointer
            let body = [0u8; 24];
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body); // GrabSuccess
        }
        27 => {
            // UngrabPointer
        }
        28 => {
            // GrabButton
        }
        29 => {
            // UngrabButton
        }
        30 => {
            // ChangeActivePointerGrab
        }
        31 => {
            // GrabKeyboard
            let body = [0u8; 24];
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body); // GrabSuccess
        }
        32 => {
            // UngrabKeyboard
        }
        33 => {
            // GrabKey
        }
        34 => {
            // UngrabKey
        }
        35 => {
            // AllowEvents
        }
        36 => {
            // GrabServer
        }
        37 => {
            // UngrabServer
        }
        38 => {
            // QueryPointer
            let mut body = [0u8; 24];
            let child = linux_x11_pick_input_window(state);
            linux_write_u32_order(&mut body, 0, LINUX_X11_ROOT_WINDOW, little);
            linux_write_u32_order(
                &mut body,
                4,
                if child == LINUX_X11_ROOT_WINDOW { 0 } else { child },
                little,
            );
            linux_write_u16_order(&mut body, 8, state.x11_pointer_x as u16, little);
            linux_write_u16_order(&mut body, 10, state.x11_pointer_y as u16, little);
            linux_write_u16_order(&mut body, 12, state.x11_pointer_x as u16, little);
            linux_write_u16_order(&mut body, 14, state.x11_pointer_y as u16, little);
            linux_write_u16_order(
                &mut body,
                16,
                linux_x11_pointer_state_mask(state.x11_pointer_buttons),
                little,
            );
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 1, &body);
        }
        39 => {
            // GetMotionEvents -> empty
            let mut body = [0u8; 24];
            linux_write_u32_order(&mut body, 0, 0, little);
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        40 => {
            // TranslateCoordinates
            let mut body = [0u8; 24];
            if req.len() >= 16 {
                let src_win = linux_read_u32_order(req, 4, little);
                let dst_win = linux_read_u32_order(req, 8, little);
                let src_x = linux_read_u16_order(req, 12, little) as i16;
                let src_y = linux_read_u16_order(req, 14, little) as i16;
                let mut dst_x = src_x;
                let mut dst_y = src_y;
                if let Some(src_idx) = linux_x11_find_window_index(state, src_win) {
                    dst_x = dst_x.saturating_add(state.x11_windows[src_idx].x);
                    dst_y = dst_y.saturating_add(state.x11_windows[src_idx].y);
                }
                if let Some(dst_idx) = linux_x11_find_window_index(state, dst_win) {
                    dst_x = dst_x.saturating_sub(state.x11_windows[dst_idx].x);
                    dst_y = dst_y.saturating_sub(state.x11_windows[dst_idx].y);
                }
                linux_write_u16_order(&mut body, 8, dst_x as u16, little);
                linux_write_u16_order(&mut body, 10, dst_y as u16, little);
                linux_write_u32_order(&mut body, 12, 0, little);
                body[0] = 1;
            }
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        41 => {
            // WarpPointer
            if req.len() >= 24 {
                let dst_x = linux_read_u16_order(req, 20, little) as i16;
                let dst_y = linux_read_u16_order(req, 22, little) as i16;
                state.x11_pointer_x = dst_x;
                state.x11_pointer_y = dst_y;
                let target = linux_x11_pick_input_window(state);
                let mut motion = [0u8; 28];
                linux_write_u32_order(&mut motion, 0, LINUX_X11_ROOT_WINDOW, little);
                linux_write_u32_order(&mut motion, 4, target, little);
                linux_write_u16_order(&mut motion, 12, dst_x as u16, little);
                linux_write_u16_order(&mut motion, 14, dst_y as u16, little);
                linux_write_u16_order(&mut motion, 16, dst_x as u16, little);
                linux_write_u16_order(&mut motion, 18, dst_y as u16, little);
                linux_write_u16_order(
                    &mut motion,
                    20,
                    linux_x11_pointer_state_mask(state.x11_pointer_buttons),
                    little,
                );
                motion[22] = 1;
                linux_x11_queue_window_event(
                    state,
                    sock_idx,
                    target,
                    LINUX_X11_EVENT_MOTION_NOTIFY,
                    0,
                    LINUX_X11_EVENT_MASK_POINTER_MOTION,
                    &motion,
                );
            }
        }
        42 => {
            // SetInputFocus
            if req.len() >= 12 {
                let focus = linux_read_u32_order(req, 8, little);
                if focus == 0 || linux_x11_find_window_index(state, focus).is_none() {
                    state.x11_focus_window = LINUX_X11_ROOT_WINDOW;
                } else {
                    state.x11_focus_window = focus;
                }
                linux_x11_update_active_window_property(state);
            }
        }
        43 => {
            // GetInputFocus
            let mut body = [0u8; 24];
            body[0] = 0;
            linux_write_u32_order(&mut body, 4, state.x11_focus_window, little);
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        44 => {
            // QueryKeymap
            let body = [0u8; 24];
            let mut extra = [0u8; 8];
            let keycode = state.x11_last_keycode.saturating_sub(8) as usize;
            if keycode < 256 {
                let idx = keycode / 8;
                let bit = keycode % 8;
                if idx < 32 {
                    if idx < 24 {
                        // body region
                        // first 24 bytes are already inside body array copy below
                    } else {
                        extra[idx - 24] |= 1u8 << bit;
                    }
                }
            }
            let mut body_mut = body;
            if keycode < 256 {
                let idx = keycode / 8;
                let bit = keycode % 8;
                if idx < 24 {
                    body_mut[idx] |= 1u8 << bit;
                }
            }
            linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body_mut, &extra);
        }
        45 => {
            // OpenFont
        }
        46 => {
            // CloseFont
        }
        47 => {
            // QueryFont
            let mut body = [0u8; 24];
            linux_write_u16_order(&mut body, 0, 8, little); // min bounds char width
            linux_write_u16_order(&mut body, 2, 16, little); // max bounds char width
            linux_write_u16_order(&mut body, 8, 8, little); // min byte1
            linux_write_u16_order(&mut body, 10, 255, little); // max byte1
            linux_write_u16_order(&mut body, 12, 8, little); // default char
            linux_write_u16_order(&mut body, 14, 16, little); // n font properties
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        48 => {
            // QueryTextExtents
            let mut body = [0u8; 24];
            linux_write_u16_order(&mut body, 8, 0, little); // font ascent
            linux_write_u16_order(&mut body, 10, 0, little); // font descent
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        49 => {
            // ListFonts
            let body = [0u8; 24];
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        50 => {
            // ListFontsWithInfo
            let body = [0u8; 24];
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        51 => {
            // SetFontPath
        }
        52 => {
            // GetFontPath
            let body = [0u8; 24];
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        53 => {
            // CreatePixmap
            if req.len() >= 16 {
                let depth = req[1];
                let pid = linux_read_u32_order(req, 4, little);
                let drawable = linux_read_u32_order(req, 8, little);
                let width = linux_read_u16_order(req, 12, little).max(1);
                let height = linux_read_u16_order(req, 14, little).max(1);
                let width = width.min(LINUX_GFX_MAX_WIDTH as u16);
                let height = height.min(LINUX_GFX_MAX_HEIGHT as u16);
                if pid != 0 {
                    let idx = linux_x11_find_pixmap_index(state, pid)
                        .or_else(|| linux_x11_alloc_pixmap_index(state));
                    if let Some(idx) = idx {
                        state.x11_pixmaps[idx] = LinuxX11PixmapSlot {
                            active: true,
                            id: pid,
                            drawable,
                            width,
                            height,
                            depth: depth.max(1),
                            _pad0: [0; 3],
                        };
                        linux_x11_clear_pixmap_storage(idx);
                    }
                }
            }
        }
        54 => {
            // FreePixmap
            if req.len() >= 8 {
                let pid = linux_read_u32_order(req, 4, little);
                if let Some(idx) = linux_x11_find_pixmap_index(state, pid) {
                    state.x11_pixmaps[idx] = LinuxX11PixmapSlot::empty();
                    linux_x11_clear_pixmap_storage(idx);
                }
            }
        }
        55 => {
            // CreateGC
            if req.len() >= 16 {
                let cid = linux_read_u32_order(req, 4, little);
                let drawable = linux_read_u32_order(req, 8, little);
                let value_mask = linux_read_u32_order(req, 12, little);
                let idx = linux_x11_find_gc_index(state, cid).or_else(|| linux_x11_alloc_gc_index(state));
                if let Some(idx) = idx {
                    let mut gc = LinuxX11GcSlot {
                        active: true,
                        id: cid,
                        drawable,
                        function: 3,
                        fill_style: 0,
                        _pad0: [0; 2],
                        foreground: 0x00E6_E6E6,
                        background: 0x0010_1018,
                        line_width: 1,
                        _pad1: [0; 2],
                    };
                    linux_x11_apply_gc_values(&mut gc, value_mask, req, little, 16);
                    state.x11_gcs[idx] = gc;
                }
            }
        }
        56 => {
            // ChangeGC
            if req.len() >= 12 {
                let gc_id = linux_read_u32_order(req, 4, little);
                let value_mask = linux_read_u32_order(req, 8, little);
                if let Some(idx) = linux_x11_find_gc_index(state, gc_id) {
                    let mut gc = state.x11_gcs[idx];
                    linux_x11_apply_gc_values(&mut gc, value_mask, req, little, 12);
                    state.x11_gcs[idx] = gc;
                }
            }
        }
        57 => {
            // CopyGC
            if req.len() >= 16 {
                let src_gc = linux_read_u32_order(req, 4, little);
                let dst_gc = linux_read_u32_order(req, 8, little);
                let value_mask = linux_read_u32_order(req, 12, little);
                if let (Some(src_idx), Some(dst_idx)) = (
                    linux_x11_find_gc_index(state, src_gc),
                    linux_x11_find_gc_index(state, dst_gc),
                ) {
                    let src = state.x11_gcs[src_idx];
                    let mut dst = state.x11_gcs[dst_idx];
                    if (value_mask & (1u32 << 0)) != 0 {
                        dst.function = src.function;
                    }
                    if (value_mask & (1u32 << 2)) != 0 {
                        dst.foreground = src.foreground;
                    }
                    if (value_mask & (1u32 << 3)) != 0 {
                        dst.background = src.background;
                    }
                    if (value_mask & (1u32 << 4)) != 0 {
                        dst.line_width = src.line_width;
                    }
                    if (value_mask & (1u32 << 8)) != 0 {
                        dst.fill_style = src.fill_style;
                    }
                    state.x11_gcs[dst_idx] = dst;
                }
            }
        }
        60 => {
            // FreeGC
            if req.len() >= 8 {
                let gc_id = linux_read_u32_order(req, 4, little);
                if let Some(idx) = linux_x11_find_gc_index(state, gc_id) {
                    state.x11_gcs[idx] = LinuxX11GcSlot::empty();
                }
            }
        }
        61 => {
            // ClearArea
            if req.len() >= 16 {
                let exposures = req[1] != 0;
                let window = linux_read_u32_order(req, 4, little);
                let x = linux_read_u16_order(req, 8, little) as i16 as i32;
                let y = linux_read_u16_order(req, 10, little) as i16 as i32;
                let mut w = linux_read_u16_order(req, 12, little);
                let mut h = linux_read_u16_order(req, 14, little);
                if let Some(win_idx) = linux_x11_find_window_index(state, window) {
                    if w == 0 {
                        w = state.x11_windows[win_idx].width;
                    }
                    if h == 0 {
                        h = state.x11_windows[win_idx].height;
                    }
                }
                let bg = linux_x11_gc_color(state, 0, window, true);
                linux_x11_fill_rect_drawable(state, window, x, y, w, h, bg);
                if linux_x11_find_window_index(state, window).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
                if exposures {
                    let mut expose = [0u8; 28];
                    linux_write_u32_order(&mut expose, 0, window, little);
                    linux_write_u16_order(&mut expose, 4, x as u16, little);
                    linux_write_u16_order(&mut expose, 6, y as u16, little);
                    linux_write_u16_order(&mut expose, 8, w, little);
                    linux_write_u16_order(&mut expose, 10, h, little);
                    linux_x11_queue_window_event(
                        state,
                        sock_idx,
                        window,
                        LINUX_X11_EVENT_EXPOSE,
                        0,
                        LINUX_X11_EVENT_MASK_EXPOSURE,
                        &expose,
                    );
                }
            }
        }
        62 => {
            // CopyArea
            if req.len() >= 28 {
                let src_drawable = linux_read_u32_order(req, 4, little);
                let dst_drawable = linux_read_u32_order(req, 8, little);
                let _gc = linux_read_u32_order(req, 12, little);
                let src_x = linux_read_u16_order(req, 16, little) as i16 as i32;
                let src_y = linux_read_u16_order(req, 18, little) as i16 as i32;
                let dst_x = linux_read_u16_order(req, 20, little) as i16 as i32;
                let dst_y = linux_read_u16_order(req, 22, little) as i16 as i32;
                let width = linux_read_u16_order(req, 24, little);
                let height = linux_read_u16_order(req, 26, little);
                linux_x11_copy_area(
                    state,
                    src_drawable,
                    dst_drawable,
                    src_x,
                    src_y,
                    dst_x,
                    dst_y,
                    width,
                    height,
                );
                if linux_x11_find_window_index(state, dst_drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        63 => {
            // CopyPlane (subset behaves like CopyArea)
            if req.len() >= 32 {
                let src_drawable = linux_read_u32_order(req, 4, little);
                let dst_drawable = linux_read_u32_order(req, 8, little);
                let _gc = linux_read_u32_order(req, 12, little);
                let src_x = linux_read_u16_order(req, 16, little) as i16 as i32;
                let src_y = linux_read_u16_order(req, 18, little) as i16 as i32;
                let dst_x = linux_read_u16_order(req, 20, little) as i16 as i32;
                let dst_y = linux_read_u16_order(req, 22, little) as i16 as i32;
                let width = linux_read_u16_order(req, 24, little);
                let height = linux_read_u16_order(req, 26, little);
                let _bit_plane = linux_read_u32_order(req, 28, little);
                linux_x11_copy_area(
                    state,
                    src_drawable,
                    dst_drawable,
                    src_x,
                    src_y,
                    dst_x,
                    dst_y,
                    width,
                    height,
                );
                if linux_x11_find_window_index(state, dst_drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        64 => {
            // PolyPoint
            if req.len() >= 12 {
                let coord_mode_relative = req[1] != 0;
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                let mut cur_x = 0i32;
                let mut cur_y = 0i32;
                let mut has_cur = false;
                while off + 4 <= req.len() {
                    let mut px = linux_read_u16_order(req, off, little) as i16 as i32;
                    let mut py = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    if coord_mode_relative && has_cur {
                        px = cur_x.saturating_add(px);
                        py = cur_y.saturating_add(py);
                    }
                    let _ = linux_x11_drawable_set_pixel(state, drawable, px, py, color);
                    cur_x = px;
                    cur_y = py;
                    has_cur = true;
                    off += 4;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        65 => {
            // PolyLine
            if req.len() >= 16 {
                let coord_mode_relative = req[1] != 0;
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                let mut prev_x = linux_read_u16_order(req, off, little) as i16 as i32;
                let mut prev_y = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                off += 4;
                while off + 4 <= req.len() {
                    let mut x = linux_read_u16_order(req, off, little) as i16 as i32;
                    let mut y = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    if coord_mode_relative {
                        x = prev_x.saturating_add(x);
                        y = prev_y.saturating_add(y);
                    }
                    linux_x11_draw_line_drawable(state, drawable, prev_x, prev_y, x, y, color);
                    prev_x = x;
                    prev_y = y;
                    off += 4;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        66 => {
            // PolySegment
            if req.len() >= 20 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                while off + 8 <= req.len() {
                    let x1 = linux_read_u16_order(req, off, little) as i16 as i32;
                    let y1 = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    let x2 = linux_read_u16_order(req, off + 4, little) as i16 as i32;
                    let y2 = linux_read_u16_order(req, off + 6, little) as i16 as i32;
                    linux_x11_draw_line_drawable(state, drawable, x1, y1, x2, y2, color);
                    off += 8;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        67 => {
            // PolyRectangle
            if req.len() >= 20 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                while off + 8 <= req.len() {
                    let x = linux_read_u16_order(req, off, little) as i16 as i32;
                    let y = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    let w = linux_read_u16_order(req, off + 4, little);
                    let h = linux_read_u16_order(req, off + 6, little);
                    linux_x11_draw_rect_outline_drawable(state, drawable, x, y, w, h, color);
                    off += 8;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        68 => {
            // PolyArc (subset: dibuja bounding boxes de arcos)
            if req.len() >= 24 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                while off + 12 <= req.len() {
                    let x = linux_read_u16_order(req, off, little) as i16 as i32;
                    let y = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    let w = linux_read_u16_order(req, off + 4, little);
                    let h = linux_read_u16_order(req, off + 6, little);
                    linux_x11_draw_rect_outline_drawable(state, drawable, x, y, w, h, color);
                    off += 12;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        69 => {
            // FillPoly (subset: rellena bounding box del poligono)
            if req.len() >= 16 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let coord_mode_relative = req[12] != 0;
                let mut off = 16usize;
                let mut min_x = i32::MAX;
                let mut min_y = i32::MAX;
                let mut max_x = i32::MIN;
                let mut max_y = i32::MIN;
                let mut cur_x = 0i32;
                let mut cur_y = 0i32;
                let mut has_point = false;
                while off + 4 <= req.len() {
                    let mut px = linux_read_u16_order(req, off, little) as i16 as i32;
                    let mut py = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    if coord_mode_relative && has_point {
                        px = cur_x.saturating_add(px);
                        py = cur_y.saturating_add(py);
                    }
                    cur_x = px;
                    cur_y = py;
                    has_point = true;
                    if px < min_x {
                        min_x = px;
                    }
                    if py < min_y {
                        min_y = py;
                    }
                    if px > max_x {
                        max_x = px;
                    }
                    if py > max_y {
                        max_y = py;
                    }
                    off += 4;
                }
                if has_point {
                    let w = max_x.saturating_sub(min_x).saturating_add(1);
                    let h = max_y.saturating_sub(min_y).saturating_add(1);
                    if w > 0 && h > 0 {
                        linux_x11_fill_rect_drawable(
                            state,
                            drawable,
                            min_x,
                            min_y,
                            w as u16,
                            h as u16,
                            color,
                        );
                    }
                    if linux_x11_find_window_index(state, drawable).is_some() {
                        linux_x11_mark_bridge_dirty();
                    }
                }
            }
        }
        70 => {
            // PolyFillRectangle
            if req.len() >= 12 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                while off + 8 <= req.len() {
                    let x = linux_read_u16_order(req, off, little) as i16 as i32;
                    let y = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    let w = linux_read_u16_order(req, off + 4, little);
                    let h = linux_read_u16_order(req, off + 6, little);
                    linux_x11_fill_rect_drawable(state, drawable, x, y, w, h, color);
                    off += 8;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
                linux_gfx_bridge_set_status("X11 subset: PolyFillRectangle aplicado.");
            }
        }
        72 => {
            // PutImage
            linux_x11_blit_put_image(state, req, little);
            linux_gfx_bridge_set_status("X11 subset: PutImage aplicado al bridge.");
        }
        71 => {
            // PolyFillArc (subset: rellena bounding boxes de arcos)
            if req.len() >= 24 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let color = linux_x11_gc_color(state, gc, drawable, false);
                let mut off = 12usize;
                while off + 12 <= req.len() {
                    let x = linux_read_u16_order(req, off, little) as i16 as i32;
                    let y = linux_read_u16_order(req, off + 2, little) as i16 as i32;
                    let w = linux_read_u16_order(req, off + 4, little);
                    let h = linux_read_u16_order(req, off + 6, little);
                    linux_x11_fill_rect_drawable(state, drawable, x, y, w, h, color);
                    off += 12;
                }
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        73 => {
            // GetImage (subset: devuelve ZPixmap 32bpp hasta buffer disponible)
            let mut body = [0u8; 24];
            if req.len() >= 20 {
                let drawable = linux_read_u32_order(req, 4, little);
                let src_x = linux_read_u16_order(req, 8, little) as i16 as i32;
                let src_y = linux_read_u16_order(req, 10, little) as i16 as i32;
                let req_w = linux_read_u16_order(req, 12, little) as usize;
                let req_h = linux_read_u16_order(req, 14, little) as usize;
                let max_payload = state.sockets[sock_idx]
                    .rx_buf
                    .len()
                    .saturating_sub(32)
                    .min(8192);
                let max_pixels = max_payload / 4;
                let mut out_w = req_w.min(LINUX_GFX_MAX_WIDTH);
                let mut out_h = req_h.min(LINUX_GFX_MAX_HEIGHT);
                while out_w.saturating_mul(out_h) > max_pixels && out_h > 1 {
                    out_h -= 1;
                }
                if out_w.saturating_mul(out_h) > max_pixels {
                    out_w = out_w.min(max_pixels.max(1));
                    out_h = 1;
                }

                let mut extra = Vec::new();
                extra.resize(out_w.saturating_mul(out_h).saturating_mul(4), 0u8);
                let mut y = 0usize;
                while y < out_h {
                    let mut x = 0usize;
                    while x < out_w {
                        let color = linux_x11_drawable_get_pixel(
                            state,
                            drawable,
                            src_x.saturating_add(x as i32),
                            src_y.saturating_add(y as i32),
                        )
                        .unwrap_or(0);
                        let off = (y.saturating_mul(out_w).saturating_add(x)).saturating_mul(4);
                        if off + 3 < extra.len() {
                            extra[off] = (color & 0xFF) as u8;
                            extra[off + 1] = ((color >> 8) & 0xFF) as u8;
                            extra[off + 2] = ((color >> 16) & 0xFF) as u8;
                            extra[off + 3] = 0;
                        }
                        x += 1;
                    }
                    y += 1;
                }
                linux_write_u32_order(&mut body, 0, LINUX_X11_VISUAL_TRUECOLOR, little);
                linux_x11_queue_reply(&mut state.sockets[sock_idx], 24, &body, extra.as_slice());
            } else {
                linux_write_u32_order(&mut body, 0, LINUX_X11_VISUAL_TRUECOLOR, little);
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 24, &body);
            }
        }
        74 | 75 => {
            // PolyText8 / PolyText16
        }
        76 | 77 => {
            // ImageText8 / ImageText16
            if req.len() >= 16 {
                let drawable = linux_read_u32_order(req, 4, little);
                let gc = linux_read_u32_order(req, 8, little);
                let x = linux_read_u16_order(req, 12, little) as i16 as i32;
                let y = linux_read_u16_order(req, 14, little) as i16 as i32;
                let text_len = req[1] as usize;
                let fg = linux_x11_gc_color(state, gc, drawable, false);
                let bg = linux_x11_gc_color(state, gc, drawable, true);
                let w = (text_len as u16).saturating_mul(8).max(8);
                linux_x11_fill_rect_drawable(
                    state,
                    drawable,
                    x,
                    y.saturating_sub(12),
                    w,
                    16,
                    bg,
                );
                linux_x11_fill_rect_drawable(
                    state,
                    drawable,
                    x.saturating_add(1),
                    y.saturating_sub(11),
                    w.saturating_sub(2).max(1),
                    2,
                    fg,
                );
                if linux_x11_find_window_index(state, drawable).is_some() {
                    linux_x11_mark_bridge_dirty();
                }
            }
        }
        78 => {
            // CreateColormap
        }
        79 => {
            // FreeColormap
        }
        80 => {
            // CopyColormapAndFree
        }
        81 => {
            // InstallColormap
        }
        82 => {
            // UninstallColormap
        }
        83 => {
            // ListInstalledColormaps
            let mut body = [0u8; 24];
            linux_write_u16_order(&mut body, 0, 1, little);
            let mut extra = [0u8; 4];
            linux_write_u32_order(&mut extra, 0, LINUX_X11_DEFAULT_COLORMAP, little);
            linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body, &extra);
        }
        84 => {
            // AllocColor
            let mut body = [0u8; 24];
            if req.len() >= 16 {
                let red = linux_read_u16_order(req, 8, little);
                let green = linux_read_u16_order(req, 10, little);
                let blue = linux_read_u16_order(req, 12, little);
                let pixel = linux_x11_rgb16_to_pixel(red, green, blue);
                linux_write_u16_order(&mut body, 0, red, little);
                linux_write_u16_order(&mut body, 2, green, little);
                linux_write_u16_order(&mut body, 4, blue, little);
                linux_write_u16_order(&mut body, 6, red, little);
                linux_write_u16_order(&mut body, 8, green, little);
                linux_write_u16_order(&mut body, 10, blue, little);
                linux_write_u32_order(&mut body, 12, pixel, little);
            }
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        85 => {
            // AllocNamedColor
            let mut body = [0u8; 24];
            let mut pixel = 0x00C0_C0C0u32;
            if req.len() >= 12 {
                let name_len = linux_read_u16_order(req, 8, little) as usize;
                if req.len() >= 12 + name_len {
                    let name = &req[12..12 + name_len];
                    let mut hash = 0u32;
                    let mut i = 0usize;
                    while i < name.len() {
                        hash = hash.wrapping_mul(33).wrapping_add(name[i] as u32);
                        i += 1;
                    }
                    let r = ((hash >> 16) & 0xFF) as u16;
                    let g = ((hash >> 8) & 0xFF) as u16;
                    let b = (hash & 0xFF) as u16;
                    pixel = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                    let red = ((r << 8) | r) as u16;
                    let green = ((g << 8) | g) as u16;
                    let blue = ((b << 8) | b) as u16;
                    linux_write_u16_order(&mut body, 0, red, little);
                    linux_write_u16_order(&mut body, 2, green, little);
                    linux_write_u16_order(&mut body, 4, blue, little);
                    linux_write_u16_order(&mut body, 6, red, little);
                    linux_write_u16_order(&mut body, 8, green, little);
                    linux_write_u16_order(&mut body, 10, blue, little);
                }
            }
            linux_write_u32_order(&mut body, 12, pixel, little);
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        91 => {
            // QueryColors
            let mut body = [0u8; 24];
            if req.len() >= 12 {
                let req_len_words = linux_read_u16_order(req, 2, little) as usize;
                let payload_bytes = req_len_words.saturating_mul(4).saturating_sub(8);
                let available_pixels = payload_bytes / 4;
                let mut n = available_pixels.min(96);
                let max_n = state.sockets[sock_idx]
                    .rx_buf
                    .len()
                    .saturating_sub(32)
                    / 8;
                n = n.min(max_n);
                linux_write_u16_order(&mut body, 0, n as u16, little);
                let mut extra = Vec::new();
                extra.resize(n.saturating_mul(8), 0);
                let mut i = 0usize;
                while i < n {
                    let pix_off = 8 + i * 4;
                    if pix_off + 4 > req.len() {
                        break;
                    }
                    let pixel = linux_read_u32_order(req, pix_off, little);
                    let (red, green, blue) = linux_x11_pixel_to_rgb16(pixel);
                    let out = i * 8;
                    if out + 8 <= extra.len() {
                        linux_write_u16_order(extra.as_mut_slice(), out, red, little);
                        linux_write_u16_order(extra.as_mut_slice(), out + 2, green, little);
                        linux_write_u16_order(extra.as_mut_slice(), out + 4, blue, little);
                    }
                    i += 1;
                }
                linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body, extra.as_slice());
            } else {
                linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
            }
        }
        92 => {
            // LookupColor
            let mut body = [0u8; 24];
            if req.len() >= 12 {
                let name_len = linux_read_u16_order(req, 8, little) as usize;
                if req.len() >= 12 + name_len {
                    let name = &req[12..12 + name_len];
                    let mut hash = 0u32;
                    let mut i = 0usize;
                    while i < name.len() {
                        hash = hash.wrapping_mul(33).wrapping_add(name[i] as u32);
                        i += 1;
                    }
                    let r = ((hash >> 16) & 0xFF) as u16;
                    let g = ((hash >> 8) & 0xFF) as u16;
                    let b = (hash & 0xFF) as u16;
                    let red = ((r << 8) | r) as u16;
                    let green = ((g << 8) | g) as u16;
                    let blue = ((b << 8) | b) as u16;
                    linux_write_u16_order(&mut body, 0, red, little);
                    linux_write_u16_order(&mut body, 2, green, little);
                    linux_write_u16_order(&mut body, 4, blue, little);
                    linux_write_u16_order(&mut body, 6, red, little);
                    linux_write_u16_order(&mut body, 8, green, little);
                    linux_write_u16_order(&mut body, 10, blue, little);
                }
            }
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        93 => {
            // CreateCursor (subset: no-op state)
        }
        94 => {
            // CreateGlyphCursor (subset: no-op state)
        }
        95 => {
            // FreeCursor (subset: no-op state)
        }
        96 => {
            // RecolorCursor (subset: no-op state)
        }
        97 => {
            // QueryBestSize
            let mut body = [0u8; 24];
            let mut w = 640u16;
            let mut h = 360u16;
            if req.len() >= 12 {
                w = linux_read_u16_order(req, 8, little).max(1);
                h = linux_read_u16_order(req, 10, little).max(1);
            }
            linux_write_u16_order(&mut body, 0, w.min(LINUX_GFX_MAX_WIDTH as u16), little);
            linux_write_u16_order(&mut body, 2, h.min(LINUX_GFX_MAX_HEIGHT as u16), little);
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        98 => {
            // QueryExtension
            let mut body = [0u8; 24];
            let mut present = false;
            let mut major_opcode = 0u8;
            if req.len() >= 8 {
                let name_len = linux_read_u16_order(req, 4, little) as usize;
                if req.len() >= 8 + name_len {
                    let name = &req[8..8 + name_len];
                    major_opcode = linux_x11_extension_major(name);
                    present = major_opcode != 0;
                }
            }
            body[0] = if present { 1 } else { 0 };
            body[1] = major_opcode;
            body[2] = linux_x11_extension_event_base(major_opcode);
            body[3] = 0;
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        99 => {
            // ListExtensions
            let ext_ids = [
                LINUX_X11_EXT_MIT_SHM,
                LINUX_X11_EXT_BIGREQ,
                LINUX_X11_EXT_RANDR,
                LINUX_X11_EXT_RENDER,
                LINUX_X11_EXT_XFIXES,
                LINUX_X11_EXT_SHAPE,
                LINUX_X11_EXT_SYNC,
                LINUX_X11_EXT_XTEST,
                LINUX_X11_EXT_XINPUT,
            ];
            let mut extra = [0u8; 256];
            let mut off = 0usize;
            let mut count = 0u8;
            let mut i = 0usize;
            while i < ext_ids.len() {
                let name = linux_x11_extension_name(ext_ids[i]);
                if name.is_empty() {
                    i += 1;
                    continue;
                }
                let need = 1usize.saturating_add(name.len());
                if off.saturating_add(need) > extra.len() {
                    break;
                }
                extra[off] = name.len() as u8;
                off = off.saturating_add(1);
                let mut j = 0usize;
                while j < name.len() {
                    extra[off + j] = name[j];
                    j += 1;
                }
                off = off.saturating_add(name.len());
                count = count.saturating_add(1);
                i += 1;
            }
            let body = [0u8; 24];
            linux_x11_queue_reply(&mut state.sockets[sock_idx], count, &body, &extra[..off]);
        }
        100 => {
            // ChangeKeyboardMapping
        }
        101 => {
            // GetKeyboardMapping
            let mut body = [0u8; 24];
            let keycode_count = if req.len() >= 8 {
                req[5].max(1) as usize
            } else {
                1
            };
            let keysyms_per_keycode = 1u8;
            body[0] = keysyms_per_keycode;
            let mut extra = Vec::new();
            extra.resize(keycode_count.saturating_mul(4), 0);
            linux_x11_queue_reply(
                &mut state.sockets[sock_idx],
                keysyms_per_keycode,
                &body,
                extra.as_slice(),
            );
        }
        102 => {
            // ChangeKeyboardControl
        }
        103 => {
            // GetKeyboardControl
            let mut body = [0u8; 24];
            body[0] = 0; // global auto-repeat off
            linux_write_u32_order(&mut body, 4, 0, little); // led mask
            linux_write_u16_order(&mut body, 8, 400, little); // key click %
            linux_write_u16_order(&mut body, 10, 0, little); // bell %
            linux_write_u16_order(&mut body, 12, 440, little); // bell pitch
            linux_write_u16_order(&mut body, 14, 100, little); // bell duration
            let mut extra = [0u8; 32]; // auto-repeat map
            linux_x11_queue_reply(&mut state.sockets[sock_idx], 0, &body, &extra);
        }
        104 => {
            // Bell
        }
        105 => {
            // ChangePointerControl
        }
        106 => {
            // GetPointerControl
            let mut body = [0u8; 24];
            linux_write_u16_order(&mut body, 0, 1, little); // accel numerator
            linux_write_u16_order(&mut body, 2, 1, little); // accel denominator
            linux_write_u16_order(&mut body, 4, 1, little); // threshold
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        107 => {
            // SetScreenSaver
        }
        108 => {
            // GetScreenSaver
            let mut body = [0u8; 24];
            linux_write_u16_order(&mut body, 0, 600, little); // timeout
            linux_write_u16_order(&mut body, 2, 600, little); // interval
            body[4] = 0; // prefer blanking
            body[5] = 1; // allow exposures
            linux_x11_queue_reply32(&mut state.sockets[sock_idx], 0, &body);
        }
        109 | 110 | 111 | 112 | 113 | 114 | 115 | 116 | 118 => {
            // Host/access/mapping control requests: no-op subset.
        }
        117 => {
            // GetPointerMapping
            let mut body = [0u8; 24];
            let mapping = [1u8, 2u8, 3u8];
            linux_x11_queue_reply(&mut state.sockets[sock_idx], mapping.len() as u8, &body, &mapping);
        }
        119 => {
            // GetModifierMapping
            let mut body = [0u8; 24];
            let keycodes_per_modifier = 2u8;
            body[0] = keycodes_per_modifier;
            let extra = [0u8; 16]; // 8 modifiers * 2 keycodes
            linux_x11_queue_reply(
                &mut state.sockets[sock_idx],
                keycodes_per_modifier,
                &body,
                &extra,
            );
        }
        127 => {
            // NoOperation
        }
        _ => {}
    }
}

fn linux_x11_consume_payload(state: &mut LinuxShimState, sock_idx: usize, payload: &[u8]) {
    if payload.is_empty() || sock_idx >= state.sockets.len() || !state.sockets[sock_idx].active {
        return;
    }

    let mut offset = 0usize;
    if state.sockets[sock_idx].x11_state == LINUX_X11_STATE_HANDSHAKE {
        if payload.len() < 12 {
            return;
        }
        let order = payload[0];
        if order != b'l' && order != b'B' {
            linux_socket_queue_x11_fail(&mut state.sockets[sock_idx], "X11: byte-order invalido");
            state.sockets[sock_idx].last_error = 22;
            state.sockets[sock_idx].x11_state = LINUX_X11_STATE_READY;
            return;
        }
        let little = order == b'l';
        let auth_proto_len = linux_read_u16_order(payload, 6, little) as usize;
        let auth_data_len = linux_read_u16_order(payload, 8, little) as usize;
        let setup_len = 12usize
            .saturating_add((auth_proto_len + 3) & !3)
            .saturating_add((auth_data_len + 3) & !3);
        if payload.len() < setup_len {
            return;
        }
        state.sockets[sock_idx].x11_byte_order = order;
        state.sockets[sock_idx].x11_state = LINUX_X11_STATE_READY;
        state.sockets[sock_idx].x11_seq = 0;
        state.sockets[sock_idx].x11_bigreq = false;
        linux_x11_ensure_root_window(state);
        linux_x11_queue_setup_success(&mut state.sockets[sock_idx]);
        linux_gfx_bridge_open(LINUX_GFX_MAX_WIDTH as u32, LINUX_GFX_MAX_HEIGHT as u32);
        linux_gfx_bridge_fill_test(timer::ticks());
        linux_gfx_bridge_set_status("X11 subset: handshake listo.");
        offset = setup_len;
    }

    let little = linux_x11_little(&state.sockets[sock_idx]);
    while offset + 4 <= payload.len() {
        let opcode = payload[offset];
        let units = linux_read_u16_order(payload, offset + 2, little) as usize;
        let mut req_len = units.saturating_mul(4);
        if units == 0 {
            if state.sockets[sock_idx].x11_bigreq && offset + 8 <= payload.len() {
                let big_units = linux_read_u32_order(payload, offset + 4, little) as usize;
                req_len = big_units.saturating_mul(4);
            } else {
                break;
            }
        }
        if req_len < 4 || offset + req_len > payload.len() {
            break;
        }
        state.sockets[sock_idx].x11_seq = state.sockets[sock_idx].x11_seq.wrapping_add(1);
        linux_x11_handle_request(state, sock_idx, opcode, &payload[offset..offset + req_len]);
        offset = offset.saturating_add(req_len);
    }
    linux_x11_pump_bridge_events(state, sock_idx);
}

fn linux_parse_sockaddr_un_path(addr_ptr: u64, addr_len: u64, out: &mut [u8; LINUX_PATH_MAX]) -> Result<usize, i64> {
    if addr_ptr == 0 || addr_len < 2 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    if addr_len > core::mem::size_of::<LinuxSockAddrUn>() as u64 {
        return Err(linux_neg_errno(22));
    }
    let addr = unsafe { ptr::read(addr_ptr as *const LinuxSockAddrUn) };
    if addr.family != LINUX_AF_UNIX {
        return Err(linux_neg_errno(97)); // EAFNOSUPPORT
    }
    let mut raw = [0u8; LINUX_PATH_MAX];
    let max_path = (addr_len as usize).saturating_sub(2).min(addr.path.len());
    let mut n = 0usize;
    if max_path > 0 && addr.path[0] == 0 {
        // Abstract AF_UNIX names start with '\0'. For X11 bridges we map the payload
        // to a normalized textual path so connect("/tmp/.X11-unix/X0") and abstract
        // "\0/tmp/.X11-unix/X0" share the same endpoint routing.
        let mut i = 1usize;
        while i < max_path && n < raw.len() {
            let b = addr.path[i];
            if b == 0 {
                break;
            }
            raw[n] = b;
            n += 1;
            i += 1;
        }
    } else {
        while n < max_path {
            let b = addr.path[n];
            if b == 0 {
                break;
            }
            raw[n] = b;
            n += 1;
        }
    }
    if n == 0 {
        return Err(linux_neg_errno(22));
    }
    let norm_len = linux_normalize_path_bytes(out, &raw[..n]);
    if norm_len == 0 {
        return Err(linux_neg_errno(2));
    }
    Ok(norm_len)
}

fn linux_socket_kind_from_type(sock_type_raw: u64) -> Option<u16> {
    let base = (sock_type_raw & LINUX_SOCK_TYPE_MASK) as u16;
    match base {
        LINUX_SOCK_STREAM | LINUX_SOCK_DGRAM | LINUX_SOCK_SEQPACKET => Some(base),
        _ => None,
    }
}

fn linux_is_open_kind_present(state: &LinuxShimState, kind: u8, object_index: usize) -> bool {
    let mut i = 0usize;
    while i < LINUX_MAX_OPEN_FILES {
        let slot = &state.open_files[i];
        if slot.active && slot.kind == kind && slot.object_index == object_index {
            return true;
        }
        i += 1;
    }
    false
}

fn linux_release_unreferenced_special_objects(state: &mut LinuxShimState) {
    let mut runtime_changed = false;
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        if state.runtime_files[i].active
            && linux_runtime_is_memfd(&state.runtime_files[i])
            && !linux_is_open_kind_present(state, LINUX_OPEN_KIND_RUNTIME, i)
        {
            linux_release_runtime_blob(&mut state.runtime_files[i]);
            state.runtime_files[i] = LinuxRuntimeFileSlot::empty();
            if state.runtime_file_count > 0 {
                state.runtime_file_count -= 1;
            }
            runtime_changed = true;
        }
        i += 1;
    }
    if runtime_changed {
        linux_recount_runtime_blob_stats(state);
    }

    i = 0;
    while i < LINUX_MAX_EVENTFDS {
        if state.eventfds[i].active && !linux_is_open_kind_present(state, LINUX_OPEN_KIND_EVENTFD, i) {
            state.eventfds[i] = LinuxEventFdSlot::empty();
        }
        i += 1;
    }

    i = 0;
    while i < LINUX_MAX_EPOLLS {
        if state.epolls[i].active && !linux_is_open_kind_present(state, LINUX_OPEN_KIND_EPOLL, i) {
            state.epolls[i] = LinuxEpollSlot::empty();
        }
        i += 1;
    }

    i = 0;
    while i < LINUX_MAX_SOCKETS {
        if state.sockets[i].active && !linux_socket_has_reference(state, i) {
            state.sockets[i] = LinuxSocketSlot::empty();
        }
        i += 1;
    }

    i = 0;
    while i < LINUX_MAX_DIR_SLOTS {
        if state.dirs[i].active && !linux_is_open_kind_present(state, LINUX_OPEN_KIND_DIR, i) {
            state.dirs[i] = LinuxDirSlot::empty();
        }
        i += 1;
    }
}

fn linux_close_open_slot(state: &mut LinuxShimState, open_idx: usize) {
    if open_idx >= LINUX_MAX_OPEN_FILES || !state.open_files[open_idx].active {
        return;
    }
    let slot = state.open_files[open_idx];
    match slot.kind {
        LINUX_OPEN_KIND_PIPE_READ | LINUX_OPEN_KIND_PIPE_WRITE => {
            if slot.object_index < LINUX_MAX_PIPES && state.pipes[slot.object_index].active {
                if slot.kind == LINUX_OPEN_KIND_PIPE_READ {
                    state.pipes[slot.object_index].read_open = false;
                } else {
                    state.pipes[slot.object_index].write_open = false;
                }
                if !state.pipes[slot.object_index].read_open && !state.pipes[slot.object_index].write_open {
                    state.pipes[slot.object_index] = LinuxPipeSlot::empty();
                }
            }
        }
        LINUX_OPEN_KIND_SOCKET => {
            if slot.object_index < LINUX_MAX_SOCKETS && state.sockets[slot.object_index].active {
                let peer = state.sockets[slot.object_index].peer_index;
                let pending = state.sockets[slot.object_index].pending_accept_index;
                state.sockets[slot.object_index].connected = false;
                state.sockets[slot.object_index].listening = false;
                state.sockets[slot.object_index].pending_accept_index = -1;
                if peer >= 0 {
                    let peer_idx = peer as usize;
                    if peer_idx < LINUX_MAX_SOCKETS && state.sockets[peer_idx].active {
                        state.sockets[peer_idx].peer_index = -1;
                        state.sockets[peer_idx].connected = false;
                    }
                }
                if pending >= 0 {
                    let pending_idx = pending as usize;
                    if pending_idx < LINUX_MAX_SOCKETS && state.sockets[pending_idx].active {
                        let pending_peer = state.sockets[pending_idx].peer_index;
                        if pending_peer >= 0 {
                            let pending_peer_idx = pending_peer as usize;
                            if pending_peer_idx < LINUX_MAX_SOCKETS && state.sockets[pending_peer_idx].active {
                                state.sockets[pending_peer_idx].peer_index = -1;
                                state.sockets[pending_peer_idx].connected = false;
                            }
                        }
                        state.sockets[pending_idx].peer_index = -1;
                        state.sockets[pending_idx].connected = false;
                    }
                }
            }
        }
        _ => {}
    }
    state.open_files[open_idx] = LinuxOpenFileSlot::empty();
    if state.open_file_count > 0 {
        state.open_file_count -= 1;
    }
    linux_release_unreferenced_special_objects(state);
}

fn linux_build_dup_template(state: &LinuxShimState, old_fd: i32) -> Result<LinuxOpenFileSlot, i64> {
    if old_fd < 0 {
        return Err(linux_neg_errno(9)); // EBADF
    }
    if old_fd <= 2 {
        let mut slot = LinuxOpenFileSlot::empty();
        slot.active = true;
        slot.kind = LINUX_OPEN_KIND_STDIO_DUP;
        slot.aux = old_fd as u64;
        return Ok(slot);
    }
    let Some(open_idx) = linux_find_open_slot_index(state, old_fd) else {
        return Err(linux_neg_errno(9));
    };
    let mut slot = state.open_files[open_idx];
    slot.active = true;
    Ok(slot)
}

fn linux_install_dup_fd(
    state: &mut LinuxShimState,
    mut slot: LinuxOpenFileSlot,
    new_fd: i32,
    set_cloexec: bool,
) -> i64 {
    if new_fd < 0 {
        return linux_neg_errno(9); // EBADF
    }
    if new_fd <= 2 {
        // stdio remap is still direct in this shim; accept without redirecting.
        return new_fd as i64;
    }
    if let Some(existing_idx) = linux_find_open_slot_index(state, new_fd) {
        linux_close_open_slot(state, existing_idx);
    }
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, new_fd) else {
        return linux_neg_errno(24); // EMFILE
    };
    if set_cloexec {
        slot.flags |= LINUX_DUP3_CLOEXEC;
    }
    slot.fd = new_fd;
    state.open_files[open_idx] = slot;
    state.open_file_count = state.open_file_count.saturating_add(1);
    new_fd as i64
}

fn linux_find_thread_slot_index(state: &LinuxShimState, tid: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if slot.active && slot.tid == tid {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_find_process_slot_index(state: &LinuxShimState, pid: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_PROCESSES {
        let slot = &state.processes[i];
        if slot.active && slot.pid == pid {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_find_current_process_slot_index(state: &LinuxShimState) -> Option<usize> {
    if state.current_pid == 0 {
        return None;
    }
    linux_find_process_slot_index(state, state.current_pid)
}

fn linux_add_process_slot(
    state: &mut LinuxShimState,
    pid: u32,
    parent_pid: u32,
    leader_tid: u32,
    brk_base: u64,
    brk_current: u64,
    brk_limit: u64,
    mmap_cursor: u64,
    mmap_count: usize,
) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_PROCESSES {
        if !state.processes[i].active {
            state.processes[i] = LinuxProcessSlot {
                active: true,
                pid,
                parent_pid,
                leader_tid,
                brk_base,
                brk_current,
                brk_limit,
                mmap_cursor,
                mmap_count,
            };
            state.process_count = state.process_count.saturating_add(1);
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_remove_process_slot(state: &mut LinuxShimState, pid: u32) {
    let Some(idx) = linux_find_process_slot_index(state, pid) else {
        return;
    };
    state.processes[idx] = LinuxProcessSlot::empty();
    if state.process_count > 0 {
        state.process_count -= 1;
    }
}

fn linux_count_threads_for_process(state: &LinuxShimState, pid: u32) -> usize {
    let mut count = 0usize;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if slot.active && slot.process_pid == pid {
            count = count.saturating_add(1);
        }
        i += 1;
    }
    count
}

fn linux_reparent_child_processes(state: &mut LinuxShimState, old_parent_pid: u32, new_parent_pid: u32) {
    if old_parent_pid == 0 || old_parent_pid == new_parent_pid {
        return;
    }
    let mut i = 0usize;
    while i < LINUX_MAX_PROCESSES {
        if state.processes[i].active && state.processes[i].parent_pid == old_parent_pid {
            state.processes[i].parent_pid = new_parent_pid;
        }
        i += 1;
    }
}

fn linux_allocate_process_pid(state: &mut LinuxShimState) -> Option<u32> {
    let mut pid = state.next_pid.saturating_add(1).max(2001);
    let start = pid;
    loop {
        if linux_find_process_slot_index(state, pid).is_none()
            && linux_find_thread_slot_index(state, pid).is_none()
        {
            state.next_pid = pid;
            return Some(pid);
        }
        pid = pid.saturating_add(1);
        if pid == 0 {
            pid = 2001;
        }
        if pid == start {
            return None;
        }
    }
}

fn linux_sync_current_process_to_slot(state: &mut LinuxShimState) {
    let Some(idx) = linux_find_current_process_slot_index(state) else {
        return;
    };
    let slot = &mut state.processes[idx];
    slot.brk_base = state.brk_base;
    slot.brk_current = state.brk_current;
    slot.brk_limit = state.brk_limit;
    slot.mmap_cursor = state.mmap_cursor;
    slot.mmap_count = state.mmap_count;
}

fn linux_sync_current_process_from_slot(state: &mut LinuxShimState) {
    let Some(idx) = linux_find_current_process_slot_index(state) else {
        return;
    };
    let slot = &state.processes[idx];
    state.brk_base = slot.brk_base;
    state.brk_current = slot.brk_current;
    state.brk_limit = slot.brk_limit;
    state.mmap_cursor = slot.mmap_cursor;
    state.mmap_count = slot.mmap_count;
}

fn linux_find_current_thread_slot_index(state: &LinuxShimState) -> Option<usize> {
    if state.current_tid == 0 {
        return None;
    }
    linux_find_thread_slot_index(state, state.current_tid)
}

fn linux_thread_context_from_privilege() -> Option<LinuxThreadContext> {
    let Some(ctx) = privilege::linux_real_context_snapshot() else {
        return None;
    };
    Some(LinuxThreadContext {
        valid: true,
        rax: ctx.rax,
        rcx: ctx.rcx,
        rbx: ctx.rbx,
        rbp: ctx.rbp,
        r12: ctx.r12,
        r13: ctx.r13,
        r14: ctx.r14,
        r15: ctx.r15,
        rdi: ctx.rdi,
        rsi: ctx.rsi,
        rdx: ctx.rdx,
        r10: ctx.r10,
        r11: ctx.r11,
        r8: ctx.r8,
        r9: ctx.r9,
        rsp: ctx.rsp,
        rip: ctx.rip,
        rflags: ctx.rflags,
    })
}

fn linux_thread_context_apply_to_privilege(ctx: &LinuxThreadContext, fs_base: u64) {
    if !ctx.valid {
        return;
    }
    let real = privilege::LinuxRealContext {
        rax: ctx.rax,
        rcx: ctx.rcx,
        rbx: ctx.rbx,
        rbp: ctx.rbp,
        r12: ctx.r12,
        r13: ctx.r13,
        r14: ctx.r14,
        r15: ctx.r15,
        rdi: ctx.rdi,
        rsi: ctx.rsi,
        rdx: ctx.rdx,
        r10: ctx.r10,
        r11: ctx.r11,
        r8: ctx.r8,
        r9: ctx.r9,
        rsp: ctx.rsp,
        rip: ctx.rip,
        rflags: ctx.rflags,
    };
    privilege::linux_real_context_restore(&real);
    privilege::linux_real_slice_set_tls(fs_base);
}

fn linux_capture_current_thread_context(state: &mut LinuxShimState, force_rax: Option<u64>) {
    let Some(ctx) = linux_thread_context_from_privilege() else {
        return;
    };
    let Some(idx) = linux_find_current_thread_slot_index(state) else {
        return;
    };
    let mut out = ctx;
    if let Some(rax) = force_rax {
        out.rax = rax;
    }
    state.thread_contexts[idx] = out;
}

fn linux_restore_thread_context(state: &LinuxShimState, tid: u32) -> bool {
    let Some(idx) = linux_find_thread_slot_index(state, tid) else {
        return false;
    };
    let ctx = state.thread_contexts[idx];
    if !ctx.valid {
        return false;
    }
    linux_thread_context_apply_to_privilege(&ctx, state.threads[idx].fs_base);
    true
}

fn linux_sync_current_thread_to_slot(state: &mut LinuxShimState) {
    let Some(idx) = linux_find_current_thread_slot_index(state) else {
        return;
    };
    let slot = &mut state.threads[idx];
    slot.process_pid = state.current_pid;
    slot.fs_base = state.fs_base;
    slot.tid_addr = state.tid_addr;
    slot.robust_list_head = state.robust_list_head;
    slot.robust_list_len = state.robust_list_len;
    slot.signal_mask = state.signal_mask;
    slot.pending_signals = state.pending_signals;
}

fn linux_clear_futex_wait_state(slot: &mut LinuxThreadSlot) {
    slot.futex_wait_addr = 0;
    slot.futex_wait_mask = LINUX_FUTEX_BITSET_MATCH_ANY;
    slot.futex_timeout_errno = 0;
    slot.futex_timeout_deadline = 0;
    slot.futex_requeue_pi_target = 0;
    slot.futex_waitv_count = 0;
}

fn linux_sync_current_thread_from_slot(state: &mut LinuxShimState) {
    let Some(idx) = linux_find_current_thread_slot_index(state) else {
        return;
    };
    let slot = &state.threads[idx];
    state.tid_value = slot.tid;
    state.current_pid = slot.process_pid;
    state.fs_base = slot.fs_base;
    state.tid_addr = slot.tid_addr;
    state.robust_list_head = slot.robust_list_head;
    state.robust_list_len = slot.robust_list_len;
    state.signal_mask = slot.signal_mask;
    state.pending_signals = slot.pending_signals;
    linux_sync_current_process_from_slot(state);
}

fn linux_set_current_thread_tid(state: &mut LinuxShimState, tid: u32) -> bool {
    if linux_find_thread_slot_index(state, tid).is_none() {
        return false;
    }
    linux_sync_current_process_to_slot(state);
    linux_sync_current_thread_to_slot(state);
    state.current_tid = tid;
    linux_sync_current_thread_from_slot(state);
    true
}

fn linux_request_thread_switch(state: &mut LinuxShimState, tid: u32) -> bool {
    if tid == 0 || tid == state.current_tid {
        return false;
    }
    let Some(idx) = linux_find_thread_slot_index(state, tid) else {
        return false;
    };
    if !state.threads[idx].active || state.threads[idx].state != LINUX_THREAD_RUNNABLE {
        return false;
    }
    state.pending_switch_tid = tid;
    true
}

fn linux_count_runnable_threads(state: &LinuxShimState) -> usize {
    let mut count = 0usize;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if slot.active && slot.state == LINUX_THREAD_RUNNABLE {
            count = count.saturating_add(1);
        }
        i += 1;
    }
    count
}

fn linux_pick_next_runnable_thread_tid(state: &LinuxShimState, after_tid: u32) -> Option<u32> {
    let mut seen_after = after_tid == 0;
    let mut first_runnable: Option<u32> = None;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if !slot.active || slot.state != LINUX_THREAD_RUNNABLE {
            i += 1;
            continue;
        }
        if first_runnable.is_none() {
            first_runnable = Some(slot.tid);
        }
        if seen_after {
            return Some(slot.tid);
        }
        if slot.tid == after_tid {
            seen_after = true;
        }
        i += 1;
    }
    first_runnable
}

fn linux_signal_bit(sig: u64) -> Option<u64> {
    if sig == 0 || sig > LINUX_MAX_SIGNAL_NUM as u64 {
        return None;
    }
    Some(1u64 << ((sig - 1) as u32))
}

fn linux_signal_is_fatal(sig: u64) -> bool {
    sig == LINUX_SIGKILL || sig == LINUX_SIGTERM
}

fn linux_signal_is_stop(sig: u64) -> bool {
    sig == LINUX_SIGSTOP || sig == LINUX_SIGTSTP || sig == LINUX_SIGTTIN || sig == LINUX_SIGTTOU
}

fn linux_stop_signal_mask() -> u64 {
    let mut mask = 0u64;
    if let Some(bit) = linux_signal_bit(LINUX_SIGSTOP) {
        mask |= bit;
    }
    if let Some(bit) = linux_signal_bit(LINUX_SIGTSTP) {
        mask |= bit;
    }
    if let Some(bit) = linux_signal_bit(LINUX_SIGTTIN) {
        mask |= bit;
    }
    if let Some(bit) = linux_signal_bit(LINUX_SIGTTOU) {
        mask |= bit;
    }
    mask
}

fn linux_find_any_thread_tid_for_process(state: &LinuxShimState, pid: u32) -> Option<u32> {
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if slot.active && slot.process_pid == pid {
            return Some(slot.tid);
        }
        i += 1;
    }
    None
}

fn linux_queue_signal_for_process_pid(state: &mut LinuxShimState, pid: u32, sig: u64) -> i64 {
    let mut delivered = 0usize;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        if state.threads[i].active && state.threads[i].process_pid == pid {
            let tid = state.threads[i].tid;
            if linux_queue_signal_for_tid(state, tid, sig) == 0 {
                delivered = delivered.saturating_add(1);
            }
        }
        i += 1;
    }
    if delivered == 0 {
        linux_neg_errno(3) // ESRCH
    } else {
        0
    }
}

fn linux_queue_signal_for_tid(state: &mut LinuxShimState, tid: u32, sig: u64) -> i64 {
    let Some(bit) = linux_signal_bit(sig) else {
        return linux_neg_errno(22); // EINVAL
    };
    let Some(idx) = linux_find_thread_slot_index(state, tid) else {
        return linux_neg_errno(3); // ESRCH
    };
    if !state.threads[idx].active {
        return linux_neg_errno(3); // ESRCH
    }
    if sig == LINUX_SIGCONT {
        let (was_stopped, child_pid, is_current, pending_after) = {
            let slot = &mut state.threads[idx];
            let was_stopped = slot.state == LINUX_THREAD_STOPPED;
            let child_pid = slot.process_pid;
            slot.pending_signals &= !linux_stop_signal_mask();
            slot.pending_signals |= bit;
            slot.state = LINUX_THREAD_RUNNABLE;
            linux_clear_futex_wait_state(slot);
            let is_current = slot.tid == state.current_tid;
            (was_stopped, child_pid, is_current, slot.pending_signals)
        };
        if was_stopped {
            if let Some(proc_idx) = linux_find_process_slot_index(state, child_pid) {
                let parent_pid = state.processes[proc_idx].parent_pid;
                if parent_pid != 0 && parent_pid != child_pid {
                    linux_push_exited_thread(
                        state,
                        parent_pid,
                        child_pid,
                        LINUX_SIGCONT as i32,
                        LINUX_CHILD_EVENT_CONTINUED,
                    );
                }
            }
        }
        if is_current {
            state.pending_signals = pending_after;
        }
        return 0;
    }
    let slot = &mut state.threads[idx];
    slot.pending_signals |= bit;
    if slot.state == LINUX_THREAD_BLOCKED_FUTEX && (slot.signal_mask & bit) == 0 {
        slot.state = LINUX_THREAD_RUNNABLE;
        linux_clear_futex_wait_state(slot);
    }
    if slot.tid == state.current_tid {
        state.pending_signals = slot.pending_signals;
    }
    0
}

fn linux_deliver_current_pending_signal(state: &mut LinuxShimState) -> Option<i64> {
    let idx = linux_find_current_thread_slot_index(state)?;
    let pending = state.threads[idx].pending_signals & !state.threads[idx].signal_mask;
    if pending == 0 {
        return None;
    }

    let sig = pending.trailing_zeros() as u64 + 1;
    let bit = 1u64 << (sig as u32 - 1);
    state.threads[idx].pending_signals &= !bit;
    state.pending_signals = state.threads[idx].pending_signals;

    if sig == LINUX_SIGCONT {
        return Some(linux_neg_errno(4)); // EINTR
    }

    if linux_signal_is_stop(sig) {
        let child_pid = state.threads[idx].process_pid;
        state.threads[idx].state = LINUX_THREAD_STOPPED;
        if let Some(proc_idx) = linux_find_process_slot_index(state, child_pid) {
            let parent_pid = state.processes[proc_idx].parent_pid;
            if parent_pid != 0 && parent_pid != child_pid {
                linux_push_exited_thread(
                    state,
                    parent_pid,
                    child_pid,
                    sig as i32,
                    LINUX_CHILD_EVENT_STOPPED,
                );
            }
        }
        if linux_count_runnable_threads(state) > 0 {
            let _ = linux_shim_schedule_next_thread(state);
        }
        return Some(linux_neg_errno(4)); // EINTR
    }

    if linux_signal_is_fatal(sig) {
        let _ = linux_sys_exit(state, 128 + sig, false);
        return Some(linux_neg_errno(4)); // EINTR
    }

    let action = state.sigactions[sig as usize];
    if action.handler == 1 {
        // SIG_IGN
        return None;
    }

    Some(linux_neg_errno(4)) // EINTR
}

fn linux_add_thread_slot(
    state: &mut LinuxShimState,
    tid: u32,
    process_pid: u32,
    parent_tid: u32,
    exit_signal: u8,
    fs_base: u64,
    tid_addr: u64,
    clone_flags: u64,
) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        if !state.threads[i].active {
            state.threads[i] = LinuxThreadSlot {
                active: true,
                tid,
                process_pid,
                parent_tid,
                exit_signal,
                state: LINUX_THREAD_RUNNABLE,
                _pad0: [0; 2],
                fs_base,
                tid_addr,
                robust_list_head: 0,
                robust_list_len: 0,
                futex_wait_addr: 0,
                futex_wait_mask: LINUX_FUTEX_BITSET_MATCH_ANY,
                futex_timeout_errno: 0,
                futex_timeout_deadline: 0,
                futex_requeue_pi_target: 0,
                futex_waitv_count: 0,
                _pad_waitv: [0; 6],
                futex_waitv_uaddrs: [0; LINUX_FUTEX_WAITV_MAX],
                clone_flags,
                signal_mask: state.signal_mask,
                pending_signals: 0,
            };
            state.thread_contexts[i] = LinuxThreadContext::empty();
            state.thread_count = state.thread_count.saturating_add(1);
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_futex_waitv_match_index(slot: &LinuxThreadSlot, uaddr: u64) -> Option<usize> {
    if uaddr == 0 {
        return None;
    }
    let count = (slot.futex_waitv_count as usize).min(LINUX_FUTEX_WAITV_MAX);
    if count == 0 {
        return None;
    }
    let mut i = 0usize;
    while i < count {
        if slot.futex_waitv_uaddrs[i] == uaddr {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_wake_futex_waiters_masked(
    state: &mut LinuxShimState,
    uaddr: u64,
    max_wake: u64,
    wake_mask: u32,
) -> i64 {
    if uaddr == 0 || max_wake == 0 || wake_mask == 0 {
        return 0;
    }
    let mut woke = 0u64;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        if woke >= max_wake {
            break;
        }
        let mut wake_tid = 0u32;
        let mut wake_result = 0i64;
        {
            let slot = &mut state.threads[i];
            if slot.active && slot.state == LINUX_THREAD_BLOCKED_FUTEX && (slot.futex_wait_mask & wake_mask) != 0 {
                let waitv_idx = linux_futex_waitv_match_index(slot, uaddr);
                let simple_match = slot.futex_waitv_count == 0 && slot.futex_wait_addr == uaddr;
                if simple_match || waitv_idx.is_some() {
                    wake_tid = slot.tid;
                    if slot.futex_requeue_pi_target != 0 {
                        wake_result = linux_neg_errno(11); // EAGAIN for interrupted WAIT_REQUEUE_PI flow
                    } else if let Some(idx) = waitv_idx {
                        wake_result = idx as i64;
                    }
                    slot.state = LINUX_THREAD_RUNNABLE;
                    linux_clear_futex_wait_state(slot);
                    woke = woke.saturating_add(1);
                }
            }
        }
        if wake_tid != 0 && wake_result != 0 {
            linux_set_thread_saved_syscall_result(state, wake_tid, wake_result);
        }
        i += 1;
    }
    woke as i64
}

fn linux_wake_futex_waiters(state: &mut LinuxShimState, uaddr: u64, max_wake: u64) -> i64 {
    linux_wake_futex_waiters_masked(state, uaddr, max_wake, LINUX_FUTEX_BITSET_MATCH_ANY)
}

fn linux_futex_find_first_waiter_tid(state: &LinuxShimState, uaddr: u64) -> Option<u32> {
    if uaddr == 0 {
        return None;
    }
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if slot.active
            && slot.state == LINUX_THREAD_BLOCKED_FUTEX
            && slot.futex_waitv_count == 0
            && slot.futex_wait_addr == uaddr
        {
            return Some(slot.tid);
        }
        i += 1;
    }
    None
}

fn linux_count_futex_waiters(state: &LinuxShimState, uaddr: u64) -> usize {
    if uaddr == 0 {
        return 0;
    }
    let mut count = 0usize;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &state.threads[i];
        if slot.active
            && slot.state == LINUX_THREAD_BLOCKED_FUTEX
            && slot.futex_waitv_count == 0
            && slot.futex_wait_addr == uaddr
        {
            count = count.saturating_add(1);
        }
        i += 1;
    }
    count
}

fn linux_set_thread_saved_syscall_result(state: &mut LinuxShimState, tid: u32, result: i64) {
    let Some(idx) = linux_find_thread_slot_index(state, tid) else {
        return;
    };
    if state.thread_contexts[idx].valid {
        state.thread_contexts[idx].rax = result as u64;
    }
}

fn linux_wake_specific_futex_waiter(
    state: &mut LinuxShimState,
    tid: u32,
    expected_uaddr: u64,
    result: i64,
) -> bool {
    let Some(idx) = linux_find_thread_slot_index(state, tid) else {
        return false;
    };
    let can_wake = {
        let slot = &state.threads[idx];
        slot.active
            && slot.state == LINUX_THREAD_BLOCKED_FUTEX
            && slot.futex_waitv_count == 0
            && slot.futex_wait_addr == expected_uaddr
    };
    if !can_wake {
        return false;
    }
    {
        let slot = &mut state.threads[idx];
        slot.state = LINUX_THREAD_RUNNABLE;
        linux_clear_futex_wait_state(slot);
    }
    linux_set_thread_saved_syscall_result(state, tid, result);
    true
}

fn linux_futex_timeout_deadline_from_ptr(timeout_ptr: u64, absolute: bool) -> Result<Option<u64>, i64> {
    if timeout_ptr == 0 {
        return Ok(None);
    }
    let ts = unsafe { ptr::read(timeout_ptr as *const LinuxTimespec) };
    if ts.tv_sec < 0 || ts.tv_nsec < 0 || ts.tv_nsec >= 1_000_000_000 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    let ms_from_sec = (ts.tv_sec as i128).saturating_mul(1000);
    let ms_from_nsec = (ts.tv_nsec as i128 + 999_999) / 1_000_000; // ceil
    let total_ms = ms_from_sec.saturating_add(ms_from_nsec);
    if total_ms <= 0 {
        return Ok(Some(0));
    }
    let timeout_ms = if total_ms > u64::MAX as i128 {
        u64::MAX
    } else {
        total_ms as u64
    };
    if absolute {
        Ok(Some(timeout_ms))
    } else {
        Ok(Some(timer::ticks().saturating_add(timeout_ms)))
    }
}

fn linux_process_futex_timeouts(state: &mut LinuxShimState) -> usize {
    let now = timer::ticks();
    let mut timedout_tids = [0u32; LINUX_MAX_THREADS];
    let mut timedout_errnos = [0u32; LINUX_MAX_THREADS];
    let mut timedout_count = 0usize;

    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        let slot = &mut state.threads[i];
        if slot.active
            && slot.state == LINUX_THREAD_BLOCKED_FUTEX
            && slot.futex_timeout_deadline != 0
            && now >= slot.futex_timeout_deadline
        {
            let tid = slot.tid;
            let errno = if slot.futex_timeout_errno > 0 {
                slot.futex_timeout_errno as u32
            } else {
                LINUX_ERRNO_ETIMEDOUT as u32
            };
            slot.state = LINUX_THREAD_RUNNABLE;
            linux_clear_futex_wait_state(slot);
            if timedout_count < LINUX_MAX_THREADS {
                timedout_tids[timedout_count] = tid;
                timedout_errnos[timedout_count] = errno;
                timedout_count = timedout_count.saturating_add(1);
            }
        }
        i += 1;
    }

    let mut j = 0usize;
    while j < timedout_count {
        linux_set_thread_saved_syscall_result(
            state,
            timedout_tids[j],
            linux_neg_errno(timedout_errnos[j] as i64),
        );
        j += 1;
    }

    if timedout_count > 0 && state.pending_switch_tid == 0 {
        let need_switch = linux_find_current_thread_slot_index(state)
            .map(|idx| state.threads[idx].state != LINUX_THREAD_RUNNABLE)
            .unwrap_or(true);
        if need_switch {
            if let Some(next_tid) = linux_pick_next_runnable_thread_tid(state, state.current_tid) {
                let _ = linux_set_current_thread_tid(state, next_tid);
            } else {
                let _ = linux_set_current_thread_tid(state, timedout_tids[0]);
            }
        }
    }

    timedout_count
}

fn linux_futex_block_current_and_request_switch(
    state: &mut LinuxShimState,
    uaddr: u64,
    wait_mask: u32,
    timeout_deadline: Option<u64>,
    timeout_errno: i64,
    requeue_pi_target: u64,
) -> i64 {
    if uaddr == 0 || wait_mask == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if let Some(deadline) = timeout_deadline {
        if timer::ticks() >= deadline {
            return linux_neg_errno(if timeout_errno > 0 {
                timeout_errno
            } else {
                LINUX_ERRNO_ETIMEDOUT
            });
        }
    }
    if linux_count_runnable_threads(state) <= 1 {
        return linux_neg_errno(11); // EAGAIN
    }
    if let Some(cur_idx) = linux_find_current_thread_slot_index(state) {
        state.threads[cur_idx].state = LINUX_THREAD_BLOCKED_FUTEX;
        state.threads[cur_idx].futex_wait_addr = uaddr;
        state.threads[cur_idx].futex_wait_mask = wait_mask;
        state.threads[cur_idx].futex_timeout_deadline = timeout_deadline.unwrap_or(0);
        state.threads[cur_idx].futex_timeout_errno = if timeout_errno > 0 {
            timeout_errno as i32
        } else {
            LINUX_ERRNO_ETIMEDOUT as i32
        };
        state.threads[cur_idx].futex_requeue_pi_target = requeue_pi_target;
        state.threads[cur_idx].futex_waitv_count = 0;
    }
    if let Some(next_tid) = linux_pick_next_runnable_thread_tid(state, state.current_tid) {
        let _ = linux_request_thread_switch(state, next_tid);
    }
    0
}

fn linux_futex_block_current_waitv_and_request_switch(
    state: &mut LinuxShimState,
    wait_uaddrs: &[u64],
    timeout_deadline: Option<u64>,
    timeout_errno: i64,
) -> i64 {
    if wait_uaddrs.is_empty() {
        return linux_neg_errno(22); // EINVAL
    }
    let first_uaddr = wait_uaddrs[0];
    if first_uaddr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if let Some(deadline) = timeout_deadline {
        if timer::ticks() >= deadline {
            return linux_neg_errno(if timeout_errno > 0 {
                timeout_errno
            } else {
                LINUX_ERRNO_ETIMEDOUT
            });
        }
    }
    if linux_count_runnable_threads(state) <= 1 {
        return linux_neg_errno(11); // EAGAIN
    }
    if let Some(cur_idx) = linux_find_current_thread_slot_index(state) {
        let slot = &mut state.threads[cur_idx];
        slot.state = LINUX_THREAD_BLOCKED_FUTEX;
        slot.futex_wait_addr = first_uaddr;
        slot.futex_wait_mask = LINUX_FUTEX_BITSET_MATCH_ANY;
        slot.futex_timeout_deadline = timeout_deadline.unwrap_or(0);
        slot.futex_timeout_errno = if timeout_errno > 0 {
            timeout_errno as i32
        } else {
            LINUX_ERRNO_ETIMEDOUT as i32
        };
        slot.futex_requeue_pi_target = 0;
        slot.futex_waitv_count = wait_uaddrs.len().min(LINUX_FUTEX_WAITV_MAX) as u16;
        let mut i = 0usize;
        while i < slot.futex_waitv_count as usize {
            slot.futex_waitv_uaddrs[i] = wait_uaddrs[i];
            i += 1;
        }
    }
    if let Some(next_tid) = linux_pick_next_runnable_thread_tid(state, state.current_tid) {
        let _ = linux_request_thread_switch(state, next_tid);
    }
    0
}

fn linux_futex_pi_lock(state: &mut LinuxShimState, uaddr: u64, try_only: bool) -> i64 {
    let self_tid = state.current_tid.max(state.tid_value);
    if self_tid == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let owner_word = unsafe { ptr::read_volatile(uaddr as *const u32) };
    let owner_tid = owner_word & LINUX_FUTEX_TID_MASK;
    if owner_tid == 0 {
        let mut new_word = owner_word & !LINUX_FUTEX_TID_MASK;
        new_word &= !LINUX_FUTEX_OWNER_DIED;
        new_word |= self_tid & LINUX_FUTEX_TID_MASK;
        unsafe {
            ptr::write_volatile(uaddr as *mut u32, new_word);
        }
        return 0;
    }
    if owner_tid == self_tid {
        return linux_neg_errno(35); // EDEADLK
    }
    unsafe {
        ptr::write_volatile(uaddr as *mut u32, owner_word | LINUX_FUTEX_WAITERS);
    }
    if try_only {
        return linux_neg_errno(11); // EAGAIN
    }
    linux_futex_block_current_and_request_switch(
        state,
        uaddr,
        LINUX_FUTEX_BITSET_MATCH_ANY,
        None,
        0,
        0,
    )
}

fn linux_futex_pi_unlock(state: &mut LinuxShimState, uaddr: u64) -> i64 {
    let self_tid = state.current_tid.max(state.tid_value);
    if self_tid == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let owner_word = unsafe { ptr::read_volatile(uaddr as *const u32) };
    let owner_tid = owner_word & LINUX_FUTEX_TID_MASK;
    if owner_tid != 0 && owner_tid != self_tid {
        return linux_neg_errno(1); // EPERM
    }
    let next_owner = linux_futex_find_first_waiter_tid(state, uaddr).unwrap_or(0);
    let mut new_word = owner_word & !(LINUX_FUTEX_TID_MASK | LINUX_FUTEX_OWNER_DIED);
    if next_owner != 0 {
        new_word |= next_owner & LINUX_FUTEX_TID_MASK;
        if linux_count_futex_waiters(state, uaddr) > 1 {
            new_word |= LINUX_FUTEX_WAITERS;
        } else {
            new_word &= !LINUX_FUTEX_WAITERS;
        }
    } else {
        new_word &= !LINUX_FUTEX_WAITERS;
    }
    unsafe {
        ptr::write_volatile(uaddr as *mut u32, new_word);
    }
    let _ = linux_wake_futex_waiters(state, uaddr, 1);
    0
}

fn linux_futex_wake_op_eval_and_store(uaddr2: u64, encoded: u32) -> Result<bool, i64> {
    if uaddr2 == 0 {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    if (uaddr2 & 0x3) != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }

    let old = unsafe { ptr::read_volatile(uaddr2 as *const u32) };
    let op = (encoded >> 28) & 0x0f;
    let cmp = (encoded >> 24) & 0x0f;
    let mut oparg = (encoded >> 12) & 0x0fff;
    let cmparg = encoded & 0x0fff;

    if (op & LINUX_FUTEX_OP_ARG_SHIFT) != 0 {
        let shift = oparg.min(31);
        oparg = 1u32 << shift;
    }
    let op_cmd = op & !LINUX_FUTEX_OP_ARG_SHIFT;
    let new = match op_cmd {
        LINUX_FUTEX_OP_SET => oparg,
        LINUX_FUTEX_OP_ADD => old.wrapping_add(oparg),
        LINUX_FUTEX_OP_OR => old | oparg,
        LINUX_FUTEX_OP_ANDN => old & !oparg,
        LINUX_FUTEX_OP_XOR => old ^ oparg,
        _ => return Err(linux_neg_errno(22)), // EINVAL
    };
    unsafe {
        ptr::write_volatile(uaddr2 as *mut u32, new);
    }

    let old_i = old as i32;
    let cmp_i = cmparg as i32;
    let cond = match cmp {
        LINUX_FUTEX_OP_CMP_EQ => old_i == cmp_i,
        LINUX_FUTEX_OP_CMP_NE => old_i != cmp_i,
        LINUX_FUTEX_OP_CMP_LT => old_i < cmp_i,
        LINUX_FUTEX_OP_CMP_LE => old_i <= cmp_i,
        LINUX_FUTEX_OP_CMP_GT => old_i > cmp_i,
        LINUX_FUTEX_OP_CMP_GE => old_i >= cmp_i,
        _ => return Err(linux_neg_errno(22)), // EINVAL
    };
    Ok(cond)
}

fn linux_requeue_futex_waiters(
    state: &mut LinuxShimState,
    uaddr: u64,
    uaddr2: u64,
    max_wake: u64,
    max_requeue: u64,
) -> i64 {
    let woke = linux_wake_futex_waiters(state, uaddr, max_wake).max(0) as u64;
    if uaddr2 == 0 || max_requeue == 0 {
        return woke as i64;
    }
    if uaddr == uaddr2 {
        return woke as i64;
    }

    let mut moved = 0u64;
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        if moved >= max_requeue {
            break;
        }
        let slot = &mut state.threads[i];
        if slot.active
            && slot.state == LINUX_THREAD_BLOCKED_FUTEX
            && slot.futex_wait_addr == uaddr
            && slot.futex_waitv_count == 0
            && slot.futex_requeue_pi_target == 0
        {
            slot.futex_wait_addr = uaddr2;
            slot.futex_wait_mask = LINUX_FUTEX_BITSET_MATCH_ANY;
            slot.futex_requeue_pi_target = 0;
            moved = moved.saturating_add(1);
        }
        i += 1;
    }

    woke.saturating_add(moved) as i64
}

fn linux_requeue_pi_waiters(
    state: &mut LinuxShimState,
    uaddr: u64,
    uaddr2: u64,
    max_wake: u64,
    max_requeue: u64,
) -> i64 {
    let woke = linux_wake_futex_waiters(state, uaddr, max_wake).max(0) as u64;
    if uaddr2 == 0 || max_requeue == 0 || uaddr == uaddr2 {
        return woke as i64;
    }

    let mut moved = 0u64;
    let mut moved_tids = [0u32; LINUX_MAX_THREADS];
    let mut i = 0usize;
    while i < LINUX_MAX_THREADS {
        if moved >= max_requeue {
            break;
        }
        let slot = &mut state.threads[i];
        if slot.active
            && slot.state == LINUX_THREAD_BLOCKED_FUTEX
            && slot.futex_wait_addr == uaddr
            && slot.futex_waitv_count == 0
            && slot.futex_requeue_pi_target == uaddr2
        {
            slot.futex_wait_addr = uaddr2;
            slot.futex_wait_mask = LINUX_FUTEX_BITSET_MATCH_ANY;
            slot.futex_requeue_pi_target = 0;
            moved_tids[moved as usize] = slot.tid;
            moved = moved.saturating_add(1);
        }
        i += 1;
    }

    if moved > 0 {
        let owner_word = unsafe { ptr::read_volatile(uaddr2 as *const u32) };
        let owner_tid = owner_word & LINUX_FUTEX_TID_MASK;
        let mut new_word = owner_word | LINUX_FUTEX_WAITERS;
        let mut promoted_tid = 0u32;
        if owner_tid == 0 {
            promoted_tid = moved_tids[0];
            new_word &= !(LINUX_FUTEX_TID_MASK | LINUX_FUTEX_OWNER_DIED);
            new_word |= promoted_tid & LINUX_FUTEX_TID_MASK;
        }
        unsafe {
            ptr::write_volatile(uaddr2 as *mut u32, new_word);
        }
        if promoted_tid != 0 {
            let _ = linux_wake_specific_futex_waiter(state, promoted_tid, uaddr2, 0);
            if linux_count_futex_waiters(state, uaddr2) == 0 {
                let cur = unsafe { ptr::read_volatile(uaddr2 as *const u32) };
                unsafe {
                    ptr::write_volatile(uaddr2 as *mut u32, cur & !LINUX_FUTEX_WAITERS);
                }
            }
        }
    }

    woke.saturating_add(moved) as i64
}

fn linux_shim_schedule_next_thread(state: &mut LinuxShimState) -> bool {
    if state.thread_count == 0 {
        return false;
    }
    if linux_count_runnable_threads(state) == 0 {
        return false;
    }
    if let Some(next_tid) = linux_pick_next_runnable_thread_tid(state, state.current_tid) {
        return linux_set_current_thread_tid(state, next_tid);
    }
    false
}

fn linux_allocate_open_slot(state: &mut LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_OPEN_FILES {
        if !state.open_files[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_allocate_runtime_slot(state: &LinuxShimState) -> Option<usize> {
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        if !state.runtime_files[i].active {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_runtime_is_memfd(slot: &LinuxRuntimeFileSlot) -> bool {
    let path_len = (slot.path_len as usize).min(slot.path.len());
    if path_len < LINUX_MEMFD_PREFIX.len() {
        return false;
    }
    let mut i = 0usize;
    while i < LINUX_MEMFD_PREFIX.len() {
        if slot.path[i] != LINUX_MEMFD_PREFIX[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn linux_build_memfd_path(path: &mut [u8; LINUX_PATH_MAX], name: &[u8], fd: i32) -> usize {
    let mut out = 0usize;
    let mut i = 0usize;
    while i < LINUX_MEMFD_PREFIX.len() && out < path.len() {
        path[out] = LINUX_MEMFD_PREFIX[i];
        out += 1;
        i += 1;
    }
    i = 0;
    while i < name.len() && out < path.len() {
        let mut b = name[i];
        if b == b'/' || b == b'\\' || b < 0x20 {
            b = b'_';
        }
        path[out] = b;
        out += 1;
        i += 1;
    }
    if out < path.len() {
        path[out] = b'-';
        out += 1;
    }
    let mut num_buf = [0u8; 16];
    let mut num_len = 0usize;
    let mut v = if fd < 0 { 0u32 } else { fd as u32 };
    if v == 0 {
        num_buf[0] = b'0';
        num_len = 1;
    } else {
        while v > 0 && num_len < num_buf.len() {
            let digit = (v % 10) as u8;
            num_buf[num_len] = b'0' + digit;
            num_len += 1;
            v /= 10;
        }
        let mut l = 0usize;
        let mut r = num_len.saturating_sub(1);
        while l < r {
            let tmp = num_buf[l];
            num_buf[l] = num_buf[r];
            num_buf[r] = tmp;
            l += 1;
            r = r.saturating_sub(1);
        }
    }
    i = 0;
    while i < num_len && out < path.len() {
        path[out] = num_buf[i];
        out += 1;
        i += 1;
    }
    out
}

fn linux_runtime_reserve_capacity(
    state: &mut LinuxShimState,
    runtime_idx: usize,
    required_len: u64,
) -> Result<(), i64> {
    if runtime_idx >= state.runtime_files.len() || !state.runtime_files[runtime_idx].active {
        return Err(linux_neg_errno(9));
    }
    let current_cap = state.runtime_files[runtime_idx].data_len;
    if required_len <= current_cap {
        return Ok(());
    }
    if required_len > usize::MAX as u64 {
        return Err(linux_neg_errno(12));
    }
    let mut new_cap = if current_cap == 0 { 4096u64 } else { current_cap };
    while new_cap < required_len {
        new_cap = new_cap.saturating_mul(2);
        if new_cap == 0 || new_cap > usize::MAX as u64 {
            return Err(linux_neg_errno(12));
        }
    }
    let projected = state
        .runtime_blob_bytes
        .saturating_sub(current_cap)
        .saturating_add(new_cap);
    if projected > LINUX_RUNTIME_BLOB_BUDGET_BYTES {
        return Err(linux_neg_errno(12));
    }
    let Ok(layout) = Layout::from_size_align(new_cap as usize, 1) else {
        return Err(linux_neg_errno(12));
    };
    let new_ptr = unsafe { alloc(layout) };
    if new_ptr.is_null() {
        return Err(linux_neg_errno(12));
    }
    unsafe {
        ptr::write_bytes(new_ptr, 0, new_cap as usize);
    }

    let slot = &mut state.runtime_files[runtime_idx];
    let old_ptr = slot.data_ptr;
    let old_cap = slot.data_len;
    let copy_len = slot.size.min(old_cap).min(new_cap);
    if old_ptr != 0 && copy_len > 0 {
        unsafe {
            ptr::copy_nonoverlapping(old_ptr as *const u8, new_ptr, copy_len as usize);
        }
    }
    if old_ptr != 0 && old_cap > 0 {
        if let Ok(old_layout) = Layout::from_size_align(old_cap as usize, 1) {
            unsafe {
                dealloc(old_ptr as *mut u8, old_layout);
            }
        }
    }
    slot.data_ptr = new_ptr as u64;
    slot.data_len = new_cap;
    linux_recount_runtime_blob_stats(state);
    Ok(())
}

fn linux_find_mmap_slot_for_range(state: &LinuxShimState, addr: u64, len: u64) -> Option<usize> {
    let end = addr.checked_add(len)?;
    let current_pid = state.current_pid;
    let mut i = 0usize;
    while i < LINUX_MAX_MMAPS {
        let slot = &state.maps[i];
        if slot.active && (current_pid == 0 || slot.process_pid == current_pid) {
            let Some(slot_end) = slot.addr.checked_add(slot.len) else {
                i += 1;
                continue;
            };
            if addr >= slot.addr && end <= slot_end {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn linux_release_runtime_blob(slot: &mut LinuxRuntimeFileSlot) {
    if slot.data_ptr != 0 && slot.data_len > 0 {
        if let Ok(layout) = Layout::from_size_align(slot.data_len as usize, 1) {
            unsafe {
                dealloc(slot.data_ptr as *mut u8, layout);
            }
        }
    }
    slot.data_ptr = 0;
    slot.data_len = 0;
}

fn linux_recount_runtime_blob_stats(state: &mut LinuxShimState) {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if slot.active && slot.data_ptr != 0 && slot.data_len > 0 {
            files = files.saturating_add(1);
            bytes = bytes.saturating_add(slot.data_len);
        }
        i += 1;
    }
    state.runtime_blob_files = files;
    state.runtime_blob_bytes = bytes;
}

fn linux_release_all_runtime_blobs(state: &mut LinuxShimState) {
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        if state.runtime_files[i].data_ptr != 0 && state.runtime_files[i].data_len > 0 {
            linux_release_runtime_blob(&mut state.runtime_files[i]);
        }
        i += 1;
    }
    state.runtime_blob_files = 0;
    state.runtime_blob_bytes = 0;
}

fn linux_release_mmap_slot(slot: &mut LinuxMmapSlot) {
    if slot.backing_ptr != 0 && slot.backing_len > 0 && slot.backing_len <= usize::MAX as u64 {
        if let Ok(layout) = Layout::from_size_align(slot.backing_len as usize, LINUX_PAGE_SIZE as usize) {
            unsafe {
                dealloc(slot.backing_ptr as *mut u8, layout);
            }
        }
    }
    *slot = LinuxMmapSlot::empty();
}

fn linux_release_all_mmaps(state: &mut LinuxShimState) {
    let mut i = 0usize;
    while i < LINUX_MAX_MMAPS {
        if state.maps[i].active || state.maps[i].backing_ptr != 0 {
            linux_release_mmap_slot(&mut state.maps[i]);
        }
        i += 1;
    }
    state.mmap_count = 0;
    state.mmap_cursor = LINUX_MMAP_BASE;
    let mut p = 0usize;
    while p < LINUX_MAX_PROCESSES {
        if state.processes[p].active {
            state.processes[p].mmap_count = 0;
            state.processes[p].mmap_cursor = LINUX_MMAP_BASE;
        }
        p += 1;
    }
}

fn linux_release_process_mmaps(state: &mut LinuxShimState, pid: u32) {
    if pid == 0 {
        return;
    }
    let mut i = 0usize;
    while i < LINUX_MAX_MMAPS {
        if state.maps[i].active && state.maps[i].process_pid == pid {
            linux_release_mmap_slot(&mut state.maps[i]);
        }
        i += 1;
    }
    if let Some(proc_idx) = linux_find_process_slot_index(state, pid) {
        state.processes[proc_idx].mmap_count = 0;
        state.processes[proc_idx].mmap_cursor = LINUX_MMAP_BASE;
    }
    if state.current_pid == pid {
        state.mmap_count = 0;
        state.mmap_cursor = LINUX_MMAP_BASE;
    }
}

fn linux_shim_watchdog_should_abort(state: &LinuxShimState) -> bool {
    if !state.active {
        return true;
    }
    if state.syscall_count >= LINUX_SHIM_WATCHDOG_MAX_CALLS {
        return true;
    }
    let elapsed = timer::ticks().saturating_sub(state.start_tick);
    elapsed > LINUX_SHIM_WATCHDOG_MAX_TICKS
}

fn linux_write_stat64_mode(stat_ptr: u64, size: u64, mode: u32) -> i64 {
    if stat_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let blocks = (size.saturating_add(511)) / 512;
    let now = (timer::ticks() / 1000) as i64;
    unsafe {
        let out = stat_ptr as *mut LinuxStat64;
        ptr::write(
            out,
            LinuxStat64 {
                st_dev: 1,
                st_ino: 1,
                st_nlink: 1,
                st_mode: mode,
                st_uid: 0,
                st_gid: 0,
                __pad0: 0,
                st_rdev: 0,
                st_size: size as i64,
                st_blksize: 4096,
                st_blocks: blocks as i64,
                st_atime: now,
                st_atime_nsec: 0,
                st_mtime: now,
                st_mtime_nsec: 0,
                st_ctime: now,
                st_ctime_nsec: 0,
                __unused: [0; 3],
            },
        );
    }
    0
}

fn linux_write_stat64(stat_ptr: u64, size: u64) -> i64 {
    linux_write_stat64_mode(stat_ptr, size, LINUX_STAT_MODE_REG)
}

fn linux_path_equals(path: &[u8], path_len: usize, expected: &str) -> bool {
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let norm_len = linux_normalize_path_str(&mut normalized, expected);
    norm_len == path_len && normalized[..norm_len] == path[..path_len]
}

fn linux_path_equals_ascii_casefold(path: &[u8], path_len: usize, expected: &str) -> bool {
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let norm_len = linux_normalize_path_str(&mut normalized, expected);
    if norm_len != path_len {
        return false;
    }
    let mut i = 0usize;
    while i < norm_len {
        if path[i].to_ascii_lowercase() != normalized[i].to_ascii_lowercase() {
            return false;
        }
        i += 1;
    }
    true
}

fn linux_path_contains_ascii_casefold(path: &[u8], path_len: usize, needle: &[u8]) -> bool {
    if needle.is_empty() || path_len == 0 || needle.len() > path_len {
        return false;
    }
    let mut i = 0usize;
    while i + needle.len() <= path_len {
        let mut j = 0usize;
        while j < needle.len() {
            if path[i + j].to_ascii_lowercase() != needle[j].to_ascii_lowercase() {
                break;
            }
            j += 1;
        }
        if j == needle.len() {
            return true;
        }
        i += 1;
    }
    false
}

fn linux_path_starts_with_ascii_casefold(path: &[u8], path_len: usize, prefix: &[u8]) -> bool {
    if prefix.is_empty() || path_len < prefix.len() {
        return false;
    }
    let mut i = 0usize;
    while i < prefix.len() {
        if path[i].to_ascii_lowercase() != prefix[i].to_ascii_lowercase() {
            return false;
        }
        i += 1;
    }
    true
}

fn linux_path_is_x11_socket(path: &[u8; LINUX_PATH_MAX], path_len: usize) -> bool {
    if path_len == 0 || path_len > LINUX_PATH_MAX {
        return false;
    }
    let base = linux_basename_start(path, path_len);
    if base >= path_len || path[base].to_ascii_lowercase() != b'x' {
        return false;
    }
    let mut i = base + 1;
    if i >= path_len {
        return false;
    }
    while i < path_len {
        if !path[i].is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    let needle = b"x11-unix/";
    if base < needle.len() {
        return false;
    }
    let start = base - needle.len();
    let mut j = 0usize;
    while j < needle.len() {
        if path[start + j].to_ascii_lowercase() != needle[j] {
            return false;
        }
        j += 1;
    }
    true
}

fn linux_path_is_virtual_x11_dir(path: &[u8], path_len: usize) -> bool {
    linux_path_equals_ascii_casefold(path, path_len, "/tmp")
        || linux_path_equals_ascii_casefold(path, path_len, "/tmp/.x11-unix")
}

fn linux_path_is_virtual_x11_socket(path: &[u8], path_len: usize) -> bool {
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let copy_len = path_len.min(normalized.len());
    if copy_len == 0 {
        return false;
    }
    let mut i = 0usize;
    while i < copy_len {
        normalized[i] = path[i];
        i += 1;
    }
    linux_path_equals_ascii_casefold(path, path_len, "/tmp/.x11-unix/x0")
        || linux_path_equals_ascii_casefold(path, path_len, "/tmp/.x11-unix/x1")
        || linux_path_is_x11_socket(&normalized, copy_len)
        || linux_path_contains_ascii_casefold(path, path_len, b"/.x11-unix/")
        || linux_path_contains_ascii_casefold(path, path_len, b"/x11-unix/")
}

fn linux_path_matches_run_user_bus(path: &[u8], path_len: usize) -> bool {
    let prefix = b"/run/user/";
    if path_len <= prefix.len() + 4 {
        return false;
    }
    let mut i = 0usize;
    while i < prefix.len() {
        if path[i].to_ascii_lowercase() != prefix[i] {
            return false;
        }
        i += 1;
    }
    if i >= path_len || !path[i].is_ascii_digit() {
        return false;
    }
    while i < path_len && path[i].is_ascii_digit() {
        i += 1;
    }
    i + 4 == path_len
        && path[i] == b'/'
        && path[i + 1].to_ascii_lowercase() == b'b'
        && path[i + 2].to_ascii_lowercase() == b'u'
        && path[i + 3].to_ascii_lowercase() == b's'
}

fn linux_path_matches_run_user_dir(path: &[u8], path_len: usize) -> bool {
    let prefix = b"/run/user/";
    if path_len <= prefix.len() {
        return false;
    }
    let mut i = 0usize;
    while i < prefix.len() {
        if path[i].to_ascii_lowercase() != prefix[i] {
            return false;
        }
        i += 1;
    }
    if i >= path_len || !path[i].is_ascii_digit() {
        return false;
    }
    while i < path_len {
        if !path[i].is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    true
}

fn linux_path_is_virtual_dbus_socket(path: &[u8], path_len: usize) -> bool {
    linux_path_equals_ascii_casefold(path, path_len, "/run/dbus/system_bus_socket")
        || linux_path_equals_ascii_casefold(path, path_len, "/var/run/dbus/system_bus_socket")
        || linux_path_matches_run_user_bus(path, path_len)
        || linux_path_starts_with_ascii_casefold(path, path_len, b"/tmp/dbus-")
        || linux_path_starts_with_ascii_casefold(path, path_len, b"tmp/dbus-")
}

fn linux_path_is_virtual_dbus_dir(path: &[u8], path_len: usize) -> bool {
    linux_path_equals_ascii_casefold(path, path_len, "/run")
        || linux_path_equals_ascii_casefold(path, path_len, "/run/user")
        || linux_path_matches_run_user_dir(path, path_len)
        || linux_path_equals_ascii_casefold(path, path_len, "/run/dbus")
        || linux_path_equals_ascii_casefold(path, path_len, "/var")
        || linux_path_equals_ascii_casefold(path, path_len, "/var/run")
        || linux_path_equals_ascii_casefold(path, path_len, "/var/run/dbus")
}

fn linux_x11_socket_path_from_display(path: &mut [u8; LINUX_PATH_MAX], display: u16) -> usize {
    let prefix = b"/tmp/.x11-unix/x";
    let mut out = 0usize;
    while out < prefix.len() && out < path.len() {
        path[out] = prefix[out];
        out += 1;
    }

    let mut digits = [0u8; 5];
    let mut digit_len = 0usize;
    let mut value = display as u32;
    if value == 0 {
        digits[0] = b'0';
        digit_len = 1;
    } else {
        while value > 0 && digit_len < digits.len() {
            digits[digit_len] = b'0' + (value % 10) as u8;
            digit_len += 1;
            value /= 10;
        }
        let mut l = 0usize;
        let mut r = digit_len.saturating_sub(1);
        while l < r {
            let tmp = digits[l];
            digits[l] = digits[r];
            digits[r] = tmp;
            l += 1;
            r = r.saturating_sub(1);
        }
    }

    let mut i = 0usize;
    while i < digit_len && out < path.len() {
        path[out] = digits[i];
        out += 1;
        i += 1;
    }
    out
}

fn linux_parse_x11_display_from_inet(addr_ptr: u64, addr_len: u64) -> Option<u16> {
    if addr_ptr == 0 || addr_len < 4 {
        return None;
    }
    let family = unsafe { ptr::read(addr_ptr as *const u16) };
    let port_hi = unsafe { ptr::read(addr_ptr.saturating_add(2) as *const u8) } as u16;
    let port_lo = unsafe { ptr::read(addr_ptr.saturating_add(3) as *const u8) } as u16;
    let port = (port_hi << 8) | port_lo;
    if !(LINUX_X11_TCP_PORT_BASE..=LINUX_X11_TCP_PORT_MAX).contains(&port) {
        return None;
    }
    let display = port.saturating_sub(LINUX_X11_TCP_PORT_BASE);

    if family == LINUX_AF_INET {
        if addr_len < 8 {
            return None;
        }
        return Some(display);
    }

    if family == LINUX_AF_INET6 {
        if addr_len < 24 {
            return None;
        }
        return Some(display);
    }

    None
}

fn linux_virtual_path_mode(path: &[u8], path_len: usize) -> Option<u32> {
    if linux_path_equals(path, path_len, "/")
        || linux_path_equals(path, path_len, "/proc")
        || linux_path_equals(path, path_len, "/proc/self")
        || linux_path_is_virtual_x11_dir(path, path_len)
        || linux_path_is_virtual_dbus_dir(path, path_len)
    {
        return Some(LINUX_STAT_MODE_DIR);
    }
    if linux_path_equals(path, path_len, "/proc/self/exe") || linux_path_equals(path, path_len, "/proc/self/cwd") {
        return Some(LINUX_STAT_MODE_REG);
    }
    if linux_path_is_virtual_x11_socket(path, path_len) || linux_path_is_virtual_dbus_socket(path, path_len) {
        return Some(LINUX_STAT_MODE_SOCK);
    }
    None
}

fn linux_copy_runtime_path(slot: &LinuxRuntimeFileSlot, out: &mut [u8]) -> usize {
    let len = (slot.path_len as usize).min(slot.path.len()).min(out.len());
    if len == 0 {
        return 0;
    }
    let mut i = 0usize;
    while i < len {
        out[i] = slot.path[i];
        i += 1;
    }
    len
}

fn linux_pick_runtime_exe_path(state: &LinuxShimState, out: &mut [u8]) -> usize {
    let mut fallback: Option<usize> = None;
    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if !slot.active {
            i += 1;
            continue;
        }
        if fallback.is_none() {
            fallback = Some(i);
        }
        let len = (slot.path_len as usize).min(slot.path.len());
        if len == 0 {
            i += 1;
            continue;
        }
        let base = linux_basename_start(&slot.path, len);
        let base_slice = &slot.path[base..len];
        let ends_so = base_slice.len() >= 3
            && (base_slice[base_slice.len() - 3..] == *b".so"
                || (base_slice.len() >= 6 && base_slice[base_slice.len() - 6..].starts_with(b".so.")));
        if !ends_so {
            return linux_copy_runtime_path(slot, out);
        }
        i += 1;
    }
    if let Some(idx) = fallback {
        return linux_copy_runtime_path(&state.runtime_files[idx], out);
    }
    0
}

fn linux_stdio_push_line(state: &mut LinuxShimState) {
    if state.stdio_line_len == 0 {
        return;
    }
    ui::terminal_system_message_bytes(&state.stdio_line[..state.stdio_line_len]);
    unsafe {
        if LINUX_GFX_BRIDGE.active {
            let gfx = &mut LINUX_GFX_BRIDGE;
            let prefix = b"APP> ";
            let mut out_len = 0usize;
            while out_len < prefix.len() && out_len < LINUX_GFX_STATUS_MAX {
                gfx.status[out_len] = prefix[out_len];
                out_len += 1;
            }
            let max_line = LINUX_GFX_STATUS_MAX.saturating_sub(out_len);
            let copy = state.stdio_line_len.min(max_line);
            let mut i = 0usize;
            while i < copy {
                gfx.status[out_len + i] = state.stdio_line[i];
                i += 1;
            }
            out_len = out_len.saturating_add(copy);
            while out_len < LINUX_GFX_STATUS_MAX {
                gfx.status[out_len] = 0;
                out_len += 1;
            }
            gfx.status_len = prefix.len().saturating_add(copy).min(LINUX_GFX_STATUS_MAX);
        }
    }
    state.stdio_line_len = 0;
}

fn linux_stdio_push_byte(state: &mut LinuxShimState, byte: u8) {
    if byte == b'\n' || byte == b'\r' {
        linux_stdio_push_line(state);
        return;
    }
    let sanitized = if byte == b'\t' || (byte >= 0x20 && byte <= 0x7E) {
        byte
    } else {
        b'?'
    };
    if state.stdio_line_len >= state.stdio_line.len() {
        linux_stdio_push_line(state);
    }
    if state.stdio_line_len < state.stdio_line.len() {
        state.stdio_line[state.stdio_line_len] = sanitized;
        state.stdio_line_len += 1;
    }
}

fn linux_stdio_capture_from_ptr(state: &mut LinuxShimState, ptr_raw: u64, len: u64, max_capture: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    if ptr_raw == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let ret_len = len.min(i64::MAX as u64) as i64;
    let capture = if len > usize::MAX as u64 {
        max_capture
    } else {
        (len as usize).min(max_capture)
    };
    unsafe {
        let src = ptr_raw as *const u8;
        let mut i = 0usize;
        while i < capture {
            linux_stdio_push_byte(state, ptr::read(src.add(i)));
            i += 1;
        }
    }
    ret_len
}

fn linux_sys_write(state: &mut LinuxShimState, fd: u64, buf: u64, len: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9);
    }
    if len == 0 {
        return 0;
    }
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }

    let mut stdio_target: Option<i32> = None;
    if fd_i == 1 || fd_i == 2 {
        stdio_target = Some(fd_i as i32);
    } else if let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) {
        let slot = state.open_files[open_idx];
        match slot.kind {
            LINUX_OPEN_KIND_RUNTIME => {
                let runtime_idx = slot.object_index;
                if runtime_idx >= state.runtime_files.len() || !state.runtime_files[runtime_idx].active {
                    return linux_neg_errno(9);
                }
                if !linux_runtime_is_memfd(&state.runtime_files[runtime_idx]) {
                    return linux_neg_errno(9);
                }
                let write_len = len.min(i64::MAX as u64);
                let cursor = state.open_files[open_idx].cursor;
                let Some(end) = cursor.checked_add(write_len) else {
                    return linux_neg_errno(12);
                };
                if let Err(err) = linux_runtime_reserve_capacity(state, runtime_idx, end) {
                    return err;
                }
                if write_len > 0 {
                    let dst_ptr = state.runtime_files[runtime_idx].data_ptr.saturating_add(cursor);
                    unsafe {
                        ptr::copy_nonoverlapping(buf as *const u8, dst_ptr as *mut u8, write_len as usize);
                    }
                }
                state.open_files[open_idx].cursor = end;
                if end > state.runtime_files[runtime_idx].size {
                    state.runtime_files[runtime_idx].size = end;
                }
                return write_len as i64;
            }
            LINUX_OPEN_KIND_STDIO_DUP => {
                let target = slot.aux as i32;
                if target == 1 || target == 2 {
                    stdio_target = Some(target);
                } else {
                    return linux_neg_errno(9);
                }
            }
            LINUX_OPEN_KIND_EVENTFD => {
                if len < 8 {
                    return linux_neg_errno(22); // EINVAL
                }
                if slot.object_index >= LINUX_MAX_EVENTFDS || !state.eventfds[slot.object_index].active {
                    return linux_neg_errno(9);
                }
                let value = unsafe { ptr::read(buf as *const u64) };
                if value == u64::MAX {
                    return linux_neg_errno(22); // EINVAL
                }
                let counter = state.eventfds[slot.object_index].counter;
                let Some(next) = counter.checked_add(value) else {
                    return linux_neg_errno(11); // EAGAIN
                };
                state.eventfds[slot.object_index].counter = next;
                return 8;
            }
            LINUX_OPEN_KIND_PIPE_WRITE => {
                if slot.object_index >= LINUX_MAX_PIPES || !state.pipes[slot.object_index].active {
                    return linux_neg_errno(9);
                }
                if !state.pipes[slot.object_index].read_open {
                    return linux_neg_errno(32); // EPIPE
                }
                let room = 64 * 1024u64;
                let pending = state.pipes[slot.object_index].pending_bytes;
                let writable = room.saturating_sub(pending);
                if writable == 0 {
                    return linux_neg_errno(11); // EAGAIN
                }
                let wrote = len.min(writable);
                state.pipes[slot.object_index].pending_bytes =
                    state.pipes[slot.object_index].pending_bytes.saturating_add(wrote);
                return wrote.min(i64::MAX as u64) as i64;
            }
            LINUX_OPEN_KIND_SOCKET => {
                return linux_socket_send_payload(state, slot.object_index, buf, len);
            }
            _ => return linux_neg_errno(9),
        }
    } else {
        return linux_neg_errno(9);
    }

    if stdio_target.is_some() {
        state.write_calls = state.write_calls.saturating_add(1);
        let result = linux_stdio_capture_from_ptr(state, buf, len, LINUX_STDIO_CAPTURE_LIMIT);
        if result >= 0 {
            linux_stdio_push_line(state);
        }
        return result;
    }
    linux_neg_errno(9)
}

fn linux_sys_writev(state: &mut LinuxShimState, fd: u64, iov_ptr: u64, iov_cnt: u64) -> i64 {
    if iov_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let count = (iov_cnt as usize).min(1024);
    if count == 0 {
        return 0;
    }

    let mut total_written = 0u64;
    unsafe {
        let base = iov_ptr as *const LinuxIovec;
        let mut i = 0usize;
        while i < count {
            let iov = ptr::read(base.add(i));
            if iov.len == 0 {
                i += 1;
                continue;
            }
            let res = linux_sys_write(state, fd, iov.base, iov.len);
            if res < 0 {
                if total_written > 0 {
                    return total_written.min(i64::MAX as u64) as i64;
                }
                return res;
            }
            let wrote = res as u64;
            total_written = total_written.saturating_add(wrote);
            if wrote < iov.len {
                break;
            }
            i += 1;
        }
    }
    total_written.min(i64::MAX as u64) as i64
}

fn linux_sys_readv(state: &mut LinuxShimState, fd: u64, iov_ptr: u64, iov_cnt: u64) -> i64 {
    if iov_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let count = (iov_cnt as usize).min(1024);
    if count == 0 {
        return 0;
    }

    let mut total_read = 0u64;
    unsafe {
        let base = iov_ptr as *const LinuxIovec;
        let mut i = 0usize;
        while i < count {
            let iov = ptr::read(base.add(i));
            if iov.len == 0 {
                i += 1;
                continue;
            }
            let res = linux_sys_read(state, fd, iov.base, iov.len);
            if res < 0 {
                if total_read > 0 {
                    return total_read.min(i64::MAX as u64) as i64;
                }
                return res;
            }
            let got = res as u64;
            total_read = total_read.saturating_add(got);
            if got < iov.len {
                break;
            }
            i += 1;
        }
    }
    total_read.min(i64::MAX as u64) as i64
}

fn linux_sys_pread64(state: &mut LinuxShimState, fd: u64, buf: u64, len: u64, offset: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    if len == 0 {
        return 0;
    }
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    let slot = state.open_files[open_idx];
    if slot.kind != LINUX_OPEN_KIND_RUNTIME {
        return linux_neg_errno(29); // ESPIPE
    }
    let runtime_idx = slot.object_index;
    if runtime_idx >= state.runtime_files.len() || !state.runtime_files[runtime_idx].active {
        return linux_neg_errno(9);
    }
    let runtime = &state.runtime_files[runtime_idx];
    let readable_len = runtime.size.min(runtime.data_len);
    if runtime.data_ptr == 0 || readable_len == 0 || offset >= readable_len {
        return 0;
    }
    let remaining = readable_len.saturating_sub(offset);
    let to_copy = remaining.min(len).min(i64::MAX as u64);
    unsafe {
        ptr::copy_nonoverlapping(
            runtime.data_ptr.saturating_add(offset) as *const u8,
            buf as *mut u8,
            to_copy as usize,
        );
    }
    to_copy as i64
}

fn linux_sys_ioctl(state: &LinuxShimState, fd: u64, req: u64, argp: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let fd_valid = fd_i <= 2 || linux_find_open_slot_index(state, fd_i as i32).is_some();
    if !fd_valid {
        return linux_neg_errno(9);
    }
    if req == LINUX_TIOCGWINSZ {
        if argp == 0 {
            return linux_neg_errno(14); // EFAULT
        }
        unsafe {
            ptr::write(
                argp as *mut LinuxWinsize,
                LinuxWinsize {
                    ws_row: 24,
                    ws_col: ui::TERM_MAX_INPUT as u16,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                },
            );
        }
        return 0;
    }
    linux_neg_errno(25) // ENOTTY
}

fn linux_sys_access_common(
    state: &mut LinuxShimState,
    path_ptr: u64,
    _mode: u64,
    sysno: u64,
) -> i64 {
    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(path_ptr, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_resolve_open_path(state, LINUX_AT_FDCWD, &input, input_len, &mut normalized) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let (exists, _is_file, _runtime_idx, _mode_bits) = linux_vfs_lookup_path(state, &normalized, path_len);
    let result = if exists {
        0
    } else {
        linux_neg_errno(2) // ENOENT
    };
    linux_record_last_path_lookup(
        state,
        sysno,
        &normalized,
        path_len,
        result,
        result >= 0,
    );
    result
}

fn linux_sys_access(state: &mut LinuxShimState, path_ptr: u64, mode: u64) -> i64 {
    linux_sys_access_common(state, path_ptr, mode, LINUX_SYS_ACCESS)
}

fn linux_sys_faccessat(state: &mut LinuxShimState, dirfd: u64, path_ptr: u64, mode: u64, _flags: u64) -> i64 {
    let dirfd_i = dirfd as i64;
    if dirfd_i != LINUX_AT_FDCWD && linux_find_open_slot_index(state, dirfd_i as i32).is_none() {
        return linux_neg_errno(9); // EBADF
    }
    linux_sys_access_common(state, path_ptr, mode, LINUX_SYS_FACCESSAT)
}

fn linux_sys_faccessat2(state: &mut LinuxShimState, dirfd: u64, path_ptr: u64, mode: u64, flags: u64) -> i64 {
    // Keep shim semantics aligned with faccessat for compatibility.
    // Most modern userspace uses flags=0 here.
    linux_sys_faccessat(state, dirfd, path_ptr, mode, flags)
}

fn linux_sys_getcwd(buf: u64, size: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if size < 2 {
        return linux_neg_errno(34); // ERANGE
    }
    unsafe {
        let dst = buf as *mut u8;
        ptr::write(dst, b'/');
        ptr::write(dst.add(1), 0);
    }
    2
}

fn linux_sys_readlink(state: &LinuxShimState, path_ptr: u64, buf: u64, buf_size: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if buf_size == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(path_ptr, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_resolve_open_path(state, LINUX_AT_FDCWD, &input, input_len, &mut normalized) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut target = [0u8; LINUX_PATH_MAX];
    let target_len = if linux_path_equals(&normalized, path_len, "/proc/self/cwd") {
        target[0] = b'/';
        1
    } else if linux_path_equals(&normalized, path_len, "/proc/self/exe") {
        let picked = linux_pick_runtime_exe_path(state, &mut target);
        if picked > 0 {
            picked
        } else {
            let fallback = b"/app/main";
            let mut i = 0usize;
            while i < fallback.len() {
                target[i] = fallback[i];
                i += 1;
            }
            fallback.len()
        }
    } else {
        return linux_neg_errno(2); // ENOENT
    };

    let copy_len = target_len.min(buf_size as usize);
    unsafe {
        ptr::copy_nonoverlapping(target.as_ptr(), buf as *mut u8, copy_len);
    }
    copy_len as i64
}

fn linux_sys_readlinkat(state: &LinuxShimState, dirfd: u64, path_ptr: u64, buf: u64, buf_size: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if buf_size == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(path_ptr, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_resolve_open_path(state, dirfd as i64, &input, input_len, &mut normalized) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut target = [0u8; LINUX_PATH_MAX];
    let target_len = if linux_path_equals(&normalized, path_len, "/proc/self/cwd") {
        target[0] = b'/';
        1
    } else if linux_path_equals(&normalized, path_len, "/proc/self/exe") {
        let picked = linux_pick_runtime_exe_path(state, &mut target);
        if picked > 0 {
            picked
        } else {
            let fallback = b"/app/main";
            let mut i = 0usize;
            while i < fallback.len() {
                target[i] = fallback[i];
                i += 1;
            }
            fallback.len()
        }
    } else {
        return linux_neg_errno(2); // ENOENT
    };

    let copy_len = target_len.min(buf_size as usize);
    unsafe {
        ptr::copy_nonoverlapping(target.as_ptr(), buf as *mut u8, copy_len);
    }
    copy_len as i64
}

fn linux_sys_fcntl(state: &mut LinuxShimState, fd: u64, cmd: u64, arg: u64) -> i64 {
    let fd_i = fd as i64;
    let valid = fd_i >= 0 && (fd_i <= 2 || linux_find_open_slot_index(state, fd_i as i32).is_some());
    if !valid {
        return linux_neg_errno(9); // EBADF
    }
    match cmd {
        LINUX_F_DUPFD => {
            let min_fd = (arg as i64).max(LINUX_FD_BASE as i64) as i32;
            let template = match linux_build_dup_template(state, fd_i as i32) {
                Ok(v) => v,
                Err(err) => return err,
            };
            let Some(new_fd) = linux_find_unused_fd(state, min_fd) else {
                return linux_neg_errno(24); // EMFILE
            };
            linux_install_dup_fd(state, template, new_fd, false)
        }
        LINUX_F_DUPFD_CLOEXEC => {
            let min_fd = (arg as i64).max(LINUX_FD_BASE as i64) as i32;
            let template = match linux_build_dup_template(state, fd_i as i32) {
                Ok(v) => v,
                Err(err) => return err,
            };
            let Some(new_fd) = linux_find_unused_fd(state, min_fd) else {
                return linux_neg_errno(24); // EMFILE
            };
            linux_install_dup_fd(state, template, new_fd, true)
        }
        LINUX_F_GETFD | LINUX_F_SETFD => 0,
        LINUX_F_GETFL => {
            if fd_i <= 2 {
                0
            } else if let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) {
                state.open_files[open_idx].flags as i64
            } else {
                linux_neg_errno(9)
            }
        }
        LINUX_F_SETFL => {
            if fd_i > 2 {
                if let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) {
                    state.open_files[open_idx].flags = arg;
                } else {
                    return linux_neg_errno(9);
                }
            }
            0
        }
        _ => 0,
    }
}

fn linux_sys_getdents64(state: &mut LinuxShimState, fd: u64, dirp: u64, count: u64) -> i64 {
    if dirp == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if count == 0 {
        return 0;
    }
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    if state.open_files[open_idx].kind != LINUX_OPEN_KIND_DIR {
        return linux_neg_errno(20); // ENOTDIR
    }
    let dir_idx = state.open_files[open_idx].object_index;
    let mut dir_path = [0u8; LINUX_PATH_MAX];
    let Some(dir_path_len) = linux_get_dir_slot_path(state, dir_idx, &mut dir_path) else {
        return linux_neg_errno(9);
    };

    let mut entries: Vec<(String, u8)> = Vec::new();
    let mut push_entry = |name: &str, d_type: u8| {
        if name.is_empty() {
            return;
        }
        for existing in entries.iter() {
            if existing.0.as_str() == name {
                return;
            }
        }
        entries.push((String::from(name), d_type));
    };

    push_entry(".", LINUX_DT_DIR);
    push_entry("..", LINUX_DT_DIR);

    if linux_path_equals(&dir_path, dir_path_len, "/") {
        push_entry("proc", LINUX_DT_DIR);
        push_entry("tmp", LINUX_DT_DIR);
        push_entry("run", LINUX_DT_DIR);
        push_entry("var", LINUX_DT_DIR);
    } else if linux_path_equals(&dir_path, dir_path_len, "/proc") {
        push_entry("self", LINUX_DT_DIR);
    } else if linux_path_equals(&dir_path, dir_path_len, "/proc/self") {
        push_entry("exe", LINUX_DT_REG);
        push_entry("cwd", LINUX_DT_DIR);
    } else if linux_path_equals(&dir_path, dir_path_len, "/tmp") {
        push_entry(".x11-unix", LINUX_DT_DIR);
    } else if linux_path_equals(&dir_path, dir_path_len, "/tmp/.x11-unix") {
        push_entry("x0", LINUX_DT_SOCK);
        push_entry("x1", LINUX_DT_SOCK);
    } else if linux_path_equals_ascii_casefold(&dir_path, dir_path_len, "/run") {
        push_entry("user", LINUX_DT_DIR);
        push_entry("dbus", LINUX_DT_DIR);
    } else if linux_path_equals_ascii_casefold(&dir_path, dir_path_len, "/run/user") {
        push_entry("0", LINUX_DT_DIR);
    } else if linux_path_matches_run_user_dir(&dir_path, dir_path_len) {
        push_entry("bus", LINUX_DT_SOCK);
    } else if linux_path_equals_ascii_casefold(&dir_path, dir_path_len, "/run/dbus") {
        push_entry("system_bus_socket", LINUX_DT_SOCK);
    } else if linux_path_equals_ascii_casefold(&dir_path, dir_path_len, "/var") {
        push_entry("run", LINUX_DT_DIR);
    } else if linux_path_equals_ascii_casefold(&dir_path, dir_path_len, "/var/run") {
        push_entry("dbus", LINUX_DT_DIR);
    } else if linux_path_equals_ascii_casefold(&dir_path, dir_path_len, "/var/run/dbus") {
        push_entry("system_bus_socket", LINUX_DT_SOCK);
    }

    let mut i = 0usize;
    while i < LINUX_MAX_RUNTIME_FILES {
        let slot = &state.runtime_files[i];
        if !slot.active || slot.path_len == 0 {
            i += 1;
            continue;
        }
        let mut abs = [0u8; LINUX_PATH_MAX];
        let abs_len = linux_runtime_slot_abs_path(slot, &mut abs);
        if abs_len <= 1 || abs[0] != b'/' {
            i += 1;
            continue;
        }
        let mut child_start = 0usize;
        if dir_path_len == 1 && dir_path[0] == b'/' {
            child_start = 1;
        } else if linux_path_prefix_of(&dir_path, dir_path_len, &abs, abs_len) {
            child_start = dir_path_len.saturating_add(1);
        } else {
            i += 1;
            continue;
        }
        if child_start >= abs_len {
            i += 1;
            continue;
        }
        let mut child_end = child_start;
        let mut child_type = LINUX_DT_REG;
        while child_end < abs_len {
            if abs[child_end] == b'/' {
                child_type = LINUX_DT_DIR;
                break;
            }
            child_end += 1;
        }
        if child_end == child_start {
            i += 1;
            continue;
        }
        let name = String::from_utf8_lossy(&abs[child_start..child_end]).into_owned();
        push_entry(name.as_str(), child_type);
        i += 1;
    }

    let total_entries = entries.len();
    let start_index = (state.open_files[open_idx].cursor as usize).min(total_entries);
    if start_index >= total_entries {
        return 0;
    }

    let cap = count.min(usize::MAX as u64) as usize;
    let mut written = 0usize;
    let mut entry_idx = start_index;
    while entry_idx < total_entries {
        let (name, d_type) = &entries[entry_idx];
        let reclen = match linux_vfs_emit_dirent64(
            dirp,
            written,
            cap,
            linux_vfs_hash_name(name.as_bytes()),
            (entry_idx + 1) as u64,
            *d_type,
            name.as_str(),
        ) {
            Some(v) => v,
            None => break,
        };
        written = written.saturating_add(reclen);
        entry_idx += 1;
    }
    if written == 0 {
        return 0;
    }
    state.open_files[open_idx].cursor = entry_idx as u64;
    written as i64
}

fn linux_fd_valid(state: &LinuxShimState, fd: i32) -> bool {
    fd >= 0 && (fd <= 2 || linux_find_open_slot_index(state, fd).is_some())
}

fn linux_epoll_events_to_poll(events: u32) -> i16 {
    let mut out = 0i16;
    if (events & LINUX_EPOLLIN) != 0 {
        out |= LINUX_POLLIN;
    }
    if (events & LINUX_EPOLLOUT) != 0 {
        out |= LINUX_POLLOUT;
    }
    if (events & LINUX_EPOLLERR) != 0 {
        out |= LINUX_POLLERR;
    }
    if (events & LINUX_EPOLLHUP) != 0 {
        out |= LINUX_POLLHUP;
    }
    out
}

fn linux_poll_to_epoll_events(events: i16) -> u32 {
    let mut out = 0u32;
    if (events & LINUX_POLLIN) != 0 {
        out |= LINUX_EPOLLIN;
    }
    if (events & LINUX_POLLOUT) != 0 {
        out |= LINUX_EPOLLOUT;
    }
    if (events & LINUX_POLLERR) != 0 || (events & LINUX_POLLNVAL) != 0 {
        out |= LINUX_EPOLLERR;
    }
    if (events & LINUX_POLLHUP) != 0 {
        out |= LINUX_EPOLLHUP;
    }
    out
}

fn linux_epoll_slot_has_ready(state: &LinuxShimState, epoll_idx: usize) -> bool {
    if epoll_idx >= state.epolls.len() || !state.epolls[epoll_idx].active {
        return false;
    }
    let mut w = 0usize;
    while w < LINUX_MAX_EPOLL_WATCHES {
        let watch = state.epolls[epoll_idx].watches[w];
        if watch.active {
            if let Some(target_idx) = linux_find_open_slot_index(state, watch.target_fd) {
                if state.open_files[target_idx].kind == LINUX_OPEN_KIND_EPOLL {
                    w += 1;
                    continue;
                }
            }
            let poll_mask = linux_epoll_events_to_poll(watch.events);
            let poll_ready = linux_poll_ready_mask(state, watch.target_fd, poll_mask);
            if poll_ready != 0 && poll_ready != LINUX_POLLNVAL {
                return true;
            }
        }
        w += 1;
    }
    false
}

fn linux_poll_ready_mask(state: &LinuxShimState, fd: i32, events: i16) -> i16 {
    if fd < 0 {
        return 0;
    }
    if !linux_fd_valid(state, fd) {
        return LINUX_POLLNVAL;
    }

    if fd <= 2 {
        let mut ready = 0i16;
        if (events & LINUX_POLLIN) != 0 && fd == 0 {
            ready |= LINUX_POLLIN;
        }
        if (events & LINUX_POLLOUT) != 0 && fd >= 1 {
            ready |= LINUX_POLLOUT;
        }
        return ready;
    }

    let Some(open_idx) = linux_find_open_slot_index(state, fd) else {
        return LINUX_POLLNVAL;
    };
    let slot = state.open_files[open_idx];

    let mut ready = 0i16;
    match slot.kind {
        LINUX_OPEN_KIND_RUNTIME => {
            if (events & LINUX_POLLIN) != 0 {
                ready |= LINUX_POLLIN;
            }
            if (events & LINUX_POLLOUT) != 0 {
                ready |= LINUX_POLLOUT;
            }
        }
        LINUX_OPEN_KIND_DIR => {
            if (events & LINUX_POLLIN) != 0 {
                ready |= LINUX_POLLIN;
            }
            if (events & LINUX_POLLOUT) != 0 {
                ready |= LINUX_POLLOUT;
            }
        }
        LINUX_OPEN_KIND_STDIO_DUP => {
            let target = slot.aux as i32;
            if (events & LINUX_POLLIN) != 0 && target == 0 {
                ready |= LINUX_POLLIN;
            }
            if (events & LINUX_POLLOUT) != 0 && target >= 1 {
                ready |= LINUX_POLLOUT;
            }
        }
        LINUX_OPEN_KIND_EVENTFD => {
            if slot.object_index >= LINUX_MAX_EVENTFDS || !state.eventfds[slot.object_index].active {
                return LINUX_POLLNVAL;
            }
            if (events & LINUX_POLLIN) != 0 && state.eventfds[slot.object_index].counter > 0 {
                ready |= LINUX_POLLIN;
            }
            if (events & LINUX_POLLOUT) != 0 {
                ready |= LINUX_POLLOUT;
            }
        }
        LINUX_OPEN_KIND_PIPE_READ => {
            if slot.object_index >= LINUX_MAX_PIPES || !state.pipes[slot.object_index].active {
                return LINUX_POLLNVAL;
            }
            let pipe = &state.pipes[slot.object_index];
            if (events & LINUX_POLLIN) != 0 && (pipe.pending_bytes > 0 || !pipe.write_open) {
                ready |= LINUX_POLLIN;
            }
            if !pipe.write_open {
                ready |= LINUX_POLLHUP;
            }
        }
        LINUX_OPEN_KIND_PIPE_WRITE => {
            if slot.object_index >= LINUX_MAX_PIPES || !state.pipes[slot.object_index].active {
                return LINUX_POLLNVAL;
            }
            let pipe = &state.pipes[slot.object_index];
            if pipe.read_open {
                if (events & LINUX_POLLOUT) != 0 {
                    ready |= LINUX_POLLOUT;
                }
            } else {
                ready |= LINUX_POLLERR;
            }
        }
        LINUX_OPEN_KIND_EPOLL => {
            if (events & LINUX_POLLIN) != 0 && linux_epoll_slot_has_ready(state, slot.object_index) {
                ready |= LINUX_POLLIN;
            }
            if (events & LINUX_POLLOUT) != 0 {
                ready |= LINUX_POLLOUT;
            }
        }
        LINUX_OPEN_KIND_SOCKET => {
            if slot.object_index >= LINUX_MAX_SOCKETS || !state.sockets[slot.object_index].active {
                return LINUX_POLLNVAL;
            }
            let sock = &state.sockets[slot.object_index];
            let mut rx_ready = linux_socket_rx_available(sock) > 0;
            if !rx_ready && sock.listening && sock.pending_accept_index >= 0 {
                rx_ready = true;
            }
            if !rx_ready
                && sock.endpoint == LINUX_SOCKET_ENDPOINT_X11
                && sock.x11_state == LINUX_X11_STATE_READY
                && linux_gfx_bridge_input_pending() > 0
            {
                rx_ready = true;
            }
            if (events & LINUX_POLLIN) != 0 && rx_ready {
                ready |= LINUX_POLLIN;
            }
            if (events & LINUX_POLLOUT) != 0 && (sock.connected || sock.sock_type == LINUX_SOCK_DGRAM) {
                ready |= LINUX_POLLOUT;
            }
            if !sock.connected && sock.peer_index < 0 && sock.endpoint == LINUX_SOCKET_ENDPOINT_PAIR {
                ready |= LINUX_POLLHUP;
            }
        }
        LINUX_OPEN_KIND_PIDFD => {
            // pidfd becomes readable (POLLIN) when the target process exits.
            let target_pid = slot.object_index as u32;
            let still_alive = linux_find_process_slot_index(state, target_pid).is_some();
            if (events & LINUX_POLLIN) != 0 && !still_alive {
                ready |= LINUX_POLLIN;
            }
        }
        _ => return LINUX_POLLNVAL,
    }
    ready
}

fn linux_sys_poll(state: &LinuxShimState, fds_ptr: u64, nfds: u64, _timeout_ms: i64) -> i64 {
    if nfds == 0 {
        return 0;
    }
    if fds_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let count = (nfds as usize).min(1024);
    let mut ready_count = 0i64;
    unsafe {
        let fds = fds_ptr as *mut LinuxPollFd;
        let mut i = 0usize;
        while i < count {
            let mut slot = ptr::read(fds.add(i));
            let ready = linux_poll_ready_mask(state, slot.fd, slot.events);
            slot.revents = ready;
            if ready != 0 {
                ready_count += 1;
            }
            ptr::write(fds.add(i), slot);
            i += 1;
        }
    }
    ready_count
}

fn linux_sys_ppoll(
    state: &LinuxShimState,
    fds_ptr: u64,
    nfds: u64,
    _tsp: u64,
    _sigmask: u64,
    _sigsetsize: u64,
) -> i64 {
    linux_sys_poll(state, fds_ptr, nfds, 0)
}

fn linux_lookup_socket_index(state: &LinuxShimState, fd: i32) -> Result<usize, i64> {
    if fd < 0 {
        return Err(linux_neg_errno(9)); // EBADF
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd) else {
        return Err(linux_neg_errno(9));
    };
    let open = state.open_files[open_idx];
    if open.kind != LINUX_OPEN_KIND_SOCKET {
        return Err(linux_neg_errno(88)); // ENOTSOCK
    }
    if open.object_index >= LINUX_MAX_SOCKETS || !state.sockets[open.object_index].active {
        return Err(linux_neg_errno(9));
    }
    Ok(open.object_index)
}

fn linux_socket_send_payload(state: &mut LinuxShimState, sock_idx: usize, buf: u64, len: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if len == 0 {
        return 0;
    }
    if sock_idx >= LINUX_MAX_SOCKETS || !state.sockets[sock_idx].active {
        return linux_neg_errno(9);
    }
    let sock_type = state.sockets[sock_idx].sock_type;
    if !state.sockets[sock_idx].connected && sock_type != LINUX_SOCK_DGRAM {
        return linux_neg_errno(107); // ENOTCONN
    }

    if state.sockets[sock_idx].endpoint == LINUX_SOCKET_ENDPOINT_DBUS {
        let mut chunk = [0u8; 512];
        let mut sent = 0u64;
        let mut remaining = len.min(i64::MAX as u64);
        while remaining > 0 {
            let copy_len = remaining.min(chunk.len() as u64) as usize;
            unsafe {
                ptr::copy_nonoverlapping(
                    buf.saturating_add(sent) as *const u8,
                    chunk.as_mut_ptr(),
                    copy_len,
                );
            }
            linux_dbus_consume_payload(&mut state.sockets[sock_idx], &chunk[..copy_len]);
            sent = sent.saturating_add(copy_len as u64);
            remaining = remaining.saturating_sub(copy_len as u64);
        }
        return sent as i64;
    }

    if state.sockets[sock_idx].endpoint == LINUX_SOCKET_ENDPOINT_X11 {
        let mut chunk = [0u8; 4096];
        let mut sent = 0u64;
        let mut remaining = len.min(i64::MAX as u64);
        while remaining > 0 {
            let copy_len = remaining.min(chunk.len() as u64) as usize;
            unsafe {
                ptr::copy_nonoverlapping(
                    buf.saturating_add(sent) as *const u8,
                    chunk.as_mut_ptr(),
                    copy_len,
                );
            }
            linux_x11_consume_payload(state, sock_idx, &chunk[..copy_len]);
            sent = sent.saturating_add(copy_len as u64);
            remaining = remaining.saturating_sub(copy_len as u64);
        }
        return sent as i64;
    }

    if state.sockets[sock_idx].endpoint == LINUX_SOCKET_ENDPOINT_PAIR {
        let peer_idx_i = state.sockets[sock_idx].peer_index;
        if peer_idx_i < 0 {
            return linux_neg_errno(32); // EPIPE
        }
        let peer_idx = peer_idx_i as usize;
        if peer_idx >= LINUX_MAX_SOCKETS || !state.sockets[peer_idx].active {
            return linux_neg_errno(32);
        }
        linux_socket_compact_rx(&mut state.sockets[peer_idx]);
        let free = state.sockets[peer_idx]
            .rx_buf
            .len()
            .saturating_sub(state.sockets[peer_idx].rx_len);
        if free == 0 {
            return linux_neg_errno(11); // EAGAIN
        }
        let write_len = free.min(len.min(i64::MAX as u64) as usize);
        unsafe {
            ptr::copy_nonoverlapping(
                buf as *const u8,
                state.sockets[peer_idx]
                    .rx_buf
                    .as_mut_ptr()
                    .add(state.sockets[peer_idx].rx_len),
                write_len,
            );
        }
        state.sockets[peer_idx].rx_len = state.sockets[peer_idx].rx_len.saturating_add(write_len);
        return write_len as i64;
    }

    // Connected but no backend service yet: accept writes to keep app progressing.
    len.min(i64::MAX as u64) as i64
}

fn linux_socket_recv_payload(state: &mut LinuxShimState, sock_idx: usize, buf: u64, len: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if len == 0 {
        return 0;
    }
    if sock_idx >= LINUX_MAX_SOCKETS || !state.sockets[sock_idx].active {
        return linux_neg_errno(9);
    }

    if state.sockets[sock_idx].endpoint == LINUX_SOCKET_ENDPOINT_X11
        && state.sockets[sock_idx].x11_state == LINUX_X11_STATE_READY
    {
        linux_x11_pump_bridge_events(state, sock_idx);
    }

    let available = linux_socket_rx_available(&state.sockets[sock_idx]);
    if available == 0 {
        if state.sockets[sock_idx].endpoint == LINUX_SOCKET_ENDPOINT_PAIR
            && state.sockets[sock_idx].peer_index < 0
        {
            return 0; // peer closed
        }
        return linux_neg_errno(11); // EAGAIN
    }
    let read_len = available.min(len.min(i64::MAX as u64) as usize);
    unsafe {
        ptr::copy_nonoverlapping(
            state.sockets[sock_idx]
                .rx_buf
                .as_ptr()
                .add(state.sockets[sock_idx].rx_cursor),
            buf as *mut u8,
            read_len,
        );
    }
    state.sockets[sock_idx].rx_cursor = state.sockets[sock_idx].rx_cursor.saturating_add(read_len);
    if state.sockets[sock_idx].rx_cursor >= state.sockets[sock_idx].rx_len {
        state.sockets[sock_idx].rx_cursor = 0;
        state.sockets[sock_idx].rx_len = 0;
    }
    read_len as i64
}

fn linux_sys_socket(state: &mut LinuxShimState, domain: u64, sock_type_raw: u64, protocol: u64) -> i64 {
    let domain_u16 = domain as u16;
    if domain_u16 != LINUX_AF_UNIX && domain_u16 != LINUX_AF_INET {
        return linux_neg_errno(97); // EAFNOSUPPORT
    }
    let Some(sock_type) = linux_socket_kind_from_type(sock_type_raw) else {
        return linux_neg_errno(22); // EINVAL
    };
    let Some(sock_idx) = linux_allocate_socket_slot(state) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(fd) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24);
    };
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, fd) else {
        return linux_neg_errno(24);
    };
    state.sockets[sock_idx] = LinuxSocketSlot {
        active: true,
        domain: domain_u16,
        sock_type,
        protocol: protocol as i32,
        nonblock: (sock_type_raw & LINUX_SOCK_NONBLOCK) != 0,
        cloexec: (sock_type_raw & LINUX_SOCK_CLOEXEC) != 0,
        connected: false,
        bound: false,
        listening: false,
        endpoint: LINUX_SOCKET_ENDPOINT_NONE,
        _pad0: [0; 2],
        peer_index: -1,
        pending_accept_index: -1,
        last_error: 0,
        path_len: 0,
        x11_seq: 0,
        x11_state: LINUX_X11_STATE_HANDSHAKE,
        x11_byte_order: b'l',
        x11_bigreq: false,
        _pad1: [0; 1],
        rx_len: 0,
        rx_cursor: 0,
        path: [0; LINUX_PATH_MAX],
        rx_buf: [0; LINUX_SOCKET_RX_BUF],
    };
    state.open_files[open_idx] = LinuxOpenFileSlot {
        active: true,
        fd,
        kind: LINUX_OPEN_KIND_SOCKET,
        _pad_kind: [0; 3],
        object_index: sock_idx,
        cursor: 0,
        flags: sock_type_raw,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(1);
    fd as i64
}

fn linux_sys_socketpair(
    state: &mut LinuxShimState,
    domain: u64,
    sock_type_raw: u64,
    protocol: u64,
    sv_ptr: u64,
) -> i64 {
    if sv_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if domain as u16 != LINUX_AF_UNIX {
        return linux_neg_errno(95); // EOPNOTSUPP
    }
    let Some(sock_type) = linux_socket_kind_from_type(sock_type_raw) else {
        return linux_neg_errno(22);
    };

    let Some(sock_a) = linux_allocate_socket_slot(state) else {
        return linux_neg_errno(24);
    };
    let Some(sock_b) = linux_allocate_socket_slot(state) else {
        return linux_neg_errno(24);
    };
    let Some(fd_a) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24);
    };
    let Some(fd_b) = linux_find_unused_fd(state, fd_a.saturating_add(1)) else {
        return linux_neg_errno(24);
    };
    let Some(open_a) = linux_allocate_open_slot_for_fd(state, fd_a) else {
        return linux_neg_errno(24);
    };
    let Some(open_b) = linux_allocate_open_slot_for_fd(state, fd_b) else {
        return linux_neg_errno(24);
    };

    let base = LinuxSocketSlot {
        active: true,
        domain: LINUX_AF_UNIX,
        sock_type,
        protocol: protocol as i32,
        nonblock: (sock_type_raw & LINUX_SOCK_NONBLOCK) != 0,
        cloexec: (sock_type_raw & LINUX_SOCK_CLOEXEC) != 0,
        connected: true,
        bound: true,
        listening: false,
        endpoint: LINUX_SOCKET_ENDPOINT_PAIR,
        _pad0: [0; 2],
        peer_index: -1,
        pending_accept_index: -1,
        last_error: 0,
        path_len: 0,
        x11_seq: 0,
        x11_state: LINUX_X11_STATE_HANDSHAKE,
        x11_byte_order: b'l',
        x11_bigreq: false,
        _pad1: [0; 1],
        rx_len: 0,
        rx_cursor: 0,
        path: [0; LINUX_PATH_MAX],
        rx_buf: [0; LINUX_SOCKET_RX_BUF],
    };
    state.sockets[sock_a] = base;
    state.sockets[sock_b] = base;
    state.sockets[sock_a].peer_index = sock_b as i32;
    state.sockets[sock_b].peer_index = sock_a as i32;

    state.open_files[open_a] = LinuxOpenFileSlot {
        active: true,
        fd: fd_a,
        kind: LINUX_OPEN_KIND_SOCKET,
        _pad_kind: [0; 3],
        object_index: sock_a,
        cursor: 0,
        flags: sock_type_raw,
        aux: 0,
    };
    state.open_files[open_b] = LinuxOpenFileSlot {
        active: true,
        fd: fd_b,
        kind: LINUX_OPEN_KIND_SOCKET,
        _pad_kind: [0; 3],
        object_index: sock_b,
        cursor: 0,
        flags: sock_type_raw,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(2);
    unsafe {
        let out = sv_ptr as *mut i32;
        ptr::write(out, fd_a);
        ptr::write(out.add(1), fd_b);
    }
    0
}

fn linux_sys_connect(state: &mut LinuxShimState, fd: u64, addr_ptr: u64, addr_len: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if state.sockets[sock_idx].connected {
        return linux_neg_errno(106); // EISCONN
    }
    if addr_ptr == 0 || addr_len < 2 {
        return linux_neg_errno(22);
    }
    let addr_family = unsafe { ptr::read(addr_ptr as *const u16) };
    if addr_family == LINUX_AF_UNIX {
        let mut norm_path = [0u8; LINUX_PATH_MAX];
        let path_len = match linux_parse_sockaddr_un_path(addr_ptr, addr_len, &mut norm_path) {
            Ok(v) => v,
            Err(err) => return err,
        };
        state.last_unix_connect_path = norm_path;
        state.last_unix_connect_len = path_len as u16;
        state.last_unix_connect_errno = 0;

        let is_x11 = linux_path_is_virtual_x11_socket(&norm_path, path_len);

        if is_x11 {
            state.sockets[sock_idx].path = norm_path;
            state.sockets[sock_idx].path_len = path_len as u16;
            state.sockets[sock_idx].connected = true;
            state.sockets[sock_idx].x11_state = LINUX_X11_STATE_HANDSHAKE;
            state.sockets[sock_idx].x11_seq = 0;
            state.sockets[sock_idx].x11_byte_order = b'l';
            state.sockets[sock_idx].x11_bigreq = false;
            state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_X11;
            state.sockets[sock_idx].peer_index = -1;
            state.sockets[sock_idx].last_error = 0;
            linux_x11_ensure_root_window(state);
            linux_gfx_bridge_open(LINUX_GFX_MAX_WIDTH as u32, LINUX_GFX_MAX_HEIGHT as u32);
            linux_gfx_bridge_set_status("X11 subset: cliente conectado.");
            return 0;
        }

        let Some(listener_idx) = linux_find_unix_bound_socket_by_path(state, &norm_path, path_len) else {
            state.sockets[sock_idx].path = norm_path;
            state.sockets[sock_idx].path_len = path_len as u16;
            state.sockets[sock_idx].peer_index = -1;
            if linux_path_is_virtual_dbus_socket(&norm_path, path_len) {
                state.sockets[sock_idx].connected = true;
                state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_DBUS;
                state.sockets[sock_idx].x11_state = LINUX_DBUS_STATE_AUTH_WAIT;
                state.sockets[sock_idx].x11_seq = 0;
                state.sockets[sock_idx].last_error = 0;
                linux_gfx_bridge_set_status("Unix DBus subset: cliente conectado.");
                state.last_unix_connect_errno = 0;
                return 0;
            }
            state.sockets[sock_idx].connected = false;
            state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_NONE;
            state.sockets[sock_idx].last_error = 2; // ENOENT
            state.last_unix_connect_errno = 2;
            return linux_neg_errno(2);
        };
        if listener_idx == sock_idx {
            state.sockets[sock_idx].last_error = 22;
            return linux_neg_errno(22); // EINVAL
        }
        if !state.sockets[listener_idx].listening {
            state.sockets[sock_idx].last_error = 111;
            state.last_unix_connect_errno = 111;
            return linux_neg_errno(111); // ECONNREFUSED
        }
        if state.sockets[listener_idx].pending_accept_index >= 0 {
            state.sockets[sock_idx].last_error = 11;
            state.last_unix_connect_errno = 11;
            return linux_neg_errno(11); // EAGAIN (pending queue full)
        }
        if state.sockets[listener_idx].sock_type != state.sockets[sock_idx].sock_type {
            state.sockets[sock_idx].last_error = 91;
            return linux_neg_errno(91); // EPROTOTYPE
        }

        let Some(server_idx) = linux_allocate_socket_slot(state) else {
            state.sockets[sock_idx].last_error = 24;
            return linux_neg_errno(24); // EMFILE
        };

        let mut accepted = LinuxSocketSlot::empty();
        accepted.active = true;
        accepted.domain = LINUX_AF_UNIX;
        accepted.sock_type = state.sockets[listener_idx].sock_type;
        accepted.protocol = state.sockets[listener_idx].protocol;
        accepted.nonblock = state.sockets[listener_idx].nonblock;
        accepted.cloexec = state.sockets[listener_idx].cloexec;
        accepted.connected = true;
        accepted.bound = true;
        accepted.listening = false;
        accepted.endpoint = LINUX_SOCKET_ENDPOINT_PAIR;
        accepted.peer_index = sock_idx as i32;
        accepted.pending_accept_index = -1;
        accepted.path = state.sockets[listener_idx].path;
        accepted.path_len = state.sockets[listener_idx].path_len;
        state.sockets[server_idx] = accepted;

        state.sockets[sock_idx].path = norm_path;
        state.sockets[sock_idx].path_len = path_len as u16;
        state.sockets[sock_idx].connected = true;
        state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_PAIR;
        state.sockets[sock_idx].peer_index = server_idx as i32;
        state.sockets[sock_idx].pending_accept_index = -1;
        state.sockets[sock_idx].last_error = 0;
        state.sockets[listener_idx].pending_accept_index = server_idx as i32;
        state.last_unix_connect_errno = 0;
        return 0;
    }
    if addr_family == LINUX_AF_INET || addr_family == LINUX_AF_INET6 {
        if let Some(display) = linux_parse_x11_display_from_inet(addr_ptr, addr_len) {
            let mut synthetic_path = [0u8; LINUX_PATH_MAX];
            let synthetic_len = linux_x11_socket_path_from_display(&mut synthetic_path, display);
            state.last_unix_connect_path = synthetic_path;
            state.last_unix_connect_len = synthetic_len as u16;
            state.last_unix_connect_errno = 0;

            state.sockets[sock_idx].path = synthetic_path;
            state.sockets[sock_idx].path_len = synthetic_len as u16;
            state.sockets[sock_idx].connected = true;
            state.sockets[sock_idx].x11_state = LINUX_X11_STATE_HANDSHAKE;
            state.sockets[sock_idx].x11_seq = 0;
            state.sockets[sock_idx].x11_byte_order = b'l';
            state.sockets[sock_idx].x11_bigreq = false;
            state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_X11;
            linux_x11_ensure_root_window(state);
            linux_gfx_bridge_open(LINUX_GFX_MAX_WIDTH as u32, LINUX_GFX_MAX_HEIGHT as u32);
            linux_gfx_bridge_set_status("X11 subset: cliente TCP conectado.");
            state.sockets[sock_idx].last_error = 0;
            return 0;
        }
        state.sockets[sock_idx].last_error = 111;
        state.last_unix_connect_errno = 111;
        return linux_neg_errno(111); // ECONNREFUSED
    }
    state.sockets[sock_idx].last_error = 97;
    linux_neg_errno(97) // EAFNOSUPPORT
}

fn linux_sys_bind(state: &mut LinuxShimState, fd: u64, addr_ptr: u64, addr_len: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if addr_ptr == 0 || addr_len < 2 {
        return linux_neg_errno(22);
    }
    let family = unsafe { ptr::read(addr_ptr as *const u16) };
    if family != LINUX_AF_UNIX {
        return linux_neg_errno(97); // EAFNOSUPPORT
    }
    if state.sockets[sock_idx].bound && state.sockets[sock_idx].path_len > 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let mut norm_path = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_parse_sockaddr_un_path(addr_ptr, addr_len, &mut norm_path) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Some(existing_idx) = linux_find_unix_bound_socket_by_path(state, &norm_path, path_len) {
        if existing_idx != sock_idx {
            return linux_neg_errno(98); // EADDRINUSE
        }
    }
    state.sockets[sock_idx].path = norm_path;
    state.sockets[sock_idx].path_len = path_len as u16;
    state.sockets[sock_idx].bound = true;
    state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_UNIX_PATH;
    state.sockets[sock_idx].pending_accept_index = -1;
    0
}

fn linux_sys_listen(state: &mut LinuxShimState, fd: u64, _backlog: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if state.sockets[sock_idx].domain != LINUX_AF_UNIX {
        return linux_neg_errno(95); // EOPNOTSUPP
    }
    if !state.sockets[sock_idx].bound {
        return linux_neg_errno(22); // EINVAL
    }
    if state.sockets[sock_idx].sock_type == LINUX_SOCK_DGRAM {
        return linux_neg_errno(95); // EOPNOTSUPP
    }
    state.sockets[sock_idx].listening = true;
    state.sockets[sock_idx].endpoint = LINUX_SOCKET_ENDPOINT_UNIX_PATH;
    state.sockets[sock_idx].pending_accept_index = -1;
    0
}

fn linux_sys_accept(state: &mut LinuxShimState, fd: u64, addr_ptr: u64, addr_len_ptr: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !state.sockets[sock_idx].listening {
        return linux_neg_errno(22); // EINVAL
    }
    let pending_idx_i = state.sockets[sock_idx].pending_accept_index;
    if pending_idx_i < 0 {
        return linux_neg_errno(11); // EAGAIN
    }
    let pending_idx = pending_idx_i as usize;
    if pending_idx >= LINUX_MAX_SOCKETS || !state.sockets[pending_idx].active {
        state.sockets[sock_idx].pending_accept_index = -1;
        return linux_neg_errno(11);
    }
    if (addr_ptr == 0) ^ (addr_len_ptr == 0) {
        return linux_neg_errno(14); // EFAULT
    }

    let Some(new_fd) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, new_fd) else {
        return linux_neg_errno(24);
    };
    state.open_files[open_idx] = LinuxOpenFileSlot {
        active: true,
        fd: new_fd,
        kind: LINUX_OPEN_KIND_SOCKET,
        _pad_kind: [0; 3],
        object_index: pending_idx,
        cursor: 0,
        flags: 0,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(1);
    state.sockets[sock_idx].pending_accept_index = -1;

    if addr_ptr != 0 && addr_len_ptr != 0 {
        let req = unsafe { ptr::read(addr_len_ptr as *const u32) } as usize;
        if req >= core::mem::size_of::<LinuxSockAddrUn>() {
            let mut out = LinuxSockAddrUn {
                family: LINUX_AF_UNIX,
                path: [0; 108],
            };
            let copy_len = (state.sockets[pending_idx].path_len as usize).min(out.path.len().saturating_sub(1));
            let mut i = 0usize;
            while i < copy_len {
                out.path[i] = state.sockets[pending_idx].path[i];
                i += 1;
            }
            unsafe {
                ptr::write(addr_ptr as *mut LinuxSockAddrUn, out);
                ptr::write(addr_len_ptr as *mut u32, core::mem::size_of::<LinuxSockAddrUn>() as u32);
            }
        } else if req >= core::mem::size_of::<u16>() {
            unsafe {
                ptr::write(addr_ptr as *mut u16, LINUX_AF_UNIX);
                ptr::write(addr_len_ptr as *mut u32, core::mem::size_of::<u16>() as u32);
            }
        } else {
            return linux_neg_errno(22); // EINVAL
        }
    }

    new_fd as i64
}

fn linux_sys_accept4(state: &mut LinuxShimState, fd: u64, addr_ptr: u64, addr_len_ptr: u64, flags: u64) -> i64 {
    if flags & !(LINUX_SOCK_FLAGS_MASK) != 0 {
        return linux_neg_errno(22);
    }
    let accepted = linux_sys_accept(state, fd, addr_ptr, addr_len_ptr);
    if accepted < 0 {
        return accepted;
    }
    let new_fd = accepted as i32;
    if let Some(open_idx) = linux_find_open_slot_index(state, new_fd) {
        state.open_files[open_idx].flags |= flags;
        if open_idx < state.open_files.len()
            && state.open_files[open_idx].kind == LINUX_OPEN_KIND_SOCKET
            && state.open_files[open_idx].object_index < state.sockets.len()
            && state.sockets[state.open_files[open_idx].object_index].active
        {
            state.sockets[state.open_files[open_idx].object_index].nonblock =
                (flags & LINUX_SOCK_NONBLOCK) != 0;
            state.sockets[state.open_files[open_idx].object_index].cloexec =
                (flags & LINUX_SOCK_CLOEXEC) != 0;
        }
    }
    accepted
}

fn linux_sys_shutdown(state: &mut LinuxShimState, fd: u64, _how: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    state.sockets[sock_idx].connected = false;
    0
}

fn linux_sys_sendto(
    state: &mut LinuxShimState,
    fd: u64,
    buf: u64,
    len: u64,
    _flags: u64,
    _dest_addr: u64,
    _addr_len: u64,
) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    linux_socket_send_payload(state, sock_idx, buf, len)
}

fn linux_sys_recvfrom(
    state: &mut LinuxShimState,
    fd: u64,
    buf: u64,
    len: u64,
    _flags: u64,
    _src_addr: u64,
    _addr_len: u64,
) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    linux_socket_recv_payload(state, sock_idx, buf, len)
}

fn linux_sys_sendmsg(state: &mut LinuxShimState, fd: u64, msg_ptr: u64, _flags: u64) -> i64 {
    if msg_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let msg = unsafe { ptr::read(msg_ptr as *const LinuxMsgHdr) };
    if msg.msg_iov == 0 || msg.msg_iovlen == 0 {
        return 0;
    }
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let count = (msg.msg_iovlen as usize).min(1024);
    if state.sockets[sock_idx].endpoint == LINUX_SOCKET_ENDPOINT_X11 {
        let mut chunk = [0u8; 4096];
        let mut total = 0u64;
        let mut i = 0usize;
        unsafe {
            let iov_ptr = msg.msg_iov as *const LinuxIovec;
            while i < count {
                let iov = ptr::read(iov_ptr.add(i));
                if iov.len > 0 && iov.base == 0 {
                    if total > 0 {
                        return total.min(i64::MAX as u64) as i64;
                    }
                    return linux_neg_errno(14); // EFAULT
                }
                let mut off = 0u64;
                while off < iov.len {
                    let copy_len = iov
                        .len
                        .saturating_sub(off)
                        .min(chunk.len() as u64) as usize;
                    ptr::copy_nonoverlapping(
                        iov.base.saturating_add(off) as *const u8,
                        chunk.as_mut_ptr(),
                        copy_len,
                    );
                    linux_x11_consume_payload(state, sock_idx, &chunk[..copy_len]);
                    total = total.saturating_add(copy_len as u64);
                    off = off.saturating_add(copy_len as u64);
                }
                i += 1;
            }
        }
        return total.min(i64::MAX as u64) as i64;
    }
    let mut total = 0u64;
    let mut i = 0usize;
    unsafe {
        let iov_ptr = msg.msg_iov as *const LinuxIovec;
        while i < count {
            let iov = ptr::read(iov_ptr.add(i));
            let res = linux_sys_sendto(
                state,
                fd,
                iov.base,
                iov.len,
                0,
                msg.msg_name,
                msg.msg_namelen as u64,
            );
            if res < 0 {
                if total > 0 {
                    return total.min(i64::MAX as u64) as i64;
                }
                return res;
            }
            let sent = res as u64;
            total = total.saturating_add(sent);
            if sent < iov.len {
                break;
            }
            i += 1;
        }
    }
    total.min(i64::MAX as u64) as i64
}

fn linux_sys_recvmsg(state: &mut LinuxShimState, fd: u64, msg_ptr: u64, _flags: u64) -> i64 {
    if msg_ptr == 0 {
        return linux_neg_errno(14);
    }
    let msg = unsafe { ptr::read(msg_ptr as *const LinuxMsgHdr) };
    if msg.msg_iov == 0 || msg.msg_iovlen == 0 {
        return 0;
    }
    let count = (msg.msg_iovlen as usize).min(1024);
    let mut total = 0u64;
    let mut i = 0usize;
    unsafe {
        let iov_ptr = msg.msg_iov as *const LinuxIovec;
        while i < count {
            let iov = ptr::read(iov_ptr.add(i));
            let res = linux_sys_recvfrom(state, fd, iov.base, iov.len, 0, 0, 0);
            if res < 0 {
                if total > 0 {
                    return total.min(i64::MAX as u64) as i64;
                }
                return res;
            }
            let got = res as u64;
            total = total.saturating_add(got);
            if got < iov.len {
                break;
            }
            i += 1;
        }
    }
    total.min(i64::MAX as u64) as i64
}

fn linux_sys_getsockname(state: &mut LinuxShimState, fd: u64, addr_ptr: u64, addr_len_ptr: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if addr_ptr == 0 || addr_len_ptr == 0 {
        return linux_neg_errno(14);
    }
    let mut out_len = unsafe { ptr::read(addr_len_ptr as *const u32) } as usize;
    if out_len < core::mem::size_of::<LinuxSockAddr>() {
        return linux_neg_errno(22);
    }
    let family = state.sockets[sock_idx].domain;
    let out = LinuxSockAddr {
        family,
        data: [0; 14],
    };
    unsafe {
        ptr::write(addr_ptr as *mut LinuxSockAddr, out);
        ptr::write(addr_len_ptr as *mut u32, core::mem::size_of::<LinuxSockAddr>() as u32);
    }
    0
}

fn linux_sys_getpeername(state: &mut LinuxShimState, fd: u64, addr_ptr: u64, addr_len_ptr: u64) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !state.sockets[sock_idx].connected {
        return linux_neg_errno(107); // ENOTCONN
    }
    linux_sys_getsockname(state, fd, addr_ptr, addr_len_ptr)
}

fn linux_sys_setsockopt(
    state: &mut LinuxShimState,
    fd: u64,
    _level: u64,
    _optname: u64,
    _optval: u64,
    _optlen: u64,
) -> i64 {
    let fd_i = fd as i64;
    let _sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    0
}

fn linux_sys_getsockopt(
    state: &mut LinuxShimState,
    fd: u64,
    level: u64,
    optname: u64,
    optval: u64,
    optlen_ptr: u64,
) -> i64 {
    let fd_i = fd as i64;
    let sock_idx = match linux_lookup_socket_index(state, fd_i as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if optval == 0 || optlen_ptr == 0 {
        return linux_neg_errno(14);
    }
    let req_len = unsafe { ptr::read(optlen_ptr as *const u32) } as usize;
    if req_len < core::mem::size_of::<i32>() {
        return linux_neg_errno(22);
    }
    let mut value: i32 = 0;
    if level == LINUX_SOL_SOCKET {
        value = match optname {
            LINUX_SO_TYPE => state.sockets[sock_idx].sock_type as i32,
            LINUX_SO_ERROR => {
                let err = state.sockets[sock_idx].last_error;
                state.sockets[sock_idx].last_error = 0;
                err
            }
            LINUX_SO_SNDBUF | LINUX_SO_RCVBUF => LINUX_SOCKET_RX_BUF as i32,
            _ => 0,
        };
    }
    unsafe {
        ptr::write(optval as *mut i32, value);
        ptr::write(optlen_ptr as *mut u32, core::mem::size_of::<i32>() as u32);
    }
    0
}

fn linux_sys_epoll_create(state: &mut LinuxShimState, size: u64) -> i64 {
    if size == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    linux_sys_epoll_create1(state, 0)
}

fn linux_sys_epoll_create1(state: &mut LinuxShimState, flags: u64) -> i64 {
    if flags & !(LINUX_EPOLL_CLOEXEC) != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(ep_idx) = linux_allocate_epoll_slot(state) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(fd) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, fd) else {
        return linux_neg_errno(24);
    };
    state.epolls[ep_idx] = LinuxEpollSlot::empty();
    state.epolls[ep_idx].active = true;
    state.open_files[open_idx] = LinuxOpenFileSlot {
        active: true,
        fd,
        kind: LINUX_OPEN_KIND_EPOLL,
        _pad_kind: [0; 3],
        object_index: ep_idx,
        cursor: 0,
        flags,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(1);
    fd as i64
}

fn linux_sys_epoll_ctl(state: &mut LinuxShimState, epfd: u64, op: u64, fd: u64, event_ptr: u64) -> i64 {
    let epfd_i = epfd as i64;
    if epfd_i < 0 {
        return linux_neg_errno(9);
    }
    let Some(ep_open_idx) = linux_find_open_slot_index(state, epfd_i as i32) else {
        return linux_neg_errno(9);
    };
    if state.open_files[ep_open_idx].kind != LINUX_OPEN_KIND_EPOLL {
        return linux_neg_errno(22);
    }
    let ep_idx = state.open_files[ep_open_idx].object_index;
    if ep_idx >= state.epolls.len() || !state.epolls[ep_idx].active {
        return linux_neg_errno(9);
    }
    let target_fd = fd as i32;
    if !linux_fd_valid(state, target_fd) || target_fd == epfd_i as i32 {
        return linux_neg_errno(9);
    }

    let mut found: Option<usize> = None;
    let mut i = 0usize;
    while i < LINUX_MAX_EPOLL_WATCHES {
        let w = state.epolls[ep_idx].watches[i];
        if w.active && w.target_fd == target_fd {
            found = Some(i);
            break;
        }
        i += 1;
    }

    match op {
        LINUX_EPOLL_CTL_ADD => {
            if found.is_some() {
                return linux_neg_errno(17); // EEXIST
            }
            if event_ptr == 0 {
                return linux_neg_errno(14); // EFAULT
            }
            let ev = unsafe { ptr::read(event_ptr as *const LinuxEpollEvent) };
            let mut free_slot = None;
            let mut j = 0usize;
            while j < LINUX_MAX_EPOLL_WATCHES {
                if !state.epolls[ep_idx].watches[j].active {
                    free_slot = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(idx) = free_slot else {
                return linux_neg_errno(28); // ENOSPC
            };
            state.epolls[ep_idx].watches[idx] = LinuxEpollWatchSlot {
                active: true,
                target_fd,
                events: ev.events,
                data: ev.data,
            };
            0
        }
        LINUX_EPOLL_CTL_DEL => {
            let Some(idx) = found else {
                return linux_neg_errno(2); // ENOENT
            };
            state.epolls[ep_idx].watches[idx] = LinuxEpollWatchSlot::empty();
            0
        }
        LINUX_EPOLL_CTL_MOD => {
            let Some(idx) = found else {
                return linux_neg_errno(2); // ENOENT
            };
            if event_ptr == 0 {
                return linux_neg_errno(14);
            }
            let ev = unsafe { ptr::read(event_ptr as *const LinuxEpollEvent) };
            state.epolls[ep_idx].watches[idx].events = ev.events;
            state.epolls[ep_idx].watches[idx].data = ev.data;
            0
        }
        _ => linux_neg_errno(22), // EINVAL
    }
}

fn linux_sys_epoll_pwait(
    state: &LinuxShimState,
    epfd: u64,
    events_ptr: u64,
    maxevents: u64,
    _timeout: i64,
    _sigmask: u64,
    _sigsetsize: u64,
) -> i64 {
    let epfd_i = epfd as i64;
    if epfd_i < 0 {
        return linux_neg_errno(9);
    }
    if events_ptr == 0 {
        return linux_neg_errno(14);
    }
    let maxevents_i = maxevents as i64;
    if maxevents_i <= 0 {
        return linux_neg_errno(22);
    }
    let max_out = (maxevents_i as usize).min(LINUX_MAX_EPOLL_WATCHES);
    let Some(ep_open_idx) = linux_find_open_slot_index(state, epfd_i as i32) else {
        return linux_neg_errno(9);
    };
    if state.open_files[ep_open_idx].kind != LINUX_OPEN_KIND_EPOLL {
        return linux_neg_errno(22);
    }
    let ep_idx = state.open_files[ep_open_idx].object_index;
    if ep_idx >= state.epolls.len() || !state.epolls[ep_idx].active {
        return linux_neg_errno(9);
    }

    let mut count = 0usize;
    unsafe {
        let out = events_ptr as *mut LinuxEpollEvent;
        let mut i = 0usize;
        while i < LINUX_MAX_EPOLL_WATCHES && count < max_out {
            let watch = state.epolls[ep_idx].watches[i];
            if watch.active {
                let poll_mask = linux_epoll_events_to_poll(watch.events);
                let poll_ready = linux_poll_ready_mask(state, watch.target_fd, poll_mask);
                let ep_ready = linux_poll_to_epoll_events(poll_ready) & watch.events;
                if ep_ready != 0 {
                    ptr::write(
                        out.add(count),
                        LinuxEpollEvent {
                            events: ep_ready,
                            _pad: 0,
                            data: watch.data,
                        },
                    );
                    count += 1;
                }
            }
            i += 1;
        }
    }
    count as i64
}

fn linux_sys_epoll_pwait2(
    state: &LinuxShimState,
    epfd: u64,
    events_ptr: u64,
    maxevents: u64,
    timeout_ptr: u64,
    sigmask: u64,
    sigsetsize: u64,
) -> i64 {
    let timeout_ms = if timeout_ptr == 0 {
        -1
    } else {
        let ts = unsafe { ptr::read(timeout_ptr as *const LinuxTimespec) };
        if ts.tv_sec < 0 || ts.tv_nsec < 0 || ts.tv_nsec >= 1_000_000_000 {
            return linux_neg_errno(22); // EINVAL
        }
        let ms_from_sec = (ts.tv_sec as i128).saturating_mul(1000);
        let ms_from_nsec = (ts.tv_nsec as i128 + 999_999) / 1_000_000; // ceil
        let total = ms_from_sec.saturating_add(ms_from_nsec);
        if total > i64::MAX as i128 {
            i64::MAX
        } else {
            total as i64
        }
    };
    linux_sys_epoll_pwait(state, epfd, events_ptr, maxevents, timeout_ms, sigmask, sigsetsize)
}

fn linux_sys_epoll_wait(state: &LinuxShimState, epfd: u64, events_ptr: u64, maxevents: u64, timeout: i64) -> i64 {
    linux_sys_epoll_pwait(state, epfd, events_ptr, maxevents, timeout, 0, 0)
}

fn linux_sys_eventfd(state: &mut LinuxShimState, initval: u64) -> i64 {
    linux_sys_eventfd2(state, initval, 0)
}

fn linux_sys_pipe(state: &mut LinuxShimState, pipefd_ptr: u64) -> i64 {
    linux_sys_pipe2(state, pipefd_ptr, 0)
}

fn linux_sys_eventfd2(state: &mut LinuxShimState, initval: u64, flags: u64) -> i64 {
    let allowed = LINUX_EFD_SEMAPHORE | LINUX_EFD_NONBLOCK | LINUX_EFD_CLOEXEC;
    if flags & !allowed != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(event_idx) = linux_allocate_eventfd_slot(state) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(fd) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24);
    };
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, fd) else {
        return linux_neg_errno(24);
    };
    state.eventfds[event_idx] = LinuxEventFdSlot {
        active: true,
        semaphore: (flags & LINUX_EFD_SEMAPHORE) != 0,
        counter: initval,
    };
    state.open_files[open_idx] = LinuxOpenFileSlot {
        active: true,
        fd,
        kind: LINUX_OPEN_KIND_EVENTFD,
        _pad_kind: [0; 3],
        object_index: event_idx,
        cursor: 0,
        flags,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(1);
    fd as i64
}

fn linux_sys_timerfd_create(state: &mut LinuxShimState, clockid: u64, flags: u64) -> i64 {
    if clockid != LINUX_CLOCK_REALTIME && clockid != LINUX_CLOCK_MONOTONIC {
        return linux_neg_errno(22); // EINVAL
    }
    let fd = linux_sys_eventfd2(state, 0, flags & (LINUX_EFD_NONBLOCK | LINUX_EFD_CLOEXEC));
    if fd < 0 {
        return fd;
    }
    let fd_i = fd as i32;
    if let Some(open_idx) = linux_find_open_slot_index(state, fd_i) {
        state.open_files[open_idx].aux = LINUX_OPEN_AUX_TIMERFD;
    }
    fd
}

fn linux_sys_timerfd_settime(
    state: &mut LinuxShimState,
    fd: u64,
    flags: u64,
    new_value_ptr: u64,
    old_value_ptr: u64,
) -> i64 {
    if flags & !LINUX_TFD_TIMER_ABSTIME != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9);
    }
    if new_value_ptr == 0 {
        return linux_neg_errno(14);
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    let open = state.open_files[open_idx];
    if open.kind != LINUX_OPEN_KIND_EVENTFD || open.aux != LINUX_OPEN_AUX_TIMERFD {
        return linux_neg_errno(22);
    }
    if open.object_index >= LINUX_MAX_EVENTFDS || !state.eventfds[open.object_index].active {
        return linux_neg_errno(9);
    }
    if old_value_ptr != 0 {
        unsafe {
            ptr::write(
                old_value_ptr as *mut LinuxItimerSpec,
                LinuxItimerSpec {
                    it_interval: LinuxTimespec { tv_sec: 0, tv_nsec: 0 },
                    it_value: LinuxTimespec { tv_sec: 0, tv_nsec: 0 },
                },
            );
        }
    }
    let new_spec = unsafe { ptr::read(new_value_ptr as *const LinuxItimerSpec) };
    if new_spec.it_value.tv_sec == 0 && new_spec.it_value.tv_nsec == 0 {
        state.eventfds[open.object_index].counter = 0;
    } else {
        state.eventfds[open.object_index].counter = 1;
    }
    0
}

fn linux_sys_timerfd_gettime(state: &LinuxShimState, fd: u64, curr_value_ptr: u64) -> i64 {
    if curr_value_ptr == 0 {
        return linux_neg_errno(14);
    }
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9);
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    let open = state.open_files[open_idx];
    if open.kind != LINUX_OPEN_KIND_EVENTFD || open.aux != LINUX_OPEN_AUX_TIMERFD {
        return linux_neg_errno(22);
    }
    if open.object_index >= LINUX_MAX_EVENTFDS || !state.eventfds[open.object_index].active {
        return linux_neg_errno(9);
    }
    let pending = state.eventfds[open.object_index].counter;
    let spec = LinuxItimerSpec {
        it_interval: LinuxTimespec { tv_sec: 0, tv_nsec: 0 },
        it_value: if pending > 0 {
            LinuxTimespec { tv_sec: 0, tv_nsec: 1 }
        } else {
            LinuxTimespec { tv_sec: 0, tv_nsec: 0 }
        },
    };
    unsafe {
        ptr::write(curr_value_ptr as *mut LinuxItimerSpec, spec);
    }
    0
}

fn linux_sys_pipe2(state: &mut LinuxShimState, pipefd_ptr: u64, flags: u64) -> i64 {
    if pipefd_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let allowed = LINUX_O_NONBLOCK | LINUX_DUP3_CLOEXEC;
    if flags & !allowed != 0 {
        return linux_neg_errno(22); // EINVAL
    }

    let Some(pipe_idx) = linux_allocate_pipe_slot(state) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(read_fd) = linux_find_unused_fd(state, LINUX_FD_BASE) else {
        return linux_neg_errno(24);
    };
    let Some(write_fd) = linux_find_unused_fd(state, read_fd.saturating_add(1)) else {
        return linux_neg_errno(24);
    };
    let Some(read_open_idx) = linux_allocate_open_slot_for_fd(state, read_fd) else {
        return linux_neg_errno(24);
    };
    let Some(write_open_idx) = linux_allocate_open_slot_for_fd(state, write_fd) else {
        return linux_neg_errno(24);
    };

    state.pipes[pipe_idx] = LinuxPipeSlot {
        active: true,
        pending_bytes: 0,
        read_open: true,
        write_open: true,
    };
    state.open_files[read_open_idx] = LinuxOpenFileSlot {
        active: true,
        fd: read_fd,
        kind: LINUX_OPEN_KIND_PIPE_READ,
        _pad_kind: [0; 3],
        object_index: pipe_idx,
        cursor: 0,
        flags,
        aux: 0,
    };
    state.open_files[write_open_idx] = LinuxOpenFileSlot {
        active: true,
        fd: write_fd,
        kind: LINUX_OPEN_KIND_PIPE_WRITE,
        _pad_kind: [0; 3],
        object_index: pipe_idx,
        cursor: 0,
        flags,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(2);
    unsafe {
        let out = pipefd_ptr as *mut i32;
        ptr::write(out, read_fd);
        ptr::write(out.add(1), write_fd);
    }
    0
}

fn linux_sys_dup(state: &mut LinuxShimState, oldfd: u64) -> i64 {
    let old_fd = oldfd as i64;
    if old_fd < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let template = match linux_build_dup_template(state, old_fd as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let Some(new_fd) = linux_find_unused_fd(state, LINUX_FD_BASE) else {
        return linux_neg_errno(24); // EMFILE
    };
    linux_install_dup_fd(state, template, new_fd, false)
}

fn linux_sys_dup2(state: &mut LinuxShimState, oldfd: u64, newfd: u64) -> i64 {
    let old_fd = oldfd as i64;
    let new_fd = newfd as i64;
    if old_fd < 0 || new_fd < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let template = match linux_build_dup_template(state, old_fd as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if old_fd == new_fd {
        return new_fd;
    }
    linux_install_dup_fd(state, template, new_fd as i32, false)
}

fn linux_sys_dup3(state: &mut LinuxShimState, oldfd: u64, newfd: u64, flags: u64) -> i64 {
    if flags & !LINUX_DUP3_CLOEXEC != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let old_fd = oldfd as i64;
    let new_fd = newfd as i64;
    if old_fd < 0 || new_fd < 0 {
        return linux_neg_errno(9); // EBADF
    }
    if old_fd == new_fd {
        return linux_neg_errno(22); // EINVAL
    }
    let template = match linux_build_dup_template(state, old_fd as i32) {
        Ok(v) => v,
        Err(err) => return err,
    };
    linux_install_dup_fd(state, template, new_fd as i32, (flags & LINUX_DUP3_CLOEXEC) != 0)
}

fn linux_sys_prctl(_option: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64) -> i64 {
    0
}

fn linux_sys_sched_setaffinity(_pid: u64, cpusetsize: u64, mask_ptr: u64) -> i64 {
    if cpusetsize == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if mask_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    0
}

fn linux_sys_sched_getaffinity(_pid: u64, cpusetsize: u64, mask_ptr: u64) -> i64 {
    if cpusetsize == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if mask_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let out_len = (cpusetsize as usize).min(core::mem::size_of::<u64>());
    unsafe {
        ptr::write_bytes(mask_ptr as *mut u8, 0, out_len);
        ptr::write(mask_ptr as *mut u8, 1u8);
    }
    out_len as i64
}

fn linux_sys_getcpu(cpu_ptr: u64, node_ptr: u64, _cache_ptr: u64) -> i64 {
    if cpu_ptr != 0 {
        unsafe {
            ptr::write(cpu_ptr as *mut u32, 0);
        }
    }
    if node_ptr != 0 {
        unsafe {
            ptr::write(node_ptr as *mut u32, 0);
        }
    }
    0
}

fn linux_sys_memfd_create(state: &mut LinuxShimState, name_ptr: u64, flags: u64) -> i64 {
    if flags & !LINUX_MFD_CLOEXEC != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(runtime_idx) = linux_allocate_runtime_slot(state) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(fd) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24);
    };
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, fd) else {
        return linux_neg_errno(24);
    };

    let mut name_buf = [0u8; 64];
    let name_len = match linux_read_raw_c_string(name_ptr, &mut name_buf) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let fallback = b"anon";
    let name_slice = if name_len == 0 {
        &fallback[..]
    } else {
        &name_buf[..name_len]
    };
    let mut path = [0u8; LINUX_PATH_MAX];
    let path_len = linux_build_memfd_path(&mut path, name_slice, fd);
    if path_len == 0 {
        return linux_neg_errno(12); // ENOMEM
    }

    state.runtime_files[runtime_idx] = LinuxRuntimeFileSlot {
        active: true,
        size: 0,
        path_len: path_len as u16,
        path,
        data_ptr: 0,
        data_len: 0,
    };
    state.runtime_file_count = state.runtime_file_count.saturating_add(1);
    state.open_files[open_idx] = LinuxOpenFileSlot {
        active: true,
        fd,
        kind: LINUX_OPEN_KIND_RUNTIME,
        _pad_kind: [0; 3],
        object_index: runtime_idx,
        cursor: 0,
        flags: if (flags & LINUX_MFD_CLOEXEC) != 0 {
            LINUX_DUP3_CLOEXEC
        } else {
            0
        },
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(1);
    fd as i64
}

fn linux_sys_shmdt(_shmaddr: u64) -> i64 {
    0
}

fn linux_write_statx_mode(buf: u64, size: u64, mode: u16) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let now = (timer::ticks() / 1000) as i64;
    let stx = LinuxStatx {
        stx_mask: 0x0000_07ff,
        stx_blksize: 4096,
        stx_attributes: 0,
        stx_nlink: 1,
        stx_uid: 0,
        stx_gid: 0,
        stx_mode: mode,
        __spare0: 0,
        stx_ino: 1,
        stx_size: size,
        stx_blocks: (size.saturating_add(511)) / 512,
        stx_attributes_mask: 0,
        stx_atime: LinuxStatxTimestamp {
            tv_sec: now,
            tv_nsec: 0,
            __reserved: 0,
        },
        stx_btime: LinuxStatxTimestamp {
            tv_sec: now,
            tv_nsec: 0,
            __reserved: 0,
        },
        stx_ctime: LinuxStatxTimestamp {
            tv_sec: now,
            tv_nsec: 0,
            __reserved: 0,
        },
        stx_mtime: LinuxStatxTimestamp {
            tv_sec: now,
            tv_nsec: 0,
            __reserved: 0,
        },
        stx_rdev_major: 0,
        stx_rdev_minor: 0,
        stx_dev_major: 1,
        stx_dev_minor: 0,
        stx_mnt_id: 1,
        stx_dio_mem_align: 0,
        stx_dio_offset_align: 0,
        __spare3: [0; 12],
    };
    unsafe {
        ptr::write(buf as *mut LinuxStatx, stx);
    }
    0
}

fn linux_write_statx(buf: u64, size: u64) -> i64 {
    linux_write_statx_mode(buf, size, LINUX_STAT_MODE_REG as u16)
}

fn linux_sys_statx(
    state: &mut LinuxShimState,
    dirfd: u64,
    path_ptr: u64,
    _flags: u64,
    _mask: u64,
    statx_buf: u64,
) -> i64 {
    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(path_ptr, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_resolve_open_path(state, dirfd as i64, &input, input_len, &mut normalized) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let (exists, is_file, runtime_idx_opt, mode_bits) = linux_vfs_lookup_path(state, &normalized, path_len);
    let result = if !exists {
        linux_neg_errno(2)
    } else if is_file {
        let Some(runtime_idx) = runtime_idx_opt else {
            return linux_neg_errno(2);
        };
        linux_write_statx(statx_buf, state.runtime_files[runtime_idx].size)
    } else {
        linux_write_statx_mode(statx_buf, 0, mode_bits as u16)
    };
    linux_record_last_path_lookup(
        state,
        LINUX_SYS_STATX,
        &normalized,
        path_len,
        result,
        exists,
    );
    result
}

fn linux_sys_rseq(_rseq: u64, _rseq_len: u64, _flags: u64, _sig: u64) -> i64 {
    0
}

fn linux_sys_membarrier(cmd: u64, _flags: u64, _cpu_id: u64) -> i64 {
    if cmd == LINUX_MEMBARRIER_CMD_QUERY {
        // Report "no special barrier commands supported" instead of ENOSYS so
        // modern runtimes can gracefully fallback to user-space paths.
        return 0;
    }
    // Best-effort success for non-query commands in shim mode.
    0
}

fn linux_sys_openat(
    state: &mut LinuxShimState,
    dirfd: u64,
    path_ptr: u64,
    flags: u64,
    _mode: u64,
) -> i64 {
    let dirfd_i = dirfd as i64;
    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(path_ptr, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_resolve_open_path(state, dirfd_i, &input, input_len, &mut normalized) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let wants_create = (flags & LINUX_O_CREAT) != 0;
    let wants_excl = (flags & LINUX_O_EXCL) != 0;
    let wants_dir = (flags & LINUX_O_DIRECTORY) != 0;
    let cloexec = (flags & LINUX_O_CLOEXEC) != 0;
    let (exists, is_file, runtime_idx_opt, mode_bits) = linux_vfs_lookup_path(state, &normalized, path_len);
    if !exists {
        let result = if wants_create {
            linux_neg_errno(30) // EROFS (rootfs read-only)
        } else {
            linux_neg_errno(2) // ENOENT
        };
        linux_record_last_path_lookup(
            state,
            LINUX_SYS_OPENAT,
            &normalized,
            path_len,
            result,
            false,
        );
        return result;
    }
    if exists && wants_create && wants_excl {
        let result = linux_neg_errno(17); // EEXIST
        linux_record_last_path_lookup(
            state,
            LINUX_SYS_OPENAT,
            &normalized,
            path_len,
            result,
            true,
        );
        return result;
    }
    if is_file && wants_dir {
        let result = linux_neg_errno(20); // ENOTDIR
        linux_record_last_path_lookup(
            state,
            LINUX_SYS_OPENAT,
            &normalized,
            path_len,
            result,
            false,
        );
        return result;
    }
    if !is_file && mode_bits == LINUX_STAT_MODE_SOCK {
        let result = linux_neg_errno(6); // ENXIO
        linux_record_last_path_lookup(
            state,
            LINUX_SYS_OPENAT,
            &normalized,
            path_len,
            result,
            false,
        );
        return result;
    };

    let Some(fd) = linux_find_unused_fd(state, state.next_fd) else {
        return linux_neg_errno(24); // EMFILE
    };
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, fd) else {
        return linux_neg_errno(24); // EMFILE
    };
    let mut open_slot = LinuxOpenFileSlot::empty();
    open_slot.active = true;
    open_slot.fd = fd;
    open_slot.cursor = 0;
    open_slot.flags = flags;
    if cloexec {
        open_slot.flags |= LINUX_DUP3_CLOEXEC;
    }
    if is_file {
        let Some(runtime_idx) = runtime_idx_opt else {
            return linux_neg_errno(2);
        };
        open_slot.kind = LINUX_OPEN_KIND_RUNTIME;
        open_slot.object_index = runtime_idx;
    } else {
        let Some(dir_idx) = linux_allocate_dir_slot(state, &normalized, path_len) else {
            return linux_neg_errno(24); // EMFILE-style exhaustion in shim metadata
        };
        open_slot.kind = LINUX_OPEN_KIND_DIR;
        open_slot.object_index = dir_idx;
    }
    state.open_files[open_idx] = open_slot;
    state.open_file_count = state.open_file_count.saturating_add(1);
    let result = fd as i64;
    linux_record_last_path_lookup(
        state,
        LINUX_SYS_OPENAT,
        &normalized,
        path_len,
        result,
        true,
    );
    result
}

fn linux_sys_openat2(
    state: &mut LinuxShimState,
    dirfd: u64,
    path_ptr: u64,
    how_ptr: u64,
    size: u64,
) -> i64 {
    if path_ptr == 0 || how_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if size < core::mem::size_of::<LinuxOpenHow>() as u64 {
        return linux_neg_errno(22); // EINVAL
    }

    let mut how = LinuxOpenHow::empty();
    let copy_len = (size as usize).min(core::mem::size_of::<LinuxOpenHow>());
    unsafe {
        ptr::copy_nonoverlapping(
            how_ptr as *const u8,
            (&mut how as *mut LinuxOpenHow) as *mut u8,
            copy_len,
        );
    }

    // Resolve constraints are currently ignored by this shim and treated as best-effort openat.
    let _resolve = how.resolve;
    linux_sys_openat(state, dirfd, path_ptr, how.flags, how.mode)
}

fn linux_sys_read(state: &mut LinuxShimState, fd: u64, buf: u64, len: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    if fd_i == 0 {
        return 0;
    }
    if len == 0 {
        return 0;
    }
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9); // EBADF
    };
    let slot = state.open_files[open_idx];
    match slot.kind {
        LINUX_OPEN_KIND_RUNTIME => {
            let runtime_idx = slot.object_index;
            if runtime_idx >= state.runtime_files.len() {
                return linux_neg_errno(9);
            }
            let runtime = &state.runtime_files[runtime_idx];
            if !runtime.active {
                return linux_neg_errno(9);
            }
            let readable_len = runtime.size.min(runtime.data_len);
            if runtime.data_ptr == 0 || readable_len == 0 {
                return 0;
            }
            let cursor = state.open_files[open_idx].cursor;
            if cursor >= readable_len {
                return 0;
            }
            let remaining = readable_len.saturating_sub(cursor);
            let to_copy = remaining.min(len).min(i64::MAX as u64);
            if to_copy == 0 {
                return 0;
            }
            unsafe {
                ptr::copy_nonoverlapping(
                    (runtime.data_ptr.saturating_add(cursor)) as *const u8,
                    buf as *mut u8,
                    to_copy as usize,
                );
            }
            state.open_files[open_idx].cursor = cursor.saturating_add(to_copy);
            to_copy as i64
        }
        LINUX_OPEN_KIND_DIR => linux_neg_errno(21), // EISDIR
        LINUX_OPEN_KIND_EVENTFD => {
            if len < 8 {
                return linux_neg_errno(22); // EINVAL
            }
            let event_idx = slot.object_index;
            if event_idx >= LINUX_MAX_EVENTFDS || !state.eventfds[event_idx].active {
                return linux_neg_errno(9);
            }
            let counter = state.eventfds[event_idx].counter;
            if counter == 0 {
                return linux_neg_errno(11); // EAGAIN
            }
            let value = if state.eventfds[event_idx].semaphore {
                state.eventfds[event_idx].counter = counter.saturating_sub(1);
                1u64
            } else {
                state.eventfds[event_idx].counter = 0;
                counter
            };
            unsafe {
                ptr::write(buf as *mut u64, value);
            }
            8
        }
        LINUX_OPEN_KIND_PIPE_READ => {
            let pipe_idx = slot.object_index;
            if pipe_idx >= LINUX_MAX_PIPES || !state.pipes[pipe_idx].active {
                return linux_neg_errno(9);
            }
            let pending = state.pipes[pipe_idx].pending_bytes;
            if pending == 0 {
                if state.pipes[pipe_idx].write_open {
                    return linux_neg_errno(11); // EAGAIN
                }
                return 0;
            }
            let to_read = pending.min(len).min(i64::MAX as u64);
            unsafe {
                ptr::write_bytes(buf as *mut u8, 0, to_read as usize);
            }
            state.pipes[pipe_idx].pending_bytes = pending.saturating_sub(to_read);
            to_read as i64
        }
        LINUX_OPEN_KIND_SOCKET => linux_socket_recv_payload(state, slot.object_index, buf, len),
        LINUX_OPEN_KIND_STDIO_DUP => {
            let target = slot.aux as i32;
            if target == 0 {
                0
            } else {
                linux_neg_errno(9)
            }
        }
        _ => linux_neg_errno(9),
    }
}

fn linux_sys_lseek(state: &mut LinuxShimState, fd: u64, offset: u64, whence: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    let kind = state.open_files[open_idx].kind;
    let base = if kind == LINUX_OPEN_KIND_RUNTIME {
        let runtime_idx = state.open_files[open_idx].object_index;
        if runtime_idx >= state.runtime_files.len() || !state.runtime_files[runtime_idx].active {
            return linux_neg_errno(9);
        }
        let size = state.runtime_files[runtime_idx].size;
        match whence {
            LINUX_SEEK_SET => 0i128,
            LINUX_SEEK_CUR => state.open_files[open_idx].cursor as i128,
            LINUX_SEEK_END => size as i128,
            _ => return linux_neg_errno(22), // EINVAL
        }
    } else if kind == LINUX_OPEN_KIND_DIR {
        match whence {
            LINUX_SEEK_SET => 0i128,
            LINUX_SEEK_CUR => state.open_files[open_idx].cursor as i128,
            _ => return linux_neg_errno(22), // EINVAL
        }
    } else {
        return linux_neg_errno(29); // ESPIPE
    };
    let new_pos = base.saturating_add(offset as i64 as i128);
    if new_pos < 0 {
        return linux_neg_errno(22);
    }
    let new_cursor = new_pos as u64;
    state.open_files[open_idx].cursor = new_cursor;
    new_cursor as i64
}

fn linux_sys_fstat(state: &mut LinuxShimState, fd: u64, stat_ptr: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    let slot = state.open_files[open_idx];
    match slot.kind {
        LINUX_OPEN_KIND_RUNTIME => {
            let runtime_idx = slot.object_index;
            if runtime_idx >= state.runtime_files.len() || !state.runtime_files[runtime_idx].active {
                return linux_neg_errno(9);
            }
            linux_write_stat64(stat_ptr, state.runtime_files[runtime_idx].size)
        }
        LINUX_OPEN_KIND_DIR => linux_write_stat64_mode(stat_ptr, 0, LINUX_STAT_MODE_DIR),
        LINUX_OPEN_KIND_EVENTFD
        | LINUX_OPEN_KIND_PIPE_READ
        | LINUX_OPEN_KIND_PIPE_WRITE
        | LINUX_OPEN_KIND_EPOLL => {
            linux_write_stat64(stat_ptr, 0)
        }
        LINUX_OPEN_KIND_SOCKET => linux_write_stat64_mode(stat_ptr, 0, LINUX_STAT_MODE_SOCK),
        LINUX_OPEN_KIND_STDIO_DUP => linux_write_stat64(stat_ptr, 0),
        _ => linux_neg_errno(9),
    }
}

fn linux_sys_newfstatat(
    state: &mut LinuxShimState,
    dirfd: u64,
    path_ptr: u64,
    stat_ptr: u64,
    _flags: u64,
) -> i64 {
    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(path_ptr, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let mut normalized = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_resolve_open_path(state, dirfd as i64, &input, input_len, &mut normalized) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let (exists, is_file, runtime_idx_opt, mode_bits) = linux_vfs_lookup_path(state, &normalized, path_len);
    let result = if !exists {
        linux_neg_errno(2)
    } else if is_file {
        let Some(runtime_idx) = runtime_idx_opt else {
            return linux_neg_errno(2);
        };
        linux_write_stat64(stat_ptr, state.runtime_files[runtime_idx].size)
    } else {
        linux_write_stat64_mode(stat_ptr, 0, mode_bits)
    };
    linux_record_last_path_lookup(
        state,
        LINUX_SYS_NEWFSTATAT,
        &normalized,
        path_len,
        result,
        exists,
    );
    result
}

fn linux_sys_close(state: &mut LinuxShimState, fd: u64) -> i64 {
    let fd_i = fd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    if fd_i <= 2 {
        return 0;
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    linux_close_open_slot(state, open_idx);
    0
}

fn linux_sys_close_range(state: &mut LinuxShimState, first: u64, last: u64, flags: u64) -> i64 {
    if (flags & !(LINUX_CLOSE_RANGE_UNSHARE | LINUX_CLOSE_RANGE_CLOEXEC)) != 0 {
        return linux_neg_errno(22); // EINVAL
    }

    let first_fd = first.min(i32::MAX as u64) as i32;
    let last_fd = last.min(i32::MAX as u64) as i32;
    if first_fd > last_fd {
        return 0;
    }

    let cloexec_only = (flags & LINUX_CLOSE_RANGE_CLOEXEC) != 0;
    let mut i = 0usize;
    while i < LINUX_MAX_OPEN_FILES {
        if state.open_files[i].active {
            let fd = state.open_files[i].fd;
            if fd >= first_fd && fd <= last_fd {
                if cloexec_only {
                    state.open_files[i].flags |= LINUX_DUP3_CLOEXEC;
                } else {
                    linux_close_open_slot(state, i);
                }
            }
        }
        i += 1;
    }
    0
}

fn linux_mark_robust_futex_owner_died(state: &mut LinuxShimState, futex_uaddr: u64, exiting_tid: u32) {
    if futex_uaddr == 0 || (futex_uaddr & 0x3) != 0 || exiting_tid == 0 {
        return;
    }
    let cur = unsafe { ptr::read_volatile(futex_uaddr as *const u32) };
    if (cur & LINUX_FUTEX_TID_MASK) != exiting_tid {
        return;
    }
    let mut next = cur & !LINUX_FUTEX_TID_MASK;
    next |= LINUX_FUTEX_OWNER_DIED;
    unsafe {
        ptr::write_volatile(futex_uaddr as *mut u32, next);
    }
    let _ = linux_wake_futex_waiters(state, futex_uaddr, 1);
}

fn linux_robust_entry_futex_uaddr(entry: u64, futex_offset: i64) -> Option<u64> {
    let entry_i = entry as i128;
    let off_i = futex_offset as i128;
    let addr_i = entry_i.saturating_add(off_i);
    if addr_i <= 0 || addr_i > u64::MAX as i128 {
        return None;
    }
    let addr = addr_i as u64;
    if (addr & 0x3) != 0 {
        return None;
    }
    Some(addr)
}

fn linux_cleanup_thread_robust_list(
    state: &mut LinuxShimState,
    robust_head: u64,
    robust_len: u64,
    exiting_tid: u32,
) {
    if robust_head == 0 || robust_len < LINUX_ROBUST_LIST_HEAD_LEN_MIN || exiting_tid == 0 {
        return;
    }
    let head = unsafe { ptr::read(robust_head as *const LinuxRobustListHead) };
    let mut node = head.list_next;
    let mut visited = 0usize;
    while node != 0 && node != robust_head && visited < LINUX_ROBUST_LIST_MAX_NODES {
        if let Some(futex_uaddr) = linux_robust_entry_futex_uaddr(node, head.futex_offset) {
            linux_mark_robust_futex_owner_died(state, futex_uaddr, exiting_tid);
        }
        let next = unsafe { ptr::read(node as *const u64) };
        if next == node {
            break;
        }
        node = next;
        visited = visited.saturating_add(1);
    }
    let pending = head.list_op_pending;
    if pending != 0 && pending != robust_head {
        if let Some(futex_uaddr) = linux_robust_entry_futex_uaddr(pending, head.futex_offset) {
            linux_mark_robust_futex_owner_died(state, futex_uaddr, exiting_tid);
        }
    }
}

fn linux_cleanup_exiting_thread_sync(state: &mut LinuxShimState, slot: LinuxThreadSlot) {
    if slot.robust_list_head != 0 && slot.robust_list_len >= LINUX_ROBUST_LIST_HEAD_LEN_MIN {
        linux_cleanup_thread_robust_list(state, slot.robust_list_head, slot.robust_list_len, slot.tid);
    }
    if slot.tid_addr != 0 {
        unsafe {
            ptr::write(slot.tid_addr as *mut u32, 0);
        }
        let _ = linux_wake_futex_waiters(state, slot.tid_addr, 1);
    }
}

fn linux_sys_exit(state: &mut LinuxShimState, code: u64, exit_group: bool) -> i64 {
    linux_stdio_push_line(state);
    let exit_code = code as i32;
    if exit_group {
        let mut i = 0usize;
        while i < LINUX_MAX_THREADS {
            let slot = state.threads[i];
            if slot.active {
                linux_cleanup_exiting_thread_sync(state, slot);
            }
            i += 1;
        }
    }
    if !exit_group {
        if let Some(cur_idx) = linux_find_current_thread_slot_index(state) {
            let exiting_slot = state.threads[cur_idx];
            let exited_pid = exiting_slot.process_pid;
            let exited_signal = exiting_slot.exit_signal as u64;
            linux_cleanup_exiting_thread_sync(state, exiting_slot);
            state.threads[cur_idx] = LinuxThreadSlot::empty();
            state.thread_contexts[cur_idx] = LinuxThreadContext::empty();
            if state.thread_count > 0 {
                state.thread_count -= 1;
            }
            if linux_count_threads_for_process(state, exited_pid) == 0 {
                let parent_pid = if let Some(proc_idx) = linux_find_process_slot_index(state, exited_pid) {
                    state.processes[proc_idx].parent_pid
                } else {
                    0
                };
                linux_reparent_child_processes(state, exited_pid, 1);
                linux_release_process_mmaps(state, exited_pid);
                linux_remove_process_slot(state, exited_pid);
                if parent_pid != 0 && parent_pid != exited_pid {
                    linux_push_exited_thread(
                        state,
                        parent_pid,
                        exited_pid,
                        exit_code,
                        LINUX_CHILD_EVENT_EXITED,
                    );
                    if exited_signal != 0 {
                        let _ = linux_queue_signal_for_process_pid(state, parent_pid, exited_signal);
                    }
                }
            }
            if state.thread_count > 0 {
                state.current_tid = 0;
                state.current_pid = 0;
                state.tid_value = 0;
                state.fs_base = 0;
                state.tid_addr = 0;
                state.signal_mask = 0;
                state.pending_signals = 0;
                state.robust_list_head = 0;
                state.robust_list_len = 0;
                privilege::linux_real_slice_request_yield();
                return 0;
            }
        }
    }

    linux_release_all_mmaps(state);
    unsafe {
        linux_shim_release_active_plan();
    }
    state.active = false;
    state.exit_code = exit_code;
    state.thread_count = 0;
    state.process_count = 0;
    state.current_tid = 0;
    state.current_pid = 0;
    state.processes = [LinuxProcessSlot::empty(); LINUX_MAX_PROCESSES];
    state.signal_mask = 0;
    state.pending_signals = 0;
    state.exited_tids = [0; LINUX_EXITED_QUEUE_CAP];
    state.exited_parent_tids = [0; LINUX_EXITED_QUEUE_CAP];
    state.exited_status = [0; LINUX_EXITED_QUEUE_CAP];
    state.exited_kinds = [0; LINUX_EXITED_QUEUE_CAP];
    state.exited_count = 0;
    0
}

fn linux_sys_brk(state: &mut LinuxShimState, requested: u64) -> i64 {
    if requested == 0 {
        return state.brk_current as i64;
    }
    let Some(new_brk) = linux_align_up(requested, 16) else {
        return state.brk_current as i64;
    };
    if new_brk < state.brk_base || new_brk > state.brk_limit {
        return state.brk_current as i64;
    }
    state.brk_current = new_brk;
    new_brk as i64
}

fn linux_sys_mmap(
    state: &mut LinuxShimState,
    requested_addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    fd_raw: u64,
    offset: u64,
) -> i64 {
    if len == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if (flags & LINUX_MAP_PRIVATE) == 0 && (flags & LINUX_MAP_SHARED) == 0 {
        return linux_neg_errno(22);
    }
    if (offset & (LINUX_PAGE_SIZE - 1)) != 0 {
        return linux_neg_errno(22);
    }

    let Some(aligned_len) = linux_align_up(len, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(12); // ENOMEM
    };
    if aligned_len > usize::MAX as u64 {
        return linux_neg_errno(12);
    }

    let map_fixed_requested = (flags & LINUX_MAP_FIXED) != 0;
    let can_try_in_place = map_fixed_requested && requested_addr != 0;

    // MAP_FIXED compat path:
    // If caller requests an exact address and we already own that exact mapped range,
    // remap in-place to avoid ENOSYS aborts on modern runtimes.
    if can_try_in_place {
        if let Some(slot_idx) = linux_find_mmap_slot_for_range(state, requested_addr, aligned_len) {
            let slot_addr = state.maps[slot_idx].addr;
            let slot_len = state.maps[slot_idx].len;
            if slot_addr == requested_addr && slot_len == aligned_len {
                unsafe {
                    ptr::write_bytes(slot_addr as *mut u8, 0, aligned_len as usize);
                }
                let is_anon = (flags & LINUX_MAP_ANONYMOUS) != 0;
                if !is_anon {
                    let fd = fd_raw as i64;
                    if fd < 0 {
                        return linux_neg_errno(9); // EBADF
                    }
                    let Some(open_idx) = linux_find_open_slot_index(state, fd as i32) else {
                        return linux_neg_errno(9);
                    };
                    if state.open_files[open_idx].kind != LINUX_OPEN_KIND_RUNTIME {
                        return linux_neg_errno(9);
                    }
                    let runtime_idx = state.open_files[open_idx].object_index;
                    if runtime_idx >= state.runtime_files.len() {
                        return linux_neg_errno(9);
                    }
                    let runtime = &state.runtime_files[runtime_idx];
                    if !runtime.active {
                        return linux_neg_errno(9);
                    }
                    let readable_len = runtime.size.min(runtime.data_len);
                    if runtime.data_ptr != 0 && readable_len > offset {
                        let copy_len = readable_len.saturating_sub(offset).min(aligned_len);
                        if copy_len > 0 {
                            unsafe {
                                ptr::copy_nonoverlapping(
                                    runtime.data_ptr.saturating_add(offset) as *const u8,
                                    slot_addr as *mut u8,
                                    copy_len as usize,
                                );
                            }
                        }
                    }
                }
                let slot = &mut state.maps[slot_idx];
                slot.prot = prot;
                slot.flags = flags;
                return slot.addr as i64;
            }
        }
    }

    let Ok(layout) = Layout::from_size_align(aligned_len as usize, LINUX_PAGE_SIZE as usize) else {
        return linux_neg_errno(12);
    };
    let mapped_ptr = unsafe { alloc(layout) };
    if mapped_ptr.is_null() {
        return linux_neg_errno(12);
    }
    unsafe {
        ptr::write_bytes(mapped_ptr, 0, aligned_len as usize);
    }

    let is_anon = (flags & LINUX_MAP_ANONYMOUS) != 0;
    let fd = fd_raw as i64;
    if is_anon {
        if fd != -1 {
            unsafe {
                dealloc(mapped_ptr, layout);
            }
            return linux_neg_errno(22); // EINVAL
        }
    } else {
        if fd < 0 {
            unsafe {
                dealloc(mapped_ptr, layout);
            }
            return linux_neg_errno(9); // EBADF
        }
        let Some(open_idx) = linux_find_open_slot_index(state, fd as i32) else {
            unsafe {
                dealloc(mapped_ptr, layout);
            }
            return linux_neg_errno(9);
        };
        if state.open_files[open_idx].kind != LINUX_OPEN_KIND_RUNTIME {
            unsafe {
                dealloc(mapped_ptr, layout);
            }
            return linux_neg_errno(9);
        }
        let runtime_idx = state.open_files[open_idx].object_index;
        if runtime_idx >= state.runtime_files.len() {
            unsafe {
                dealloc(mapped_ptr, layout);
            }
            return linux_neg_errno(9);
        }
        let runtime = &state.runtime_files[runtime_idx];
        if !runtime.active {
            unsafe {
                dealloc(mapped_ptr, layout);
            }
            return linux_neg_errno(9);
        }
        let readable_len = runtime.size.min(runtime.data_len);
        if runtime.data_ptr != 0 && readable_len > offset {
            let copy_len = readable_len.saturating_sub(offset).min(aligned_len);
            if copy_len > 0 {
                unsafe {
                    ptr::copy_nonoverlapping(
                        runtime.data_ptr.saturating_add(offset) as *const u8,
                        mapped_ptr,
                        copy_len as usize,
                    );
                }
            }
        }
    }

    let mut slot = None;
    let mut i = 0usize;
    while i < LINUX_MAX_MMAPS {
        if !state.maps[i].active {
            slot = Some(i);
            break;
        }
        i += 1;
    }
    let Some(slot_idx) = slot else {
        unsafe {
            dealloc(mapped_ptr, layout);
        }
        return linux_neg_errno(12);
    };

    let addr = mapped_ptr as u64;

    state.maps[slot_idx] = LinuxMmapSlot {
        active: true,
        process_pid: state.current_pid,
        addr,
        len: aligned_len,
        prot,
        flags,
        backing_ptr: addr,
        backing_len: aligned_len,
    };
    state.mmap_count = state.mmap_count.saturating_add(1);
    state.mmap_cursor = state.mmap_cursor.saturating_add(aligned_len).min(LINUX_MMAP_LIMIT);
    addr as i64
}

fn linux_sys_mprotect(state: &mut LinuxShimState, addr: u64, len: u64, prot: u64) -> i64 {
    if addr == 0 || len == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(aligned_len) = linux_align_up(len, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(22);
    };
    let Some(slot_idx) = linux_find_mmap_slot_for_range(state, addr, aligned_len) else {
        return linux_neg_errno(12); // ENOMEM
    };
    state.maps[slot_idx].prot = prot;
    0
}

fn linux_sys_madvise(_addr: u64, _len: u64, _advice: u64) -> i64 {
    // Advisory only in this shim; accept to avoid ENOSYS in libc startup paths.
    0
}

fn linux_sys_msync(_addr: u64, _len: u64, flags: u64) -> i64 {
    let allowed = LINUX_MS_ASYNC | LINUX_MS_INVALIDATE | LINUX_MS_SYNC;
    if (flags & !allowed) != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if (flags & LINUX_MS_ASYNC) != 0 && (flags & LINUX_MS_SYNC) != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    0
}

fn linux_sys_mincore(addr: u64, len: u64, vec: u64) -> i64 {
    if vec == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if len == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if (addr & (LINUX_PAGE_SIZE - 1)) != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(aligned_len) = linux_align_up(len, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(22);
    };
    let pages = aligned_len / LINUX_PAGE_SIZE;
    if pages > usize::MAX as u64 {
        return linux_neg_errno(12); // ENOMEM
    }
    unsafe {
        ptr::write_bytes(vec as *mut u8, 1, pages as usize);
    }
    0
}

fn linux_sys_mlock(_addr: u64, _len: u64) -> i64 {
    0
}

fn linux_sys_munlock(_addr: u64, _len: u64) -> i64 {
    0
}

fn linux_sys_mlockall(flags: u64) -> i64 {
    // MCL_CURRENT=1, MCL_FUTURE=2, MCL_ONFAULT=4
    if (flags & !0x7) != 0 {
        return linux_neg_errno(22); // EINVAL
    }
    0
}

fn linux_sys_munlockall() -> i64 {
    0
}

fn linux_sys_munmap(state: &mut LinuxShimState, addr: u64, len: u64) -> i64 {
    if addr == 0 || len == 0 {
        return linux_neg_errno(22); // EINVAL
    }

    let Some(aligned_len) = linux_align_up(len, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(22);
    };
    let Some(slot_idx) = linux_find_mmap_slot_for_range(state, addr, aligned_len) else {
        return linux_neg_errno(22);
    };

    let slot = &mut state.maps[slot_idx];
    if slot.addr != addr || slot.len != aligned_len {
        let slot_end = slot.addr.saturating_add(slot.len);
        let unmap_end = addr.saturating_add(aligned_len);

        if addr == slot.addr && aligned_len < slot.len {
            // Trim head.
            slot.addr = slot.addr.saturating_add(aligned_len);
            slot.len = slot.len.saturating_sub(aligned_len);
            return 0;
        }
        if unmap_end == slot_end && aligned_len < slot.len {
            // Trim tail.
            slot.len = slot.len.saturating_sub(aligned_len);
            return 0;
        }

        // Middle-hole unmap is accepted as compat no-op to keep user-space alive.
        return 0;
    }

    linux_release_mmap_slot(slot);
    if state.mmap_count > 0 {
        state.mmap_count -= 1;
    }
    if state.mmap_count == 0 {
        state.mmap_cursor = LINUX_MMAP_BASE;
    }
    0
}

fn linux_sys_clock_gettime(clock_id: u64, tp: u64) -> i64 {
    if tp == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if clock_id != LINUX_CLOCK_REALTIME && clock_id != LINUX_CLOCK_MONOTONIC {
        return linux_neg_errno(22); // EINVAL
    }

    let ticks = timer::ticks();
    let secs = (ticks / 1000) as i64;
    let nanos = ((ticks % 1000) * 1_000_000) as i64;
    unsafe {
        let out = tp as *mut LinuxTimespec;
        ptr::write(
            out,
            LinuxTimespec {
                tv_sec: secs,
                tv_nsec: nanos,
            },
        );
    }
    0
}

fn linux_sys_clock_getres(clock_id: u64, tp: u64) -> i64 {
    if tp == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if clock_id != LINUX_CLOCK_REALTIME && clock_id != LINUX_CLOCK_MONOTONIC {
        return linux_neg_errno(22); // EINVAL
    }

    unsafe {
        let out = tp as *mut LinuxTimespec;
        ptr::write(
            out,
            LinuxTimespec {
                tv_sec: 0,
                // Shim timer granularity is effectively 1ms.
                tv_nsec: 1_000_000,
            },
        );
    }
    0
}

fn linux_sys_gettimeofday(tv: u64, tz: u64) -> i64 {
    let ticks = timer::ticks();
    let secs = (ticks / 1000) as i64;
    let usec = ((ticks % 1000) * 1000) as i64;

    if tv != 0 {
        unsafe {
            let out = tv as *mut LinuxTimeval;
            ptr::write(
                out,
                LinuxTimeval {
                    tv_sec: secs,
                    tv_usec: usec,
                },
            );
        }
    }

    if tz != 0 {
        unsafe {
            let out = tz as *mut LinuxTimezone;
            ptr::write(
                out,
                LinuxTimezone {
                    tz_minuteswest: 0,
                    tz_dsttime: 0,
                },
            );
        }
    }
    0
}

fn linux_sys_getrusage(_who: u64, usage_ptr: u64) -> i64 {
    if usage_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let ticks = timer::ticks() as i64;
    let utime = LinuxTimeval {
        tv_sec: ticks / 1000,
        tv_usec: ((ticks % 1000) * 1000),
    };
    let stime = LinuxTimeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let usage = LinuxRusage {
        ru_utime: utime,
        ru_stime: stime,
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    };
    unsafe {
        ptr::write(usage_ptr as *mut LinuxRusage, usage);
    }
    0
}

fn linux_sys_sysinfo(state: &LinuxShimState, info_ptr: u64) -> i64 {
    if info_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let mem = crate::memory::stats();
    let total = mem.total_bytes();
    let free = mem.conventional_bytes().min(total);
    let info = LinuxSysinfo {
        uptime: (timer::ticks() / 1000) as i64,
        loads: [0, 0, 0],
        totalram: total,
        freeram: free,
        sharedram: 0,
        bufferram: 0,
        totalswap: 0,
        freeswap: 0,
        procs: state.process_count.min(u16::MAX as usize) as u16,
        _pad: 0,
        totalhigh: 0,
        freehigh: 0,
        mem_unit: 1,
        _f: [0; 8],
    };
    unsafe {
        ptr::write(info_ptr as *mut LinuxSysinfo, info);
    }
    0
}

fn linux_sys_times(buf_ptr: u64) -> i64 {
    let ticks = timer::ticks() as i64;
    if buf_ptr != 0 {
        let tms = LinuxTms {
            tms_utime: ticks,
            tms_stime: 0,
            tms_cutime: 0,
            tms_cstime: 0,
        };
        unsafe {
            ptr::write(buf_ptr as *mut LinuxTms, tms);
        }
    }
    ticks
}

fn linux_sys_nanosleep(req: u64, rem: u64) -> i64 {
    if req == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    // Cooperative stub for phase0/phase1: we acknowledge sleep requests
    // without blocking the UI thread.
    if rem != 0 {
        unsafe {
            let out = rem as *mut LinuxTimespec;
            ptr::write(
                out,
                LinuxTimespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
            );
        }
    }
    0
}

fn linux_sys_sched_yield(state: &mut LinuxShimState) -> i64 {
    if linux_count_runnable_threads(state) > 1 {
        if let Some(next_tid) = linux_pick_next_runnable_thread_tid(state, state.current_tid) {
            let _ = linux_request_thread_switch(state, next_tid);
        }
    }
    0
}

fn linux_sys_futex(
    state: &mut LinuxShimState,
    uaddr: u64,
    op: u64,
    val: u64,
    timeout_or_val2: u64,
    uaddr2: u64,
    val3: u64,
) -> i64 {
    if uaddr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if (uaddr & 0x3) != 0 {
        return linux_neg_errno(22); // EINVAL
    }

    let cmd = op & !(LINUX_FUTEX_PRIVATE_FLAG | LINUX_FUTEX_CLOCK_REALTIME);
    match cmd {
        LINUX_FUTEX_WAIT | LINUX_FUTEX_WAIT_BITSET => {
            if cmd == LINUX_FUTEX_WAIT_BITSET && val3 == 0 {
                return linux_neg_errno(22); // EINVAL
            }
            let current = unsafe { ptr::read_volatile(uaddr as *const u32) as u64 };
            if current != (val as u32 as u64) {
                return linux_neg_errno(11); // EAGAIN
            }
            let absolute_timeout = cmd == LINUX_FUTEX_WAIT_BITSET && (op & LINUX_FUTEX_CLOCK_REALTIME) != 0;
            let timeout_deadline =
                match linux_futex_timeout_deadline_from_ptr(timeout_or_val2, absolute_timeout) {
                Ok(v) => v,
                Err(err) => return err,
            };
            linux_futex_block_current_and_request_switch(
                state,
                uaddr,
                if cmd == LINUX_FUTEX_WAIT_BITSET {
                    val3 as u32
                } else {
                    LINUX_FUTEX_BITSET_MATCH_ANY
                },
                timeout_deadline,
                LINUX_ERRNO_ETIMEDOUT,
                0,
            )
        }
        LINUX_FUTEX_WAKE => linux_wake_futex_waiters(state, uaddr, val),
        LINUX_FUTEX_WAKE_OP => {
            let woke_first = linux_wake_futex_waiters(state, uaddr, val).max(0) as u64;
            let cond = match linux_futex_wake_op_eval_and_store(uaddr2, val3 as u32) {
                Ok(v) => v,
                Err(err) => return err,
            };
            let woke_second = if cond {
                linux_wake_futex_waiters(state, uaddr2, timeout_or_val2).max(0) as u64
            } else {
                0
            };
            woke_first.saturating_add(woke_second) as i64
        }
        LINUX_FUTEX_LOCK_PI | LINUX_FUTEX_LOCK_PI2 => linux_futex_pi_lock(state, uaddr, false),
        LINUX_FUTEX_TRYLOCK_PI => linux_futex_pi_lock(state, uaddr, true),
        LINUX_FUTEX_UNLOCK_PI => linux_futex_pi_unlock(state, uaddr),
        LINUX_FUTEX_WAKE_BITSET => {
            if val3 == 0 {
                return linux_neg_errno(22); // EINVAL
            }
            linux_wake_futex_waiters_masked(state, uaddr, val, val3 as u32)
        }
        LINUX_FUTEX_REQUEUE => {
            if uaddr2 == 0 {
                return linux_neg_errno(14); // EFAULT
            }
            if (uaddr2 & 0x3) != 0 {
                return linux_neg_errno(22); // EINVAL
            }
            linux_requeue_futex_waiters(state, uaddr, uaddr2, val, timeout_or_val2)
        }
        LINUX_FUTEX_CMP_REQUEUE => {
            if uaddr2 == 0 {
                return linux_neg_errno(14); // EFAULT
            }
            if (uaddr2 & 0x3) != 0 {
                return linux_neg_errno(22); // EINVAL
            }
            let current = unsafe { ptr::read_volatile(uaddr as *const u32) as u64 };
            if current != (val3 as u32 as u64) {
                return linux_neg_errno(11); // EAGAIN
            }
            linux_requeue_futex_waiters(state, uaddr, uaddr2, val, timeout_or_val2)
        }
        LINUX_FUTEX_WAIT_REQUEUE_PI => {
            if uaddr2 == 0 {
                return linux_neg_errno(14); // EFAULT
            }
            if (uaddr2 & 0x3) != 0 {
                return linux_neg_errno(22); // EINVAL
            }
            if uaddr == uaddr2 {
                return linux_neg_errno(22); // EINVAL
            }
            if val3 != 0 {
                return linux_neg_errno(22); // EINVAL
            }
            let current = unsafe { ptr::read_volatile(uaddr as *const u32) as u64 };
            if current != (val as u32 as u64) {
                return linux_neg_errno(11); // EAGAIN
            }
            let absolute_timeout = (op & LINUX_FUTEX_CLOCK_REALTIME) != 0;
            let timeout_deadline =
                match linux_futex_timeout_deadline_from_ptr(timeout_or_val2, absolute_timeout) {
                Ok(v) => v,
                Err(err) => return err,
            };
            linux_futex_block_current_and_request_switch(
                state,
                uaddr,
                LINUX_FUTEX_BITSET_MATCH_ANY,
                timeout_deadline,
                LINUX_ERRNO_ETIMEDOUT,
                uaddr2,
            )
        }
        LINUX_FUTEX_CMP_REQUEUE_PI => {
            if uaddr2 == 0 {
                return linux_neg_errno(14); // EFAULT
            }
            if (uaddr2 & 0x3) != 0 {
                return linux_neg_errno(22); // EINVAL
            }
            if uaddr == uaddr2 {
                return linux_neg_errno(22); // EINVAL
            }
            if val != 1 {
                return linux_neg_errno(22); // EINVAL (Linux requires nr_wake=1)
            }
            let current = unsafe { ptr::read_volatile(uaddr as *const u32) as u64 };
            if current != (val3 as u32 as u64) {
                return linux_neg_errno(11); // EAGAIN
            }
            linux_requeue_pi_waiters(state, uaddr, uaddr2, val, timeout_or_val2)
        }
        _ => linux_neg_errno(38), // ENOSYS
    }
}

fn linux_sys_futex_waitv(
    state: &mut LinuxShimState,
    waiters_ptr: u64,
    nr_futexes: u64,
    flags: u64,
    timeout: u64,
    _clockid: u64,
) -> i64 {
    if waiters_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let count = nr_futexes as usize;
    if count == 0 || count > LINUX_FUTEX_WAITV_MAX {
        return linux_neg_errno(22); // EINVAL
    }
    let allowed_global = LINUX_FUTEX_PRIVATE_FLAG | LINUX_FUTEX_CLOCK_REALTIME;
    if (flags & !allowed_global) != 0 {
        return linux_neg_errno(22); // EINVAL
    }

    let mut wait_uaddrs = [0u64; LINUX_FUTEX_WAITV_MAX];
    let mut i = 0usize;
    while i < count {
        let item = unsafe { ptr::read((waiters_ptr as *const LinuxFutexWaitV).add(i)) };
        if item._reserved != 0 {
            return linux_neg_errno(22); // EINVAL
        }
        if item.uaddr == 0 {
            return linux_neg_errno(14); // EFAULT
        }
        if (item.uaddr & 0x3) != 0 {
            return linux_neg_errno(22); // EINVAL
        }
        let allowed_entry =
            (LINUX_FUTEX_PRIVATE_FLAG | LINUX_FUTEX_CLOCK_REALTIME) as u32 | LINUX_FUTEX_32;
        if (item.flags & !allowed_entry) != 0 {
            return linux_neg_errno(22); // EINVAL
        }
        if (item.flags & LINUX_FUTEX_32) == 0 {
            return linux_neg_errno(22); // EINVAL
        }
        let current = unsafe { ptr::read_volatile(item.uaddr as *const u32) as u64 };
        if current != (item.val as u32 as u64) {
            return linux_neg_errno(11); // EAGAIN
        }
        wait_uaddrs[i] = item.uaddr;
        i += 1;
    }
    let timeout_deadline = match linux_futex_timeout_deadline_from_ptr(timeout, true) {
        Ok(v) => v,
        Err(err) => return err,
    };
    linux_futex_block_current_waitv_and_request_switch(
        state,
        &wait_uaddrs[..count],
        timeout_deadline,
        LINUX_ERRNO_ETIMEDOUT,
    )
}

fn linux_sys_getpid(state: &LinuxShimState) -> i64 {
    if state.current_pid != 0 {
        state.current_pid as i64
    } else {
        (1000u64.saturating_add(state.session_id) & 0xFFFF_FFFF) as i64
    }
}

fn linux_sys_getpgid(state: &LinuxShimState, pid: u64) -> i64 {
    let target_pid = if pid == 0 {
        state.current_pid
    } else {
        pid as u32
    };
    if target_pid == 0 || linux_find_process_slot_index(state, target_pid).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    target_pid as i64
}

fn linux_sys_getsid(state: &LinuxShimState, pid: u64) -> i64 {
    // Minimal compat: one session rooted at init-like process id 1.
    let target_pid = if pid == 0 {
        state.current_pid
    } else {
        pid as u32
    };
    if target_pid == 0 || linux_find_process_slot_index(state, target_pid).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    1
}

fn linux_sys_setpgid(state: &LinuxShimState, pid: u64, _pgid: u64) -> i64 {
    let target_pid = if pid == 0 {
        state.current_pid
    } else {
        pid as u32
    };
    if target_pid == 0 || linux_find_process_slot_index(state, target_pid).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    0
}

fn linux_sys_getppid(state: &LinuxShimState) -> i64 {
    if let Some(idx) = linux_find_current_process_slot_index(state) {
        let ppid = state.processes[idx].parent_pid;
        if ppid != 0 {
            return ppid as i64;
        }
    }
    1
}

fn linux_sys_gettid(state: &LinuxShimState) -> i64 {
    if state.current_tid != 0 {
        state.current_tid as i64
    } else {
        state.tid_value as i64
    }
}

fn linux_sys_getuid() -> i64 {
    0
}

fn linux_sys_getgid() -> i64 {
    0
}

fn linux_sys_setuid(_uid: u64) -> i64 {
    0
}

fn linux_sys_setgid(_gid: u64) -> i64 {
    0
}

fn linux_sys_setresuid(_ruid: u64, _euid: u64, _suid: u64) -> i64 {
    0
}

fn linux_sys_setresgid(_rgid: u64, _egid: u64, _sgid: u64) -> i64 {
    0
}

fn linux_sys_getresuid(ruid: u64, euid: u64, suid: u64) -> i64 {
    unsafe {
        if ruid != 0 {
            ptr::write(ruid as *mut u32, 0);
        }
        if euid != 0 {
            ptr::write(euid as *mut u32, 0);
        }
        if suid != 0 {
            ptr::write(suid as *mut u32, 0);
        }
    }
    0
}

fn linux_sys_getresgid(rgid: u64, egid: u64, sgid: u64) -> i64 {
    unsafe {
        if rgid != 0 {
            ptr::write(rgid as *mut u32, 0);
        }
        if egid != 0 {
            ptr::write(egid as *mut u32, 0);
        }
        if sgid != 0 {
            ptr::write(sgid as *mut u32, 0);
        }
    }
    0
}

fn linux_sys_arch_prctl(state: &mut LinuxShimState, code: u64, addr: u64) -> i64 {
    match code {
        LINUX_ARCH_SET_FS => {
            state.fs_base = addr;
            linux_sync_current_thread_to_slot(state);
            0
        }
        LINUX_ARCH_GET_FS => {
            if addr == 0 {
                return linux_neg_errno(14); // EFAULT
            }
            unsafe {
                let out = addr as *mut u64;
                ptr::write(out, state.fs_base);
            }
            0
        }
        _ => linux_neg_errno(22), // EINVAL
    }
}

fn linux_sys_set_tid_address(state: &mut LinuxShimState, addr: u64) -> i64 {
    state.tid_addr = addr;
    linux_sync_current_thread_to_slot(state);
    if addr != 0 {
        unsafe {
            let out = addr as *mut u32;
            ptr::write(out, state.current_tid.max(state.tid_value));
        }
    }
    state.current_tid.max(state.tid_value) as i64
}

fn linux_sys_clone_spawn(
    state: &mut LinuxShimState,
    flags: u64,
    child_stack: u64,
    parent_tid_ptr: u64,
    child_tid_ptr: u64,
    tls: u64,
    requested_tid: Option<u32>,
    require_clone_vm: bool,
) -> i64 {
    let shares_vm = (flags & LINUX_CLONE_VM) != 0;
    if require_clone_vm && !shares_vm {
        return linux_neg_errno(38); // ENOSYS
    }

    let parent_tid = state.current_tid;
    let parent_pid = state.current_pid;
    let exit_signal = (flags & LINUX_CLONE_SIGNAL_MASK) as u8;

    let mut child_pid = parent_pid;
    if !shares_vm {
        let Some(new_pid) = linux_allocate_process_pid(state) else {
            return linux_neg_errno(11); // EAGAIN
        };
        let Some(parent_proc_idx) = linux_find_process_slot_index(state, parent_pid) else {
            return linux_neg_errno(38); // ENOSYS: process model not initialized.
        };
        let parent_proc = state.processes[parent_proc_idx];
        if linux_add_process_slot(
            state,
            new_pid,
            parent_pid,
            new_pid,
            parent_proc.brk_base,
            parent_proc.brk_current,
            parent_proc.brk_limit,
            parent_proc.mmap_cursor,
            0,
        )
        .is_none()
        {
            return linux_neg_errno(11); // EAGAIN
        }
        child_pid = new_pid;
    }

    let mut child_tid = if let Some(req) = requested_tid {
        if req == 0 {
            return linux_neg_errno(22);
        }
        if linux_find_thread_slot_index(state, req).is_some() {
            return linux_neg_errno(17); // EEXIST
        }
        req
    } else if shares_vm {
        state.next_tid.saturating_add(1).max(2001)
    } else {
        child_pid.max(2001)
    };
    if requested_tid.is_none() {
        while linux_find_thread_slot_index(state, child_tid).is_some() {
            child_tid = child_tid.saturating_add(1);
            if child_tid == 0 {
                return linux_neg_errno(11); // EAGAIN
            }
        }
    }

    let child_fs = if (flags & LINUX_CLONE_SETTLS) != 0 && tls != 0 {
        tls
    } else {
        state.fs_base
    };
    let child_clear_tid = if (flags & LINUX_CLONE_CHILD_CLEARTID) != 0 {
        child_tid_ptr
    } else {
        0
    };
    let child_slot_idx = if let Some(idx) = linux_add_thread_slot(
        state,
        child_tid,
        child_pid,
        parent_tid,
        exit_signal,
        child_fs,
        child_clear_tid,
        flags,
    ) {
        idx
    } else {
        if !shares_vm {
            linux_remove_process_slot(state, child_pid);
        }
        return linux_neg_errno(11); // EAGAIN
    };
    state.next_tid = child_tid;
    if !shares_vm {
        if let Some(proc_idx) = linux_find_process_slot_index(state, child_pid) {
            state.processes[proc_idx].leader_tid = child_tid;
        }
    }
    if !shares_vm {
        state.next_pid = state.next_pid.max(child_pid);
    }

    if (flags & LINUX_CLONE_PARENT_SETTID) != 0 && parent_tid_ptr != 0 {
        unsafe {
            ptr::write(parent_tid_ptr as *mut u32, child_tid);
        }
    }
    if (flags & LINUX_CLONE_CHILD_SETTID) != 0 && child_tid_ptr != 0 {
        unsafe {
            ptr::write(child_tid_ptr as *mut u32, child_tid);
        }
    }

    if let Some(mut child_ctx) = linux_thread_context_from_privilege() {
        child_ctx.rax = 0;
        if child_stack != 0 {
            child_ctx.rsp = child_stack;
        }
        state.thread_contexts[child_slot_idx] = child_ctx;
    }
    child_tid as i64
}

fn linux_sys_clone(
    state: &mut LinuxShimState,
    flags: u64,
    child_stack: u64,
    parent_tid_ptr: u64,
    child_tid_ptr: u64,
    tls: u64,
) -> i64 {
    linux_sys_clone_spawn(
        state,
        flags,
        child_stack,
        parent_tid_ptr,
        child_tid_ptr,
        tls,
        None,
        false,
    )
}

fn linux_sys_fork(state: &mut LinuxShimState) -> i64 {
    linux_sys_clone_spawn(state, LINUX_SIGCHLD, 0, 0, 0, 0, None, false)
}

fn linux_sys_vfork(state: &mut LinuxShimState) -> i64 {
    linux_sys_clone_spawn(
        state,
        LINUX_SIGCHLD | LINUX_CLONE_VFORK,
        0,
        0,
        0,
        0,
        None,
        false,
    )
}

fn linux_create_pidfd(state: &mut LinuxShimState, target_pid: u32) -> Option<i32> {
    let fd = state.next_fd;
    state.next_fd = state.next_fd.saturating_add(1);
    let Some(open_idx) = linux_allocate_open_slot_for_fd(state, fd) else {
        return None;
    };
    state.open_files[open_idx] = LinuxOpenFileSlot {
        active: true,
        fd,
        kind: LINUX_OPEN_KIND_PIDFD,
        _pad_kind: [0; 3],
        object_index: target_pid as usize,
        cursor: 0,
        flags: 0,
        aux: 0,
    };
    state.open_file_count = state.open_file_count.saturating_add(1);
    Some(fd)
}

fn linux_sys_pidfd_open(state: &mut LinuxShimState, pid: u64, _flags: u64) -> i64 {
    let target = pid as u32;
    if target == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if linux_find_process_slot_index(state, target).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    match linux_create_pidfd(state, target) {
        Some(fd) => fd as i64,
        None => linux_neg_errno(24), // EMFILE
    }
}

fn linux_sys_pidfd_send_signal(
    state: &mut LinuxShimState,
    pidfd: u64,
    sig: u64,
    _info: u64,
    _flags: u64,
) -> i64 {
    let fd_i = pidfd as i64;
    if fd_i < 0 {
        return linux_neg_errno(9); // EBADF
    }
    let Some(open_idx) = linux_find_open_slot_index(state, fd_i as i32) else {
        return linux_neg_errno(9);
    };
    let slot = state.open_files[open_idx];
    if slot.kind != LINUX_OPEN_KIND_PIDFD {
        return linux_neg_errno(9); // EBADF
    }

    let target_pid = slot.object_index as u32;
    if target_pid == 0 || linux_find_process_slot_index(state, target_pid).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    if sig == 0 {
        return 0;
    }
    linux_queue_signal_for_process_pid(state, target_pid, sig)
}

fn linux_runtime_slot_view(slot: LinuxRuntimeFileSlot) -> Option<(*const u8, usize)> {
    if !slot.active || slot.data_ptr == 0 || slot.data_len == 0 {
        return None;
    }
    let len_u64 = slot.size.min(slot.data_len);
    if len_u64 == 0 || len_u64 > usize::MAX as u64 {
        return None;
    }
    Some((slot.data_ptr as *const u8, len_u64 as usize))
}

fn linux_path_buf_to_string(path: &[u8; LINUX_PATH_MAX], path_len: usize) -> String {
    let capped = path_len.min(LINUX_PATH_MAX);
    String::from_utf8_lossy(&path[..capped]).into_owned()
}

fn linux_read_execve_item_vector(ptr_raw: u64, max_items: usize) -> Result<Vec<String>, i64> {
    let mut items: Vec<String> = Vec::new();
    if ptr_raw == 0 {
        return Ok(items);
    }
    let mut scratch = [0u8; LINUX_EXECVE_MAX_ITEM_LEN];
    let mut i = 0usize;
    while i < max_items {
        let item_ptr = unsafe { ptr::read((ptr_raw as *const u64).add(i)) };
        if item_ptr == 0 {
            break;
        }
        let len = linux_read_raw_c_string(item_ptr, &mut scratch)?;
        items.push(String::from_utf8_lossy(&scratch[..len]).into_owned());
        i += 1;
    }
    if i == max_items {
        return Err(linux_neg_errno(7)); // E2BIG
    }
    Ok(items)
}

fn linux_close_cloexec_fds(state: &mut LinuxShimState) {
    let mut i = 0usize;
    while i < LINUX_MAX_OPEN_FILES {
        if state.open_files[i].active && (state.open_files[i].flags & LINUX_DUP3_CLOEXEC) != 0 {
            linux_close_open_slot(state, i);
        }
        i += 1;
    }
}

fn linux_execve_reset_process_image(state: &mut LinuxShimState, tls_tcb_addr: u64) {
    let current_pid = if state.current_pid != 0 {
        state.current_pid
    } else {
        state.tid_value.max(1)
    };
    let current_tid = if state.current_tid != 0 {
        state.current_tid
    } else {
        state.tid_value.max(1)
    };
    let parent_pid = linux_find_process_slot_index(state, current_pid)
        .map(|idx| state.processes[idx].parent_pid)
        .unwrap_or(1)
        .max(1);
    let mut kept_thread = linux_find_thread_slot_index(state, current_tid)
        .map(|idx| state.threads[idx])
        .unwrap_or_else(LinuxThreadSlot::empty);

    linux_release_process_mmaps(state, current_pid);

    let brk_base = LINUX_MMAP_BASE.saturating_sub(LINUX_BRK_REGION_BYTES);
    let brk_base_aligned = linux_align_up(brk_base, LINUX_PAGE_SIZE).unwrap_or(brk_base);
    let brk_limit = brk_base_aligned.saturating_add(LINUX_BRK_REGION_BYTES);

    state.brk_base = brk_base_aligned;
    state.brk_current = brk_base_aligned;
    state.brk_limit = brk_limit;
    state.mmap_cursor = LINUX_MMAP_BASE;
    state.mmap_count = 0;

    state.processes = [LinuxProcessSlot::empty(); LINUX_MAX_PROCESSES];
    state.processes[0] = LinuxProcessSlot {
        active: true,
        pid: current_pid,
        parent_pid,
        leader_tid: current_tid,
        brk_base: brk_base_aligned,
        brk_current: brk_base_aligned,
        brk_limit,
        mmap_cursor: LINUX_MMAP_BASE,
        mmap_count: 0,
    };
    state.process_count = 1;
    state.current_pid = current_pid;

    kept_thread.active = true;
    kept_thread.tid = current_tid;
    kept_thread.process_pid = current_pid;
    kept_thread.parent_tid = 0;
    kept_thread.exit_signal = 0;
    kept_thread.state = LINUX_THREAD_RUNNABLE;
    kept_thread.fs_base = tls_tcb_addr;
    kept_thread.tid_addr = 0;
    kept_thread.robust_list_head = 0;
    kept_thread.robust_list_len = 0;
    linux_clear_futex_wait_state(&mut kept_thread);
    kept_thread.clone_flags = 0;
    kept_thread.pending_signals = 0;
    state.threads = [LinuxThreadSlot::empty(); LINUX_MAX_THREADS];
    state.thread_contexts = [LinuxThreadContext::empty(); LINUX_MAX_THREADS];
    state.threads[0] = kept_thread;
    state.thread_count = 1;
    state.current_tid = current_tid;
    state.tid_value = current_tid;
    state.next_tid = state.next_tid.max(current_tid);
    state.next_pid = state.next_pid.max(current_pid);

    state.tid_addr = 0;
    state.fs_base = tls_tcb_addr;
    state.robust_list_head = 0;
    state.robust_list_len = 0;
    state.pending_signals = 0;
    linux_sync_current_thread_to_slot(state);
    linux_sync_current_process_to_slot(state);
}

fn linux_sys_execve(state: &mut LinuxShimState, filename: u64, argv: u64, envp: u64) -> i64 {
    let mut path = [0u8; LINUX_PATH_MAX];
    let path_len = match linux_read_c_string(filename, &mut path) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let execfn = linux_path_buf_to_string(&path, path_len);

    let Some(main_idx) = linux_find_runtime_index(state, &path, path_len) else {
        let result = linux_neg_errno(2); // ENOENT
        linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, false);
        return result;
    };
    let main_slot = state.runtime_files[main_idx];
    let Some((main_ptr, main_len)) = linux_runtime_slot_view(main_slot) else {
        let result = linux_neg_errno(8); // ENOEXEC
        linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
        return result;
    };
    let main_raw = unsafe { core::slice::from_raw_parts(main_ptr, main_len) };

    let dynamic = match crate::linux_compat::inspect_dynamic_elf64(main_raw) {
        Ok(v) => v,
        Err(_) => {
            let result = linux_neg_errno(8); // ENOEXEC
            linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
            return result;
        }
    };

    let interp_path = dynamic
        .interp_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let Some(interp_path) = interp_path else {
        let result = linux_neg_errno(8); // ENOEXEC
        linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
        return result;
    };

    let mut interp_norm = [0u8; LINUX_PATH_MAX];
    let interp_norm_len = linux_normalize_path_str(&mut interp_norm, interp_path.as_str());
    if interp_norm_len == 0 {
        let result = linux_neg_errno(8); // ENOEXEC
        linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
        return result;
    }
    let Some(interp_idx) = linux_find_runtime_index(state, &interp_norm, interp_norm_len) else {
        let result = linux_neg_errno(2); // ENOENT
        linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
        return result;
    };
    let interp_slot = state.runtime_files[interp_idx];
    let Some((interp_ptr, interp_len)) = linux_runtime_slot_view(interp_slot) else {
        let result = linux_neg_errno(8); // ENOEXEC
        linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
        return result;
    };
    let interp_raw = unsafe { core::slice::from_raw_parts(interp_ptr, interp_len) };

    let mut dep_records: Vec<(String, usize)> = Vec::new();
    for dep in dynamic.needed.iter() {
        let name = dep.trim();
        if name.is_empty() {
            continue;
        }
        let mut dep_norm = [0u8; LINUX_PATH_MAX];
        let dep_norm_len = linux_normalize_path_str(&mut dep_norm, name);
        if dep_norm_len == 0 {
            continue;
        }
        let Some(dep_idx) = linux_find_runtime_index(state, &dep_norm, dep_norm_len) else {
            let result = linux_neg_errno(2); // ENOENT
            linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
            return result;
        };
        dep_records.push((String::from(name), dep_idx));
    }

    let mut dep_inputs: Vec<crate::linux_compat::LinuxDynDependencyInput<'_>> = Vec::new();
    for (name, dep_idx) in dep_records.iter() {
        let dep_slot = state.runtime_files[*dep_idx];
        let Some((dep_ptr, dep_len)) = linux_runtime_slot_view(dep_slot) else {
            let result = linux_neg_errno(8); // ENOEXEC
            linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
            return result;
        };
        let dep_raw = unsafe { core::slice::from_raw_parts(dep_ptr, dep_len) };
        dep_inputs.push(crate::linux_compat::LinuxDynDependencyInput {
            soname: name.as_str(),
            raw: dep_raw,
        });
    }

    let mut argv_items = match linux_read_execve_item_vector(argv, LINUX_EXECVE_MAX_ARG_ITEMS) {
        Ok(v) => v,
        Err(err) => {
            linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, err, true);
            return err;
        }
    };
    if argv_items.is_empty() {
        argv_items.push(execfn.clone());
    }
    let env_items = match linux_read_execve_item_vector(envp, LINUX_EXECVE_MAX_ENV_ITEMS) {
        Ok(v) => v,
        Err(err) => {
            linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, err, true);
            return err;
        }
    };

    let mut argv_refs: Vec<&str> = Vec::new();
    for item in argv_items.iter() {
        let trimmed = item.trim();
        if !trimmed.is_empty() {
            argv_refs.push(trimmed);
        }
    }
    if argv_refs.is_empty() {
        argv_refs.push(execfn.as_str());
    }
    let mut env_refs: Vec<&str> = Vec::new();
    for item in env_items.iter() {
        let trimmed = item.trim();
        if !trimmed.is_empty() {
            env_refs.push(trimmed);
        }
    }

    let plan = match crate::linux_compat::prepare_phase2_interp_launch_with_deps_and_argv(
        main_raw,
        interp_raw,
        dep_inputs.as_slice(),
        argv_refs.as_slice(),
        execfn.as_str(),
        env_refs.as_slice(),
    ) {
        Ok(v) => v,
        Err(_) => {
            let result = linux_neg_errno(8); // ENOEXEC
            linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, result, true);
            return result;
        }
    };

    let main_entry = plan.main_entry;
    let interp_entry = plan.interp_entry;
    let stack_ptr = plan.stack_ptr;
    let tls_tcb_addr = plan.tls_tcb_addr;
    unsafe {
        linux_shim_store_active_plan(plan);
    }

    linux_close_cloexec_fds(state);
    state.main_entry = main_entry;
    state.interp_entry = interp_entry;
    state.stack_ptr = stack_ptr;
    linux_execve_reset_process_image(state, tls_tcb_addr);
    state.watchdog_triggered = false;
    state.exit_code = 0;
    state.exec_transition_pending = true;
    state.start_tick = timer::ticks();
    linux_record_last_path_lookup(state, LINUX_SYS_EXECVE, &path, path_len, 0, true);

    // Return to kernel/GUI at syscall boundary and restart from the new image next slice.
    privilege::linux_real_slice_request_yield();
    0
}

fn linux_sys_execveat(
    state: &mut LinuxShimState,
    dirfd: u64,
    pathname: u64,
    argv: u64,
    envp: u64,
    flags: u64,
) -> i64 {
    if pathname == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if (flags & !LINUX_AT_EMPTY_PATH) != 0 {
        return linux_neg_errno(22); // EINVAL
    }

    let mut input = [0u8; LINUX_PATH_MAX];
    let input_len = match linux_read_c_string(pathname, &mut input) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let mut resolved = [0u8; LINUX_PATH_MAX];
    let resolved_len = if input_len == 0 {
        if (flags & LINUX_AT_EMPTY_PATH) == 0 {
            return linux_neg_errno(2); // ENOENT
        }
        let dirfd_i = dirfd as i64;
        if dirfd_i < 0 {
            return linux_neg_errno(9); // EBADF
        }
        let Some(open_idx) = linux_find_open_slot_index(state, dirfd_i as i32) else {
            return linux_neg_errno(9); // EBADF
        };
        let open = state.open_files[open_idx];
        if open.kind != LINUX_OPEN_KIND_RUNTIME || open.object_index >= LINUX_MAX_RUNTIME_FILES {
            return linux_neg_errno(2); // ENOENT
        }
        let slot = state.runtime_files[open.object_index];
        if !slot.active || slot.path_len == 0 {
            return linux_neg_errno(2);
        }
        let len = (slot.path_len as usize).min(LINUX_PATH_MAX);
        let mut i = 0usize;
        while i < len {
            resolved[i] = slot.path[i];
            i += 1;
        }
        len
    } else {
        match linux_resolve_open_path(state, dirfd as i64, &input, input_len, &mut resolved) {
            Ok(v) => v,
            Err(err) => return err,
        }
    };

    if resolved_len == 0 || resolved_len >= LINUX_PATH_MAX {
        return linux_neg_errno(2); // ENOENT
    }
    let mut resolved_c = [0u8; LINUX_PATH_MAX + 1];
    let mut i = 0usize;
    while i < resolved_len {
        resolved_c[i] = resolved[i];
        i += 1;
    }
    resolved_c[resolved_len] = 0;

    linux_sys_execve(state, resolved_c.as_ptr() as u64, argv, envp)
}

fn linux_clone3_validate_args(args: &LinuxCloneArgs) -> Result<(u64, u64), i64> {
    let known_flags = LINUX_CLONE_VM
        | LINUX_CLONE_FS
        | LINUX_CLONE_FILES
        | LINUX_CLONE_SIGHAND
        | LINUX_CLONE_PIDFD
        | LINUX_CLONE_PTRACE
        | LINUX_CLONE_VFORK
        | LINUX_CLONE_PARENT
        | LINUX_CLONE_THREAD
        | LINUX_CLONE_NEWNS
        | LINUX_CLONE_SYSVSEM
        | LINUX_CLONE_SETTLS
        | LINUX_CLONE_PARENT_SETTID
        | LINUX_CLONE_CHILD_CLEARTID
        | LINUX_CLONE_DETACHED
        | LINUX_CLONE_UNTRACED
        | LINUX_CLONE_CHILD_SETTID
        | LINUX_CLONE_NEWCGROUP
        | LINUX_CLONE_NEWUTS
        | LINUX_CLONE_NEWIPC
        | LINUX_CLONE_NEWUSER
        | LINUX_CLONE_NEWPID
        | LINUX_CLONE_NEWNET
        | LINUX_CLONE_IO
        | LINUX_CLONE_CLEAR_SIGHAND
        | LINUX_CLONE_INTO_CGROUP
        | LINUX_CLONE_SIGNAL_MASK;
    if (args.flags & !known_flags) != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }

    let unsupported_ns_flags = LINUX_CLONE_NEWNS
        | LINUX_CLONE_NEWCGROUP
        | LINUX_CLONE_NEWUTS
        | LINUX_CLONE_NEWIPC
        | LINUX_CLONE_NEWUSER
        | LINUX_CLONE_NEWPID
        | LINUX_CLONE_NEWNET;
    if (args.flags & unsupported_ns_flags) != 0 {
        return Err(linux_neg_errno(38)); // ENOSYS
    }
    if (args.flags & LINUX_CLONE_DETACHED) != 0 {
        return Err(linux_neg_errno(22)); // EINVAL (deprecated/unsupported with clone3)
    }

    if args.stack == 0 && args.stack_size != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    if (args.exit_signal & !LINUX_CLONE_SIGNAL_MASK) != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }

    let flags_sig = args.flags & LINUX_CLONE_SIGNAL_MASK;
    let exit_sig = if args.exit_signal != 0 {
        args.exit_signal & LINUX_CLONE_SIGNAL_MASK
    } else {
        flags_sig
    };
    if flags_sig != 0 && args.exit_signal != 0 && flags_sig != exit_sig {
        return Err(linux_neg_errno(22)); // EINVAL
    }

    if (args.flags & LINUX_CLONE_SIGHAND) != 0 && (args.flags & LINUX_CLONE_VM) == 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    if (args.flags & LINUX_CLONE_THREAD) != 0 {
        if (args.flags & LINUX_CLONE_VM) == 0 || (args.flags & LINUX_CLONE_SIGHAND) == 0 {
            return Err(linux_neg_errno(22)); // EINVAL
        }
        if exit_sig != 0 {
            return Err(linux_neg_errno(22)); // EINVAL
        }
    }
    if (args.flags & LINUX_CLONE_CLEAR_SIGHAND) != 0 && (args.flags & LINUX_CLONE_SIGHAND) != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    if (args.flags & LINUX_CLONE_PIDFD) != 0 && (args.flags & LINUX_CLONE_THREAD) != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    if args.set_tid_size == 0 && args.set_tid != 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }
    if args.set_tid_size != 0 && args.set_tid == 0 {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    if args.set_tid_size > LINUX_MAX_THREADS as u64 {
        return Err(linux_neg_errno(22)); // EINVAL
    }

    if (args.flags & LINUX_CLONE_PARENT_SETTID) != 0 && args.parent_tid == 0 {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    if ((args.flags & LINUX_CLONE_CHILD_SETTID) != 0 || (args.flags & LINUX_CLONE_CHILD_CLEARTID) != 0)
        && args.child_tid == 0
    {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    if (args.flags & LINUX_CLONE_PIDFD) != 0 && args.pidfd == 0 {
        return Err(linux_neg_errno(14)); // EFAULT
    }
    if (args.flags & LINUX_CLONE_INTO_CGROUP) != 0 && args.cgroup == 0 {
        return Err(linux_neg_errno(22)); // EINVAL
    }

    let effective_flags = (args.flags
        & !(LINUX_CLONE_SIGNAL_MASK
            | LINUX_CLONE_PIDFD
            | LINUX_CLONE_INTO_CGROUP
            | LINUX_CLONE_CLEAR_SIGHAND))
        | exit_sig;
    Ok((effective_flags, exit_sig))
}

fn linux_sys_clone3(state: &mut LinuxShimState, clone_args_ptr: u64, size: u64) -> i64 {
    if clone_args_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }

    if size > core::mem::size_of::<LinuxCloneArgs>() as u64 {
        return linux_neg_errno(7); // E2BIG
    }
    // Need fields through `tls` for basic thread creation.
    if size < 64 {
        return linux_neg_errno(22); // EINVAL
    }

    let copy_len = (size as usize).min(core::mem::size_of::<LinuxCloneArgs>());
    let mut args = LinuxCloneArgs::empty();
    unsafe {
        ptr::copy_nonoverlapping(
            clone_args_ptr as *const u8,
            (&mut args as *mut LinuxCloneArgs) as *mut u8,
            copy_len,
        );
    }

    let (effective_flags, _effective_exit_signal) = match linux_clone3_validate_args(&args) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let wants_pidfd = (args.flags & LINUX_CLONE_PIDFD) != 0;

    // --- set_tid compat ---
    // Support multi-entry set_tid arrays when they resolve to the same requested tid.
    // This keeps clone3 callers working while retaining a single namespace model.
    let requested_tid = if args.set_tid_size == 0 {
        None
    } else {
        let max_scan = (args.set_tid_size as usize).min(LINUX_MAX_THREADS);
        let mut desired = 0u32;
        let mut i = 0usize;
        while i < max_scan {
            let candidate = unsafe { ptr::read((args.set_tid as *const u32).add(i)) };
            if candidate != 0 {
                if desired == 0 {
                    desired = candidate;
                } else if desired != candidate {
                    return linux_neg_errno(22); // EINVAL: ambiguous multi-namespace request
                }
            }
            i += 1;
        }
        if desired == 0 {
            return linux_neg_errno(22);
        }
        Some(desired)
    };

    let child_tid = linux_sys_clone_spawn(
        state,
        effective_flags,
        args.stack,
        args.parent_tid,
        args.child_tid,
        args.tls,
        requested_tid,
        false,
    );

    // Create pidfd for the parent if requested and spawn succeeded.
    if child_tid > 0 && wants_pidfd {
        let target_pid = linux_find_thread_slot_index(state, child_tid as u32)
            .map(|idx| state.threads[idx].process_pid)
            .unwrap_or(child_tid as u32);
        if let Some(pidfd) = linux_create_pidfd(state, target_pid) {
            unsafe { ptr::write(args.pidfd as *mut i32, pidfd); }
        } else {
            // Could not allocate fd – write -1 so caller knows.
            unsafe { ptr::write(args.pidfd as *mut i32, -1); }
        }
    }

    child_tid
}

fn linux_any_active_child_exists(state: &LinuxShimState, parent_pid: u32, pid_filter: i64) -> bool {
    if parent_pid == 0 {
        return false;
    }
    let mut i = 0usize;
    while i < LINUX_MAX_PROCESSES {
        let slot = &state.processes[i];
        if !slot.active || slot.parent_pid != parent_pid || slot.pid == parent_pid {
            i += 1;
            continue;
        }
        if pid_filter == -1 || pid_filter == 0 || slot.pid == pid_filter as u32 {
            return true;
        }
        i += 1;
    }
    false
}

fn linux_push_exited_thread(
    state: &mut LinuxShimState,
    parent_pid: u32,
    child_pid: u32,
    exit_code: i32,
    event_kind: u8,
) {
    if child_pid == 0 {
        return;
    }
    if state.exited_count >= LINUX_EXITED_QUEUE_CAP {
        let mut i = 1usize;
        while i < LINUX_EXITED_QUEUE_CAP {
            state.exited_tids[i - 1] = state.exited_tids[i];
            state.exited_parent_tids[i - 1] = state.exited_parent_tids[i];
            state.exited_status[i - 1] = state.exited_status[i];
            state.exited_kinds[i - 1] = state.exited_kinds[i];
            i += 1;
        }
        state.exited_count = LINUX_EXITED_QUEUE_CAP - 1;
    }
    let idx = state.exited_count;
    state.exited_tids[idx] = child_pid;
    state.exited_parent_tids[idx] = parent_pid;
    state.exited_status[idx] = exit_code;
    state.exited_kinds[idx] = event_kind;
    state.exited_count = state.exited_count.saturating_add(1);
}

fn linux_find_exited_index(state: &LinuxShimState, parent_pid: u32, pid_filter: i64) -> Option<usize> {
    let mut i = 0usize;
    while i < state.exited_count {
        if state.exited_parent_tids[i] != parent_pid || state.exited_kinds[i] != LINUX_CHILD_EVENT_EXITED {
            i += 1;
            continue;
        }
        let child_pid = state.exited_tids[i];
        if pid_filter == -1 || pid_filter == 0 || child_pid == pid_filter as u32 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_find_waitid_event_index(
    state: &LinuxShimState,
    parent_pid: u32,
    pid_filter: i64,
    options: u64,
) -> Option<usize> {
    let mut i = 0usize;
    while i < state.exited_count {
        if state.exited_parent_tids[i] != parent_pid {
            i += 1;
            continue;
        }
        let child_pid = state.exited_tids[i];
        if pid_filter != -1 && pid_filter != 0 && child_pid != pid_filter as u32 {
            i += 1;
            continue;
        }
        let kind = state.exited_kinds[i];
        let wanted = (kind == LINUX_CHILD_EVENT_EXITED && (options & LINUX_WEXITED) != 0)
            || (kind == LINUX_CHILD_EVENT_STOPPED && (options & LINUX_WSTOPPED) != 0)
            || (kind == LINUX_CHILD_EVENT_CONTINUED && (options & LINUX_WCONTINUED) != 0);
        if wanted {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn linux_take_exited_at(state: &mut LinuxShimState, idx: usize) -> Option<(u32, i32, u8)> {
    if idx >= state.exited_count {
        return None;
    }
    let pid = state.exited_tids[idx];
    let code = state.exited_status[idx];
    let kind = state.exited_kinds[idx];
    let mut i = idx + 1;
    while i < state.exited_count {
        state.exited_tids[i - 1] = state.exited_tids[i];
        state.exited_parent_tids[i - 1] = state.exited_parent_tids[i];
        state.exited_status[i - 1] = state.exited_status[i];
        state.exited_kinds[i - 1] = state.exited_kinds[i];
        i += 1;
    }
    state.exited_count -= 1;
    state.exited_tids[state.exited_count] = 0;
    state.exited_parent_tids[state.exited_count] = 0;
    state.exited_status[state.exited_count] = 0;
    state.exited_kinds[state.exited_count] = 0;
    Some((pid, code, kind))
}

fn linux_wait_status_from_exit_code(exit_code: i32) -> i32 {
    (exit_code & 0xff) << 8
}

fn linux_waitid_filter_from_id(idtype: u64, id: u64) -> Option<i64> {
    match idtype {
        LINUX_P_ALL => Some(-1),
        LINUX_P_PID => Some(id as i64),
        LINUX_P_PGID => Some(0),
        _ => None,
    }
}

fn linux_waitid_write_siginfo(infop: u64, pid: u32, si_code: i32, status: i32) {
    unsafe {
        ptr::write_bytes(infop as *mut u8, 0, 128);
        ptr::write(infop as *mut i32, LINUX_SIGCHLD as i32); // si_signo
        ptr::write(infop.saturating_add(4) as *mut i32, 0); // si_errno
        ptr::write(infop.saturating_add(8) as *mut i32, si_code); // si_code
        ptr::write(infop.saturating_add(16) as *mut i32, pid as i32); // si_pid
        ptr::write(infop.saturating_add(24) as *mut i32, status); // si_status
    }
}

fn linux_waitid_write_empty_siginfo(infop: u64) {
    unsafe {
        ptr::write_bytes(infop as *mut u8, 0, 128);
    }
}

fn linux_sys_wait4(
    state: &mut LinuxShimState,
    pid: u64,
    wstatus_ptr: u64,
    options: u64,
    _rusage_ptr: u64,
) -> i64 {
    let pid_i = pid as i64;
    let parent_pid = state.current_pid;
    let nohang = (options & LINUX_WNOHANG) != 0;

    if let Some(idx) = linux_find_exited_index(state, parent_pid, pid_i) {
        if let Some((child_pid, status, _kind)) = linux_take_exited_at(state, idx) {
            if wstatus_ptr != 0 {
                unsafe {
                    ptr::write(wstatus_ptr as *mut i32, linux_wait_status_from_exit_code(status));
                }
            }
            return child_pid as i64;
        }
    }

    if linux_any_active_child_exists(state, parent_pid, pid_i) {
        if nohang {
            return 0;
        }
        return linux_neg_errno(11); // EAGAIN (cooperative non-blocking shim)
    }

    linux_neg_errno(10) // ECHILD
}

fn linux_sys_waitid(
    state: &mut LinuxShimState,
    idtype: u64,
    id: u64,
    infop: u64,
    options: u64,
    _rusage_ptr: u64,
) -> i64 {
    if infop == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let wants_events = options & (LINUX_WEXITED | LINUX_WSTOPPED | LINUX_WCONTINUED);
    if wants_events == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(pid_filter) = linux_waitid_filter_from_id(idtype, id) else {
        return linux_neg_errno(22); // EINVAL
    };
    let parent_pid = state.current_pid;
    let nohang = (options & LINUX_WNOHANG) != 0;
    let nowait = (options & LINUX_WNOWAIT) != 0;

    if let Some(idx) = linux_find_waitid_event_index(state, parent_pid, pid_filter, options) {
        let (child_pid, status, kind) = if nowait {
            (
                state.exited_tids[idx],
                state.exited_status[idx],
                state.exited_kinds[idx],
            )
        } else if let Some(tuple) = linux_take_exited_at(state, idx) {
            tuple
        } else {
            (0, 0, 0)
        };
        if child_pid != 0 {
            let (si_code, si_status) = match kind {
                LINUX_CHILD_EVENT_EXITED => (LINUX_CLD_EXITED, status & 0xff),
                LINUX_CHILD_EVENT_STOPPED => (LINUX_CLD_STOPPED, status),
                LINUX_CHILD_EVENT_CONTINUED => (LINUX_CLD_CONTINUED, status),
                _ => (LINUX_CLD_EXITED, status & 0xff),
            };
            linux_waitid_write_siginfo(infop, child_pid, si_code, si_status);
            return 0;
        }
    }

    if nohang {
        linux_waitid_write_empty_siginfo(infop);
        return 0;
    }
    if linux_any_active_child_exists(state, parent_pid, pid_filter) {
        return linux_neg_errno(11); // EAGAIN (cooperative non-blocking shim)
    }
    linux_neg_errno(10) // ECHILD
}

fn linux_sys_set_robust_list(state: &mut LinuxShimState, head: u64, len: u64) -> i64 {
    state.robust_list_head = head;
    state.robust_list_len = len;
    linux_sync_current_thread_to_slot(state);
    0
}

fn linux_sys_get_robust_list(state: &LinuxShimState, pid: u64, head_ptr: u64, len_ptr: u64) -> i64 {
    if head_ptr == 0 || len_ptr == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let target_tid = if pid == 0 {
        state.current_tid
    } else {
        pid as u32
    };
    let Some(idx) = linux_find_thread_slot_index(state, target_tid) else {
        return linux_neg_errno(3); // ESRCH
    };
    let slot = &state.threads[idx];
    if !slot.active {
        return linux_neg_errno(3); // ESRCH
    }
    unsafe {
        ptr::write(head_ptr as *mut u64, slot.robust_list_head);
        ptr::write(len_ptr as *mut u64, slot.robust_list_len);
    }
    0
}

fn linux_sys_tgkill(state: &mut LinuxShimState, tgid: u64, tid: u64, sig: u64) -> i64 {
    if tgid != 0 && linux_find_process_slot_index(state, tgid as u32).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    if tid != 0 {
        let Some(tidx) = linux_find_thread_slot_index(state, tid as u32) else {
            return linux_neg_errno(3); // ESRCH
        };
        if tgid != 0 && state.threads[tidx].process_pid != tgid as u32 {
            return linux_neg_errno(3); // ESRCH
        }
    }
    if sig == 0 {
        return 0;
    }
    let target_tid = if tid == 0 {
        if tgid != 0 {
            linux_find_any_thread_tid_for_process(state, tgid as u32).unwrap_or(0)
        } else {
            state.current_tid
        }
    } else {
        tid as u32
    };
    if target_tid == 0 {
        return linux_neg_errno(3); // ESRCH
    }
    linux_queue_signal_for_tid(state, target_tid, sig)
}

fn linux_sys_kill(state: &mut LinuxShimState, pid: u64, sig: u64) -> i64 {
    let target_pid = if pid == 0 {
        state.current_pid
    } else {
        pid as u32
    };

    if sig == 0 {
        if target_pid != 0 && linux_find_process_slot_index(state, target_pid).is_some() {
            return 0;
        }
        return linux_neg_errno(3); // ESRCH
    }

    if linux_signal_bit(sig).is_none() {
        return linux_neg_errno(22); // EINVAL
    }
    if target_pid == 0 || linux_find_process_slot_index(state, target_pid).is_none() {
        return linux_neg_errno(3); // ESRCH
    }
    linux_queue_signal_for_process_pid(state, target_pid, sig)
}

fn linux_sys_rt_sigaction(state: &mut LinuxShimState, sig: u64, act: u64, oldact: u64, _size: u64) -> i64 {
    if sig == 0 || sig > LINUX_MAX_SIGNAL_NUM as u64 {
        return linux_neg_errno(22); // EINVAL
    }
    let idx = sig as usize;
    if oldact != 0 {
        unsafe {
            ptr::write(oldact as *mut LinuxKernelSigAction, state.sigactions[idx]);
        }
    }
    if act != 0 {
        let new_action = unsafe { ptr::read(act as *const LinuxKernelSigAction) };
        state.sigactions[idx] = new_action;
    }
    0
}

fn linux_sys_rt_sigprocmask(state: &mut LinuxShimState, how: u64, set: u64, oldset: u64, sigset_size: u64) -> i64 {
    if sigset_size != 0 && sigset_size < core::mem::size_of::<u64>() as u64 {
        return linux_neg_errno(22); // EINVAL
    }
    if oldset != 0 {
        unsafe {
            ptr::write(oldset as *mut u64, state.signal_mask);
        }
    }
    if set == 0 {
        return 0;
    }
    let new_mask = unsafe { ptr::read(set as *const u64) };
    match how {
        LINUX_SIG_BLOCK => state.signal_mask |= new_mask,
        LINUX_SIG_UNBLOCK => state.signal_mask &= !new_mask,
        LINUX_SIG_SETMASK => state.signal_mask = new_mask,
        _ => return linux_neg_errno(22), // EINVAL
    }
    linux_sync_current_thread_to_slot(state);
    0
}

fn linux_sys_rt_sigpending(state: &LinuxShimState, set: u64, sigset_size: u64) -> i64 {
    if set == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    if sigset_size != 0 && sigset_size < core::mem::size_of::<u64>() as u64 {
        return linux_neg_errno(22); // EINVAL
    }
    unsafe {
        ptr::write(set as *mut u64, state.pending_signals);
    }
    0
}

fn linux_sys_rt_sigsuspend(state: &mut LinuxShimState, set: u64, sigset_size: u64) -> i64 {
    if set != 0 {
        if sigset_size != 0 && sigset_size < core::mem::size_of::<u64>() as u64 {
            return linux_neg_errno(22); // EINVAL
        }
        let new_mask = unsafe { ptr::read(set as *const u64) };
        state.signal_mask = new_mask;
        linux_sync_current_thread_to_slot(state);
    }
    linux_neg_errno(4) // EINTR
}

fn linux_sys_sigaltstack(_uss: u64, uoss: u64) -> i64 {
    if uoss != 0 {
        unsafe {
            ptr::write(
                uoss as *mut LinuxStackT,
                LinuxStackT {
                    sp: 0,
                    flags: LINUX_SS_DISABLE,
                    _pad: 0,
                    size: 0,
                },
            );
        }
    }
    0
}

fn linux_sys_restart_syscall() -> i64 {
    0
}

fn linux_sys_clock_nanosleep(_clock_id: u64, _flags: u64, req: u64, rem: u64) -> i64 {
    linux_sys_nanosleep(req, rem)
}

fn linux_sys_rt_sigreturn() -> i64 {
    0
}

fn linux_sys_getrlimit(resource: u64, old_limit: u64) -> i64 {
    linux_sys_prlimit64(0, resource, 0, old_limit)
}

fn linux_sys_setrlimit(resource: u64, new_limit: u64) -> i64 {
    linux_sys_prlimit64(0, resource, new_limit, 0)
}

fn linux_sys_prlimit64(_pid: u64, _resource: u64, _new_limit: u64, old_limit: u64) -> i64 {
    if old_limit != 0 {
        unsafe {
            let out = old_limit as *mut LinuxRlimit;
            ptr::write(
                out,
                LinuxRlimit {
                    rlim_cur: u64::MAX,
                    rlim_max: u64::MAX,
                },
            );
        }
    }
    0
}

fn linux_sys_mremap(
    state: &mut LinuxShimState,
    old_addr: u64,
    old_size: u64,
    new_size: u64,
    flags: u64,
    new_addr: u64,
) -> i64 {
    if old_addr == 0 || old_size == 0 || new_size == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    if (flags & LINUX_MREMAP_FIXED) != 0 {
        // Linux requires MAYMOVE with FIXED. We accept compat relocation but
        // cannot guarantee exact virtual address mapping in this shim.
        if (flags & LINUX_MREMAP_MAYMOVE) == 0 {
            return linux_neg_errno(22); // EINVAL
        }
        if new_addr == 0 {
            return linux_neg_errno(22); // EINVAL
        }
    }
    let Some(old_len) = linux_align_up(old_size, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(22);
    };
    let Some(new_len) = linux_align_up(new_size, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(22);
    };
    let Some(slot_idx) = linux_find_mmap_slot_for_range(state, old_addr, old_len) else {
        return linux_neg_errno(22);
    };
    if state.maps[slot_idx].addr != old_addr || state.maps[slot_idx].len != old_len {
        return linux_neg_errno(22); // EINVAL: partial mremap unsupported
    }

    if new_len == old_len {
        return old_addr as i64;
    }

    if new_len < old_len {
        state.maps[slot_idx].len = new_len;
        state.maps[slot_idx].backing_len = new_len;
        return old_addr as i64;
    }

    if (flags & LINUX_MREMAP_MAYMOVE) == 0 {
        return linux_neg_errno(12); // ENOMEM
    }
    let old_prot = state.maps[slot_idx].prot;
    let old_flags = state.maps[slot_idx].flags;

    if new_len > usize::MAX as u64 {
        return linux_neg_errno(12);
    }
    let Ok(layout) = Layout::from_size_align(new_len as usize, LINUX_PAGE_SIZE as usize) else {
        return linux_neg_errno(12);
    };
    let new_ptr = unsafe { alloc(layout) };
    if new_ptr.is_null() {
        return linux_neg_errno(12);
    }
    unsafe {
        ptr::write_bytes(new_ptr, 0, new_len as usize);
        let copy_len = old_len.min(new_len) as usize;
        ptr::copy_nonoverlapping(old_addr as *const u8, new_ptr, copy_len);
    }
    linux_release_mmap_slot(&mut state.maps[slot_idx]);
    state.maps[slot_idx] = LinuxMmapSlot {
        active: true,
        process_pid: state.current_pid,
        addr: new_ptr as u64,
        len: new_len,
        prot: old_prot,
        flags: old_flags,
        backing_ptr: new_ptr as u64,
        backing_len: new_len,
    };
    state.mmap_cursor = state.mmap_cursor.saturating_add(new_len).min(LINUX_MMAP_LIMIT);
    new_ptr as u64 as i64
}

fn linux_sys_shmget(state: &mut LinuxShimState, _key: u64, size: u64, _shmflg: u64) -> i64 {
    if size == 0 {
        return linux_neg_errno(22); // EINVAL
    }
    let Some(aligned) = linux_align_up(size, LINUX_PAGE_SIZE) else {
        return linux_neg_errno(12);
    };
    state.shm_size_hint = aligned;
    let id = state.shm_next_id;
    state.shm_next_id = state.shm_next_id.saturating_add(1);
    id as i64
}

fn linux_sys_shmat(state: &mut LinuxShimState, shmid: u64, shmaddr: u64, shmflg: u64) -> i64 {
    if shmid == 0 {
        return linux_neg_errno(22);
    }
    let size = if state.shm_size_hint == 0 {
        LINUX_PAGE_SIZE
    } else {
        state.shm_size_hint
    };
    linux_sys_mmap(
        state,
        shmaddr,
        size,
        0x3,
        LINUX_MAP_SHARED | LINUX_MAP_ANONYMOUS | (shmflg & LINUX_MAP_FIXED),
        u64::MAX,
        0,
    )
}

fn linux_sys_shmctl(_state: &mut LinuxShimState, _shmid: u64, cmd: u64, _buf: u64) -> i64 {
    if cmd == LINUX_IPC_RMID {
        return 0;
    }
    0
}

fn linux_sys_getrandom(state: &LinuxShimState, buf: u64, len: u64, _flags: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    let copy_len = (len as usize).min(LINUX_GETRANDOM_MAX);
    let mut seed = timer::ticks() ^ state.session_id.rotate_left(17);
    unsafe {
        let dst = buf as *mut u8;
        let mut i = 0usize;
        while i < copy_len {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ptr::write(dst.add(i), (seed & 0xFF) as u8);
            i += 1;
        }
    }
    copy_len as i64
}

fn linux_sys_uname(buf: u64) -> i64 {
    if buf == 0 {
        return linux_neg_errno(14); // EFAULT
    }
    unsafe {
        let dst = buf as *mut u8;
        ptr::write_bytes(dst, 0, LINUX_UTS_FIELD_LEN * 6);
        let mut field = 0usize;
        while field < 6 {
            let field_ptr = dst.add(field * LINUX_UTS_FIELD_LEN);
            let field_slice = core::slice::from_raw_parts_mut(field_ptr, LINUX_UTS_FIELD_LEN);
            match field {
                0 => linux_fill_ascii_field(field_slice, "Linux"),
                1 => linux_fill_ascii_field(field_slice, "reduxos"),
                2 => linux_fill_ascii_field(field_slice, "6.6.0"),
                3 => linux_fill_ascii_field(field_slice, "#1 ReduxOS"),
                4 => linux_fill_ascii_field(field_slice, "x86_64"),
                _ => linux_fill_ascii_field(field_slice, ""),
            }
            field += 1;
        }
    }
    0
}

const SYSCALL_TABLE: [SysHandler; SYS_COUNT] = [
    handle_write_line,
    handle_clear_lines,
    handle_get_tick,
    handle_get_runtime_flags,
    handle_recv_command,
    handle_thread_info,
    handle_syscall_count,
    handle_priv_status,
    handle_priv_next,
    handle_priv_unsafe_test,
];

static mut SYSCALL_COUNTS: [u64; SYS_COUNT] = [0; SYS_COUNT];
static mut RUNTIME_STATE: RuntimeState = RuntimeState::empty();
static mut CMD_QUEUE: CommandQueue = CommandQueue::new();
static mut LINUX_SHIM: LinuxShimState = LinuxShimState::empty();
static mut LINUX_SHIM_NEXT_SESSION_ID: u64 = 1;
static mut LINUX_SHIM_ACTIVE_PLAN: *mut crate::linux_compat::LinuxDynLaunchPlan = core::ptr::null_mut();
static mut LINUX_GFX_BRIDGE: LinuxGfxBridgeState = LinuxGfxBridgeState::empty();
static mut LINUX_GFX_PIXELS: [u32; LINUX_GFX_MAX_PIXELS] = [0; LINUX_GFX_MAX_PIXELS];
static mut LINUX_X11_PIXMAP_PIXELS: [u32; LINUX_X11_MAX_PIXMAPS * LINUX_X11_PIXMAP_SLOT_PIXELS] =
    [0; LINUX_X11_MAX_PIXMAPS * LINUX_X11_PIXMAP_SLOT_PIXELS];

unsafe fn linux_shim_release_active_plan() {
    if LINUX_SHIM_ACTIVE_PLAN.is_null() {
        return;
    }
    let ptr = LINUX_SHIM_ACTIVE_PLAN;
    LINUX_SHIM_ACTIVE_PLAN = core::ptr::null_mut();
    drop(Box::from_raw(ptr));
}

unsafe fn linux_shim_store_active_plan(plan: crate::linux_compat::LinuxDynLaunchPlan) {
    linux_shim_release_active_plan();
    LINUX_SHIM_ACTIVE_PLAN = Box::into_raw(Box::new(plan));
}

pub fn linux_shim_begin(main_entry: u64, interp_entry: u64, stack_ptr: u64, tls_tcb_addr: u64) -> u64 {
    unsafe {
        linux_release_all_mmaps(&mut LINUX_SHIM);
        linux_release_all_runtime_blobs(&mut LINUX_SHIM);
        linux_shim_release_active_plan();
        privilege::linux_real_slice_reset();
        let mut session_id = LINUX_SHIM_NEXT_SESSION_ID;
        if session_id == 0 {
            session_id = 1;
        }
        LINUX_SHIM_NEXT_SESSION_ID = session_id.saturating_add(1);
        let brk_base = LINUX_MMAP_BASE.saturating_sub(LINUX_BRK_REGION_BYTES);
        let brk_base_aligned = linux_align_up(brk_base, LINUX_PAGE_SIZE).unwrap_or(brk_base);
        let brk_limit = brk_base_aligned.saturating_add(LINUX_BRK_REGION_BYTES);
        let mut pid_value = (1000u64.saturating_add(session_id) & 0xFFFF_FFFF) as u32;
        if pid_value == 0 {
            pid_value = 1;
        }
        let tid_value = (session_id as u32).saturating_add(2000);
        let shim_ptr = &mut LINUX_SHIM as *mut LinuxShimState;
        ptr::write_bytes(
            shim_ptr as *mut u8,
            0,
            core::mem::size_of::<LinuxShimState>(),
        );
        let state = &mut *shim_ptr;
        state.active = true;
        state.session_id = session_id;
        state.main_entry = main_entry;
        state.interp_entry = interp_entry;
        state.stack_ptr = stack_ptr;
        state.fs_base = tls_tcb_addr;
        state.brk_base = brk_base_aligned;
        state.brk_current = brk_base_aligned;
        state.brk_limit = brk_limit;
        state.mmap_cursor = LINUX_MMAP_BASE;
        state.tid_value = tid_value;
        state.current_tid = tid_value;
        state.current_pid = pid_value;
        state.next_tid = tid_value;
        state.next_pid = pid_value;
        state.thread_count = 1;
        state.process_count = 1;
        state.next_fd = LINUX_FD_BASE;
        state.shm_next_id = 1;
        state.start_tick = timer::ticks();
        state.processes[0] = LinuxProcessSlot {
            active: true,
            pid: pid_value,
            parent_pid: 1,
            leader_tid: tid_value,
            brk_base: brk_base_aligned,
            brk_current: brk_base_aligned,
            brk_limit,
            mmap_cursor: LINUX_MMAP_BASE,
            mmap_count: 0,
        };
        state.threads[0] = LinuxThreadSlot {
            active: true,
            tid: tid_value,
            process_pid: pid_value,
            parent_tid: 0,
            exit_signal: 0,
            state: LINUX_THREAD_RUNNABLE,
            _pad0: [0; 2],
            fs_base: tls_tcb_addr,
            tid_addr: 0,
            robust_list_head: 0,
            robust_list_len: 0,
            futex_wait_addr: 0,
            futex_wait_mask: LINUX_FUTEX_BITSET_MATCH_ANY,
            futex_timeout_errno: 0,
            futex_timeout_deadline: 0,
            futex_requeue_pi_target: 0,
            futex_waitv_count: 0,
            _pad_waitv: [0; 6],
            futex_waitv_uaddrs: [0; LINUX_FUTEX_WAITV_MAX],
            clone_flags: 0,
            signal_mask: 0,
            pending_signals: 0,
        };
        state.thread_contexts[0] = LinuxThreadContext::empty();
        linux_x11_reset_server(state);
        session_id
    }
}

pub fn linux_shim_run_real_slice(
    entry: u64,
    stack_ptr: u64,
    tls_tcb_addr: u64,
    call_budget: usize,
) -> LinuxShimSliceSummary {
    let mut summary = LinuxShimSliceSummary::empty();
    if !linux_shim_active() {
        return summary;
    }

    let (entry_eff, stack_eff, tls_eff, reset_context) = unsafe {
        let state = &mut LINUX_SHIM;
        let _ = linux_process_futex_timeouts(state);
        let mut reset_context = state.exec_transition_pending;
        state.exec_transition_pending = false;

        if !reset_context && privilege::linux_real_context_valid() {
            linux_capture_current_thread_context(state, None);
        }

        let runnable = linux_count_runnable_threads(state);
        let current_runnable = linux_find_current_thread_slot_index(state)
            .map(|idx| state.threads[idx].state == LINUX_THREAD_RUNNABLE)
            .unwrap_or(false);
        if runnable > 0 && (runnable > 1 || !current_runnable) {
            let current_tid = state.current_tid;
            if linux_shim_schedule_next_thread(state) {
                if state.current_tid != current_tid {
                    if let Some(next_idx) = linux_find_current_thread_slot_index(state) {
                        if !state.thread_contexts[next_idx].valid {
                            let _ = linux_set_current_thread_tid(state, current_tid);
                        }
                    } else {
                        let _ = linux_set_current_thread_tid(state, current_tid);
                    }
                }
            }
        }

        if let Some(cur_idx) = linux_find_current_thread_slot_index(state) {
            let ctx = state.thread_contexts[cur_idx];
            if ctx.valid {
                linux_thread_context_apply_to_privilege(&ctx, state.threads[cur_idx].fs_base);
            } else {
                reset_context = true;
                privilege::linux_real_slice_set_tls(state.threads[cur_idx].fs_base);
            }
        }

        let entry_eff = if state.interp_entry != 0 {
            state.interp_entry
        } else {
            entry
        };
        let stack_eff = if state.stack_ptr != 0 {
            state.stack_ptr
        } else {
            stack_ptr
        };
        let tls_eff = if state.fs_base != 0 {
            state.fs_base
        } else {
            tls_tcb_addr
        };
        (entry_eff, stack_eff, tls_eff, reset_context)
    };

    if entry_eff == 0 || stack_eff == 0 {
        return summary;
    }
    if reset_context {
        privilege::linux_real_slice_discard_resume_context();
    }

    let report = privilege::linux_real_slice_run(entry_eff, stack_eff, tls_eff, call_budget);
    summary.completed_calls = report.calls.min(u32::MAX as u64) as u32;

    let status = linux_shim_status();
    summary.active = status.active;
    summary.watchdog_triggered = status.watchdog_triggered;
    summary.exit_code = status.exit_code;
    summary.last_sysno = status.last_sysno;
    summary.last_result = status.last_result;
    if status.last_result < 0 {
        summary.failed = 1;
        summary.first_errno = status.last_result;
    } else {
        summary.ok = summary.completed_calls;
        summary.first_errno = 0;
    }
    summary
}

pub fn linux_shim_status() -> LinuxShimStatus {
    unsafe {
        LinuxShimStatus {
            active: LINUX_SHIM.active,
            session_id: LINUX_SHIM.session_id,
            main_entry: LINUX_SHIM.main_entry,
            interp_entry: LINUX_SHIM.interp_entry,
            stack_ptr: LINUX_SHIM.stack_ptr,
            brk_current: LINUX_SHIM.brk_current,
            brk_limit: LINUX_SHIM.brk_limit,
            mmap_count: LINUX_SHIM.mmap_count,
            mmap_cursor: LINUX_SHIM.mmap_cursor,
            fs_base: LINUX_SHIM.fs_base,
            tid_value: LINUX_SHIM.tid_value,
            current_tid: LINUX_SHIM.current_tid,
            current_pid: LINUX_SHIM.current_pid,
            thread_count: LINUX_SHIM.thread_count,
            process_count: LINUX_SHIM.process_count,
            runnable_threads: linux_count_runnable_threads(&LINUX_SHIM),
            signal_mask: LINUX_SHIM.signal_mask,
            pending_signals: LINUX_SHIM.pending_signals,
            runtime_file_count: LINUX_SHIM.runtime_file_count,
            runtime_blob_bytes: LINUX_SHIM.runtime_blob_bytes,
            runtime_blob_files: LINUX_SHIM.runtime_blob_files,
            open_file_count: LINUX_SHIM.open_file_count,
            next_fd: LINUX_SHIM.next_fd,
            exit_code: LINUX_SHIM.exit_code,
            syscall_count: LINUX_SHIM.syscall_count,
            last_sysno: LINUX_SHIM.last_sysno,
            last_result: LINUX_SHIM.last_result,
            last_errno: LINUX_SHIM.last_errno,
            last_path_len: (LINUX_SHIM.last_path_len as usize).min(LINUX_PATH_MAX),
            last_path: LINUX_SHIM.last_path,
            last_path_errno: LINUX_SHIM.last_path_errno,
            last_path_sysno: LINUX_SHIM.last_path_sysno,
            last_path_runtime_hit: LINUX_SHIM.last_path_runtime_hit,
            watchdog_triggered: LINUX_SHIM.watchdog_triggered,
        }
    }
}

pub fn linux_x11_socket_status() -> LinuxX11SocketStatus {
    unsafe {
        let mut status = LinuxX11SocketStatus {
            endpoint_count: 0,
            connected_count: 0,
            ready_count: 0,
            handshake_count: 0,
            last_error: 0,
            last_path_len: 0,
            last_path: [0; LINUX_PATH_MAX],
            last_unix_connect_errno: LINUX_SHIM.last_unix_connect_errno,
            last_unix_connect_len: (LINUX_SHIM.last_unix_connect_len as usize).min(LINUX_PATH_MAX),
            last_unix_connect_path: LINUX_SHIM.last_unix_connect_path,
        };
        let mut i = 0usize;
        while i < LINUX_MAX_SOCKETS {
            let sock = &LINUX_SHIM.sockets[i];
            if sock.active && sock.endpoint == LINUX_SOCKET_ENDPOINT_X11 {
                status.endpoint_count = status.endpoint_count.saturating_add(1);
                if sock.connected {
                    status.connected_count = status.connected_count.saturating_add(1);
                }
                if sock.x11_state == LINUX_X11_STATE_READY {
                    status.ready_count = status.ready_count.saturating_add(1);
                } else {
                    status.handshake_count = status.handshake_count.saturating_add(1);
                }
                status.last_error = sock.last_error;
                status.last_path_len = (sock.path_len as usize).min(LINUX_PATH_MAX);
                status.last_path = sock.path;
            }
            i += 1;
        }
        status
    }
}

pub fn linux_shim_active() -> bool {
    unsafe { LINUX_SHIM.active }
}

pub fn linux_shim_register_runtime_path(path: &str, size: u64) -> bool {
    unsafe {
        let state = &mut LINUX_SHIM;
        if !state.active {
            return false;
        }
        let mut normalized = [0u8; LINUX_PATH_MAX];
        let path_len = linux_normalize_path_str(&mut normalized, path);
        if path_len == 0 {
            return false;
        }

        if let Some(existing) = linux_find_runtime_index(state, &normalized, path_len) {
            state.runtime_files[existing].size = size;
            return true;
        }

        let Some(slot_idx) = linux_allocate_runtime_slot(state) else {
            return false;
        };
        state.runtime_files[slot_idx] = LinuxRuntimeFileSlot {
            active: true,
            size,
            path_len: path_len as u16,
            path: normalized,
            data_ptr: 0,
            data_len: 0,
        };
        state.runtime_file_count = state.runtime_file_count.saturating_add(1);
        true
    }
}

pub fn linux_shim_register_runtime_blob(path: &str, data: &[u8]) -> bool {
    unsafe {
        let state = &mut LINUX_SHIM;
        if !state.active {
            return false;
        }

        let mut normalized = [0u8; LINUX_PATH_MAX];
        let path_len = linux_normalize_path_str(&mut normalized, path);
        if path_len == 0 {
            return false;
        }

        let slot_idx = if let Some(existing) = linux_find_runtime_index(state, &normalized, path_len) {
            existing
        } else {
            let Some(new_slot) = linux_allocate_runtime_slot(state) else {
                return false;
            };
            state.runtime_files[new_slot] = LinuxRuntimeFileSlot {
                active: true,
                size: data.len() as u64,
                path_len: path_len as u16,
                path: normalized,
                data_ptr: 0,
                data_len: 0,
            };
            state.runtime_file_count = state.runtime_file_count.saturating_add(1);
            new_slot
        };

        let existing_len = state.runtime_files[slot_idx].data_len;
        let requested_len = data.len() as u64;
        let projected = state
            .runtime_blob_bytes
            .saturating_sub(existing_len)
            .saturating_add(requested_len);
        if projected > LINUX_RUNTIME_BLOB_BUDGET_BYTES {
            return false;
        }

        let mut new_ptr = 0u64;
        if !data.is_empty() {
            let Ok(layout) = Layout::from_size_align(data.len(), 1) else {
                return false;
            };
            let raw_ptr = alloc(layout);
            if raw_ptr.is_null() {
                return false;
            }
            ptr::copy_nonoverlapping(data.as_ptr(), raw_ptr, data.len());
            new_ptr = raw_ptr as u64;
        }

        let slot = &mut state.runtime_files[slot_idx];
        if slot.data_ptr != 0 && slot.data_len > 0 {
            linux_release_runtime_blob(slot);
        }
        slot.active = true;
        slot.path_len = path_len as u16;
        slot.path = normalized;
        slot.data_ptr = new_ptr;
        slot.data_len = requested_len;
        if slot.size < requested_len {
            slot.size = requested_len;
        }
        linux_recount_runtime_blob_stats(state);
        true
    }
}

pub fn linux_shim_invoke(sysno: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
    unsafe {
        let state = &mut LINUX_SHIM;
        if !state.active {
            return linux_neg_errno(3); // ESRCH
        }
        let _ = linux_process_futex_timeouts(state);
        if linux_shim_watchdog_should_abort(state) {
            linux_stdio_push_line(state);
            linux_release_all_mmaps(state);
            linux_shim_release_active_plan();
            state.watchdog_triggered = true;
            state.active = false;
            state.exit_code = -(LINUX_ERRNO_ETIMEDOUT as i32);
            state.last_sysno = sysno;
            state.last_result = linux_neg_errno(LINUX_ERRNO_ETIMEDOUT);
            state.last_errno = LINUX_ERRNO_ETIMEDOUT;
            return state.last_result;
        }

        if let Some(sig_res) = linux_deliver_current_pending_signal(state) {
            state.syscall_count = state.syscall_count.saturating_add(1);
            state.last_sysno = sysno;
            state.last_result = sig_res;
            state.last_errno = if sig_res < 0 { (-sig_res).min(i64::MAX) } else { 0 };
            return sig_res;
        }

        let result = match sysno {
            LINUX_SYS_READ => linux_sys_read(state, a0, a1, a2),
            LINUX_SYS_MSYNC => linux_sys_msync(a0, a1, a2),
            LINUX_SYS_MINCORE => linux_sys_mincore(a0, a1, a2),
            LINUX_SYS_PREAD64 => linux_sys_pread64(state, a0, a1, a2, a3),
            LINUX_SYS_READV => linux_sys_readv(state, a0, a1, a2),
            LINUX_SYS_PIPE => linux_sys_pipe(state, a0),
            LINUX_SYS_POLL => linux_sys_poll(state, a0, a1, a2 as i64),
            LINUX_SYS_WRITE => linux_sys_write(state, a0, a1, a2),
            LINUX_SYS_WRITEV => linux_sys_writev(state, a0, a1, a2),
            LINUX_SYS_CLOSE => linux_sys_close(state, a0),
            LINUX_SYS_CLOSE_RANGE => linux_sys_close_range(state, a0, a1, a2),
            LINUX_SYS_FSTAT => linux_sys_fstat(state, a0, a1),
            LINUX_SYS_LSEEK => linux_sys_lseek(state, a0, a1, a2),
            LINUX_SYS_SOCKET => linux_sys_socket(state, a0, a1, a2),
            LINUX_SYS_CONNECT => linux_sys_connect(state, a0, a1, a2),
            LINUX_SYS_ACCEPT => linux_sys_accept(state, a0, a1, a2),
            LINUX_SYS_SENDTO => linux_sys_sendto(state, a0, a1, a2, a3, a4, a5),
            LINUX_SYS_RECVFROM => linux_sys_recvfrom(state, a0, a1, a2, a3, a4, a5),
            LINUX_SYS_SENDMSG => linux_sys_sendmsg(state, a0, a1, a2),
            LINUX_SYS_RECVMSG => linux_sys_recvmsg(state, a0, a1, a2),
            LINUX_SYS_SHUTDOWN => linux_sys_shutdown(state, a0, a1),
            LINUX_SYS_BIND => linux_sys_bind(state, a0, a1, a2),
            LINUX_SYS_LISTEN => linux_sys_listen(state, a0, a1),
            LINUX_SYS_GETSOCKNAME => linux_sys_getsockname(state, a0, a1, a2),
            LINUX_SYS_GETPEERNAME => linux_sys_getpeername(state, a0, a1, a2),
            LINUX_SYS_SOCKETPAIR => linux_sys_socketpair(state, a0, a1, a2, a3),
            LINUX_SYS_SETSOCKOPT => linux_sys_setsockopt(state, a0, a1, a2, a3, a4),
            LINUX_SYS_GETSOCKOPT => linux_sys_getsockopt(state, a0, a1, a2, a3, a4),
            LINUX_SYS_ACCEPT4 => linux_sys_accept4(state, a0, a1, a2, a3),
            LINUX_SYS_IOCTL => linux_sys_ioctl(state, a0, a1, a2),
            LINUX_SYS_ACCESS => linux_sys_access(state, a0, a1),
            LINUX_SYS_SCHED_YIELD => linux_sys_sched_yield(state),
            LINUX_SYS_SCHED_SETAFFINITY => linux_sys_sched_setaffinity(a0, a1, a2),
            LINUX_SYS_SCHED_GETAFFINITY => linux_sys_sched_getaffinity(a0, a1, a2),
            LINUX_SYS_DUP => linux_sys_dup(state, a0),
            LINUX_SYS_DUP2 => linux_sys_dup2(state, a0, a1),
            LINUX_SYS_NANOSLEEP => linux_sys_nanosleep(a0, a1),
            LINUX_SYS_FORK => linux_sys_fork(state),
            LINUX_SYS_VFORK => linux_sys_vfork(state),
            LINUX_SYS_EXIT => linux_sys_exit(state, a0, false),
            LINUX_SYS_EXIT_GROUP => linux_sys_exit(state, a0, true),
            LINUX_SYS_BRK => linux_sys_brk(state, a0),
            LINUX_SYS_MMAP => linux_sys_mmap(state, a0, a1, a2, a3, a4, a5),
            LINUX_SYS_MREMAP => linux_sys_mremap(state, a0, a1, a2, a3, a4),
            LINUX_SYS_SHMGET => linux_sys_shmget(state, a0, a1, a2),
            LINUX_SYS_SHMAT => linux_sys_shmat(state, a0, a1, a2),
            LINUX_SYS_SHMCTL => linux_sys_shmctl(state, a0, a1, a2),
            LINUX_SYS_SHMDT => linux_sys_shmdt(a0),
            LINUX_SYS_MPROTECT => linux_sys_mprotect(state, a0, a1, a2),
            LINUX_SYS_MLOCK => linux_sys_mlock(a0, a1),
            LINUX_SYS_MUNLOCK => linux_sys_munlock(a0, a1),
            LINUX_SYS_MLOCKALL => linux_sys_mlockall(a0),
            LINUX_SYS_MUNLOCKALL => linux_sys_munlockall(),
            LINUX_SYS_MADVISE => linux_sys_madvise(a0, a1, a2),
            LINUX_SYS_MUNMAP => linux_sys_munmap(state, a0, a1),
            LINUX_SYS_CLONE => linux_sys_clone(state, a0, a1, a2, a3, a4),
            LINUX_SYS_OPENAT => linux_sys_openat(state, a0, a1, a2, a3),
            LINUX_SYS_OPENAT2 => linux_sys_openat2(state, a0, a1, a2, a3),
            LINUX_SYS_NEWFSTATAT => linux_sys_newfstatat(state, a0, a1, a2, a3),
            LINUX_SYS_FACCESSAT => linux_sys_faccessat(state, a0, a1, a2, a3),
            LINUX_SYS_FACCESSAT2 => linux_sys_faccessat2(state, a0, a1, a2, a3),
            LINUX_SYS_GETCWD => linux_sys_getcwd(a0, a1),
            LINUX_SYS_READLINK => linux_sys_readlink(state, a0, a1, a2),
            LINUX_SYS_READLINKAT => linux_sys_readlinkat(state, a0, a1, a2, a3),
            LINUX_SYS_GETTIMEOFDAY => linux_sys_gettimeofday(a0, a1),
            LINUX_SYS_SYSINFO => linux_sys_sysinfo(state, a0),
            LINUX_SYS_GETRUSAGE => linux_sys_getrusage(a0, a1),
            LINUX_SYS_TIMES => linux_sys_times(a0),
            LINUX_SYS_FCNTL => linux_sys_fcntl(state, a0, a1, a2),
            LINUX_SYS_GETDENTS64 => linux_sys_getdents64(state, a0, a1, a2),
            LINUX_SYS_PRCTL => linux_sys_prctl(a0, a1, a2, a3, a4),
            LINUX_SYS_RT_SIGACTION => linux_sys_rt_sigaction(state, a0, a1, a2, a3),
            LINUX_SYS_RT_SIGPROCMASK => linux_sys_rt_sigprocmask(state, a0, a1, a2, a3),
            LINUX_SYS_RT_SIGPENDING => linux_sys_rt_sigpending(state, a0, a1),
            LINUX_SYS_RT_SIGSUSPEND => linux_sys_rt_sigsuspend(state, a0, a1),
            LINUX_SYS_SIGALTSTACK => linux_sys_sigaltstack(a0, a1),
            LINUX_SYS_GETPID => linux_sys_getpid(state),
            LINUX_SYS_GETPGID => linux_sys_getpgid(state, a0),
            LINUX_SYS_GETSID => linux_sys_getsid(state, a0),
            LINUX_SYS_SETPGID => linux_sys_setpgid(state, a0, a1),
            LINUX_SYS_WAIT4 => linux_sys_wait4(state, a0, a1, a2, a3),
            LINUX_SYS_WAITID => linux_sys_waitid(state, a0, a1, a2, a3, a4),
            LINUX_SYS_KILL => linux_sys_kill(state, a0, a1),
            LINUX_SYS_GETUID => linux_sys_getuid(),
            LINUX_SYS_GETGID => linux_sys_getgid(),
            LINUX_SYS_SETUID => linux_sys_setuid(a0),
            LINUX_SYS_SETGID => linux_sys_setgid(a0),
            LINUX_SYS_SETRESUID => linux_sys_setresuid(a0, a1, a2),
            LINUX_SYS_GETRESUID => linux_sys_getresuid(a0, a1, a2),
            LINUX_SYS_SETRESGID => linux_sys_setresgid(a0, a1, a2),
            LINUX_SYS_GETRESGID => linux_sys_getresgid(a0, a1, a2),
            LINUX_SYS_GETPPID => linux_sys_getppid(state),
            LINUX_SYS_GETEUID => linux_sys_getuid(),
            LINUX_SYS_GETEGID => linux_sys_getgid(),
            LINUX_SYS_UNAME => linux_sys_uname(a0),
            LINUX_SYS_ARCH_PRCTL => linux_sys_arch_prctl(state, a0, a1),
            LINUX_SYS_GETTID => linux_sys_gettid(state),
            LINUX_SYS_SET_TID_ADDRESS => linux_sys_set_tid_address(state, a0),
            LINUX_SYS_RESTART_SYSCALL => linux_sys_restart_syscall(),
            LINUX_SYS_CLOCK_GETTIME => linux_sys_clock_gettime(a0, a1),
            LINUX_SYS_CLOCK_GETRES => linux_sys_clock_getres(a0, a1),
            LINUX_SYS_CLOCK_NANOSLEEP => linux_sys_clock_nanosleep(a0, a1, a2, a3),
            LINUX_SYS_FUTEX => linux_sys_futex(state, a0, a1, a2, a3, a4, a5),
            LINUX_SYS_FUTEX_WAITV => linux_sys_futex_waitv(state, a0, a1, a2, a3, a4),
            LINUX_SYS_EPOLL_CTL => linux_sys_epoll_ctl(state, a0, a1, a2, a3),
            LINUX_SYS_TGKILL => linux_sys_tgkill(state, a0, a1, a2),
            LINUX_SYS_PPOLL => linux_sys_ppoll(state, a0, a1, a2, a3, a4),
            LINUX_SYS_SET_ROBUST_LIST => linux_sys_set_robust_list(state, a0, a1),
            LINUX_SYS_GET_ROBUST_LIST => linux_sys_get_robust_list(state, a0, a1, a2),
            LINUX_SYS_GETRLIMIT => linux_sys_getrlimit(a0, a1),
            LINUX_SYS_SETRLIMIT => linux_sys_setrlimit(a0, a1),
            LINUX_SYS_PRLIMIT64 => linux_sys_prlimit64(a0, a1, a2, a3),
            LINUX_SYS_GETCPU => linux_sys_getcpu(a0, a1, a2),
            LINUX_SYS_GETRANDOM => linux_sys_getrandom(state, a0, a1, a2),
            LINUX_SYS_EPOLL_WAIT => linux_sys_epoll_wait(state, a0, a1, a2, a3 as i64),
            LINUX_SYS_EPOLL_PWAIT => linux_sys_epoll_pwait(state, a0, a1, a2, a3 as i64, a4, a5),
            LINUX_SYS_EPOLL_PWAIT2 => linux_sys_epoll_pwait2(state, a0, a1, a2, a3, a4, a5),
            LINUX_SYS_EVENTFD => linux_sys_eventfd(state, a0),
            LINUX_SYS_TIMERFD_CREATE => linux_sys_timerfd_create(state, a0, a1),
            LINUX_SYS_TIMERFD_SETTIME => linux_sys_timerfd_settime(state, a0, a1, a2, a3),
            LINUX_SYS_TIMERFD_GETTIME => linux_sys_timerfd_gettime(state, a0, a1),
            LINUX_SYS_EPOLL_CREATE => linux_sys_epoll_create(state, a0),
            LINUX_SYS_EVENTFD2 => linux_sys_eventfd2(state, a0, a1),
            LINUX_SYS_EPOLL_CREATE1 => linux_sys_epoll_create1(state, a0),
            LINUX_SYS_DUP3 => linux_sys_dup3(state, a0, a1, a2),
            LINUX_SYS_PIPE2 => linux_sys_pipe2(state, a0, a1),
            LINUX_SYS_MEMFD_CREATE => linux_sys_memfd_create(state, a0, a1),
            LINUX_SYS_STATX => linux_sys_statx(state, a0, a1, a2, a3, a4),
            LINUX_SYS_RSEQ => linux_sys_rseq(a0, a1, a2, a3),
            LINUX_SYS_MEMBARRIER => linux_sys_membarrier(a0, a1, a2),
            LINUX_SYS_CLONE3 => linux_sys_clone3(state, a0, a1),
            LINUX_SYS_EXECVE => linux_sys_execve(state, a0, a1, a2),
            LINUX_SYS_EXECVEAT => linux_sys_execveat(state, a0, a1, a2, a3, a4),
            LINUX_SYS_PIDFD_OPEN => linux_sys_pidfd_open(state, a0, a1),
            LINUX_SYS_PIDFD_SEND_SIGNAL => linux_sys_pidfd_send_signal(state, a0, a1, a2, a3),
            LINUX_SYS_RT_SIGRETURN => linux_sys_rt_sigreturn(),
            _ => linux_neg_errno(38), // ENOSYS
        };
        linux_sync_current_process_to_slot(state);
        state.syscall_count = state.syscall_count.saturating_add(1);
        state.last_sysno = sysno;
        state.last_result = result;
        state.last_errno = if result < 0 { (-result).min(i64::MAX) } else { 0 };

        if state.active {
            let pending_switch_tid = core::mem::replace(&mut state.pending_switch_tid, 0);
            let mut captured = false;
            let mut switched = false;
            if pending_switch_tid != 0 && pending_switch_tid != state.current_tid {
                linux_capture_current_thread_context(state, Some(result as u64));
                captured = true;
                switched = linux_set_current_thread_tid(state, pending_switch_tid);
                if switched {
                    privilege::linux_real_slice_request_yield();
                }
            }
            if !switched && linux_count_runnable_threads(state) > 1 {
                if !captured {
                    linux_capture_current_thread_context(state, Some(result as u64));
                }
                privilege::linux_real_slice_request_yield();
            }
        }
        result
    }
}

pub fn linux_shim_probe_baseline() -> LinuxShimProbeSummary {
    let mut summary = LinuxShimProbeSummary::empty();
    let mut ts = LinuxTimespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut fs_out: u64 = 0;
    let mut tid_out: u32 = 0;
    let mut rand_buf = [0u8; 32];
    let mut uname_buf = [0u8; LINUX_UTS_FIELD_LEN * 6];
    let mut rlimit = LinuxRlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    let brk_before = linux_shim_invoke(LINUX_SYS_BRK, 0, 0, 0, 0, 0, 0);
    summary.brk_before = brk_before;
    linux_probe_mark(&mut summary, brk_before);

    let brk_after = if brk_before > 0 {
        linux_shim_invoke(
            LINUX_SYS_BRK,
            (brk_before as u64).saturating_add(0x4000),
            0,
            0,
            0,
            0,
            0,
        )
    } else {
        brk_before
    };
    summary.brk_after = brk_after;
    linux_probe_mark(&mut summary, brk_after);

    let mmap_res = linux_shim_invoke(
        LINUX_SYS_MMAP,
        0,
        0x4000,
        0x3,
        LINUX_MAP_PRIVATE | LINUX_MAP_ANONYMOUS,
        u64::MAX,
        0,
    );
    summary.mmap_res = mmap_res;
    linux_probe_mark(&mut summary, mmap_res);

    let mprotect_res = if mmap_res > 0 {
        linux_shim_invoke(
            LINUX_SYS_MPROTECT,
            mmap_res as u64,
            0x4000,
            0x1,
            0,
            0,
            0,
        )
    } else {
        linux_neg_errno(12)
    };
    summary.mprotect_res = mprotect_res;
    linux_probe_mark(&mut summary, mprotect_res);

    let mut futex_word: u32 = 0;
    let futex_res = linux_shim_invoke(
        LINUX_SYS_FUTEX,
        (&mut futex_word as *mut u32) as u64,
        LINUX_FUTEX_WAKE,
        1,
        0,
        0,
        0,
    );
    summary.futex_res = futex_res;
    linux_probe_mark(&mut summary, futex_res);

    let clock_res = linux_shim_invoke(
        LINUX_SYS_CLOCK_GETTIME,
        LINUX_CLOCK_MONOTONIC,
        (&mut ts as *mut LinuxTimespec) as u64,
        0,
        0,
        0,
        0,
    );
    summary.clock_res = clock_res;
    linux_probe_mark(&mut summary, clock_res);

    let getpid_res = linux_shim_invoke(LINUX_SYS_GETPID, 0, 0, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, getpid_res);

    let gettid_res = linux_shim_invoke(LINUX_SYS_GETTID, 0, 0, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, gettid_res);

    let set_tid_res = linux_shim_invoke(
        LINUX_SYS_SET_TID_ADDRESS,
        (&mut tid_out as *mut u32) as u64,
        0,
        0,
        0,
        0,
        0,
    );
    linux_probe_mark(&mut summary, set_tid_res);

    let set_fs_res = linux_shim_invoke(LINUX_SYS_ARCH_PRCTL, LINUX_ARCH_SET_FS, 0x7fff_1234_0000, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, set_fs_res);

    let get_fs_res = linux_shim_invoke(
        LINUX_SYS_ARCH_PRCTL,
        LINUX_ARCH_GET_FS,
        (&mut fs_out as *mut u64) as u64,
        0,
        0,
        0,
        0,
    );
    linux_probe_mark(&mut summary, get_fs_res);

    let set_robust_res = linux_shim_invoke(LINUX_SYS_SET_ROBUST_LIST, 0, 24, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, set_robust_res);

    let sigaction_res = linux_shim_invoke(LINUX_SYS_RT_SIGACTION, 2, 0, 0, 8, 0, 0);
    linux_probe_mark(&mut summary, sigaction_res);

    let sigmask_res = linux_shim_invoke(LINUX_SYS_RT_SIGPROCMASK, 0, 0, 0, 8, 0, 0);
    linux_probe_mark(&mut summary, sigmask_res);

    let prlimit_res = linux_shim_invoke(
        LINUX_SYS_PRLIMIT64,
        0,
        7,
        0,
        (&mut rlimit as *mut LinuxRlimit) as u64,
        0,
        0,
    );
    linux_probe_mark(&mut summary, prlimit_res);

    let random_res = linux_shim_invoke(
        LINUX_SYS_GETRANDOM,
        rand_buf.as_mut_ptr() as u64,
        rand_buf.len() as u64,
        0,
        0,
        0,
        0,
    );
    summary.random_res = random_res;
    linux_probe_mark(&mut summary, random_res);

    let uname_res = linux_shim_invoke(
        LINUX_SYS_UNAME,
        uname_buf.as_mut_ptr() as u64,
        0,
        0,
        0,
        0,
        0,
    );
    summary.uname_res = uname_res;
    linux_probe_mark(&mut summary, uname_res);

    let mut open_path = [0u8; LINUX_PATH_MAX + 1];
    let mut open_path_len = 0usize;
    unsafe {
        let mut best_idx = None;
        let mut i = 0usize;
        while i < LINUX_MAX_RUNTIME_FILES {
            let slot = &LINUX_SHIM.runtime_files[i];
            if slot.active && slot.path_len > 0 {
                if best_idx.is_none() {
                    best_idx = Some(i);
                }
                if slot.data_ptr != 0 && slot.data_len > 0 {
                    best_idx = Some(i);
                    break;
                }
            }
            i += 1;
        }
        if let Some(idx) = best_idx {
            let slot = &LINUX_SHIM.runtime_files[idx];
            open_path_len = (slot.path_len as usize).min(LINUX_PATH_MAX);
            let mut p = 0usize;
            while p < open_path_len {
                open_path[p] = slot.path[p];
                p += 1;
            }
            open_path[open_path_len] = 0;
        }
    }

    if open_path_len > 0 {
        let openat_res = linux_shim_invoke(
            LINUX_SYS_OPENAT,
            LINUX_AT_FDCWD as u64,
            open_path.as_ptr() as u64,
            0,
            0,
            0,
            0,
        );
        summary.openat_res = openat_res;
        linux_probe_mark(&mut summary, openat_res);

        if openat_res >= 0 {
            let fd = openat_res as u64;
            let mut stat = LinuxStat64 {
                st_dev: 0,
                st_ino: 0,
                st_nlink: 0,
                st_mode: 0,
                st_uid: 0,
                st_gid: 0,
                __pad0: 0,
                st_rdev: 0,
                st_size: 0,
                st_blksize: 0,
                st_blocks: 0,
                st_atime: 0,
                st_atime_nsec: 0,
                st_mtime: 0,
                st_mtime_nsec: 0,
                st_ctime: 0,
                st_ctime_nsec: 0,
                __unused: [0; 3],
            };
            let fstat_res = linux_shim_invoke(
                LINUX_SYS_FSTAT,
                fd,
                (&mut stat as *mut LinuxStat64) as u64,
                0,
                0,
                0,
                0,
            );
            summary.fstat_res = fstat_res;
            linux_probe_mark(&mut summary, fstat_res);

            let lseek_res = linux_shim_invoke(LINUX_SYS_LSEEK, fd, 0, LINUX_SEEK_SET, 0, 0, 0);
            summary.lseek_res = lseek_res;
            linux_probe_mark(&mut summary, lseek_res);

            let read_res = linux_shim_invoke(
                LINUX_SYS_READ,
                fd,
                rand_buf.as_mut_ptr() as u64,
                16,
                0,
                0,
                0,
            );
            summary.read_res = read_res;
            linux_probe_mark(&mut summary, read_res);

            let close_res = linux_shim_invoke(LINUX_SYS_CLOSE, fd, 0, 0, 0, 0, 0);
            summary.close_res = close_res;
            linux_probe_mark(&mut summary, close_res);
        }
    } else {
        summary.openat_res = linux_neg_errno(2);
        summary.fstat_res = linux_neg_errno(2);
        summary.lseek_res = linux_neg_errno(2);
        summary.read_res = linux_neg_errno(2);
        summary.close_res = linux_neg_errno(2);
    }

    let getuid_res = linux_shim_invoke(LINUX_SYS_GETUID, 0, 0, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, getuid_res);

    let geteuid_res = linux_shim_invoke(LINUX_SYS_GETEUID, 0, 0, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, geteuid_res);

    let getgid_res = linux_shim_invoke(LINUX_SYS_GETGID, 0, 0, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, getgid_res);

    let getegid_res = linux_shim_invoke(LINUX_SYS_GETEGID, 0, 0, 0, 0, 0, 0);
    linux_probe_mark(&mut summary, getegid_res);

    summary
}

pub fn linux_shim_run_slice(call_budget: usize) -> LinuxShimSliceSummary {
    let mut summary = LinuxShimSliceSummary::empty();
    let budget = call_budget.max(1).min(256);

    if !linux_shim_active() {
        return summary;
    }

    let mut ts = LinuxTimespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut tv = LinuxTimeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut random_buf = [0u8; 16];
    let mut sigset_old: u64 = 0;
    let mut sigset_new: u64 = 0;
    let mut pending_set: u64 = 0;
    let mut parent_tid_out: u32 = 0;
    let mut child_tid_out: u32 = 0;
    let mut ruid: u32 = 0;
    let mut euid: u32 = 0;
    let mut suid: u32 = 0;
    let mut rgid: u32 = 0;
    let mut egid: u32 = 0;
    let mut sgid: u32 = 0;
    let mut poll_fds = [
        LinuxPollFd {
            fd: 0,
            events: LINUX_POLLIN,
            revents: 0,
        },
        LinuxPollFd {
            fd: 1,
            events: LINUX_POLLOUT,
            revents: 0,
        },
    ];
    let mut i = 0usize;
    while i < budget {
        let scheduled = unsafe { linux_shim_schedule_next_thread(&mut LINUX_SHIM) };
        if !scheduled {
            linux_slice_mark(&mut summary, linux_neg_errno(11)); // EAGAIN (no runnable threads)
            break;
        }
        let status_snapshot = linux_shim_status();
        let selector = unsafe { (LINUX_SHIM.syscall_count as usize).saturating_add(i) % 21 };
        let result = match selector {
            0 => linux_shim_invoke(
                LINUX_SYS_CLOCK_GETTIME,
                LINUX_CLOCK_MONOTONIC,
                (&mut ts as *mut LinuxTimespec) as u64,
                0,
                0,
                0,
                0,
            ),
            1 => linux_shim_invoke(LINUX_SYS_GETPID, 0, 0, 0, 0, 0, 0),
            2 => linux_shim_invoke(LINUX_SYS_GETTID, 0, 0, 0, 0, 0, 0),
            3 => linux_shim_invoke(LINUX_SYS_FUTEX, 0, LINUX_FUTEX_WAKE, 1, 0, 0, 0),
            4 => linux_shim_invoke(
                LINUX_SYS_GETRANDOM,
                random_buf.as_mut_ptr() as u64,
                random_buf.len() as u64,
                0,
                0,
                0,
                0,
            ),
            5 => linux_shim_invoke(LINUX_SYS_BRK, 0, 0, 0, 0, 0, 0),
            6 => linux_shim_invoke(LINUX_SYS_PRLIMIT64, 0, 7, 0, 0, 0, 0),
            7 => linux_shim_invoke(LINUX_SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0),
            8 => linux_shim_invoke(
                LINUX_SYS_NANOSLEEP,
                (&mut ts as *mut LinuxTimespec) as u64,
                0,
                0,
                0,
                0,
                0,
            ),
            9 => linux_shim_invoke(LINUX_SYS_GETPPID, 0, 0, 0, 0, 0, 0),
            10 => linux_shim_invoke(
                LINUX_SYS_POLL,
                poll_fds.as_mut_ptr() as u64,
                poll_fds.len() as u64,
                0,
                0,
                0,
                0,
            ),
            11 => linux_shim_invoke(
                LINUX_SYS_GETTIMEOFDAY,
                (&mut tv as *mut LinuxTimeval) as u64,
                0,
                0,
                0,
                0,
                0,
            ),
            12 => {
                if status_snapshot.thread_count < 4 {
                    linux_shim_invoke(
                        LINUX_SYS_CLONE,
                        LINUX_CLONE_VM
                            | LINUX_CLONE_SETTLS
                            | LINUX_CLONE_PARENT_SETTID
                            | LINUX_CLONE_CHILD_SETTID
                            | LINUX_CLONE_CHILD_CLEARTID,
                        0,
                        (&mut parent_tid_out as *mut u32) as u64,
                        (&mut child_tid_out as *mut u32) as u64,
                        0x7FFF_0000_0000u64.saturating_add((i as u64) << 12),
                        0,
                    )
                } else {
                    linux_shim_invoke(LINUX_SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0)
                }
            }
            13 => {
                let mut target_tid = 0u64;
                unsafe {
                    if let Some(tid) =
                        linux_pick_next_runnable_thread_tid(&LINUX_SHIM, status_snapshot.current_tid)
                    {
                        if tid != status_snapshot.current_tid {
                            target_tid = tid as u64;
                        }
                    }
                }
                if target_tid != 0 {
                    let pid = status_snapshot.current_pid as u64;
                    linux_shim_invoke(LINUX_SYS_TGKILL, pid, target_tid, LINUX_SIGTERM, 0, 0, 0)
                } else {
                    linux_shim_invoke(LINUX_SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0)
                }
            }
            14 => {
                sigset_new ^= 1u64 << ((LINUX_SIGTERM - 1) as u32);
                linux_shim_invoke(
                    LINUX_SYS_RT_SIGPROCMASK,
                    LINUX_SIG_SETMASK,
                    (&mut sigset_new as *mut u64) as u64,
                    (&mut sigset_old as *mut u64) as u64,
                    core::mem::size_of::<u64>() as u64,
                    0,
                    0,
                )
            }
            15 => linux_shim_invoke(
                LINUX_SYS_RT_SIGPENDING,
                (&mut pending_set as *mut u64) as u64,
                core::mem::size_of::<u64>() as u64,
                0,
                0,
                0,
                0,
            ),
            16 => linux_shim_invoke(
                LINUX_SYS_CLOCK_NANOSLEEP,
                LINUX_CLOCK_MONOTONIC,
                0,
                (&mut ts as *mut LinuxTimespec) as u64,
                0,
                0,
                0,
            ),
            17 => linux_shim_invoke(
                LINUX_SYS_GETRESUID,
                (&mut ruid as *mut u32) as u64,
                (&mut euid as *mut u32) as u64,
                (&mut suid as *mut u32) as u64,
                0,
                0,
                0,
            ),
            18 => {
                let main_tid = (status_snapshot.session_id as u32).saturating_add(2000);
                if status_snapshot.thread_count > 1 && status_snapshot.current_tid != main_tid {
                    linux_shim_invoke(LINUX_SYS_EXIT, 0, 0, 0, 0, 0, 0)
                } else {
                    linux_shim_invoke(
                        LINUX_SYS_GETRESGID,
                        (&mut rgid as *mut u32) as u64,
                        (&mut egid as *mut u32) as u64,
                        (&mut sgid as *mut u32) as u64,
                        0,
                        0,
                        0,
                    )
                }
            }
            19 => linux_shim_invoke(
                LINUX_SYS_KILL,
                0,
                0,
                0,
                0,
                0,
                0,
            ),
            _ => {
                // Selector 20: exercise clone3 with CLONE_PIDFD.
                if status_snapshot.thread_count < 4 {
                    let mut cl_args = LinuxCloneArgs::empty();
                    cl_args.flags = LINUX_CLONE_VM | LINUX_CLONE_PIDFD
                        | LINUX_CLONE_SETTLS | LINUX_CLONE_PARENT_SETTID
                        | LINUX_CLONE_CHILD_CLEARTID;
                    cl_args.exit_signal = 17; // SIGCHLD
                    cl_args.parent_tid = (&mut parent_tid_out as *mut u32) as u64;
                    cl_args.child_tid = (&mut child_tid_out as *mut u32) as u64;
                    cl_args.tls = 0x7FFF_0000_0000u64.saturating_add((i as u64) << 12);
                    linux_shim_invoke(
                        LINUX_SYS_CLONE3,
                        (&cl_args as *const LinuxCloneArgs) as u64,
                        core::mem::size_of::<LinuxCloneArgs>() as u64,
                        0, 0, 0, 0,
                    )
                } else {
                    linux_shim_invoke(LINUX_SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0)
                }
            }
        };
        linux_slice_mark(&mut summary, result);
        summary.completed_calls = summary.completed_calls.saturating_add(1);
        i += 1;

        let status = linux_shim_status();
        if !status.active || status.watchdog_triggered {
            break;
        }
    }

    let status = linux_shim_status();
    summary.active = status.active;
    summary.watchdog_triggered = status.watchdog_triggered;
    summary.exit_code = status.exit_code;
    summary.last_sysno = status.last_sysno;
    summary.last_result = status.last_result;
    summary
}

fn linux_gfx_set_status_locked(state: &mut LinuxGfxBridgeState, text: &str) {
    let src = text.as_bytes();
    let n = src.len().min(LINUX_GFX_STATUS_MAX);
    let mut i = 0usize;
    while i < n {
        state.status[i] = src[i];
        i += 1;
    }
    while i < LINUX_GFX_STATUS_MAX {
        state.status[i] = 0;
        i += 1;
    }
    state.status_len = n;
}

fn linux_gfx_bridge_present_direct_locked(state: &mut LinuxGfxBridgeState) {
    if !state.active || !state.direct_present {
        return;
    }
    let now = timer::ticks();
    if now.saturating_sub(state.direct_last_present_tick) < LINUX_GFX_DIRECT_PRESENT_MIN_TICKS {
        return;
    }
    state.direct_last_present_tick = now;

    let src_w = (state.width as usize).min(LINUX_GFX_MAX_WIDTH);
    let src_h = (state.height as usize).min(LINUX_GFX_MAX_HEIGHT);
    let count = src_w.saturating_mul(src_h).min(LINUX_GFX_MAX_PIXELS);
    if src_w == 0 || src_h == 0 || count == 0 {
        return;
    }

    let (fb_w, fb_h) = framebuffer::dimensions();
    if fb_w == 0 || fb_h == 0 {
        return;
    }

    let dst_x = fb_w.saturating_sub(src_w) / 2;
    let dst_y = fb_h.saturating_sub(src_h) / 2;
    framebuffer::rect(0, 0, fb_w, fb_h, 0x000000);
    let src = unsafe { core::slice::from_raw_parts(LINUX_GFX_PIXELS.as_ptr(), count) };
    framebuffer::blit(dst_x, dst_y, src_w, src_h, src);
    framebuffer::draw_text_5x7(8, 8, "LINUX REAL MODE (NO HOST)", 0x6FD9A8);
    framebuffer::present();
    state.dirty = false;
}

fn linux_gfx_push_event_locked(state: &mut LinuxGfxBridgeState, event: LinuxGfxInputEvent) -> bool {
    if !state.active {
        return false;
    }
    if state.event_count >= LINUX_GFX_EVENT_CAP {
        state.event_dropped = state.event_dropped.saturating_add(1);
        state.event_head = (state.event_head + 1) % LINUX_GFX_EVENT_CAP;
        if state.event_count > 0 {
            state.event_count -= 1;
        }
    }
    state.events[state.event_tail] = event;
    state.event_tail = (state.event_tail + 1) % LINUX_GFX_EVENT_CAP;
    state.event_count = state.event_count.saturating_add(1).min(LINUX_GFX_EVENT_CAP);
    state.event_seq = state.event_seq.saturating_add(1);
    state.last_input_tick = timer::ticks();
    true
}

pub fn linux_gfx_bridge_open(width: u32, height: u32) -> bool {
    unsafe {
        let w = (width as usize).clamp(64, LINUX_GFX_MAX_WIDTH);
        let h = (height as usize).clamp(64, LINUX_GFX_MAX_HEIGHT);
        let count = w.saturating_mul(h).min(LINUX_GFX_MAX_PIXELS);

        let mut i = 0usize;
        while i < count {
            LINUX_GFX_PIXELS[i] = 0x10141A;
            i += 1;
        }

        let state = &mut LINUX_GFX_BRIDGE;
        state.active = true;
        state.width = w as u32;
        state.height = h as u32;
        state.frame_seq = state.frame_seq.saturating_add(1);
        state.dirty = true;
        state.event_head = 0;
        state.event_tail = 0;
        state.event_count = 0;
        state.event_dropped = 0;
        state.event_seq = 0;
        state.last_input_tick = 0;
        state.direct_present = false;
        state.direct_last_present_tick = 0;
        linux_gfx_set_status_locked(state, "SDL/X11 bridge activo (subset).");
    }
    true
}

pub fn linux_gfx_bridge_close() {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        state.active = false;
        state.frame_seq = state.frame_seq.saturating_add(1);
        state.dirty = true;
        state.event_head = 0;
        state.event_tail = 0;
        state.event_count = 0;
        state.direct_present = false;
        state.direct_last_present_tick = 0;
        linux_gfx_set_status_locked(state, "SDL/X11 bridge detenido.");
    }
}

pub fn linux_gfx_bridge_set_status(text: &str) {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        linux_gfx_set_status_locked(state, text);
    }
}

pub fn linux_gfx_bridge_set_direct_present(enabled: bool) {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        state.direct_present = enabled && state.active;
        state.direct_last_present_tick = 0;
        if state.direct_present {
            linux_gfx_set_status_locked(state, "Linux bridge: direct-present activo (sin host).");
            linux_gfx_bridge_present_direct_locked(state);
        }
    }
}

pub fn linux_gfx_bridge_status() -> LinuxGfxBridgeStatus {
    unsafe {
        LinuxGfxBridgeStatus {
            active: LINUX_GFX_BRIDGE.active,
            width: LINUX_GFX_BRIDGE.width,
            height: LINUX_GFX_BRIDGE.height,
            frame_seq: LINUX_GFX_BRIDGE.frame_seq,
            status_len: LINUX_GFX_BRIDGE.status_len,
            status: LINUX_GFX_BRIDGE.status,
            dirty: LINUX_GFX_BRIDGE.dirty,
            event_count: LINUX_GFX_BRIDGE.event_count,
            event_dropped: LINUX_GFX_BRIDGE.event_dropped,
            event_seq: LINUX_GFX_BRIDGE.event_seq,
            last_input_tick: LINUX_GFX_BRIDGE.last_input_tick,
            direct_present: LINUX_GFX_BRIDGE.direct_present,
        }
    }
}

pub fn linux_gfx_bridge_copy_frame(dst: &mut [u32]) -> Option<(u32, u32, u64)> {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active {
            return None;
        }

        let width = (state.width as usize).min(LINUX_GFX_MAX_WIDTH);
        let height = (state.height as usize).min(LINUX_GFX_MAX_HEIGHT);
        let count = width.saturating_mul(height).min(LINUX_GFX_MAX_PIXELS);
        if count == 0 || dst.len() < count {
            return None;
        }

        let mut i = 0usize;
        while i < count {
            dst[i] = LINUX_GFX_PIXELS[i];
            i += 1;
        }
        state.dirty = false;
        Some((state.width, state.height, state.frame_seq))
    }
}

pub fn linux_gfx_bridge_push_pointer_event(x: i32, y: i32, left_down: bool, right_down: bool) -> bool {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        let event = LinuxGfxInputEvent {
            kind: 1, // pointer
            down: ((left_down as u8) & 1) | (((right_down as u8) & 1) << 1),
            x,
            y,
            code: 0,
        };
        linux_gfx_push_event_locked(state, event)
    }
}

pub fn linux_gfx_bridge_push_key_event(ch: char, down: bool) -> bool {
    let code = ch as u32;
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        let event = LinuxGfxInputEvent {
            kind: 2, // keyboard
            down: if down { 1 } else { 0 },
            x: 0,
            y: 0,
            code,
        };
        linux_gfx_push_event_locked(state, event)
    }
}

pub fn linux_gfx_bridge_pop_input_event() -> Option<LinuxGfxInputEvent> {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active || state.event_count == 0 {
            return None;
        }
        let event = state.events[state.event_head];
        state.event_head = (state.event_head + 1) % LINUX_GFX_EVENT_CAP;
        state.event_count -= 1;
        Some(event)
    }
}

pub fn linux_gfx_bridge_render_runtime(seed: u64) {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active {
            return;
        }

        let width = (state.width as usize).min(LINUX_GFX_MAX_WIDTH);
        let height = (state.height as usize).min(LINUX_GFX_MAX_HEIGHT);
        let count = width.saturating_mul(height).min(LINUX_GFX_MAX_PIXELS);
        if width == 0 || height == 0 || count == 0 {
            return;
        }

        let shim = &LINUX_SHIM;
        let mut x11_live = false;
        let mut si = 0usize;
        while si < LINUX_MAX_SOCKETS {
            let sock = &shim.sockets[si];
            if sock.active
                && sock.connected
                && sock.endpoint == LINUX_SOCKET_ENDPOINT_X11
                && sock.x11_state == LINUX_X11_STATE_READY
            {
                x11_live = true;
                break;
            }
            si += 1;
        }
        if x11_live {
            return;
        }

        let calls = shim.syscall_count as usize;
        let last_sys = shim.last_sysno as usize;
        let last_errno = shim.last_errno.max(0) as usize;
        let accent = if shim.last_errno == 0 { 0x30AA55 } else { 0xCC3344 };

        let mut y = 0usize;
        while y < height {
            let row = y.saturating_mul(width);
            let mut x = 0usize;
            while x < width {
                let idx = row.saturating_add(x);
                if idx >= count {
                    break;
                }
                let t = seed as usize;
                let r = ((x.saturating_add((t >> 1) ^ last_sys) ^ (y >> 2)) & 0x3F) as u32;
                let g = ((y.saturating_add(calls >> 2) ^ (x >> 3)) & 0x5F) as u32;
                let b = ((x ^ y ^ (last_errno << 2) ^ t) & 0x7F) as u32;
                let base = ((r + 0x10) << 16) | ((g + 0x12) << 8) | (b + 0x18);
                LINUX_GFX_PIXELS[idx] = base;
                x += 1;
            }
            y += 1;
        }

        // Progress bars from shim counters.
        let bar_w = (width / 6).max(8);
        let bar_h = (height / 3).max(16);
        let bars = [
            (shim.mmap_count.min(64) * bar_h / 64, 0x45A6FFu32),
            (shim.open_file_count.min(64) * bar_h / 64, 0xF2B632u32),
            (shim.runtime_blob_files.min(64) * bar_h / 64, 0x7D6BFFu32),
            ((shim.thread_count.min(32) * bar_h) / 32, 0x2ED573u32),
            (((last_sys & 0xFF) * bar_h) / 255, accent),
        ];
        let mut bi = 0usize;
        while bi < bars.len() {
            let (fill, color) = bars[bi];
            let x0 = 6 + bi * (bar_w + 4);
            let mut by = 0usize;
            while by < fill {
                let ypix = height.saturating_sub(6 + by);
                if ypix >= height {
                    by += 1;
                    continue;
                }
                let row = ypix.saturating_mul(width);
                let mut bx = 0usize;
                while bx < bar_w && x0 + bx < width {
                    let idx = row.saturating_add(x0 + bx);
                    if idx < count {
                        LINUX_GFX_PIXELS[idx] = color;
                    }
                    bx += 1;
                }
                by += 1;
            }
            bi += 1;
        }

        // Cursor marker from latest pointer event if present.
        if state.last_input_tick != 0 {
            let mx = ((state.last_input_tick as usize).wrapping_add(seed as usize) % width) as usize;
            let my = ((state.event_seq as usize).wrapping_add(seed as usize / 3) % height) as usize;
            let mut d = 0usize;
            while d < 10 {
                if mx + d < width {
                    let idx = my.saturating_mul(width).saturating_add(mx + d);
                    if idx < count {
                        LINUX_GFX_PIXELS[idx] = 0xFFFFFF;
                    }
                }
                if my + d < height {
                    let idx = (my + d).saturating_mul(width).saturating_add(mx);
                    if idx < count {
                        LINUX_GFX_PIXELS[idx] = 0xFFFFFF;
                    }
                }
                d += 1;
            }
        }

        state.frame_seq = state.frame_seq.saturating_add(1);
        state.dirty = true;
        linux_gfx_bridge_present_direct_locked(state);
    }
}

pub fn linux_gfx_bridge_fill_test(seed: u64) {
    unsafe {
        let state = &mut LINUX_GFX_BRIDGE;
        if !state.active {
            return;
        }

        let width = (state.width as usize).min(LINUX_GFX_MAX_WIDTH);
        let height = (state.height as usize).min(LINUX_GFX_MAX_HEIGHT);
        let count = width.saturating_mul(height).min(LINUX_GFX_MAX_PIXELS);
        if width == 0 || height == 0 || count == 0 {
            return;
        }

        let t = seed as usize;
        let mut y = 0usize;
        while y < height {
            let row = y.saturating_mul(width);
            let mut x = 0usize;
            while x < width {
                let idx = row.saturating_add(x);
                if idx >= count {
                    break;
                }
                let r = ((x.saturating_add(t)) & 0xFF) as u32;
                let g = ((y.saturating_mul(2).saturating_add(t >> 1)) & 0xFF) as u32;
                let b = (((x ^ y).saturating_add(t >> 2)) & 0xFF) as u32;
                LINUX_GFX_PIXELS[idx] = (r << 16) | (g << 8) | b;
                x += 1;
            }
            y += 1;
        }

        state.frame_seq = state.frame_seq.saturating_add(1);
        state.dirty = true;
        linux_gfx_set_status_locked(state, "SDL/X11 subset: frame actualizado.");
        linux_gfx_bridge_present_direct_locked(state);
    }
}

pub fn init() {
    unsafe {
        SYSCALL_COUNTS = [0; SYS_COUNT];
        CMD_QUEUE.reset();
        RUNTIME_STATE = RuntimeState::empty();
        linux_release_all_mmaps(&mut LINUX_SHIM);
        linux_release_all_runtime_blobs(&mut LINUX_SHIM);
        linux_shim_release_active_plan();
        ptr::write_bytes(
            (&mut LINUX_SHIM as *mut LinuxShimState) as *mut u8,
            0,
            core::mem::size_of::<LinuxShimState>(),
        );
        privilege::linux_real_slice_reset();
        LINUX_SHIM_NEXT_SESSION_ID = 1;
        LINUX_GFX_BRIDGE = LinuxGfxBridgeState::empty();
        LINUX_GFX_PIXELS = [0; LINUX_GFX_MAX_PIXELS];
    }
}

pub fn set_runtime_state(tick: u64, running: bool, irq_mode: bool) {
    unsafe {
        RUNTIME_STATE.tick = tick;
        RUNTIME_STATE.running = running;
        RUNTIME_STATE.irq_mode = irq_mode;
    }
}

pub fn enqueue_command(bytes: &[u8]) {
    unsafe {
        CMD_QUEUE.push(bytes);
    }
}

pub fn invoke(thread_index: usize, syscall_id: usize, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let ring = match process::ring_of_thread(thread_index) {
        Some(r) => r,
        None => return SYS_ERR_BAD_THREAD,
    };

    if ring != process::RingLevel::User {
        return SYS_ERR_PERMISSION;
    }

    if syscall_id >= SYS_COUNT {
        return SYS_ERR_BAD_SYSCALL;
    }

    unsafe {
        SYSCALL_COUNTS[syscall_id] = SYSCALL_COUNTS[syscall_id].saturating_add(1);
    }

    let handler = SYSCALL_TABLE[syscall_id];
    handler(thread_index, a0, a1, a2, a3)
}
