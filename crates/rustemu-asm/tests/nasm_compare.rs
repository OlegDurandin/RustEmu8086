//! Compares instruction encodings with NASM (`cpu 8086`). Skipped when `nasm` is not installed.

use std::collections::BTreeMap;
use std::process::Command;

struct Case {
    masm: String,
    nasm: String,
}

const R8: [&str; 8] = ["al", "cl", "dl", "bl", "ah", "ch", "dh", "bh"];
const R16: [&str; 8] = ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"];
const SREG: [&str; 4] = ["es", "cs", "ss", "ds"];

/// Memory operands as (masm, nasm) without size.
fn mems() -> Vec<(String, String)> {
    let bases = ["bx", "bp", "si", "di", "bx+si", "bx+di", "bp+si", "bp+di"];
    let disps = ["", "+5", "-3", "+7Fh", "-80h", "+80h", "+1234h", "-200h"];
    let mut v = Vec::new();
    for b in bases {
        for d in disps {
            v.push((format!("[{b}{d}]"), format!("[{b}{d}]")));
        }
    }
    v.push(("[1234h]".into(), "[1234h]".into()));
    v.push(("[0]".into(), "[0]".into()));
    for s in ["es", "cs", "ss", "ds"] {
        v.push((format!("{s}:[bx+si+2]"), format!("[{s}:bx+si+2]")));
        v.push((format!("{s}:[0100h]"), format!("[{s}:0100h]")));
    }
    v
}

fn sized(m: &(String, String), w: bool) -> (String, String) {
    let (ms, ns) = if w { ("word ptr ", "word ") } else { ("byte ptr ", "byte ") };
    (format!("{ms}{}", m.0), format!("{ns}{}", m.1))
}

