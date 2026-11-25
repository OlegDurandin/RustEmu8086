//! Expansion bombs stop with an error at the line they come from instead of growing to hundreds
//! of millions of lines (TODO item 15): nested REPT, nested `%rep`, macros that call each other
//! twice.

use rustemu_asm::assemble_str;

fn error_of(src: &str) -> (usize, String) {
    let e = assemble_str(src).expect_err("a bomb does not assemble");
    let first = &e.errors[0];
    (first.line as usize, first.msg.clone())
}

#[test]
fn expansion_bombs_stop() {
    let nested_rept = "org 100h\nrept 10000\nrept 10000\nnop\nendm\nendm\n";
    let nested_nasm = "bits 16\norg 100h\n%rep 10000\n%rep 10000\nnop\n%endrep\n%endrep\n";
    let mut doubling = String::from("org 100h\nm0 macro\nnop\nendm\n");
    for i in 1..30 {
        doubling += &format!("m{i} macro\nm{}\nm{}\nendm\n", i - 1, i - 1);
    }
    doubling += "m29\n";
    for (name, src, line) in [("REPT", nested_rept, 2), ("%rep", nested_nasm, 3), ("macros", &doubling[..], 121)] {
        let (at, msg) = error_of(src);
        assert!(msg.contains("the expansion is too large"), "{name}: {msg}");
        assert_eq!(at, line, "{name}: the error is on the outermost line");
    }
}

#[test]
fn a_full_segment_of_rept_still_assembles() {
    let p = assemble_str("org 100h\nrept 65000\ndb 0\nendm\nret\n").unwrap();
    assert_eq!(p.file_bytes().len(), 65001);
}

/// Deep or long expressions are errors, not a stack overflow that ends the process (TODO item
/// 7): parentheses, unary minus, NASM's `?:`, a chain of additions.
#[test]
fn deep_expressions_are_errors() {
    let deep = |open: &str, inner: &str, close: &str, n: usize| format!("{}{inner}{}", open.repeat(n), close.repeat(n));
    let cases = [
        format!("org 100h\nmov ax, {}\n", deep("(", "1", ")", 20_000)),
        format!("org 100h\nmov ax, {}\n", deep("-", "1", "", 20_000)),
        format!("org 100h\nmov ax, 1{}\n", "+1".repeat(200_000)),
        format!("bits 16\nmov ax, {}\n", deep("1 ? ", "1", " : 0", 20_000)),
        format!("org 100h\nx equ {}\n", deep("NOT ", "1", "", 20_000)),
    ];
    for src in &cases {
        let (line, msg) = error_of(src);
        assert_eq!(line, 2);
        assert!(msg.contains("nested too deeply") || msg.contains("too long"), "{msg}");
    }
    // Real expressions stay far below the limits.
    let p = assemble_str(&format!("org 100h\nmov ax, {}\nmov bx, 1{}\n", deep("(", "1", ")", 20), "+1".repeat(200)))
        .unwrap();
    assert_eq!(p.file_bytes()[..6], [0xB8, 1, 0, 0xBB, 0xC9, 0x00]);
}

/// Text substitutions (`%define`, text `EQU`) are rescanned, so chains of them are bounded
/// (TODO item 20): a name never expands inside its own text, nesting is limited, and so is the
/// length a line may grow to.
#[test]
fn text_substitution_bombs_stop() {
    // Each %define doubles the text of the next: 2^40 without a limit.
    let mut doubling = String::from("bits 16\norg 100h\n%define d0 1\n");
    for i in 1..40 {
        doubling += &format!("%define d{i} d{}+d{}\n", i - 1, i - 1);
    }
    doubling += "mov ax, d39\n";
    let (at, msg) = error_of(&doubling);
    assert!(msg.contains("longer than"), "{msg}");
    assert_eq!(at, 43);
    // A chain longer than the nesting limit.
    let mut chain = String::from("org 100h\nx0 equ <1>\n");
    for i in 1..100 {
        chain += &format!("x{i} equ <x{}>\n", i - 1);
    }
    chain += "mov ax, x99\nret\n";
    let (_, msg) = error_of(&chain);
    assert!(msg.contains("nested deeper than"), "{msg}");
    // Names that refer to each other stop instead of looping.
    let (_, msg) = error_of("bits 16\norg 100h\n%define a b\n%define b a\nmov ax, a\n");
    assert!(msg.contains("undefined symbol"), "{msg}");
}

