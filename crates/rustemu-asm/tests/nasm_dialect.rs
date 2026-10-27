//! The NASM dialect: whole programs are compared byte for byte with `nasm -f bin` (skipped when
//! `nasm` is not installed), plus dialect detection and NASM-specific errors.

use std::process::Command;

use rustemu_asm::{Dialect, Format, assemble_str, detect_dialect};

fn nasm_installed() -> bool {
    Command::new("nasm").arg("-v").output().is_ok_and(|o| o.status.success())
}

/// Output of the real NASM for `src` (`-f bin`).
fn nasm(name: &str, src: &str) -> Vec<u8> {
    let dir = std::env::temp_dir().join(format!("rustemu_nasm_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (asm, bin) = (dir.join("p.asm"), dir.join("p.bin"));
    std::fs::write(&asm, src).unwrap();
    let out = Command::new("nasm").args(["-f", "bin", "-o"]).arg(&bin).arg(&asm).output().unwrap();
    assert!(out.status.success(), "{name}: nasm failed: {}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&bin).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    bytes
}

const PROGRAMS: &[(&str, &str)] = &[
    (
        "com_sections",
        "bits 16
org 100h
section .text
start:
    mov dx, msg
    mov ah, 9
    int 21h
    mov ax, [count]
    inc word [count]
    mov si, buffer
    mov byte [si], 'x'
    mov ax, 4C00h
    int 21h
section .data
msg     db 'Hello from NASM!', 13, 10, '$'
count   dw 7
section .bss
buffer  resb 16
words   resw 4
",
    ),
    (
        "boot_sector",
        "[bits 16]
[org 0x7c00]
start:
    xor ax, ax
    mov ds, ax
    mov si, msg
.next:
    lodsb
    or al, al
    jz .done
    mov ah, 0x0e
    int 0x10
    jmp .next
.done:
    jmp $
print_hex:
.loop:
    dec cx
    jnz .loop
    ret
msg: db `Boot!\\r\\n`, 0
times 510-($-$$) db 0
dw 0xAA55
",
    ),
    (
        "preprocessor",
        "%define CR 13
%define LF 10
%assign COUNT 3
%macro print 1
    mov dx, %1
    mov ah, 9
    int 21h
%endmacro
%macro delay 1
    mov cx, %1
%%wait:
    loop %%wait
%endmacro
org 100h
    print hello
    delay 5
    delay 0x10
%rep COUNT
    nop
%endrep
%if COUNT > 5
    hlt
%elif COUNT == 3
    cld
%else
    std
%endif
%ifdef CR
    mov al, CR
%endif
%ifndef UNDEFINED_NAME
    mov ah, LF
%endif
    ret
hello db 'hi', CR, LF, '$'
",
    ),
    (
        "expressions_and_data",
        "org 100h
    mov ax, 0x1234 | 0x00F0
    mov bx, 0xFF00 & 0x0FF0
    mov cx, 0x5555 ^ 0xFFFF
    mov dx, ~0
    mov si, 1 << 4 + 3
    mov di, 100 % 7
    mov ax, 'ab'
    mov bl, $1F
    mov bh, 0b1010
    mov cl, 17q
    mov ch, 0o17
    mov dl, 1010y
    mov ax, end - start
start:
    db 'text', 0
    dw 'ab', 'abc'
    dd 0x12345678
    times 3 db 0x90
    align 4
    dw start, end
end:
",
    ),
    (
        "memory_operands",
        "org 100h
    mov ax, [es:di]
    mov word [bx+si+4], 1234h
    mov byte [bp-2], 7
    add ax, [cs:table+2]
    lea si, [table+bx]
    push word [table]
    pop word [table+2]
    inc byte [table]
    rep movsb
    les di, [farptr]
    jmp short skip
    nop
skip:
    call near sub1
    jmp 0xF000:0x1234
    push 5
    imul ax, bx, 7
sub1:
    ret
table dw 1, 2, 3
farptr dd 0xB8000000
",
    ),
];

#[test]
fn programs_match_nasm() {
    for (name, src) in PROGRAMS {
        let p = assemble_str(src).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(p.dialect, Dialect::Nasm, "{name}");
        if nasm_installed() {
            assert_eq!(p.file_bytes(), nasm(name, src), "{name}: differs from nasm -f bin");
        }
    }
}

#[test]
fn formats_from_org() {
    assert_eq!(assemble_str("bits 16\norg 100h\nret\n").unwrap().format, Format::Com);
    assert_eq!(assemble_str("[org 0x7c00]\nhlt\ntimes 510-($-$$) db 0\ndw 0xAA55\n").unwrap().format, Format::Boot);
    let bin = assemble_str("bits 16\norg 0x500\nstart: jmp start\n").unwrap();
    assert_eq!((bin.format, bin.origin), (Format::Bin, 0x500));
}

#[test]
fn nasm_specific_errors() {
    let err = |s: &str| assemble_str(s).unwrap_err().to_string();
    assert!(err("bits 16\norg 100h\nmov [bx], 5\n").contains("operation size not specified"));
    assert!(err("bits 16\ninc [si]\n").contains("size not specified"));
    assert!(err("bits 32\n").contains("16-bit"));
    assert!(err("bits 16\nsection .weird\n").contains("only .text, .data and .bss"));
    assert!(err("%define f(x) x+1\nbits 16\n").contains("%define with parameters"));
    assert!(err("bits 16\nmov ax, nothing\n").contains("NOTHING"));
}

#[test]
fn local_labels_are_scoped() {
    // `.loop` belongs to the label before it: two different `.loop`s are fine.
    let p = assemble_str("bits 16\norg 100h\na:\n.loop: jmp .loop\nb:\n.loop: jmp .loop\n").unwrap();
    let names: Vec<&str> = p.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"A.LOOP") && names.contains(&"B.LOOP"), "{names:?}");
    assert_eq!(p.image, [0xEB, 0xFE, 0xEB, 0xFE]);
}

#[test]
fn data_labels_are_variables_for_the_debugger() {
    let p = assemble_str("bits 16\norg 100h\nmov ax, [v]\nret\nv dw 5\nbuf times 4 db 0\n").unwrap();
    let v = p.symbols.iter().find(|s| s.name == "V").unwrap();
    assert_eq!((v.kind, v.size), (rustemu_asm::SymbolKind::Variable, Some(2)));
    let b = p.symbols.iter().find(|s| s.name == "BUF").unwrap();
    assert_eq!(b.size, Some(1));
}

#[test]
fn dialect_detection() {
    let masm = [
        "org 100h\nmov ax, 5\nret\n",
        "data segment\nmsg db 'Price $$'\ndata ends\n",
        "org 100h\ntimes dw 0\n",
        "include 'emu8086.inc'\nprint 'hi'\n",
    ];
    for s in masm {
        assert_eq!(detect_dialect("a.asm", s), Dialect::Masm, "{s}");
    }
    let nasm = [
        "bits 16\norg 100h\n",
        "[org 0x7c00]\n",
        "section .text\n",
        "%define X 1\n",
        "org 100h\ntimes 10 db 0\n",
        "buf resb 10\n",
        "#nasm#\norg 100h\n",
    ];
    for s in nasm {
        assert_eq!(detect_dialect("a.asm", s), Dialect::Nasm, "{s}");
    }
    assert_eq!(detect_dialect("prog.nasm", "org 100h\n"), Dialect::Nasm);
}
