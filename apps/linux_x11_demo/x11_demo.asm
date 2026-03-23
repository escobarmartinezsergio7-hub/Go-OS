BITS 64

global _start

%define SYS_READ        0
%define SYS_WRITE       1
%define SYS_CLOSE       3
%define SYS_POLL        7
%define SYS_SOCKET      41
%define SYS_CONNECT     42
%define SYS_EXIT        60

%define AF_UNIX         1
%define SOCK_STREAM     1
%define POLLIN          1

section .text
_start:
    ; socket(AF_UNIX, SOCK_STREAM, 0)
    mov eax, SYS_SOCKET
    mov edi, AF_UNIX
    mov esi, SOCK_STREAM
    xor edx, edx
    syscall
    test eax, eax
    js .fail
    mov r12d, eax

    ; connect("/tmp/.X11-unix/X0"), fallback X1
    mov eax, SYS_CONNECT
    mov edi, r12d
    lea rsi, [rel sockaddr_x0]
    mov edx, sockaddr_x0_len
    syscall
    test eax, eax
    jns .connected

    mov eax, SYS_CONNECT
    mov edi, r12d
    lea rsi, [rel sockaddr_x1]
    mov edx, sockaddr_x1_len
    syscall
    test eax, eax
    js .fail_close

.connected:
    ; X11 setup handshake
    mov edi, r12d
    lea rsi, [rel x11_setup]
    mov edx, x11_setup_len
    call send_all
    test eax, eax
    js .fail_close

    mov eax, SYS_READ
    mov edi, r12d
    lea rsi, [rel rx_buf]
    mov edx, 4096
    syscall
    cmp eax, 8
    jl .fail_close
    cmp byte [rel rx_buf], 1
    jne .fail_close

    ; CreateWindow
    mov edi, r12d
    lea rsi, [rel create_window_req]
    mov edx, create_window_req_len
    call send_all
    test eax, eax
    js .fail_close

    ; CreateGC
    mov edi, r12d
    lea rsi, [rel create_gc_req]
    mov edx, create_gc_req_len
    call send_all
    test eax, eax
    js .fail_close

    ; MapWindow
    mov edi, r12d
    lea rsi, [rel map_window_req]
    mov edx, map_window_req_len
    call send_all
    test eax, eax
    js .fail_close

    ; Paint full window
    mov edi, r12d
    lea rsi, [rel fill_full_req]
    mov edx, fill_full_req_len
    call send_all
    test eax, eax
    js .fail_close

    ; Prepare pollfd
    mov dword [rel pollfd], r12d
    mov word [rel pollfd + 4], POLLIN
    mov word [rel pollfd + 6], 0

.loop:
    ; poll(socket, 1, 500ms)
    mov eax, SYS_POLL
    lea rdi, [rel pollfd]
    mov esi, 1
    mov edx, 500
    syscall
    cmp eax, 0
    jl .loop
    je .animate

    ; drain pending events
    mov eax, SYS_READ
    mov edi, r12d
    lea rsi, [rel rx_buf]
    mov edx, 4096
    syscall

.animate:
    ; Toggle accent color and repaint a small bar to show live updates.
    xor byte [rel accent_toggle], 1
    cmp byte [rel accent_toggle], 0
    jne .accent_a
    mov dword [rel change_gc_color], 0x00FF7A45
    jmp .paint_accent
.accent_a:
    mov dword [rel change_gc_color], 0x005AD7FF

.paint_accent:
    mov edi, r12d
    lea rsi, [rel change_gc_req]
    mov edx, change_gc_req_len
    call send_all
    test eax, eax
    js .fail_close

    mov edi, r12d
    lea rsi, [rel fill_accent_req]
    mov edx, fill_accent_req_len
    call send_all
    test eax, eax
    js .fail_close

    jmp .loop

.fail_close:
    mov eax, SYS_CLOSE
    mov edi, r12d
    syscall

.fail:
    mov eax, SYS_EXIT
    mov edi, 1
    syscall

; send_all(fd=edi, buf=rsi, len=edx) -> eax=0 or -1
send_all:
    push rbx
    mov ebx, edi
    mov r8, rsi
    mov r9d, edx
.send_loop:
    test r9d, r9d
    jz .send_ok
    mov eax, SYS_WRITE
    mov edi, ebx
    mov rsi, r8
    mov edx, r9d
    syscall
    test eax, eax
    jle .send_err
    add r8, rax
    sub r9, rax
    jmp .send_loop
.send_ok:
    xor eax, eax
    pop rbx
    ret
.send_err:
    mov eax, -1
    pop rbx
    ret

section .data
sockaddr_x0:
    dw AF_UNIX
    db "/tmp/.X11-unix/X0", 0
sockaddr_x0_len equ $ - sockaddr_x0

sockaddr_x1:
    dw AF_UNIX
    db "/tmp/.X11-unix/X1", 0
sockaddr_x1_len equ $ - sockaddr_x1

; X11 setup request (little-endian, no auth)
x11_setup:
    db 'l', 0
    dw 11
    dw 0
    dw 0
    dw 0
    dw 0
x11_setup_len equ $ - x11_setup

; CreateWindow opcode=1, len=10 (40 bytes)
create_window_req:
    db 1, 0
    dw 10
    dd 0x01010001              ; window id
    dd 0x00000100              ; parent root
    dw 40, 40                  ; x, y
    dw 640, 360                ; width, height
    dw 0                       ; border
    dw 1                       ; InputOutput
    dd 0x00000000              ; CopyFromParent visual
    dd 0x00000802              ; CWBackPixel | CWEventMask
    dd 0x00102030              ; background pixel
    dd 0x00028000              ; Exposure | StructureNotify
create_window_req_len equ $ - create_window_req

; CreateGC opcode=55, len=6 (24 bytes)
create_gc_req:
    db 55, 0
    dw 6
    dd 0x01010002              ; gc id
    dd 0x01010001              ; drawable (window)
    dd 0x0000000C              ; GCForeground | GCBackground
    dd 0x0095E05A              ; fg
    dd 0x00101020              ; bg
create_gc_req_len equ $ - create_gc_req

; MapWindow opcode=8, len=2
map_window_req:
    db 8, 0
    dw 2
    dd 0x01010001
map_window_req_len equ $ - map_window_req

; PolyFillRectangle opcode=70, len=5
fill_full_req:
    db 70, 0
    dw 5
    dd 0x01010001              ; drawable
    dd 0x01010002              ; gc
    dw 0, 0
    dw 640, 360
fill_full_req_len equ $ - fill_full_req

; ChangeGC opcode=56, len=4
change_gc_req:
    db 56, 0
    dw 4
    dd 0x01010002              ; gc id
    dd 0x00000004              ; GCForeground
change_gc_color:
    dd 0x005AD7FF
change_gc_req_len equ $ - change_gc_req

; Accent rectangle to prove live repaint.
fill_accent_req:
    db 70, 0
    dw 5
    dd 0x01010001              ; drawable
    dd 0x01010002              ; gc
    dw 72, 72
    dw 220, 120
fill_accent_req_len equ $ - fill_accent_req

section .bss
rx_buf:         resb 4096
pollfd:         resb 8
accent_toggle:  resb 1
