//! `rustemu` - command line front end: assemble, run headless, disassemble; 8086 programs and
//! MIPS32 ones (MARS syntax), chosen by `--arch`, the file extension (`.s`) or the source.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustemu_core::Event;
use rustemu_core::disasm::disassemble;
use rustemu_session::{Arch, Session};

const USAGE: &str = "\
usage:
  rustemu asm <file> [-o <out>] [-l] [-s]       assemble (-l listing, -s symbols)
      --syntax <s>       8086: nasm or masm (Emu8086); detected from the source by default
      -o <out>           MIPS: write the text segment as binary (MARS's dump .text)
  rustemu run <file> [options]                  run .asm/.s/.com/.exe/.bin/.boot headless
      --input <text>     keyboard input (\\n = Enter); otherwise stdin is used
      --args <text>      DOS command line parameters (PSP:80h)
      --max <n>          instruction limit (default 100000000)
      --regs             print the registers at the end
      --screen           print the final text screen instead of the output stream (8086)
      --trace            print every executed instruction
      --timer-irq        deliver IRQ0 (INT 08h/1Ch) 18.2 times a second (8086)
  rustemu disasm <file> [--org <hex>]           disassemble a binary
      --mars             MIPS: MARS's basic notation (register numbers, branch offsets)

  --arch <a>             8086 or mips; by default .s files and sources written for MIPS
                         (registers like $t0, syscall) are MIPS32 programs (MARS)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("asm") => cmd_asm(&args[1..]),
        Some("run") => cmd_run(&args[1..]),
        Some("disasm") => cmd_disasm(&args[1..]),
        _ => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn file_arg(args: &[String]) -> Result<PathBuf, String> {
    args.first().filter(|a| !a.starts_with("--")).map(PathBuf::from).ok_or_else(|| USAGE.to_string())
}

/// The processor of a file: `--arch`, else MIPS for `.s` and for sources written for MIPS, else
/// the 8086 (binaries too, unless `--arch mips`).
fn arch_of(path: &Path, text: Option<&str>, args: &[String]) -> Result<Arch, String> {
    if let Some(a) = opt(args, "--arch") {
        return match a.to_ascii_lowercase().as_str() {
            "8086" | "x86" | "i8086" => Ok(Arch::I8086),
            "mips" | "mips32" => Ok(Arch::Mips32),
            _ => Err(format!("unknown processor '{a}' (8086 or mips)")),
        };
    }
    if is_ext(path, "s") {
        return Ok(Arch::Mips32);
    }
    Ok(text.and_then(rustemu_session::detect_arch).unwrap_or(Arch::I8086))
}

/// The text of a source file (None for a binary).
fn source_text(path: &Path) -> Result<Option<String>, String> {
    let binary = ["com", "exe", "bin", "boot"].iter().any(|e| is_ext(path, e));
    if binary {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Assembles MIPS source `text` of file `path`, its `.include` files read from disk; the errors as
/// `file: line L column C: message`, warnings printed.
fn assemble_mips(path: &Path, text: &str) -> Result<rustemu_mips::asm::Program, String> {
    let name = path.display().to_string();
    let mut read = |p: &str| std::fs::read(p).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
    let prog = rustemu_mips::asm::assemble_with(&name, text, &mut read).map_err(|e| e.to_string())?;
    for w in &prog.warnings {
        eprintln!("{}: warning: {w}", prog.files[w.file].name);
    }
    Ok(prog)
}

fn cmd_asm_mips(path: &Path, text: &str, args: &[String]) -> Result<u8, String> {
    let prog = assemble_mips(path, text)?;
    let words: usize = prog.statements.iter().map(|s| s.words.len()).sum();
    let data: u32 = prog.data.iter().map(|d| d.len).sum();
    println!("{} (MIPS32): {words} instructions, {data} bytes of data", path.display());
    if let Some(out) = opt(args, "-o") {
        let bytes = prog.text_bytes();
        std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
        println!("text segment -> {out} ({} bytes from 0x00400000)", bytes.len());
    }
    if flag(args, "-l") {
        let lst = path.with_extension("lst");
        std::fs::write(&lst, prog.listing()).map_err(|e| e.to_string())?;
        println!("listing: {}", lst.display());
    }
    if flag(args, "-s") {
        let sym = path.with_extension("symbol");
        std::fs::write(&sym, prog.symbols_text()).map_err(|e| e.to_string())?;
        println!("symbols: {}", sym.display());
    }
    Ok(0)
}

fn cmd_asm(args: &[String]) -> Result<u8, String> {
    let path = file_arg(args)?;
    if let Some(text) = source_text(&path)?
        && arch_of(&path, Some(&text), args)? == Arch::Mips32
    {
        return cmd_asm_mips(&path, &text, args);
    }
    let dialect = match opt(args, "--syntax").map(str::to_ascii_lowercase).as_deref() {
        None => None,
        Some("nasm") => Some(rustemu_asm::Dialect::Nasm),
        Some("masm" | "emu8086") => Some(rustemu_asm::Dialect::Masm),
        Some(s) => return Err(format!("unknown syntax '{s}' (nasm or masm)")),
    };
    let text =
        String::from_utf8_lossy(&std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?).into_owned();
    let mut loader = rustemu_asm::FsLoader { include_dirs: Vec::new() };
    let prog = rustemu_asm::assemble_dialect(&path.to_string_lossy(), &text, &mut loader, dialect)
        .map_err(|e| e.to_string())?;
    let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| path.with_extension(prog.format.extension()));
    std::fs::write(&out, prog.file_bytes()).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "{} -> {} ({} syntax, {} format, {} bytes)",
        path.display(),
        out.display(),
        prog.dialect.name(),
        prog.format.name(),
        prog.file_bytes().len()
    );
    if flag(args, "-l") {
        let lst = path.with_extension("lst");
        std::fs::write(&lst, prog.listing_text()).map_err(|e| e.to_string())?;
        println!("listing: {}", lst.display());
    }
    if flag(args, "-s") {
        let sym = path.with_extension("symbol");
        std::fs::write(&sym, prog.symbols_text()).map_err(|e| e.to_string())?;
        println!("symbols: {}", sym.display());
    }
    Ok(0)
}

fn cmd_run(args: &[String]) -> Result<u8, String> {
    let path = file_arg(args)?;
    let text = source_text(&path)?;
    let mips = arch_of(&path, text.as_deref(), args)? == Arch::Mips32;
    let mut s = if mips { Session::new_mips() } else { Session::new() };
    if mips {
        // MIPS file services work in the folder of the program, as DOS's drive C: does here.
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
        if let Some(m) = s.mips_mut() {
            m.files.storage = rustemu_mips::files::Storage::Host(dir);
        }
        let text = text.ok_or("a MIPS program is run from its source (.s / .asm)")?;
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let prog = assemble_mips(&path, &text)?;
        s.load_mips(prog, &name)?;
    } else {
        s.command_line = opt(args, "--args").unwrap_or("").to_string();
        s.load_path(&path)?;
    }
    // The run loop goes through the generic target; screen, log, printer, timer and the trace
    // format are the 8086's.
    let timer_irq = flag(args, "--timer-irq");
    if let Some(m) = s.x86_mut() {
        m.timer_irq = timer_irq;
    }
    let max: u64 = opt(args, "--max").and_then(|v| v.parse().ok()).unwrap_or(100_000_000);
    let screen = flag(args, "--screen");
    let trace = flag(args, "--trace");
    // Enter is CR for DOS, a new line for MIPS's console.
    let enter = if mips { "\n" } else { "\r" };
    let mut input: Option<Vec<u8>> = opt(args, "--input").map(|t| t.replace("\\n", enter).into_bytes());
    let mut stdout = std::io::stdout();
    let mut printed = 0;
    let mut executed = 0u64;
    let code = loop {
        let ev = if trace && let Some(m) = s.mips() {
            let pc = m.cpu.pc;
            let word = u32::from_le_bytes(s.target.read_bytes(pc, 4).try_into().unwrap_or([0; 4]));
            let ins = rustemu_mips::disasm::disassemble(word, pc, rustemu_mips::disasm::Style::Named);
            let r = &m.cpu.regs;
            eprintln!(
                "{pc:08x}  {word:08x}  {ins:<28} v0={:08x} a0={:08x} t0={:08x} t1={:08x} sp={:08x}",
                r[2], r[4], r[8], r[9], r[29]
            );
            executed += 1;
            if executed > max { Event::Budget } else { s.target.step() }
        } else if trace {
            let m = s.x86().ok_or("--trace needs an 8086 or MIPS program")?;
            let pc = m.cpu.pc();
            let bytes = m.sys.mem.slice(pc, 8).to_vec();
            let ins = disassemble(&bytes, m.cpu.ip);
            let c = &m.cpu;
            eprintln!(
                "{:04X}:{:04X}  {:<28} AX={:04X} BX={:04X} CX={:04X} DX={:04X} SP={:04X} F={:04X}",
                c.cs(),
                c.ip,
                ins.text,
                c.regs[0],
                c.regs[3],
                c.regs[1],
                c.regs[2],
                c.regs[4],
                c.flags
            );
            executed += 1;
            if executed > max { Event::Budget } else { s.target.step() }
        } else {
            s.target.set_history(false);
            run_chunk(&mut s, &mut executed, max)
        };
        // `printed` counts every byte written: the output keeps only its last megabyte or so.
        let (out, dropped) = (s.target.output(), s.target.output_dropped());
        if !screen && dropped + out.len() > printed {
            let new = &out[printed.saturating_sub(dropped)..];
            // MIPS's console holds UTF-8 text; DOS's output is CP437 with CR LF.
            let text = if mips { new.to_vec() } else { to_host_text(new) };
            stdout.write_all(&text).ok();
            stdout.flush().ok();
            printed = dropped + out.len();
        }
        match ev {
            Event::Continue => {}
            Event::Sleeping => std::thread::sleep(std::time::Duration::from_millis(2)),
            Event::WaitingInput => {
                let next = match &mut input {
                    Some(buf) if !buf.is_empty() => Some(buf.remove(0)),
                    Some(_) => None,
                    None => {
                        let mut b = [0u8; 1];
                        match std::io::stdin().read(&mut b) {
                            Ok(1) => Some(if b[0] == b'\n' && !mips { b'\r' } else { b[0] }),
                            _ => None,
                        }
                    }
                };
                match next {
                    Some(c) => s.target.push_char(c),
                    None => {
                        eprintln!("\n[input exhausted]");
                        break 3;
                    }
                }
            }
            Event::Terminated(c) => break c,
            Event::Halted => {
                eprintln!("\n[halted at {}]", s.target.pc_text());
                break 0;
            }
            Event::Breakpoint => {}
            Event::Budget => {
                eprintln!("\n[instruction limit reached]");
                break 4;
            }
            Event::Error(e) => {
                eprintln!("\n[error at {}: {e}]", s.target.pc_text());
                break 5;
            }
        }
    };
    if flag(args, "--regs") {
        eprint!("{}", registers_text(&s));
    }
    if let Some(m) = s.x86() {
        if screen {
            print!("{}", m.screen_text().trim_end_matches('\n'));
            println!();
        }
        for l in m.log.lines() {
            eprintln!("[log] {l}");
        }
        if !m.sys.dev.printer.is_empty() {
            eprintln!("[printer] {}", String::from_utf8_lossy(m.sys.dev.printer.bytes()));
        }
    }
    Ok(code)
}

/// The registers at the end of a run (`--regs`).
fn registers_text(s: &Session) -> String {
    if let Some(m) = s.mips() {
        let c = &m.cpu;
        let mut out = String::new();
        for (i, name) in rustemu_mips::cpu::REG_NAMES.iter().enumerate() {
            out += &format!("{:>5} = {:08x}{}", format!("${name}"), c.regs[i], if i % 4 == 3 { "\n" } else { "   " });
        }
        out += &format!("   pc = {:08x}      hi = {:08x}      lo = {:08x}\n", c.pc, c.hi, c.lo);
        return out;
    }
    let Some(m) = s.x86() else { return String::new() };
    let c = &m.cpu;
    format!(
        "AX={:04X} BX={:04X} CX={:04X} DX={:04X} SI={:04X} DI={:04X} BP={:04X} SP={:04X}\n\
         CS={:04X} DS={:04X} ES={:04X} SS={:04X} IP={:04X} FLAGS={:04X}\n",
        c.regs[0],
        c.regs[3],
        c.regs[1],
        c.regs[2],
        c.regs[6],
        c.regs[7],
        c.regs[5],
        c.regs[4],
        c.cs(),
        c.sregs[3],
        c.sregs[0],
        c.sregs[2],
        c.ip,
        c.flags
    )
}

/// Runs one bounded chunk and charges only the steps that were actually
/// consumed before an event such as input or a timed wait stopped execution.
fn run_chunk(s: &mut Session, executed: &mut u64, max: u64) -> Event {
    let budget = max.saturating_sub(*executed).min(1_000_000);
    let (event, done) = s.target.run_counted(budget);
    *executed = executed.saturating_add(done);
    if event == Event::Budget && *executed < max { Event::Continue } else { event }
}

/// Converts DOS console output to host text (CR LF -> LF, CP437 -> UTF-8).
fn to_host_text(bytes: &[u8]) -> Vec<u8> {
    let mut s = String::new();
    for &b in bytes {
        match b {
            b'\r' => {}
            b'\n' => s.push('\n'),
            7 => {}
            8 => s.push('\u{8}'),
            b'\t' => s.push('\t'),
            _ => s.push(rustemu_core::font::cp437_to_char(b)),
        }
    }
    s.into_bytes()
}

fn cmd_disasm(args: &[String]) -> Result<u8, String> {
    let path = file_arg(args)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if arch_of(&path, None, args)? == Arch::Mips32 {
        return Ok(disasm_mips(&bytes, args));
    }
    let (code, org) = disasm_input(bytes, &path, args)?;
    let mut pos = 0;
    while pos < code.len() {
        let end = (pos + 8).min(code.len());
        let i = disassemble(&code[pos..end], org.wrapping_add(pos as u16));
        let hex: Vec<String> =
            code[pos..pos + i.len.min(code.len() - pos)].iter().map(|b| format!("{b:02X}")).collect();
        println!("{:04X}  {:<14} {}", org.wrapping_add(pos as u16), hex.join(" "), i.text);
        pos += i.len.max(1);
    }
    Ok(0)
}

/// MIPS words (little-endian) from `--org` (by default 0x00400000, the text segment).
fn disasm_mips(bytes: &[u8], args: &[String]) -> u8 {
    let org = opt(args, "--org")
        .and_then(|v| u32::from_str_radix(v.trim_start_matches("0x").trim_end_matches(['h', 'H']), 16).ok())
        .unwrap_or(rustemu_mips::memory::TEXT_BASE);
    let style =
        if flag(args, "--mars") { rustemu_mips::disasm::Style::Mars } else { rustemu_mips::disasm::Style::Named };
    for (k, chunk) in bytes.chunks(4).enumerate() {
        let mut w = [0u8; 4];
        w[..chunk.len()].copy_from_slice(chunk);
        let word = u32::from_le_bytes(w);
        let addr = org.wrapping_add(4 * k as u32);
        println!("{addr:08x}  {word:08x}  {}", rustemu_mips::disasm::disassemble(word, addr, style));
    }
    0
}

fn disasm_input(bytes: Vec<u8>, path: &Path, args: &[String]) -> Result<(Vec<u8>, u16), String> {
    if bytes.starts_with(b"MZ") || bytes.starts_with(b"ZM") {
        let code = rustemu_core::machine::mz_image(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((code.to_vec(), 0))
    } else {
        let default = if is_ext(path, "com") { 0x100 } else { 0 };
        let org = opt(args, "--org")
            .and_then(|v| u16::from_str_radix(v.trim_end_matches(['h', 'H']), 16).ok())
            .unwrap_or(default);
        Ok((bytes, org))
    }
}

fn is_ext(p: &Path, e: &str) -> bool {
    p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeping_only_consumes_steps_actually_executed() {
        let src = "org 100h\nmov ah, 86h\nmov cx, 004Ch\nmov dx, 4B40h\nint 15h\nhlt\n";
        let mut session = Session::new();
        session.load_program(rustemu_asm::assemble_str(src).unwrap()).unwrap();
        session.target.set_history(false);
        let mut executed = 0;

        assert_eq!(run_chunk(&mut session, &mut executed, 100_000_000), Event::Sleeping);
        let first = executed;
        assert!(first < 100, "only a few setup instructions should run, got {first}");

        assert_eq!(run_chunk(&mut session, &mut executed, 100_000_000), Event::Sleeping);
        assert_eq!(executed - first, 1, "re-polling the blocked BIOS service is one step");
    }

    #[test]
    fn the_processor_of_a_file() {
        let none: [String; 0] = [];
        let arch = |p: &str, text: Option<&str>, args: &[String]| arch_of(Path::new(p), text, args).unwrap();
        assert_eq!(arch("a.s", None, &none), Arch::Mips32);
        assert_eq!(arch("a.asm", Some("main: li $v0, 10\nsyscall\n"), &none), Arch::Mips32);
        assert_eq!(arch("a.asm", Some("org 100h\nmov ax, 1\nmov bx, ax\n"), &none), Arch::I8086);
        assert_eq!(arch("a.bin", None, &none), Arch::I8086);
        assert_eq!(arch("a.s", None, &["--arch".into(), "8086".into()]), Arch::I8086);
        assert_eq!(arch("a.bin", None, &["--arch".into(), "MIPS32".into()]), Arch::Mips32);
        assert!(arch_of(Path::new("a.s"), None, &["--arch".into(), "z80".into()]).is_err());
    }

    #[test]
    fn mips_text_segment_round_trip() {
        let prog = rustemu_mips::asm::assemble("main: li $t0, 0x12345678\nsyscall\n").unwrap();
        let bytes = prog.text_bytes();
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[..4], &0x3c01_1234u32.to_le_bytes(), "lui $at, 0x1234");
        assert!(prog.listing().contains("1: main: li $t0, 0x12345678"));
        assert_eq!(prog.symbols_text(), "00400000  text  main\n");
    }

    #[test]
    fn short_mz_is_reported_instead_of_panicking() {
        let err = disasm_input(b"MZ".to_vec(), Path::new("broken.exe"), &[]).unwrap_err();
        assert!(err.contains("broken.exe: not an MZ executable"), "{err}");
    }
}
