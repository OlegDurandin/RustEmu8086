//! Which processor a source file is written for, from its text (opening a file, a program saved
//! in the browser, the command line).

use rustemu_target::Arch;

/// The processor `text` is clearly written for: MIPS registers (`$t0`, `$sp`, `$4`) and `syscall`
/// against 8086 registers (`ax`, `dl`, `si`) outside comments. None when it is not clear (an empty
/// file, a few lines of data) — the file then opens in the current workspace.
pub fn detect_arch(text: &str) -> Option<Arch> {
    const MIPS_REGS: [&str; 30] = [
        "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1",
        "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "gp", "sp", "fp", "ra",
    ];
    const X86_REGS: [&str; 20] = [
        "ax", "bx", "cx", "dx", "al", "ah", "bl", "bh", "cl", "ch", "dl", "dh", "si", "di", "bp", "sp", "cs", "ds",
        "es", "ss",
    ];
    let (mut mips, mut x86) = (0usize, 0usize);
    for line in text.lines() {
        // Comments: `#` (MIPS) and `;` (8086); `#` also starts Emu8086's directives `#make_com#`.
        let line = line.split(';').next().unwrap_or("");
        if line.trim_start().starts_with("#make_") {
            x86 += 5;
            continue;
        }
        let code = line.split('#').next().unwrap_or("");
        let mut in_string = None;
        let mut word = String::new();
        let mut dollar = false;
        for c in code.chars().chain(std::iter::once(' ')) {
            if let Some(q) = in_string {
                if c == q {
                    in_string = None;
                }
                continue;
            }
            if c.is_ascii_alphanumeric() || c == '_' {
                word.push(c.to_ascii_lowercase());
                continue;
            }
            if !word.is_empty() {
                if dollar {
                    if MIPS_REGS.contains(&word.as_str()) || word.parse::<u8>().is_ok_and(|n| n < 32) {
                        mips += 1;
                    }
                } else if word == "syscall" {
                    mips += 3;
                } else if X86_REGS.contains(&word.as_str()) {
                    x86 += 1;
                }
                word.clear();
            }
            dollar = c == '$';
            if c == '\'' || c == '"' {
                in_string = Some(c);
            }
        }
    }
    if mips >= 2 && mips > 2 * x86 {
        Some(Arch::Mips32)
    } else if x86 >= 2 && x86 > 2 * mips {
        Some(Arch::I8086)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_processor() {
        let mips = ".data\nmsg: .asciiz \"ax bx\"\n.text\nmain: li $v0, 4  # print ax\n  la $a0, msg\n  syscall\n";
        assert_eq!(detect_arch(mips), Some(Arch::Mips32));
        let x86 = "org 100h ; $t0\nmov ax, 5\nmov dx, offset msg\nint 21h\nmsg db 'syscall $'\n";
        assert_eq!(detect_arch(x86), Some(Arch::I8086));
        assert_eq!(detect_arch("#make_bin#\nnop\n"), Some(Arch::I8086));
        assert_eq!(detect_arch("bits 16\nmov ax, $ - start\nmov sp, ax\n"), Some(Arch::I8086));
        assert_eq!(detect_arch("addi $8, $9, 1\nsw $8, 0($29)\n"), Some(Arch::Mips32));
        assert_eq!(detect_arch(""), None);
        assert_eq!(detect_arch("nop\n"), None);
    }
}
