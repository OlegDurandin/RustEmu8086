//! The emulated PC: CPU + memory + devices + BIOS/DOS services.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use crate::bus::{Bus, Memory, linear};
use crate::cpu::{self, Cpu, StepResult, flags};
use crate::devices::Devices;
use crate::dos::DosState;
use crate::video::{self, Video};

/// Segment of the ROM area that holds interrupt stubs.
pub const ROM_SEG: u16 = 0xF000;
/// Offset of the first interrupt stub (`0F FF nn CF`, 4 bytes per vector).
pub const STUB_OFF: u16 = 0x1000;
/// Offset of the ROM INT 08h handler.
const INT08_OFF: u16 = 0xE000;
/// Segment where programs are loaded (PSP for COM/EXE).
pub const PROGRAM_SEG: u16 = 0x0700;
const HISTORY_DEPTH: usize = 2000;

/// Everything on the bus except the CPU.
pub struct System {
    pub mem: Memory,
    pub dev: Devices,
    /// Keyboard buffer: (scan code << 8) | ASCII.
    pub keys: VecDeque<u16>,
    pub irq: VecDeque<u8>,
}

impl Bus for System {
    #[inline]
    fn read8(&mut self, a: u32) -> u8 {
        self.mem.read8(a)
    }
    #[inline]
    fn write8(&mut self, a: u32, v: u8) {
        self.mem.write8(a, v)
    }
    fn port_in8(&mut self, p: u16) -> u8 {
        self.dev.input8(p)
    }
    fn port_out8(&mut self, p: u16, v: u8) {
        self.dev.output8(p, v)
    }
    fn port_in16(&mut self, p: u16) -> u16 {
        self.dev.input16(p)
    }
    fn port_out16(&mut self, p: u16, v: u16) {
        self.dev.output16(p, v)
    }
    fn take_irq(&mut self) -> Option<u8> {
        self.irq.pop_front()
    }
}

/// Result of [`Machine::step`] / [`Machine::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Instruction executed, nothing special.
    Continue,
    /// Execution is blocked until a key is pressed.
    WaitingInput,
    /// A timed BIOS wait (INT 15h/86h) is in progress; run again later.
    Sleeping,
    /// HLT executed (waiting for an interrupt).
    Halted,
    /// Program terminated with an exit code.
    Terminated(u8),
    /// Breakpoint reached (or INT 3 executed).
    Breakpoint,
    /// Instruction budget of [`Machine::run`] used up.
    Budget,
    /// Fatal error (divide error, unsupported service...); execution stops.
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    Com,
    Exe,
    Bin,
    Boot,
}

/// Information about the loaded program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadInfo {
    pub kind: LoadKind,
    /// Physical address of the first byte of the load image.
    pub image_base: u32,
    pub image_len: u32,
    /// Segment that relocations were based on (EXE) / load segment.
    pub load_segment: u16,
    pub psp: Option<u16>,
}

/// Service result of an HLE interrupt handler.
pub(crate) enum Svc {
    Ret,
    Block,
    /// Like `Block`, but waiting for time to pass rather than for a key.
    Sleep,
    Terminate(u8),
    /// Return from the interrupt immediately and stop with this event.
    Stop(Event),
}

struct Snapshot {
    cpu: Cpu,
    undo: Vec<(u32, u8)>,
    keys: VecDeque<u16>,
    output_len: usize,
}

