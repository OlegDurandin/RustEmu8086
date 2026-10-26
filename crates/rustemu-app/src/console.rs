//! DEBUG.EXE-style command console.

use egui::{RichText, Ui};
use rustemu_core::cpu::{REG16_NAMES, SREG_NAMES};
use rustemu_core::disasm::disassemble;
use rustemu_core::linear;

use crate::app::App;

const HELP: &str = "\
commands (numbers are hex, addresses are seg:off, linear or register:off):
  r [reg [value]]      show registers / set a register
  d [addr] [len]       dump memory (continues after the last dump)
  e addr b1 b2 ...     enter bytes (or 'text')
  f addr len byte      fill memory
  u [addr] [count]     unassemble
  t [n]                trace n instructions
  p                    step over
  g [addr]             run (until addr)
  h a b                hex sum and difference
  cls                  clear the console";

#[derive(Default)]
pub struct ConsoleState {
    pub input: String,
    pub lines: Vec<String>,
    pub history: Vec<String>,
    last_dump: Option<u32>,
    last_unasm: Option<(u16, u16)>,
    /// State last written to the log: (instruction count, CS:IP).
    logged: Option<(u64, u32)>,
}

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim_end_matches(['h', 'H']), 16).ok()
}

/// Parses an address: `seg:off`, `reg:off`, or a bare offset in `default_seg`.
fn addr(app: &App, s: &str, default_seg: u16) -> Option<(u16, u16)> {
    let c = &app.session.machine.cpu;
    let part = |t: &str| -> Option<u16> {
        let u = t.to_ascii_uppercase();
        if let Some(i) = SREG_NAMES.iter().position(|r| *r == u) {
            return Some(c.sregs[i]);
        }
        if let Some(i) = REG16_NAMES.iter().position(|r| *r == u) {
            return Some(c.regs[i]);
        }
        if u == "IP" {
            return Some(c.ip);
        }
        hex(t).map(|v| v as u16)
    };
    match s.split_once(':') {
        Some((a, b)) => Some((part(a)?, part(b)?)),
        None => Some((default_seg, part(s)?)),
    }
}

fn regs_text(app: &App) -> Vec<String> {
    let c = &app.session.machine.cpu;
    let mut l1 = String::new();
    // DEBUG.EXE order: AX BX CX DX SP BP SI DI.
    for i in [0, 3, 1, 2, 4, 5, 6, 7] {
        l1.push_str(&format!("{}={:04X}  ", REG16_NAMES[i], c.regs[i]));
    }
    let f = c.flags;
    use rustemu_core::cpu::flags::*;
    let fl = |b: u16, on: &str, off: &str| if f & b != 0 { on.to_string() } else { off.to_string() };
    let l2 = format!(
        "DS={:04X}  ES={:04X}  SS={:04X}  CS={:04X}  IP={:04X}   {} {} {} {} {} {} {} {}",
        c.sregs[3],
        c.sregs[0],
        c.sregs[2],
        c.sregs[1],
        c.ip,
        fl(OF, "OV", "NV"),
        fl(DF, "DN", "UP"),
        fl(IF, "EI", "DI"),
        fl(SF, "NG", "PL"),
        fl(ZF, "ZR", "NZ"),
        fl(AF, "AC", "NA"),
        fl(PF, "PE", "PO"),
        fl(CF, "CY", "NC")
    );
    let pc = c.pc();
    let ins = disassemble(app.session.machine.sys.mem.slice(pc, 8), c.ip);
    let bytes: String = app.session.machine.sys.mem.slice(pc, ins.len).iter().map(|b| format!("{b:02X}")).collect();
    vec![l1.trim_end().to_string(), l2, format!("{:04X}:{:04X} {bytes:<14} {}", c.cs(), c.ip, ins.text)]
}

pub fn execute(app: &mut App, line: &str) {
    let out = run_command(app, line);
    let con = &mut app.console;
    con.lines.push(format!("-{line}"));
    con.lines.extend(out);
    let n = con.lines.len();
    if n > 2000 {
        con.lines.drain(..n - 2000);
    }
}

