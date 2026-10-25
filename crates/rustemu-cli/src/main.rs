//! `rustemu` - command line front end: assemble, run headless, disassemble.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustemu_core::Event;
use rustemu_core::disasm::disassemble;
use rustemu_session::Session;

const USAGE: &str = "\
usage:
  rustemu asm <file.asm> [-o <out>] [-l] [-s]   assemble (-l listing, -s symbols)
  rustemu run <file> [options]                  run .asm/.com/.exe/.bin/.boot headless
      --input <text>     keyboard input (\\n = Enter); otherwise stdin is used
      --args <text>      DOS command line parameters (PSP:80h)
      --max <n>          instruction limit (default 100000000)
      --screen           print the final text screen instead of the output stream
      --trace            print every executed instruction
      --timer-irq        deliver IRQ0 (INT 08h/1Ch) 18.2 times a second
  rustemu disasm <file> [--org <hex>]           disassemble a binary
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

fn cmd_asm(args: &[String]) -> Result<u8, String> {
    let path = file_arg(args)?;
    let prog = rustemu_asm::assemble_file(&path).map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| path.with_extension(prog.format.extension()));
    std::fs::write(&out, prog.file_bytes()).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "{} -> {} ({} format, {} bytes)",
        path.display(),
        out.display(),
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
    let mut s = Session::new();
    s.command_line = opt(args, "--args").unwrap_or("").to_string();
    s.load_path(&path)?;
    s.machine.timer_irq = flag(args, "--timer-irq");
    let max: u64 = opt(args, "--max").and_then(|v| v.parse().ok()).unwrap_or(100_000_000);
    let screen = flag(args, "--screen");
    let trace = flag(args, "--trace");
    let mut input: Option<Vec<u8>> = opt(args, "--input").map(|t| t.replace("\\n", "\r").into_bytes());
    let mut stdout = std::io::stdout();
    let mut printed = 0;
    let mut executed = 0u64;
    let code = loop {
        let ev = if trace {
            let pc = s.machine.cpu.pc();
            let bytes = s.machine.sys.mem.slice(pc, 8).to_vec();
            let ins = disassemble(&bytes, s.machine.cpu.ip);
            let c = &s.machine.cpu;
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
            if executed > max { Event::Budget } else { s.machine.step() }
        } else {
            s.machine.history_enabled = false;
            let e = s.machine.run(max.saturating_sub(executed).min(1_000_000));
            executed += 1_000_000;
            if e == Event::Budget && executed < max { Event::Continue } else { e }
        };
        if !screen && s.machine.output.len() > printed {
            stdout.write_all(&to_host_text(&s.machine.output[printed..])).ok();
            stdout.flush().ok();
            printed = s.machine.output.len();
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
                            Ok(1) => Some(if b[0] == b'\n' { b'\r' } else { b[0] }),
                            _ => None,
                        }
                    }
                };
                match next {
                    Some(c) => s.machine.push_char(c),
                    None => {
                        eprintln!("\n[input exhausted]");
                        break 3;
                    }
                }
            }
            Event::Terminated(c) => break c,
            Event::Halted => {
                eprintln!("\n[halted at {:04X}:{:04X}]", s.machine.cpu.cs(), s.machine.cpu.ip);
                break 0;
            }
            Event::Breakpoint => {}
            Event::Budget => {
                eprintln!("\n[instruction limit reached]");
                break 4;
            }
            Event::Error(e) => {
                eprintln!("\n[error at {:04X}:{:04X}: {e}]", s.machine.cpu.cs(), s.machine.cpu.ip);
                break 5;
            }
        }
    };
    if screen {
        print!("{}", s.machine.screen_text().trim_end_matches('\n'));
        println!();
    }
    for l in &s.machine.log {
        eprintln!("[log] {l}");
    }
    if !s.machine.sys.dev.printer.is_empty() {
        eprintln!("[printer] {}", String::from_utf8_lossy(&s.machine.sys.dev.printer));
    }
    Ok(code)
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
    let (code, org) = if bytes.starts_with(b"MZ") {
        let hdr = u16::from_le_bytes([bytes[8], bytes[9]]) as usize * 16;
        (bytes[hdr.min(bytes.len())..].to_vec(), 0u16)
    } else {
        let default = if is_ext(&path, "com") { 0x100 } else { 0 };
        let org = opt(args, "--org")
            .and_then(|v| u16::from_str_radix(v.trim_end_matches(['h', 'H']), 16).ok())
            .unwrap_or(default);
        (bytes, org)
    };
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

fn is_ext(p: &Path, e: &str) -> bool {
    p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e))
}
