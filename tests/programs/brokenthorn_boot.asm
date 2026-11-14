; Lesson 03: our first BIOS boot sector.
; nasm -f bin -o build/boot.bin src/boot.asm
;
; This is NOT a FAT12 BPB yet. We only demonstrate the BIOS boot sequence.

bits 16
org 0x7C00

    ; BIOS may enter at 0000:7C00 or 07C0:0000. Normalize CS to zero.
    jmp 0x0000:start

start:
    ; Set known real-mode data segments and a safe initial stack.
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    cld

    mov si, message

.next_char:
    lodsb                       ; AL = byte [DS:SI], SI++
    test al, al
    jz .stop

    ; Emulator-only debug console: visible with `make run-headless`.
    mov dx, 0x00E9
    out dx, al

    ; BIOS text output: visible with `make run`.
    push si
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    pop si
    jmp .next_char

.stop:
    cli
    hlt
    jmp .stop

message: db 'BrokenThorn Modern: boot sector OK', 13, 10, 0

times 510 - ($ - $$) db 0      ; Pad to byte 510.
dw 0xAA55                      ; Little-endian bytes: 55 AA.