fn cases() -> Vec<Case> {
    let mut c = Vec::new();
    let mut add = |m: String, n: String| c.push(Case { masm: m, nasm: n });
    let both = |s: String| (s.clone(), s);
    let imm8 = ["0", "5", "7Fh", "80h", "0FFh", "-1", "-128"];
    let imm16 = ["0", "5", "-1", "1234h", "0FFFFh", "-129", "80h", "7Fh", "-128"];
    let mems = mems();

    for op in ["add", "or", "adc", "sbb", "and", "sub", "xor", "cmp", "mov", "test"] {
        for a in R8 {
            for b in R8 {
                let (m, n) = both(format!("{op} {a}, {b}"));
                add(m, n);
            }
            for i in imm8 {
                let (m, n) = both(format!("{op} {a}, {i}"));
                add(m, n);
            }
        }
        for a in R16 {
            for b in R16 {
                let (m, n) = both(format!("{op} {a}, {b}"));
                add(m, n);
            }
            for i in imm16 {
                let (m, n) = both(format!("{op} {a}, {i}"));
                add(m, n);
            }
        }
        for mm in &mems {
            for (k, r) in R8.iter().enumerate().step_by(3) {
                let _ = k;
                add(format!("{op} {}, {r}", mm.0), format!("{op} {}, {r}", mm.1));
                if op != "test" {
                    add(format!("{op} {r}, {}", mm.0), format!("{op} {r}, {}", mm.1));
                }
            }
            for r in ["ax", "bx", "si"] {
                add(format!("{op} {}, {r}", mm.0), format!("{op} {}, {r}", mm.1));
                if op != "test" {
                    add(format!("{op} {r}, {}", mm.0), format!("{op} {r}, {}", mm.1));
                }
            }
            let (b, nb) = sized(mm, false);
            let (w, nw) = sized(mm, true);
            add(format!("{op} {b}, 12h"), format!("{op} {nb}, 12h"));
            add(format!("{op} {w}, 1234h"), format!("{op} {nw}, 1234h"));
            add(format!("{op} {w}, 5"), format!("{op} {nw}, 5"));
            add(format!("{op} {w}, -2"), format!("{op} {nw}, -2"));
        }
    }
    for s in SREG {
        for r in R16 {
            add(format!("mov {r}, {s}"), format!("mov {r}, {s}"));
            if s != "cs" {
                add(format!("mov {s}, {r}"), format!("mov {s}, {r}"));
            }
        }
        for mm in mems.iter().step_by(5) {
            add(format!("mov {}, {s}", mm.0), format!("mov {}, {s}", mm.1));
            if s != "cs" {
                add(format!("mov {s}, {}", mm.0), format!("mov {s}, {}", mm.1));
            }
        }
        add(format!("push {s}"), format!("push {s}"));
        if s != "cs" {
            add(format!("pop {s}"), format!("pop {s}"));
        }
    }
    for a in R8 {
        for b in R8 {
            add(format!("xchg {a}, {b}"), format!("xchg {a}, {b}"));
        }
    }
    for a in R16 {
        for b in R16 {
            add(format!("xchg {a}, {b}"), format!("xchg {a}, {b}"));
        }
        for op in ["inc", "dec", "push", "pop", "not", "neg", "mul", "imul", "div", "idiv"] {
            add(format!("{op} {a}"), format!("{op} {a}"));
        }
        for op in ["rol", "ror", "rcl", "rcr", "shl", "sal", "shr", "sar"] {
            add(format!("{op} {a}, 1"), format!("{op} {a}, 1"));
            add(format!("{op} {a}, cl"), format!("{op} {a}, cl"));
        }
    }
    for a in R8 {
        for op in ["inc", "dec", "not", "neg", "mul", "imul", "div", "idiv"] {
            add(format!("{op} {a}"), format!("{op} {a}"));
        }
        for op in ["rol", "ror", "rcl", "rcr", "shl", "shr", "sar"] {
            add(format!("{op} {a}, 1"), format!("{op} {a}, 1"));
            add(format!("{op} {a}, cl"), format!("{op} {a}, cl"));
        }
    }
    for mm in &mems {
        for w in [false, true] {
            let (m, n) = sized(mm, w);
            for op in ["inc", "dec", "not", "neg", "mul", "imul", "div", "idiv"] {
                add(format!("{op} {m}"), format!("{op} {n}"));
            }
            for op in ["rol", "rcr", "shl", "sar"] {
                add(format!("{op} {m}, 1"), format!("{op} {n}, 1"));
                add(format!("{op} {m}, cl"), format!("{op} {n}, cl"));
            }
            add(format!("mov {m}, 7"), format!("mov {n}, 7"));
        }
        let (w, nw) = sized(mm, true);
        add(format!("push {w}"), format!("push {nw}"));
        add(format!("pop {w}"), format!("pop {nw}"));
        add(format!("call {w}"), format!("call {nw}"));
        add(format!("jmp {w}"), format!("jmp {nw}"));
        add(format!("call dword ptr {}", mm.0), format!("call far {}", mm.1));
        add(format!("jmp dword ptr {}", mm.0), format!("jmp far {}", mm.1));
        for r in ["ax", "si", "di"] {
            add(format!("lea {r}, {}", mm.0), format!("lea {r}, {}", mm.1));
            add(format!("lds {r}, {}", mm.0), format!("lds {r}, {}", mm.1));
            add(format!("les {r}, {}", mm.0), format!("les {r}, {}", mm.1));
        }
    }
    for r in R16 {
        add(format!("call {r}"), format!("call {r}"));
        add(format!("jmp {r}"), format!("jmp {r}"));
    }
    for s in [
        "aaa",
        "aas",
        "cbw",
        "cwd",
        "clc",
        "cld",
        "cli",
        "cmc",
        "daa",
        "das",
        "hlt",
        "into",
        "iret",
        "lahf",
        "sahf",
        "nop",
        "pushf",
        "popf",
        "stc",
        "std",
        "sti",
        "movsb",
        "movsw",
        "cmpsb",
        "cmpsw",
        "stosb",
        "stosw",
        "lodsb",
        "lodsw",
        "scasb",
        "scasw",
        "ret",
        "retf",
        "xlatb",
        "aam",
        "aad",
        "rep movsb",
        "repe cmpsb",
        "repne scasw",
        "rep stosw",
        "lock xchg ax, [bx]",
        "wait",
    ] {
        add(s.to_string(), s.to_string());
    }
    for n in ["0", "4", "10h", "0FFFFh"] {
        add(format!("ret {n}"), format!("ret {n}"));
        add(format!("retf {n}"), format!("retf {n}"));
    }
    for n in ["0", "10h", "21h", "0FFh"] {
        add(format!("int {n}"), format!("int {n}"));
    }
    for p in ["0", "60h", "0FFh"] {
        add(format!("in al, {p}"), format!("in al, {p}"));
        add(format!("in ax, {p}"), format!("in ax, {p}"));
        add(format!("out {p}, al"), format!("out {p}, al"));
        add(format!("out {p}, ax"), format!("out {p}, ax"));
    }
    for s in ["in al, dx", "in ax, dx", "out dx, al", "out dx, ax"] {
        add(s.into(), s.into());
    }
    add("mov al, [1234h]".into(), "mov al, [1234h]".into());
    add("mov ax, es:[1234h]".into(), "mov ax, [es:1234h]".into());
    add("mov [5], al".into(), "mov [5], al".into());
    add("mov [5], ax".into(), "mov [5], ax".into());
    add("jmp 1234h:5678h".into(), "jmp 1234h:5678h".into());
    add("call 0F000h:0".into(), "call 0F000h:0".into());
    add("aam 16".into(), "aam 16".into());
    add("aad 7".into(), "aad 7".into());
    add("movs byte ptr es:[di], es:[si]".into(), "es movsb".into());
    add("lods word ptr cs:[si]".into(), "cs lodsw".into());
    c
}

