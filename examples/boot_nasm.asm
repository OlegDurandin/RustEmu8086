; NASM syntax: a boot sector (loaded by the BIOS at 0000:7C00).
; Run it with emulate (F9); it prints a message with the BIOS and stops.
[bits 16]
[org 0x7c00]

start:
    xor ax, ax
    mov ds, ax
    mov si, msg
.next:
    lodsb                       ; AL = [DS:SI], SI + 1
    or al, al
    jz .done
    mov ah, 0x0e                ; BIOS teletype output
    int 0x10
    jmp .next
.done:
    hlt
    jmp .done

msg db 'Booted from a NASM boot sector!', 13, 10, 0

times 510-($-$$) db 0           ; pad to 510 bytes ($$ = start of the section)
dw 0xAA55                       ; boot signature
