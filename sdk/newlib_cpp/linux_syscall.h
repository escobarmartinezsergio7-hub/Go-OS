#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

static inline long redux_linux_syscall6(
    long n,
    long a0,
    long a1,
    long a2,
    long a3,
    long a4,
    long a5
) {
    long ret;
    register long r10 __asm__("r10") = a3;
    register long r8 __asm__("r8") = a4;
    register long r9 __asm__("r9") = a5;
    __asm__ volatile(
        "syscall"
        : "=a"(ret)
        : "a"(n), "D"(a0), "S"(a1), "d"(a2), "r"(r10), "r"(r8), "r"(r9)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static inline long redux_linux_syscall5(long n, long a0, long a1, long a2, long a3, long a4) {
    return redux_linux_syscall6(n, a0, a1, a2, a3, a4, 0);
}

static inline long redux_linux_syscall4(long n, long a0, long a1, long a2, long a3) {
    return redux_linux_syscall6(n, a0, a1, a2, a3, 0, 0);
}

static inline long redux_linux_syscall3(long n, long a0, long a1, long a2) {
    return redux_linux_syscall6(n, a0, a1, a2, 0, 0, 0);
}

static inline long redux_linux_syscall2(long n, long a0, long a1) {
    return redux_linux_syscall6(n, a0, a1, 0, 0, 0, 0);
}

static inline long redux_linux_syscall1(long n, long a0) {
    return redux_linux_syscall6(n, a0, 0, 0, 0, 0, 0);
}

static inline long redux_linux_syscall0(long n) {
    return redux_linux_syscall6(n, 0, 0, 0, 0, 0, 0);
}

#ifdef __cplusplus
}
#endif