fn nasm_listing(src: &str) -> Option<BTreeMap<u32, Vec<u8>>> {
    let dir = std::env::temp_dir().join(format!("rustemu_nasm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let asm = dir.join("t.asm");
    let lst = dir.join("t.lst");
    std::fs::write(&asm, src).ok()?;
    let out = Command::new("nasm")
        .args(["-f", "bin", "-o"])
        .arg(dir.join("t.bin"))
        .arg("-l")
        .arg(&lst)
        .arg(&asm)
        .output()
        .ok()?;
    if !out.status.success() {
        panic!("nasm failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let text = std::fs::read_to_string(&lst).ok()?;
    let mut map: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for l in text.lines() {
        let mut it = l.split_whitespace();
        let Some(n) = it.next().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Some(_addr) = it.next().filter(|a| a.len() == 8 && u32::from_str_radix(a, 16).is_ok()) else {
            continue;
        };
        let Some(hex) = it.next() else { continue };
        let hex = hex.trim_end_matches('-');
        if hex.len() % 2 != 0 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap());
        map.entry(n).or_default().extend(bytes);
    }
    std::fs::remove_dir_all(&dir).ok();
    Some(map)
}

#[test]
fn encodings_match_nasm() {
    if Command::new("nasm").arg("-v").output().is_err() {
        eprintln!("nasm not found, skipping");
        return;
    }
    let cases = cases();
    let header = "cpu 8086\nbits 16\norg 100h\n";
    let nasm_src: String = header.to_string() + &cases.iter().map(|c| c.nasm.clone() + "\n").collect::<String>();
    let ours_src: String = "org 100h\n".to_string() + &cases.iter().map(|c| c.masm.clone() + "\n").collect::<String>();
    let nasm = nasm_listing(&nasm_src).expect("nasm listing");
    let prog = rustemu_asm::assemble_str(&ours_src).unwrap_or_else(|e| panic!("{e}"));
    let ours: BTreeMap<u32, Vec<u8>> = prog.listing.iter().map(|l| (l.line, l.bytes.clone())).collect();
    let mut fails = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let want = nasm.get(&(i as u32 + 4)).cloned().unwrap_or_default();
        let got = ours.get(&(i as u32 + 2)).cloned().unwrap_or_default();
        if want != got {
            fails.push(format!("{:<36} ours {:02X?} nasm {:02X?}", c.masm, got, want));
        }
    }
    println!("{} cases, {} mismatches", cases.len(), fails.len());
    for f in fails.iter().take(40) {
        println!("{f}");
    }
    assert!(fails.is_empty());
}
