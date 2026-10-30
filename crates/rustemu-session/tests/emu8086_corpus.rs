//! Compatibility corpus: the examples shipped with Emu8086 (the repository's `examples`, as they
//! are in Emu8086's `examples` folder).
//! Run: `cargo test -p rustemu-session --test emu8086_corpus -- --nocapture`

use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Our emu8086.inc (`include 'emu8086.inc'`).
fn inc_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../rustemu-asm/inc")
}

/// Examples that need FASM syntax (`#fasm#`), which is not supported.
const UNSUPPORTED: &[&str] = &["fasm_compatibility.asm"];

#[test]
fn all_examples_assemble() {
    let mut files: Vec<_> = std::fs::read_dir(examples_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("asm")))
        .collect();
    files.sort();
    let mut failed = Vec::new();
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        if UNSUPPORTED.contains(&name.as_str()) {
            continue;
        }
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned();
        let mut loader = rustemu_asm::FsLoader { include_dirs: vec![inc_dir()] };
        if let Err(e) = rustemu_asm::assemble(&f.to_string_lossy(), &text, &mut loader) {
            let first: Vec<String> = e.errors.iter().take(3).map(|x| format!("({}) {}", x.line, x.msg)).collect();
            failed.push(format!("{name}: {}", first.join(" | ")));
        }
    }
    println!("{} examples, {} failed to assemble", files.len(), failed.len());
    for f in &failed {
        println!("  {f}");
    }
    assert!(failed.is_empty());
}

/// Runs every example with generic keyboard input and reports how it ended.
/// Fails on emulator errors and on BIOS/DOS services that are not implemented.
#[test]
fn all_examples_run() {
    use rustemu_core::Event;
    use rustemu_session::Session;
    let mut files: Vec<_> = std::fs::read_dir(examples_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("asm")))
        .collect();
    files.sort();
    let sandbox = std::env::temp_dir().join(format!("rustemu_corpus_{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).unwrap();
    let mut problems = Vec::new();
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        if UNSUPPORTED.contains(&name.as_str()) {
            continue;
        }
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned();
        let mut loader = rustemu_asm::FsLoader { include_dirs: vec![inc_dir()] };
        let Ok(prog) = rustemu_asm::assemble(&f.to_string_lossy(), &text, &mut loader) else { continue };
        let mut s = Session::new();
        s.machine.instant_waits = true;
        s.machine.instructions_per_tick = Some(2000);
        s.machine.history_enabled = false;
        s.machine.set_dos_root(sandbox.clone());
        if let Err(e) = s.load_program(prog) {
            problems.push(format!("{name}: load error {e}"));
            continue;
        }
        // Generic input: numbers, an operator, Enter, Esc and plenty of "any key" presses.
        let mut keys: Vec<u8> = b"12\r+\r30\rtest.txt\r\x1b".to_vec();
        keys.extend(std::iter::repeat_n(b'\r', 40));
        keys.push(0x1b);
        let mut outcome = String::new();
        for _ in 0..200 {
            match s.machine.run(2_000_000) {
                Event::WaitingInput if !keys.is_empty() => s.machine.push_char(keys.remove(0)),
                Event::WaitingInput => {
                    outcome = "waits for input".into();
                    break;
                }
                Event::Sleeping => {}
                Event::Budget => {
                    outcome = "still running".into();
                    break;
                }
                Event::Terminated(c) => {
                    outcome = format!("terminated ({c})");
                    break;
                }
                Event::Halted => {
                    outcome = "halted".into();
                    break;
                }
                Event::Breakpoint => {}
                Event::Error(e) => {
                    outcome = format!("ERROR: {e}");
                    problems.push(format!("{name}: {e} at {:04X}:{:04X}", s.machine.cpu.cs(), s.machine.cpu.ip));
                    break;
                }
                Event::Continue => {}
            }
        }
        for l in &s.machine.log {
            problems.push(format!("{name}: {l}"));
        }
        let out: String =
            String::from_utf8_lossy(&s.machine.output).chars().filter(|c| !c.is_control()).take(50).collect();
        println!("{name:<36} {outcome:<18} {out}");
    }
    std::fs::remove_dir_all(&sandbox).ok();
    println!("--- problems:");
    for p in &problems {
        println!("  {p}");
    }
    assert!(problems.is_empty());
}

/// micro-os: the boot loader reads the kernel from the virtual floppy with INT 13h.
#[test]
fn micro_os_boots_from_floppy() {
    use rustemu_core::Event;
    use rustemu_session::Session;
    let build = |name: &str| {
        let path = examples_dir().join(name);
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        let mut loader = rustemu_asm::FsLoader { include_dirs: vec![inc_dir()] };
        rustemu_asm::assemble(&path.to_string_lossy(), &text, &mut loader).unwrap()
    };
    let boot = build("micro-os_loader.asm");
    let kernel = build("micro-os_kernel.asm");
    let mut s = Session::new();
    s.machine.floppies[0] = vec![0; 1_474_560];
    s.machine.floppies[0][..boot.image.len()].copy_from_slice(&boot.image);
    s.machine.floppies[0][512..512 + kernel.image.len()].copy_from_slice(&kernel.image);
    s.boot_from_floppy(0).unwrap();
    assert_eq!(s.machine.run(5_000_000), Event::WaitingInput);
    let screen = s.machine.screen_text();
    assert!(screen.contains("Welcome to micro-os!"), "{screen}");
    for c in b"help\r" {
        s.machine.push_char(*c);
        s.machine.run(1_000_000);
    }
    assert!(s.machine.screen_text().contains("short list of supported commands"));
}
