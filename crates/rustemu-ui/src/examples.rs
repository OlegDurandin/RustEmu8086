//! Example programs bundled with the application.

/// Templates for File -> New (like Emu8086's start-up templates).
pub const TEMPLATES: &[(&str, &str)] = &[
    (
        "COM program",
        "\
; COM program: one segment, loaded at offset 100h.
org 100h

        ; add your code here

        ret
",
    ),
    (
        "EXE program",
        "\
; EXE program with separate data, stack and code segments.

data segment
    ; add your data here
    pkey db \"press any key...$\"
ends

stack segment
    dw 128 dup(0)
ends

code segment
start:
    ; set segment registers:
    mov ax, data
    mov ds, ax
    mov es, ax

    ; add your code here

    lea dx, pkey
    mov ah, 9
    int 21h         ; print the message at DS:DX

    mov ah, 1       ; wait for a key
    int 21h

    mov ax, 4C00h   ; exit to the operating system
    int 21h
ends

end start           ; entry point
",
    ),
    (
        "BIN file",
        "\
; BIN file: raw binary loaded where the directives say.
#make_bin#

#LOAD_SEGMENT=0500h#
#LOAD_OFFSET=0000h#

; registers on start:
#CS=0500h#
#IP=0000h#
#DS=0500h#
#ES=0500h#
#SS=0500h#
#SP=FFFEh#

        ; add your code here

        hlt
",
    ),
    (
        "boot sector",
        "\
; Boot sector: loaded by the BIOS at 0000:7C00 (at most 512 bytes).
; DOS services (INT 21h) are not available here.
#make_boot#
org 7C00h

        ; add your code here

        int 19h         ; reboot
",
    ),
];

macro_rules! example {
    ($name:literal) => {
        ($name, include_str!(concat!("../../../examples/", $name)))
    };
}

pub const EXAMPLES: &[(&str, &str)] = &[
    example!("hello.asm"),
    example!("hello_exe.asm"),
    example!("echo.asm"),
    example!("calc.asm"),
    example!("factorial.asm"),
    example!("bubble_sort.asm"),
    example!("keyboard.asm"),
    example!("colors.asm"),
    example!("graphics.asm"),
    example!("traffic_lights.asm"),
    example!("stepper_motor.asm"),
    example!("led_display.asm"),
    example!("thermometer.asm"),
    example!("printer.asm"),
    example!("robot.asm"),
    example!("hello_nasm.asm"),
    example!("boot_nasm.asm"),
];

/// Templates for File -> New with "NASM syntax" (COM, BIN, boot sector).
pub const NASM_TEMPLATES: &[(&str, &str)] = &[
    (
        "COM program (NASM)",
        "\
; COM program in NASM syntax: loaded at offset 100h.
bits 16
org 100h

section .text
start:
        ; add your code here

        mov ax, 4C00h           ; exit to DOS
        int 21h

section .data
        ; msg db 'Hello!', 13, 10, '$'

section .bss
        ; buffer resb 16
",
    ),
    (
        "BIN file (NASM)",
        "\
; Pure binary in NASM syntax, loaded at 0100:0000 (CS = DS = 0100h).
bits 16
org 0

start:
        ; add your code here

        hlt
",
    ),
    (
        "boot sector (NASM)",
        "\
; Boot sector in NASM syntax: loaded by the BIOS at 0000:7C00.
; DOS services (INT 21h) are not available here.
bits 16
org 0x7c00

start:
        ; add your code here

        int 19h                 ; reboot

times 510-($-$$) db 0           ; pad to 510 bytes
dw 0xAA55                       ; boot signature
",
    ),
];
