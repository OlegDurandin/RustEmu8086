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

/// The examples of Emu8086 (its `examples` folder, as they are) and our NASM examples, by file
/// name: "more examples..." lists them all.
pub const EXAMPLES: &[(&str, &str)] = &[
    example!("0_sample_add_huge_numbers.asm"),
    example!("0_sample_vga_graphics.asm"),
    example!("1_sample.asm"),
    example!("2_sample.asm"),
    example!("3_sample.asm"),
    example!("4_sample.asm"),
    example!("5_sample.asm"),
    example!("add_two_arrays.asm"),
    example!("attrib.asm"),
    example!("bcd_aaa.asm"),
    example!("bcd_aas.asm"),
    example!("bin2dec.asm"),
    example!("bintest.asm"),
    example!("boot_nasm.asm"),
    example!("calc.asm"),
    example!("Calculator.asm"),
    example!("cmpsb.asm"),
    example!("cmpsw.asm"),
    example!("colors.asm"),
    example!("convert_string_number_to_binary.asm"),
    example!("convert_string_to_packed_bcd.asm"),
    example!("convert_to_upper_case.asm"),
    example!("count_chars.asm"),
    example!("Count_Key_Presses.asm"),
    example!("Custom_Interrupt.asm"),
    example!("datefile.asm"),
    example!("double_word.asm"),
    example!("encrypt_subrotine.asm"),
    example!("exetest.asm"),
    example!("factorial.asm"),
    example!("fahrenheit.asm"),
    example!("far_call-2.asm"),
    example!("far_call.asm"),
    example!("far_call_advanced.asm"),
    example!("fasm_compatibility.asm"),
    example!("file-operations.asm"),
    example!("float.asm"),
    example!("hello.asm"),
    example!("hello_nasm.asm"),
    example!("HelloWorld.asm"),
    example!("HexConvertor.asm"),
    example!("include.asm"),
    example!("int10_13.asm"),
    example!("int21.asm"),
    example!("keybrd.asm"),
    example!("LED_display_test.asm"),
    example!("loops.asm"),
    example!("matrix.asm"),
    example!("micro-os_kernel.asm"),
    example!("micro-os_loader.asm"),
    example!("mouse.asm"),
    example!("mouse2.asm"),
    example!("no-close.asm"),
    example!("pages.asm"),
    example!("palindrome.asm"),
    example!("param.asm"),
    example!("paramexe.asm"),
    example!("pixel.asm"),
    example!("print.asm"),
    example!("print_al.asm"),
    example!("print_AX.asm"),
    example!("print_char_by_char.asm"),
    example!("print_hex_digit.asm"),
    example!("PrinterDemo.asm"),
    example!("reverse.asm"),
    example!("robot.asm"),
    example!("scasb.asm"),
    example!("scasw.asm"),
    example!("scroll.asm"),
    example!("self_modifying_code.asm"),
    example!("simple_io.asm"),
    example!("snake.asm"),
    example!("sort.asm"),
    example!("stack.asm"),
    example!("stepper_motor.asm"),
    example!("string.asm"),
    example!("thermometer.asm"),
    example!("timer.asm"),
    example!("ToBin.asm"),
    example!("traffic_lights.asm"),
    example!("traffic_lights2.asm"),
    example!("writebin.asm"),
    example!("z01.asm"),
    example!("z02.asm"),
    example!("z03.asm"),
    example!("z04.asm"),
];

/// The Examples menu: Emu8086's items (its captions) and the files they open, then NASM.
pub const MENU: &[(&str, &str)] = &[
    ("Hello, world (.com)", "1_sample.asm"),
    ("Hello, world (.exe)", "exetest.asm"),
    ("add / subtract", "2_sample.asm"),
    ("calculate sum", "3_sample.asm"),
    ("compare numbers", "4_sample.asm"),
    ("binary, hex and octal values", "5_sample.asm"),
    ("traffic lights", "traffic_lights.asm"),
    ("palindrome", "palindrome.asm"),
    ("LED display test", "LED_display_test.asm"),
    ("stepper motor", "stepper_motor.asm"),
    ("simple i/o", "simple_io.asm"),
    ("Hello, world (NASM)", "hello_nasm.asm"),
    ("boot sector (NASM)", "boot_nasm.asm"),
];

/// Index of the example `file` in [`EXAMPLES`].
pub fn index(file: &str) -> Option<usize> {
    EXAMPLES.iter().position(|e| e.0.eq_ignore_ascii_case(file))
}

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
