use rustemu_asm::{Format, assemble_str};

fn bytes(src: &str) -> Vec<u8> {
    assemble_str(src).unwrap_or_else(|e| panic!("{e}")).image
}

fn err(src: &str) -> String {
    match assemble_str(src) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn com_hello() {
    let p = assemble_str(
        "org 100h\n\
         mov dx, offset msg\n\
         mov ah, 9\n\
         int 21h\n\
         ret\n\
         msg db 'Hi$'\n",
    )
    .unwrap();
    assert_eq!(p.format, Format::Com);
    assert_eq!(p.image, [0xBA, 0x08, 0x01, 0xB4, 0x09, 0xCD, 0x21, 0xC3, b'H', b'i', b'$']);
    assert_eq!(p.entry, (0, 0x100));
}

#[test]
fn variables_are_memory() {
    // mov al, var -> A0 disp16 ; lea dx, var -> 8D 16 ; mov var, 5 -> C6 06 ...
    let b = bytes("org 100h\nmov al, v\nlea dx, v\nmov v, 5\nmov ax, w1\nmov w1[bx], 3\nret\nv db 1\nw1 dw 2\n");
    let v = 0x100 + 3 + 4 + 5 + 3 + 6 + 1;
    let w = v + 1;
    assert_eq!(
        b[..22],
        [
            0xA0,
            v as u8,
            (v >> 8) as u8, // mov al, v
            0x8D,
            0x16,
            v as u8,
            (v >> 8) as u8, // lea dx, v
            0xC6,
            0x06,
            v as u8,
            (v >> 8) as u8,
            5, // mov v, 5
            0xA1,
            w as u8,
            (w >> 8) as u8, // mov ax, w1
            0xC7,
            0x87,
            w as u8,
            (w >> 8) as u8,
            3,
            0, // mov w1[bx], 3
            0xC3
        ]
    );
}

#[test]
fn operand_sizes_follow_emu8086() {
    // Untyped memory with an immediate: byte when the value fits in 8 bits, word otherwise.
    assert_eq!(
        bytes("org 100h\nmov [bx], 1\nmov [bx], 1234h\ncmp [si], 0\n"),
        [0xC6, 0x07, 1, 0xC7, 0x07, 0x34, 0x12, 0x80, 0x3C, 0]
    );
    // The register decides over a variable's type or PTR.
    let b = bytes("org 100h\nmov ax, v\nmov w. v, al\nret\nv db 1, 2\n");
    assert_eq!(&b[..3], &[0xA1, 0x07, 0x01]);
    assert_eq!(&b[3..6], &[0xA2, 0x07, 0x01]);
    // Still errors:
    assert!(err("org 100h\nmov ax, bl\n").contains("size mismatch"));
    assert!(err("org 100h\nmov al, 300\n").contains("out of byte range"));
    assert!(err("org 100h\nmov ax, nothing\n").contains("undefined symbol"));
    assert!(err("org 100h\ninc [bx]\n").contains("operand size"));
    // b./w. shorthand
    assert_eq!(bytes("org 100h\nmov b.[bx], 1\nmov w.[bx], 1\n"), [0xC6, 0x07, 1, 0xC7, 0x07, 1, 0]);
}

#[test]
fn emu8086_syntax_quirks() {
    // `msg:` with a colon is an address label: MOV DX, msg loads the offset.
    assert_eq!(&bytes("org 100h\nmov dx, msg\nret\nmsg: db 'x$'\n")[..3], &[0xBA, 0x04, 0x01]);
    // SIZE/LOW/HIGH can be names; << >> shifts; trailing comma; label colon in a jump.
    let b = bytes("org 100h\nsize = 3 << 2\nmov cx, size\njl low\nlow: nop\ndb 1, 2,\njmp low:\n");
    assert_eq!(&b[..3], &[0xB9, 12, 0]);
    // Text after END is ignored.
    assert_eq!(bytes("org 100h\nnop\nend\nthis is not code, at all\n"), [0x90]);
    // Text equates.
    assert_eq!(bytes("org 100h\nnl equ 0Dh, 0Ah, '$'\ndb 'a', nl\n"), [b'a', 13, 10, b'$']);
    // A far JCXZ becomes OR CX,CX + JZ.
    let b = bytes("org 100h\njcxz far_away\ndb 300 dup(0)\nfar_away: ret\n");
    assert_eq!(&b[..5], &[0x0B, 0xC9, 0x75, 0x03, 0xE9]);
    // 80186: PUSHA / POPA / PUSH imm / two- and three-operand IMUL imm.
    assert_eq!(
        bytes("org 100h\npusha\npopa\npush 5\npush 1234h\nimul si, bx, 7\nimul ax, 1234h\n"),
        [0x60, 0x61, 0x6A, 5, 0x68, 0x34, 0x12, 0x6B, 0xF3, 7, 0x69, 0xC0, 0x34, 0x12]
    );
    // ORG 100h with .data/.code is a COM file with the code first.
    let p = assemble_str("org 100h\n.data\nm db 'hi$'\n.code\nmov ax, @data\nmov dx, offset m\nret\n").unwrap();
    assert_eq!(p.format, Format::Com);
    assert_eq!(p.image, [0xB8, 0, 0, 0xBA, 0x07, 0x01, 0xC3, b'h', b'i', b'$']);
    assert_eq!(p.relocs, vec![1]);
}

#[test]
fn jumps_short_near_and_inverted() {
    // Backward short loop.
    let b = bytes("org 100h\nl1: dec cx\njnz l1\njmp l1\n");
    assert_eq!(b, [0x49, 0x75, 0xFD, 0xEB, 0xFB]);
    // Forward short jump resolved in later passes.
    let b = bytes("org 100h\njmp fwd\nnop\nfwd: ret\n");
    assert_eq!(b, [0xEB, 0x01, 0x90, 0xC3]);
    // Conditional jump out of range -> JNZ +3 / JMP near.
    let src = "org 100h\njz far_away\ndb 200 dup(0)\nfar_away: ret\n";
    let b = bytes(src);
    assert_eq!(&b[..5], &[0x75, 0x03, 0xE9, 200, 0]);
    // LOOP out of range is an error.
    assert!(err("org 100h\nl:\ndb 200 dup(0)\nloop l\n").contains("out of range"));
    // CALL / near JMP forms.
    let b = bytes("org 100h\ncall p\njmp near ptr p\np: ret\n");
    assert_eq!(b, [0xE8, 0x03, 0x00, 0xE9, 0x00, 0x00, 0xC3]);
}

#[test]
fn equ_dup_and_expressions() {
    let b = bytes(
        "org 100h\n\
         N equ 4\n\
         X = N * 2 + 1\n\
         mov cx, N\n\
         mov ax, X\n\
         mov bx, len\n\
         msg db 'abc', 0\n\
         len equ $ - msg\n\
         arr dw N dup(0FFFFh)\n\
         db 2 dup(1, 2 dup(3))\n\
         dd 12345678h\n\
         dw 'AB'\n",
    );
    assert_eq!(&b[..9], &[0xB9, 4, 0, 0xB8, 9, 0, 0xBB, 4, 0]);
    assert_eq!(&b[9..13], b"abc\0");
    assert_eq!(&b[13..21], &[0xFF; 8]);
    assert_eq!(&b[21..27], &[1, 3, 3, 1, 3, 3]);
    assert_eq!(&b[27..31], &[0x78, 0x56, 0x34, 0x12]);
    assert_eq!(&b[31..33], &[0x42, 0x41]);
}

#[test]
fn exe_segments_and_relocations() {
    let src = "\
data segment
    msg db 'Hello$'
ends
stack segment
    dw 64 dup(0)
ends
code segment
start:
    mov ax, data
    mov ds, ax
    lea dx, msg
    mov ah, 9
    int 21h
    mov ax, 4C00h
    int 21h
ends
end start
";
    let p = assemble_str(src).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(p.format, Format::Exe);
    assert_eq!(p.segments.len(), 3);
    assert_eq!(p.segments[0].paragraph, 0);
    assert_eq!(p.segments[1].paragraph, 1);
    assert_eq!(p.segments[2].paragraph, 1 + 8);
    assert_eq!(p.entry, (9, 0));
    assert_eq!(p.stack, (1, 128));
    // mov ax, data -> B8 0000 with a relocation at the immediate.
    let code = 9 * 16;
    assert_eq!(&p.image[code..code + 3], &[0xB8, 0, 0]);
    assert_eq!(p.relocs, vec![code as u32 + 1]);
    let f = p.file_bytes();
    assert_eq!(&f[..2], b"MZ");
    assert_eq!(u16::from_le_bytes([f[6], f[7]]), 1);
}

#[test]
fn simplified_segments() {
    let src = ".model small\n.stack 100h\n.data\nm db 'x$'\n.code\nmain proc\nmov ax, @data\nmov ds, ax\nmov ah, 4ch\nint 21h\nmain endp\nend main\n";
    let p = assemble_str(src).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(p.format, Format::Exe);
    assert_eq!(p.relocs.len(), 1);
    assert!(p.stack.1 == 0x100);
}

#[test]
fn far_procs_and_calls() {
    let src = "\
code segment
start:
    call far_p
    call far ptr near_p
    mov ax, 4C00h
    int 21h
far_p proc far
    ret
far_p endp
near_p proc
    ret
near_p endp
ends
other segment
o: retf
ends
end start
";
    let p = assemble_str(src).unwrap_or_else(|e| panic!("{e}"));
    // call far_p -> 9A off seg
    assert_eq!(&p.image[0..5], &[0x9A, 15, 0, 0, 0]);
    // retf inside far proc
    assert_eq!(p.image[15], 0xCB);
    assert_eq!(p.image[16], 0xC3);
    assert_eq!(p.relocs.len(), 2);
}

#[test]
fn macros_and_include() {
    let src = "\
include 'emu8086.inc'
org 100h
mymac macro a, b
    mov a, b
endm
mymac ax, 5
lbl: mymac bl, 'x'
ret
";
    let b = bytes(src);
    assert_eq!(b, [0xB8, 5, 0, 0xB3, b'x', 0xC3]);
}

#[test]
fn listing_and_line_map() {
    let p = assemble_str("org 100h\nstart:\n  mov ax, 1\n  nop\n").unwrap();
    assert_eq!(p.line_at(0).map(|l| l.line), Some(3));
    assert_eq!(p.line_at(3).map(|l| l.line), Some(4));
    let lst = p.listing_text();
    assert!(lst.contains("0000:0100  B8 01 00"), "{lst}");
    assert!(p.symbols_text().contains("START"));
}

#[test]
fn emu_directives_and_formats() {
    let p = assemble_str("#make_bin#\n#LOAD_SEGMENT=0500h#\n#AX=1234h#\nmov ax, 1\nhlt\n").unwrap();
    assert_eq!(p.format, Format::Bin);
    assert_eq!(p.emu.load_segment, Some(0x500));
    assert_eq!(p.emu.regs, vec![("AX".to_string(), 0x1234)]);
    let p = assemble_str("#make_boot#\norg 7c00h\njmp $\n").unwrap();
    assert_eq!(p.format, Format::Boot);
    assert_eq!(p.image, [0xEB, 0xFE]);
    assert_eq!(p.file_bytes().len(), 512);
}

#[test]
fn shifts_by_immediate_expand() {
    assert_eq!(bytes("org 100h\nshl ax, 3\n"), [0xD1, 0xE0, 0xD1, 0xE0, 0xD1, 0xE0]);
}

#[test]
fn error_has_line_number() {
    let e = assemble_str("org 100h\nnop\nfoo ax\n").unwrap_err();
    assert_eq!(e.errors[0].line, 3);
}

#[test]
fn masm6_startup_and_exit() {
    let src = ".model small\n.stack 100h\n.data\nm db 'x$'\n.code\n    nop\n.startup\n    lea dx, m\n    mov ah, 9\n    int 21h\n.exit 3\nend\n";
    let p = assemble_str(src).unwrap_or_else(|e| panic!("{e}"));
    let code = p.segments.iter().find(|s| s.name == "CODE").unwrap();
    // Entry is at .startup (after the NOP).
    assert_eq!(p.entry, (code.paragraph, 1));
    let c = code.paragraph as usize * 16;
    let data = p.segments.iter().find(|s| s.name == "DATA").unwrap().paragraph;
    assert_eq!(&p.image[c + 1..c + 6], &[0xB8, data as u8, (data >> 8) as u8, 0x8E, 0xD8]);
    assert!(p.image[c..].windows(5).any(|w| w == [0xB8, 0x03, 0x4C, 0xCD, 0x21]));
}
