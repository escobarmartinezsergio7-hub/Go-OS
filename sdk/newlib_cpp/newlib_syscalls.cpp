#include "linux_syscall.h"

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/times.h>
#include <sys/types.h>
#include <unistd.h>

namespace {
constexpr long SYS_READ = 0;
constexpr long SYS_WRITE = 1;
constexpr long SYS_CLOSE = 3;
constexpr long SYS_LSEEK = 8;
constexpr long SYS_BRK = 12;
constexpr long SYS_GETPID = 39;
constexpr long SYS_KILL = 62;
constexpr long SYS_GETTIMEOFDAY = 96;
constexpr long SYS_EXIT_GROUP = 231;
constexpr long SYS_OPENAT = 257;
constexpr long SYS_FSTATAT = 262;
constexpr long SYS_FSTAT = 5;

constexpr long AT_FDCWD = -100;
constexpr long SEEK_SET_VALUE = 0;

extern "C" char _end __attribute__((weak));
extern "C" char end __attribute__((weak));

uintptr_t align_up_16(uintptr_t value) {
    return (value + 15u) & ~static_cast<uintptr_t>(15u);
}

uintptr_t initial_heap_base() {
    uintptr_t base = reinterpret_cast<uintptr_t>(&_end);
    if (base == 0) {
        base = reinterpret_cast<uintptr_t>(&end);
    }
    if (base == 0) {
        // Fallback conservador si el linker no exporta _end/end.
        base = 0x0000'0007'2000'0000ull;
    }
    return align_up_16(base);
}

uintptr_t heap_break = 0;

int set_errno_from_ret(long rc) {
    if (rc >= 0) {
        return static_cast<int>(rc);
    }
    errno = static_cast<int>(-rc);
    return -1;
}

void fill_basic_stat(struct stat* st, int fd_hint) {
    if (!st) {
        return;
    }
    memset(st, 0, sizeof(*st));
    st->st_blksize = 4096;
    st->st_nlink = 1;
    if (fd_hint >= 0 && fd_hint <= 2) {
        st->st_mode = S_IFCHR | 0644;
    } else {
        st->st_mode = S_IFREG | 0644;
    }
}
} // namespace

extern "C" void _exit(int status) {
    (void)redux_linux_syscall1(SYS_EXIT_GROUP, static_cast<long>(status));
    for (;;) {}
}

extern "C" int _close(int fd) {
    return set_errno_from_ret(redux_linux_syscall1(SYS_CLOSE, fd));
}

