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