/// MASM text equates refer to each other too (TODO item 20).
#[test]
fn nested_text_equates() {
    let p = assemble_str("org 100h\na equ <1>\nb equ <a+a>\nc equ <(b)*3>\nmov ax, c\nret\n").unwrap();
    assert_eq!(p.file_bytes(), [0xB8, 6, 0, 0xC3]);
}

/// `i64::MIN / -1` and `i64::MIN mod -1` overflow; they wrap like the other operators instead of
/// panicking, in the assembler and in the calculator (`evaluate`).
#[test]
fn overflowing_division_is_not_a_panic() {
    // MASM: out of word range; the NASM fallback divides unsigned (0). Either way, no panic.
    let _ = assemble_str("org 100h\nmov ax, (1 shl 63) / -1\n");
    let p = assemble_str("org 100h\nmov bx, (1 shl 63) mod -1\n").unwrap();
    assert_eq!(p.file_bytes(), [0xBB, 0, 0]);
    assert_eq!(rustemu_asm::evaluate("(1 shl 63) / -1"), Ok(i64::MIN));
    assert_eq!(rustemu_asm::evaluate("(1 shl 63) mod -1"), Ok(0));
}

/// A directive after a separator that is not ASCII (`,` then a non-breaking space) is sliced at
/// the word, not at a fixed byte offset that falls inside the character.
#[test]
fn directive_after_unicode_separator_is_not_a_panic() {
    for src in [
        "org 100h\n,\u{a0}IF 1\nnop\n,\u{a0}ENDIF\n",
        "org 100h\n,\u{a0}REPT 2\nnop\nENDM\n",
        "org 100h\n,\u{a0}INCLUDE 'missing.inc'\n",
        "org 100h\n,\u{a0}.EXIT\n",
        "org 100h\nm macro\nendm\n,\u{a0}PURGE m\n",
    ] {
        let _ = assemble_str(src);
    }
    assert_eq!(assemble_str("org 100h\n,\u{a0}IF 1\nnop\n,\u{a0}ENDIF\n").unwrap().file_bytes(), [0x90]);
    assert_eq!(assemble_str("org 100h\n,\u{a0}REPT 2\nnop\nENDM\n").unwrap().file_bytes(), [0x90, 0x90]);
}

/// TODO 64: the MZ header counts relocations in a word. More of them is an error, not a header
/// with a truncated count.
#[test]
fn too_many_exe_relocations_are_an_error() {
    let segs = |n: usize| {
        let mut s = String::from("d segment\nx dw 0\nd ends\n");
        for i in 0..n {
            s += &format!("t{i} segment\ndw 32767 dup(seg x)\nt{i} ends\n");
        }
        s + "c segment\nstart: mov ax, seg x\nc ends\nend start\n"
    };
    // 2 × 32767 + 1 = 65535: still fits.
    let p = assemble_str(&segs(2)).expect("65535 fixups assemble");
    assert_eq!(p.relocs.len(), 65535);
    let file = p.file_bytes();
    assert_eq!(u16::from_le_bytes([file[6], file[7]]), 65535);
    let (_, msg) = error_of(&segs(3));
    assert!(msg.contains("too many segment fixups"), "{msg}");
}

/// Every copy of `DUP (SEG x)` is fixed up, at its own offset.
#[test]
fn dup_of_a_segment_gets_a_fixup_per_copy() {
    let src = "d segment\nx dw 0\nd ends\nt segment\ndw 1\ndw 3 dup(seg x, 7)\nt ends\n\
               c segment\nstart: ret\nc ends\nend start\n";
    let p = assemble_str(src).expect("assembles");
    let t = p.segments.iter().find(|s| s.name.eq_ignore_ascii_case("t")).expect("segment t");
    let base = t.paragraph as u32 * 16;
    let mut at: Vec<u32> = p.relocs.iter().map(|&r| r - base).collect();
    at.sort();
    assert_eq!(at, vec![2, 6, 10]);
}