extern "C" int _execve(const char*, char* const*, char* const*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _fork(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _fstat(int fd, struct stat* st) {
    if (!st) {
        errno = EFAULT;
        return -1;
    }
    long rc = redux_linux_syscall2(SYS_FSTAT, fd, reinterpret_cast<long>(st));
    if (rc >= 0) {
        return 0;
    }
    // Fallback para entornos parciales: al menos reportar algo valido.
    fill_basic_stat(st, fd);
    return 0;
}

extern "C" int _getpid(void) {
    long rc = redux_linux_syscall0(SYS_GETPID);
    if (rc >= 0) {
        return static_cast<int>(rc);
    }
    errno = static_cast<int>(-rc);
    return 1;
}

extern "C" int _isatty(int fd) {
    return (fd >= 0 && fd <= 2) ? 1 : 0;
}

extern "C" int _kill(int pid, int sig) {
    return set_errno_from_ret(redux_linux_syscall2(SYS_KILL, pid, sig));
}

extern "C" int _link(const char*, const char*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _lseek(int fd, int ptr, int dir) {
    long rc = redux_linux_syscall3(SYS_LSEEK, fd, ptr, dir);
    return set_errno_from_ret(rc);
}

extern "C" int _open(const char* path, int flags, int mode) {
    if (!path) {
        errno = EFAULT;
        return -1;
    }
    long rc = redux_linux_syscall4(
        SYS_OPENAT,
        AT_FDCWD,
        reinterpret_cast<long>(path),
        flags,
        mode
    );
    return set_errno_from_ret(rc);
}

extern "C" int _read(int fd, char* ptr, int len) {
    if (len < 0) {
        errno = EINVAL;
        return -1;
    }
    long rc = redux_linux_syscall3(SYS_READ, fd, reinterpret_cast<long>(ptr), len);
    return set_errno_from_ret(rc);
}

extern "C" void* _sbrk(ptrdiff_t increment) {
    if (heap_break == 0) {
        heap_break = initial_heap_base();
        long init_rc = redux_linux_syscall1(SYS_BRK, static_cast<long>(heap_break));
        if (init_rc < 0) {
            errno = static_cast<int>(-init_rc);
            return reinterpret_cast<void*>(-1);
        }
        heap_break = static_cast<uintptr_t>(init_rc);
    }

    uintptr_t current = heap_break;
    uintptr_t requested = current;
    if (increment >= 0) {
        uintptr_t inc = static_cast<uintptr_t>(increment);
        if (requested > (UINTPTR_MAX - inc)) {
            errno = ENOMEM;
            return reinterpret_cast<void*>(-1);
        }
        requested += inc;
    } else {
        uintptr_t dec = static_cast<uintptr_t>(-increment);
        if (dec > requested) {
            errno = ENOMEM;
            return reinterpret_cast<void*>(-1);
        }
        requested -= dec;
    }

    requested = align_up_16(requested);
    long rc = redux_linux_syscall1(SYS_BRK, static_cast<long>(requested));
    if (rc < 0) {
        errno = static_cast<int>(-rc);
        return reinterpret_cast<void*>(-1);
    }
    if (static_cast<uintptr_t>(rc) < requested) {
        errno = ENOMEM;
        return reinterpret_cast<void*>(-1);
    }

    heap_break = static_cast<uintptr_t>(rc);
    return reinterpret_cast<void*>(current);
}

extern "C" int _stat(const char* path, struct stat* st) {
    if (!path || !st) {
        errno = EFAULT;
        return -1;
    }
    long rc = redux_linux_syscall4(
        SYS_FSTATAT,
        AT_FDCWD,
        reinterpret_cast<long>(path),
        reinterpret_cast<long>(st),
        0
    );
    if (rc >= 0) {
        return 0;
    }
    fill_basic_stat(st, -1);
    return 0;
}

extern "C" int _times(struct tms*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _unlink(const char*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _wait(int*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _write(int fd, const char* ptr, int len) {
    if (len < 0) {
        errno = EINVAL;
        return -1;
    }
    long rc = redux_linux_syscall3(SYS_WRITE, fd, reinterpret_cast<long>(ptr), len);
    return set_errno_from_ret(rc);
}

extern "C" int _gettimeofday(struct timeval* tv, void* tz) {
    long rc = redux_linux_syscall2(SYS_GETTIMEOFDAY, reinterpret_cast<long>(tv), reinterpret_cast<long>(tz));
    return set_errno_from_ret(rc);
}

extern "C" int _raise(int sig) {
    return _kill(_getpid(), sig);
}

extern "C" int _system(const char*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _rename(const char*, const char*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _access(const char*, int) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _chdir(const char*) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _getcwd(char*, size_t) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _getentropy(void* buf, size_t len) {
    // En fase1 aun no hay getrandom completo para newlib host profile.
    // Devolvemos ENOSYS para que la app decida fallback.
    (void)buf;
    (void)len;
    errno = ENOSYS;
    return -1;
}

extern "C" int _mmap_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _munmap_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _nanosleep_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _sched_yield_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _dup_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _pipe_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _socket_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _connect_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _accept_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _send_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _recv_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _poll_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _ioctl_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _fcntl_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _epoll_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _eventfd_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _clock_gettime_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _futex_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _clone_stub(void) {
    errno = ENOSYS;
    return -1;
}

extern "C" int _tgkill_stub(void) {
    errno = ENOSYS;
    return -1;
}
