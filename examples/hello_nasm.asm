; NASM syntax: a COM program with sections and a local label.
; In NASM `mov dx, msg` loads the address of msg and `[count]` reads memory.
bits 16
org 100h

section .text
start:
    mov word [count], 3         ; WORD: the size must be given for memory + number
.again:
    mov dx, msg
    mov ah, 9                   ; DOS: print the $-terminated string at DS:DX
    int 21h
    dec word [count]
    jnz .again                  ; .again belongs to start: (start.again)
    mov ax, 4C00h
    int 21h

section .data
msg     db `Hello from NASM!\r\n$`  ; backquotes allow C escapes
count   dw 0
