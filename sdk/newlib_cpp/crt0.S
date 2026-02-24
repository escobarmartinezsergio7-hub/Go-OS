.section .text
.global _start
.type _start, @function

.extern __libc_init_array
.extern __libc_fini_array
.extern main
.extern exit

_start:
    xor %rbp, %rbp

    # Linux ABI entry stack:
    #   [rsp+0]   = argc
    #   [rsp+8]   = argv[0]
    #   ...
    #   [rsp+8*argc] = NULL
    #   [rsp+8*(argc+1)] = envp[0]
    mov (%rsp), %rdi
    lea 8(%rsp), %rsi
    lea 16(%rsp,%rdi,8), %rdx

    andq $-16, %rsp
    call __libc_init_array
    call main

    mov %eax, %edi
    call exit

1:
    jmp 1b
