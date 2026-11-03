//! The assembler answers every line with code or an error, never a panic: short lines made of
//! the tokens that have broken parsers before (a directive with its operand missing, a lone
//! bracket or operator), in both dialects. A panic would crash the Mac application and freeze the
//! web page — for anyone who opens a shared link with such a line (TODO item 14).

use rustemu_asm::{Dialect, FsLoader, assemble_dialect};

const TOKENS: &[&str] = &[
    "times", "2", "nop", "db", "dw", "x:", "bits", "16", "mov", "ax", ",", "[", "]", "$", "$$", "equ", "dup", "(", ")",
    "rep", "label", "org", "%define", "%rep", "%endrep", "segment", "ends", "proc", "endp", "byte", "ptr", "offset",
    "-", "'a'", "?", "struc", "endstruc", "resb", "section", "=",
];

#[test]
fn short_lines_never_panic() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut panics = Vec::new();
    let n = TOKENS.len();
    for len in 1..=3u32 {
        for mut k in 0..n.pow(len) {
            let mut line = Vec::new();
            for _ in 0..len {
                line.push(TOKENS[k % n]);
                k /= n;
            }
            let line = line.join(" ");
            for (dialect, src) in
                [(Dialect::Nasm, format!("bits 16\n{line}\n")), (Dialect::Masm, format!("org 100h\n{line}\n"))]
            {
                let r = std::panic::catch_unwind(|| {
                    let _ = assemble_dialect("t.asm", &src, &mut FsLoader { include_dirs: Vec::new() }, Some(dialect));
                });
                if r.is_err() {
                    panics.push(format!("{dialect:?}: {line}"));
                }
            }
        }
    }
    std::panic::set_hook(hook);
    assert!(panics.is_empty(), "{} lines panic, e.g. {:?}", panics.len(), &panics[..panics.len().min(20)]);
}