fn run_command(app: &mut App, line: &str) -> Vec<String> {
    let mut words = line.split_whitespace();
    let Some(cmd) = words.next() else { return vec![] };
    let args: Vec<&str> = words.collect();
    let loaded = app.loaded();
    let ds = app.session.machine.cpu.sregs[3];
    let cs = app.session.machine.cpu.cs();
    match cmd.to_ascii_lowercase().as_str() {
        "?" | "help" => HELP.lines().map(str::to_string).collect(),
        "cls" => {
            app.console.lines.clear();
            vec![]
        }
        "r" => {
            if let Some(reg) = args.first() {
                let Some(v) = args.get(1).and_then(|v| hex(v)) else { return vec!["usage: r reg value".into()] };
                if !app.session.machine.set_register(reg, v as u16) {
                    return vec![format!("unknown register {reg}")];
                }
            }
            regs_text(app)
        }
        "d" => {
            let start = match args.first() {
                Some(a) => match addr(app, a, ds) {
                    Some((s, o)) => linear(s, o),
                    None => return vec!["bad address".into()],
                },
                None => app.console.last_dump.unwrap_or(linear(ds, 0)),
            };
            let len = args.get(1).and_then(|v| hex(v)).unwrap_or(0x80);
            let mem = &app.session.machine.sys.mem;
            let mut out = Vec::new();
            let mut a = start & !0xF;
            while a < start + len {
                let mut hexs = String::new();
                let mut chars = String::new();
                for i in 0..16 {
                    let x = a + i;
                    if x < start || x >= start + len {
                        hexs.push_str("   ");
                        chars.push(' ');
                    } else {
                        let b = mem.read8(x);
                        hexs.push_str(&format!("{b:02X}{}", if i == 7 { "-" } else { " " }));
                        chars.push(if (32..127).contains(&b) { b as char } else { '.' });
                    }
                }
                out.push(format!("{:05X}  {hexs} {chars}", a));
                a += 16;
            }
            app.console.last_dump = Some(start + len);
            out
        }
        "e" => {
            let Some((s, o)) = args.first().and_then(|a| addr(app, a, ds)) else {
                return vec!["usage: e addr bytes...".into()];
            };
            let mut bytes = Vec::new();
            let rest = line.splitn(3, char::is_whitespace).nth(2).unwrap_or("");
            if let Some(t) = rest.trim().strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
                bytes.extend(t.bytes());
            } else {
                for b in &args[1..] {
                    match hex(b) {
                        Some(v) => bytes.push(v as u8),
                        None => return vec![format!("bad byte {b}")],
                    }
                }
            }
            for (i, b) in bytes.iter().enumerate() {
                app.session.machine.sys.mem.write8(linear(s, o.wrapping_add(i as u16)), *b);
            }
            vec![format!("{} byte(s) written", bytes.len())]
        }
        "f" => {
            let (Some((s, o)), Some(len), Some(v)) = (
                args.first().and_then(|a| addr(app, a, ds)),
                args.get(1).and_then(|v| hex(v)),
                args.get(2).and_then(|v| hex(v)),
            ) else {
                return vec!["usage: f addr len byte".into()];
            };
            for i in 0..len {
                app.session.machine.sys.mem.write8(linear(s, o.wrapping_add(i as u16)), v as u8);
            }
            vec![]
        }
        "u" => {
            let (s, mut o) = match args.first() {
                Some(a) => match addr(app, a, cs) {
                    Some(x) => x,
                    None => return vec!["bad address".into()],
                },
                None => app.console.last_unasm.unwrap_or((cs, app.session.machine.cpu.ip)),
            };
            let n = args.get(1).and_then(|v| hex(v)).unwrap_or(0x10);
            let mut out = Vec::new();
            for _ in 0..n {
                let a = linear(s, o);
                let mem = &app.session.machine.sys.mem;
                let ins = disassemble(mem.slice(a, 8), o);
                let bytes: String = mem.slice(a, ins.len).iter().map(|b| format!("{b:02X}")).collect();
                out.push(format!("{s:04X}:{o:04X} {bytes:<14} {}", ins.text));
                o = o.wrapping_add(ins.len as u16);
            }
            app.console.last_unasm = Some((s, o));
            out
        }
        "t" if loaded => {
            let n = args.first().and_then(|v| hex(v)).unwrap_or(1);
            let mut out = Vec::new();
            for _ in 0..n {
                app.single_step();
                out.extend(regs_text(app));
                if app.session.machine.terminated.is_some() {
                    out.push("program terminated".into());
                    break;
                }
            }
            out
        }
        "p" if loaded => {
            app.step_over();
            vec!["stepping over...".into()]
        }
        "g" if loaded => {
            if let Some(a) = args.first() {
                let Some((s, o)) = addr(app, a, cs) else { return vec!["bad address".into()] };
                app.begin_exec();
                app.run = crate::app::RunMode::RunTo(vec![linear(s, o)]);
            } else {
                app.start_run();
            }
            vec!["running...".into()]
        }
        "h" => match (args.first().and_then(|v| hex(v)), args.get(1).and_then(|v| hex(v))) {
            (Some(a), Some(b)) => vec![format!("{:04X}  {:04X}", (a + b) as u16, a.wrapping_sub(b) as u16)],
            _ => vec!["usage: h a b".into()],
        },
        "t" | "p" | "g" => vec!["no program loaded".into()],
        c => vec![format!("unknown command '{c}' (type ? for help)")],
    }
}

pub fn console_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.windows.console;
    egui::Window::new("debug log")
        .open(&mut open)
        .default_pos(egui::pos2(680.0, 470.0))
        .default_size([620.0, 360.0])
        .show(ctx, |ui| console_ui(app, ui));
    app.windows.console = open;
}

/// Writes the machine state to the log whenever execution stops at a new place (like Emu8086's
/// debug window, which logs every step).
pub fn log_state(app: &mut App) {
    if !app.loaded() || app.run != crate::app::RunMode::Paused {
        return;
    }
    let c = &app.session.machine.cpu;
    let now = (c.instr_count, c.pc());
    if app.console.logged == Some(now) {
        return;
    }
    app.console.logged = Some(now);
    let text = regs_text(app);
    let con = &mut app.console;
    if !con.lines.is_empty() {
        con.lines.push(String::new());
    }
    con.lines.extend(text);
}

fn console_ui(app: &mut App, ui: &mut Ui) {
    if app.console.lines.is_empty() {
        app.console.lines.push("RustEmu8086 debug console - type ? for help".into());
    }
    log_state(app);
    let h = ui.available_height() - 30.0;
    egui::ScrollArea::vertical().max_height(h.max(100.0)).stick_to_bottom(true).auto_shrink([false, false]).show(
        ui,
        |ui| {
            for l in &app.console.lines {
                ui.label(RichText::new(l).monospace().size(12.5));
            }
        },
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new("-").monospace());
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.console.input)
                .font(egui::TextStyle::Monospace)
                .hint_text("command")
                .desired_width(f32::INFINITY),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let cmd = std::mem::take(&mut app.console.input);
            if !cmd.trim().is_empty() {
                app.console.history.push(cmd.clone());
                execute(app, cmd.trim());
            }
            r.request_focus();
        }
        if r.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::ArrowUp))
            && let Some(last) = app.console.history.last()
        {
            app.console.input = last.clone();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Settings};

    #[test]
    fn debug_commands() {
        let mut a = App::with_settings(Settings { last_file: None, ..Settings::default() });
        a.load_example(0);
        a.emulate();
        execute(&mut a, "r");
        assert!(a.console.lines.iter().any(|l| l.contains("IP=0100")));
        execute(&mut a, "e ds:200 41 42 43");
        execute(&mut a, "d ds:200 3");
        assert!(a.console.lines.last().unwrap().contains("41 42 43"));
        execute(&mut a, "u cs:100 1");
        assert!(a.console.lines.last().unwrap().contains("MOV DX, 0108h"));
        execute(&mut a, "t");
        assert_eq!(a.session.machine.cpu.ip, 0x103);
        execute(&mut a, "r ax 1234");
        assert_eq!(a.session.machine.cpu.regs[0], 0x1234);
        execute(&mut a, "h 10 1");
        assert_eq!(a.console.lines.last().unwrap(), "0011  000F");
    }
}