pub struct Machine {
    pub cpu: Cpu,
    pub sys: System,
    pub breakpoints: HashSet<u32>,
    pub loaded: Option<LoadInfo>,
    /// Text written by the program to the console (DOS output, BIOS teletype).
    pub output: Vec<u8>,
    /// Messages for the emulator log (unsupported services and such).
    pub log: Vec<String>,
    pub terminated: Option<u8>,
    /// Record history for step-back.
    pub history_enabled: bool,
    history: VecDeque<Snapshot>,
    pub(crate) dos: DosState,
    /// Fixed date/time for INT 21h/1Ah (tests); `None` = host clock.
    pub fixed_time: Option<crate::dos::DateTime>,
    /// Mouse state for INT 33h: x, y (in pixels of a 640x200 virtual screen), buttons.
    pub mouse: (u16, u16, u16),
    pub mouse_visible: bool,
    /// Floppy disk image for INT 13h (drive A:), 1.44 MB geometry.
    pub floppy: Vec<u8>,
    /// Deliver IRQ0 (INT 08h -> INT 1Ch) on timer ticks. When off, the BIOS tick count at
    /// 0040:006C is still advanced.
    pub timer_irq: bool,
    /// Timer source: `None` = host clock (18.2 Hz), `Some(n)` = one tick every `n` instructions.
    pub instructions_per_tick: Option<u64>,
    tick_instr: u64,
    tick_time: Option<std::time::Instant>,
    tick_frac: f64,
    /// End of a running INT 15h/86h wait.
    pub(crate) wait_until: Option<std::time::Instant>,
    /// Complete INT 15h/86h waits immediately (tests, headless runs).
    pub instant_waits: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        let mut cpu = Cpu::new();
        // Emu8086 programs use a few 80186 instructions (PUSHA/POPA, PUSH imm, ...).
        cpu.model = crate::cpu::CpuModel::I80186;
        let mut m = Machine {
            cpu,
            sys: System { mem: Memory::new(), dev: Devices::new(), keys: VecDeque::new(), irq: VecDeque::new() },
            breakpoints: HashSet::new(),
            loaded: None,
            output: Vec::new(),
            log: Vec::new(),
            terminated: None,
            history_enabled: true,
            history: VecDeque::new(),
            dos: DosState::default(),
            fixed_time: None,
            mouse: (0, 0, 0),
            mouse_visible: false,
            floppy: Vec::new(),
            timer_irq: false,
            instructions_per_tick: None,
            tick_instr: 0,
            tick_time: None,
            tick_frac: 0.0,
            wait_until: None,
            instant_waits: false,
        };
        m.reset();
        m
    }

    /// Directory that DOS file functions operate in (the virtual drive C:).
    pub fn set_dos_root(&mut self, root: PathBuf) {
        self.dos.root = Some(root);
    }

    /// Cold boot: clears memory and installs the BIOS.
    pub fn reset(&mut self) {
        let root = self.dos.root.take();
        self.sys.mem.set_journaling(false);
        self.sys.mem.clear();
        // Devices are separate from the CPU (separate programs in Emu8086): the robot's room,
        // the air temperature and the clock survive a reset.
        let old = std::mem::take(&mut self.sys.dev);
        self.sys.dev.clock = old.clock;
        self.sys.dev.air_temperature = old.air_temperature;
        self.sys.dev.robot = old.robot;
        self.sys.dev.robot.reset_state();
        self.sys.keys.clear();
        self.sys.irq.clear();
        self.cpu.reset();
        self.output.clear();
        self.log.clear();
        self.terminated = None;
        self.history.clear();
        self.loaded = None;
        self.dos = DosState { root, ..DosState::default() };
        self.tick_instr = 0;
        self.tick_time = None;
        self.tick_frac = 0.0;
        self.install_bios();
    }

    fn install_bios(&mut self) {
        let mem = &mut self.sys.mem;
        for n in 0..=255u16 {
            let off = STUB_OFF + n * 4;
            let a = linear(ROM_SEG, off);
            mem.load(a, &[0x0F, 0xFF, n as u8, 0xCF]);
            mem.write16(n as u32 * 4, off);
            mem.write16(n as u32 * 4 + 2, ROM_SEG);
        }
        // INT 08h (IRQ0) handler: bump the BDA tick count and chain to INT 1Ch.
        //   push ax / push ds / xor ax,ax / mov ds,ax / inc word [46Ch] / jnz +4 /
        //   inc word [46Eh] / int 1Ch / mov al,20h / out 20h,al / pop ds / pop ax / iret
        mem.load(
            linear(ROM_SEG, INT08_OFF),
            &[
                0x50, 0x1E, 0x31, 0xC0, 0x8E, 0xD8, 0xFF, 0x06, 0x6C, 0x04, 0x75, 0x04, 0xFF, 0x06, 0x6E, 0x04, 0xCD,
                0x1C, 0xB0, 0x20, 0xE6, 0x20, 0x1F, 0x58, 0xCF,
            ],
        );
        mem.write16(8 * 4, INT08_OFF);
        // Reset vector: JMP F000:E05B -> HLT loop (no ROM BASIC here).
        mem.load(0xFFFF0, &[0xEA, 0x5B, 0xE0, 0x00, 0xF0]);
        mem.load(linear(ROM_SEG, 0xE05B), &[0xF4, 0xEB, 0xFD]);
        // BIOS data area.
        mem.write16(video::BDA + 0x10, 0x0021); // equipment: video 80x25 color, 1 floppy
        mem.write16(video::BDA + 0x13, 640);
        mem.write16(video::BDA + 0x1A, 0x1E);
        mem.write16(video::BDA + 0x1C, 0x1E);
        mem.load(0xFFFF5, b"10/25/26");
        mem.write8(0xFFFFE, 0xFF); // model: PC
        Video::set_mode(mem, 3);
    }

    // ---- loading ------------------------------------------------------------

    fn make_psp(&mut self, seg: u16, end_seg: u16) {
        let base = linear(seg, 0);
        let mem = &mut self.sys.mem;
        for i in 0..256 {
            mem.write8(base + i, 0);
        }
        mem.load(base, &[0xCD, 0x20]);
        mem.write16(base + 2, end_seg);
        // Far call to DOS dispatcher (INT 21h stub) at PSP:0005.
        mem.write8(base + 5, 0x9A);
        mem.write16(base + 6, STUB_OFF + 0x21 * 4);
        mem.write16(base + 8, ROM_SEG);
        mem.load(base + 0x50, &[0xCD, 0x21, 0xCB]);
        mem.write8(base + 0x80, 0);
        mem.write8(base + 0x81, 0x0D);
        self.dos.psp = seg;
        self.dos.dta = (seg, 0x80);
    }

    fn after_load(&mut self, info: LoadInfo) {
        self.loaded = Some(info);
        self.terminated = None;
        self.history.clear();
        self.output.clear();
        self.cpu.set_flags_word(flags::IF);
        self.cpu.halted = false;
    }

    /// Loads a COM program at PROGRAM_SEG:0100.
    pub fn load_com(&mut self, image: &[u8]) -> Result<(), String> {
        if image.len() > 0xFF00 - 2 {
            return Err("COM program is too large".into());
        }
        let seg = PROGRAM_SEG;
        self.make_psp(seg, 0xA000);
        let base = linear(seg, 0x100);
        self.sys.mem.load(base, image);
        let c = &mut self.cpu;
        c.regs = [0; 8];
        c.sregs = [seg; 4];
        c.ip = 0x100;
        c.regs[cpu::SP] = 0xFFFE;
        self.sys.mem.write16(linear(seg, 0xFFFE), 0);
        self.after_load(LoadInfo {
            kind: LoadKind::Com,
            image_base: base,
            image_len: image.len() as u32,
            load_segment: seg,
            psp: Some(seg),
        });
        Ok(())
    }

    /// Loads an MZ executable after a PSP at PROGRAM_SEG.
    pub fn load_exe(&mut self, file: &[u8]) -> Result<(), String> {
        if file.len() < 0x1C || &file[..2] != b"MZ" && &file[..2] != b"ZM" {
            return Err("not an MZ executable".into());
        }
        let w = |o: usize| u16::from_le_bytes([file[o], file[o + 1]]);
        let last = w(2) as usize;
        let pages = w(4) as usize;
        let nrel = w(6) as usize;
        let hdr = w(8) as usize * 16;
        let (ss, sp, ip, cs, rel_off) = (w(0x0E), w(0x10), w(0x14), w(0x16), w(0x18) as usize);
        let mut size = pages * 512;
        if last != 0 {
            size = size - 512 + last;
        }
        let size = size.min(file.len());
        if hdr > size {
            return Err("corrupt EXE header".into());
        }
        let image = &file[hdr..size];
        let psp = PROGRAM_SEG;
        let load = psp + 0x10;
        if linear(load, 0) as usize + image.len() > 0xA0000 {
            return Err("EXE program is too large".into());
        }
        self.make_psp(psp, 0xA000);
        let base = linear(load, 0);
        self.sys.mem.load(base, image);
        for i in 0..nrel {
            let o = rel_off + i * 4;
            if o + 4 > file.len() {
                return Err("corrupt relocation table".into());
            }
            let a = linear(load.wrapping_add(w(o + 2)), w(o));
            let v = self.sys.mem.read16(a).wrapping_add(load);
            self.sys.mem.write16(a, v);
        }
        let c = &mut self.cpu;
        c.regs = [0; 8];
        c.sregs = [psp, load.wrapping_add(cs), load.wrapping_add(ss), psp];
        c.ip = ip;
        c.regs[cpu::SP] = sp;
        self.after_load(LoadInfo {
            kind: LoadKind::Exe,
            image_base: base,
            image_len: image.len() as u32,
            load_segment: load,
            psp: Some(psp),
        });
        Ok(())
    }

    /// Loads a raw binary at `seg:off` and starts it there (Emu8086 `#make_bin#`).
    pub fn load_bin(&mut self, image: &[u8], seg: u16, off: u16) -> Result<(), String> {
        let base = linear(seg, off);
        if base as usize + image.len() > 0xA0000 {
            return Err("binary does not fit in memory".into());
        }
        self.sys.mem.load(base, image);
        let c = &mut self.cpu;
        c.regs = [0; 8];
        c.sregs = [seg; 4];
        c.ip = off;
        c.regs[cpu::SP] = 0xFFFE;
        self.dos.psp = seg;
        self.after_load(LoadInfo {
            kind: LoadKind::Bin,
            image_base: base,
            image_len: image.len() as u32,
            load_segment: seg,
            psp: None,
        });
        Ok(())
    }

    /// Loads a boot sector at 0000:7C00.
    pub fn load_boot(&mut self, image: &[u8]) -> Result<(), String> {
        if image.len() > 512 {
            return Err("boot sector must be at most 512 bytes".into());
        }
        let base = 0x7C00;
        self.sys.mem.load(base, image);
        let c = &mut self.cpu;
        c.regs = [0; 8];
        c.sregs = [0; 4];
        c.ip = 0x7C00;
        c.regs[cpu::SP] = 0x7C00;
        c.regs[cpu::DX] = 0;
        self.after_load(LoadInfo {
            kind: LoadKind::Boot,
            image_base: base,
            image_len: image.len() as u32,
            load_segment: 0,
            psp: None,
        });
        Ok(())
    }

    /// Writes DOS command line parameters into the PSP (offset 80h: length, text, CR).
    /// Like DOS, the text starts with a space: `"p1 p2"` is stored as `" p1 p2"`.
    pub fn set_command_line(&mut self, args: &str) {
        let Some(psp) = self.loaded.as_ref().and_then(|l| l.psp) else { return };
        let text: Vec<u8> =
            if args.is_empty() { Vec::new() } else { std::iter::once(b' ').chain(args.bytes()).take(126).collect() };
        let base = linear(psp, 0x80);
        self.sys.mem.write8(base, text.len() as u8);
        self.sys.mem.load(base + 1, &text);
        self.sys.mem.write8(base + 1 + text.len() as u32, 0x0D);
    }

    /// Sets a register by name (`AX`..`DI`, `CS`..`SS`, `IP`, `FLAGS`).
    pub fn set_register(&mut self, name: &str, v: u16) -> bool {
        let u = name.to_ascii_uppercase();
        if let Some(i) = cpu::REG16_NAMES.iter().position(|r| *r == u) {
            self.cpu.regs[i] = v;
        } else if let Some(i) = cpu::REG8_NAMES.iter().position(|r| *r == u) {
            self.cpu.set_reg8(i as u8, v as u8);
        } else if let Some(i) = cpu::SREG_NAMES.iter().position(|r| *r == u) {
            self.cpu.sregs[i] = v;
        } else if u == "IP" {
            self.cpu.ip = v;
        } else if u == "FLAGS" {
            self.cpu.set_flags_word(v);
        } else {
            return false;
        }
        true
    }

    // ---- keyboard -------------------------------------------------------------

    pub fn push_key(&mut self, scan: u8, ascii: u8) {
        if self.sys.keys.len() < 64 {
            self.sys.keys.push_back(((scan as u16) << 8) | ascii as u16);
        }
        self.sys.dev.last_scan = scan;
        self.cpu.halted = false;
    }

    /// Pushes a character typed by the user, deriving the scan code.
    pub fn push_char(&mut self, c: u8) {
        let scan = crate::keyboard::scan_code_for(c);
        self.push_key(scan, c);
    }

    pub fn is_waiting_input(&self) -> bool {
        self.dos.waiting
    }

    // ---- execution --------------------------------------------------------------

    /// Whether `addr` lies in BIOS ROM (stubs and ROM routines). Code there runs within one step.
    fn in_stubs(addr: u32) -> bool {
        addr >= linear(ROM_SEG, 0)
    }

    /// Executes one instruction. Calls into BIOS/DOS stubs complete within the same step.
    pub fn step(&mut self) -> Event {
        let ev = self.step_cpu();
        self.sys.dev.tick(1);
        self.timer(1);
        ev
    }

    /// Advances the system timer after `instructions` executed instructions.
    fn timer(&mut self, instructions: u64) {
        let ticks = match self.instructions_per_tick {
            Some(n) => {
                self.tick_instr += instructions;
                let t = self.tick_instr / n.max(1);
                self.tick_instr %= n.max(1);
                t
            }
            None => {
                let now = std::time::Instant::now();
                let last = *self.tick_time.get_or_insert(now);
                let el = now.duration_since(last).as_secs_f64();
                if el < 0.01 {
                    return;
                }
                self.tick_time = Some(now);
                // Long pauses (debugger stopped) do not produce a burst of ticks.
                self.tick_frac += el.min(0.5) * 18.206_5;
                let t = self.tick_frac as u64;
                self.tick_frac -= t as f64;
                t
            }
        };
        if ticks == 0 {
            return;
        }
        if self.timer_irq {
            if self.sys.irq.is_empty() {
                self.sys.irq.push_back(8);
            }
        } else {
            let a = video::BDA + 0x6C;
            let v = self.sys.mem.read16(a) as u32 | ((self.sys.mem.read16(a + 2) as u32) << 16);
            let v = (v + ticks as u32) % 0x1800B0;
            self.sys.mem.write16(a, v as u16);
            self.sys.mem.write16(a + 2, (v >> 16) as u16);
        }
    }

    fn step_cpu(&mut self) -> Event {
        if let Some(code) = self.terminated {
            return Event::Terminated(code);
        }
        if !self.history_enabled {
            if self.sys.mem.is_journaling() {
                self.sys.mem.set_journaling(false);
            }
            let mut ev = self.step_inner();
            let mut guard = 0;
            while ev == Event::Continue && Self::in_stubs(self.cpu.pc()) && guard < 256 {
                ev = self.step_inner();
                guard += 1;
            }
            return ev;
        }
        self.sys.mem.set_journaling(true);
        let snap_cpu = self.cpu.clone();
        let snap_keys = self.sys.keys.clone();
        let out_len = self.output.len();
        let mut ev = self.step_inner();
        // Finish BIOS stub code so that INT n steps over the service routine.
        let mut guard = 0;
        while ev == Event::Continue && Self::in_stubs(self.cpu.pc()) && guard < 256 {
            ev = self.step_inner();
            guard += 1;
        }
        {
            let undo = self.sys.mem.take_journal();
            // Re-polling a blocked input service changes nothing and is not worth recording.
            let idle = matches!(ev, Event::WaitingInput | Event::Sleeping)
                && undo.is_empty()
                && self.cpu.same_state(&snap_cpu);
            if !idle {
                if self.history.len() >= HISTORY_DEPTH {
                    self.history.pop_front();
                }
                self.history.push_back(Snapshot { cpu: snap_cpu, undo, keys: snap_keys, output_len: out_len });
            }
        }
        ev
    }

    fn step_inner(&mut self) -> Event {
        match self.cpu.step(&mut self.sys) {
            StepResult::Ok => Event::Continue,
            StepResult::Halted if self.sys.irq.is_empty() => Event::Halted,
            StepResult::Halted => Event::Continue,
            StepResult::Bop(n) => self.service(n),
        }
    }

    /// Runs up to `budget` steps, stopping at breakpoints and events.
    pub fn run(&mut self, budget: u64) -> Event {
        // Devices advance in small slices so programs polling them see progress.
        const SLICE: u64 = 128;
        let mut ev = Event::Budget;
        let mut done = 0;
        let mut pending = 0;
        while done < budget {
            if done > 0 && !self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc()) {
                ev = Event::Breakpoint;
                break;
            }
            let e = self.step_cpu();
            done += 1;
            pending += 1;
            if pending == SLICE {
                self.sys.dev.tick(pending);
                self.timer(pending);
                pending = 0;
            }
            if e != Event::Continue {
                ev = e;
                break;
            }
        }
        self.sys.dev.tick(pending);
        self.timer(pending);
        ev
    }

    /// Undoes the last step. Returns false when there is no history.
    pub fn step_back(&mut self) -> bool {
        match self.history.pop_back() {
            Some(s) => {
                self.sys.mem.undo(&s.undo);
                self.cpu = s.cpu;
                self.sys.keys = s.keys;
                self.output.truncate(s.output_len);
                self.terminated = None;
                self.dos.waiting = false;
                self.wait_until = None;
                true
            }
            None => false,
        }
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    fn service(&mut self, n: u8) -> Event {
        // Status flags as seen by the caller (they were pushed by INT).
        let ss = self.cpu.sregs[cpu::SS];
        let sp = self.cpu.regs[cpu::SP];
        let fl_addr = linear(ss, sp.wrapping_add(4));
        let caller_flags = self.sys.mem.read16(fl_addr);
        self.cpu.flags = (self.cpu.flags & !flags::STATUS) | (caller_flags & flags::STATUS);
        let r = match n {
            0x00 => Svc::Stop(Event::Error("divide error (division by zero or overflow)".into())),
            0x03 => Svc::Stop(Event::Breakpoint),
            0x05 | 0x08 | 0x1C | 0x01 | 0x04 => Svc::Ret,
            0x10 => self.int10(),
            0x11 => {
                self.cpu.regs[cpu::AX] = self.sys.mem.read16(video::BDA + 0x10);
                Svc::Ret
            }
            0x12 => {
                self.cpu.regs[cpu::AX] = 640;
                Svc::Ret
            }
            0x13 => self.int13(),
            0x15 => self.int15(),
            0x16 => self.int16(),
            // Reboot: the program ends (Emu8086 restarts the machine).
            0x19 => Svc::Terminate(0),
            0x1A => self.int1a(),
            0x20 => Svc::Terminate(0),
            0x21 => self.int21(),
            0x33 => self.int33(),
            _ => {
                self.log.push(format!("INT {n:02X}h is not implemented (AX={:04X})", self.cpu.regs[cpu::AX]));
                Svc::Ret
            }
        };
        match r {
            Svc::Ret => {
                self.dos.waiting = false;
                let f = self.sys.mem.read16(fl_addr);
                let nf = (f & !flags::STATUS) | (self.cpu.flags & flags::STATUS);
                self.sys.mem.write16(fl_addr, nf);
                Event::Continue
            }
            Svc::Block => {
                self.dos.waiting = true;
                self.cpu.ip = self.cpu.ip.wrapping_sub(3);
                Event::WaitingInput
            }
            Svc::Sleep => {
                self.cpu.ip = self.cpu.ip.wrapping_sub(3);
                Event::Sleeping
            }
            Svc::Terminate(code) => {
                self.dos.waiting = false;
                self.terminated = Some(code);
                self.hle_iret();
                Event::Terminated(code)
            }
            Svc::Stop(ev) => {
                self.hle_iret();
                ev
            }
        }
    }

    /// Performs the IRET of the stub immediately.
    fn hle_iret(&mut self) {
        let ip = self.cpu.pop(&mut self.sys);
        let cs = self.cpu.pop(&mut self.sys);
        let f = self.cpu.pop(&mut self.sys);
        self.cpu.ip = ip;
        self.cpu.sregs[cpu::CS] = cs;
        self.cpu.set_flags_word(f);
    }

    /// Writes a character to the console (screen + output capture).
    pub(crate) fn console_out(&mut self, ch: u8) {
        self.output.push(ch);
        let attr = 0x07;
        Video::teletype(&mut self.sys.mem, ch, attr);
    }

    pub fn screen_text(&self) -> String {
        Video::screen_text(&self.sys.mem)
    }

    /// Reads a `$`-terminated or NUL-terminated string from memory (max `limit` bytes).
    pub fn read_string(&self, seg: u16, off: u16, term: u8, limit: usize) -> Vec<u8> {
        let mut v = Vec::new();
        let mut o = off;
        while v.len() < limit {
            let b = self.sys.mem.read8(linear(seg, o));
            if b == term {
                break;
            }
            v.push(b);
            o = o.wrapping_add(1);
        }
        v
    }
}
