MAGIC        equ 0xe85250d6
ARCH         equ 0
HEADER_LEN   equ header_end - header_start
CHECKSUM     equ -(MAGIC + ARCH + HEADER_LEN)

section .multiboot
align 8
header_start:
    dd MAGIC
    dd ARCH
    dd HEADER_LEN
    dd CHECKSUM

    ; End tag (required)
    dw 0
    dw 0
    dd 8
header_end:

section .text
bits 32
global start
extern kmain

start:
    cli
    mov esp, stack_top
    call kmain

.hang:
    hlt
    jmp .hang

section .bss
align 16
stack_bottom:
    resb 16384
stack_top:
