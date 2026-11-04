//! The MIPS machine as a [`Target`]: what the session and the architecture-neutral interface use
//! (stepping, step back, breakpoints, memory, registers by name, Step Over / Step Out, the call
//! stack).

use std::any::Any;
use std::collections::{BTreeMap, HashSet, VecDeque};

use rustemu_target::{Arch, Event, Flow, Frame, FrameKind, Instruction, RegKind, Register, State, Target};

use crate::console::Console;
use crate::cpu::{Cpu, RA, REG_NAMES, SP};
use crate::disasm::{self, Style};
use crate::machine::{Finish, Machine};
use crate::memory::{self, Memory, Segment};
use crate::random::JavaRandom;

/// The registers before an execution command ([`Target::checkpoint`]).
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// The processor.
    pub cpu: Cpu,
}

/// A snapshot of the machine ([`Target::save_state`]); open files are not part of it.
#[derive(Debug, Clone)]
struct Snapshot {
    cpu: Cpu,
    mem: Memory,
    console: Console,
    input: VecDeque<u8>,
    heap: u32,
    random: BTreeMap<i32, JavaRandom>,
    mmio: crate::mmio::KeyboardDisplay,
    lab: crate::mmio::DigitalLab,
    finished: Option<Finish>,
    loaded: bool,
    waiting: bool,
}

/// Number of registers in [`REGISTERS`] before PC: 32 by name, 32 by number.
const GPR_ENTRIES: usize = 64;
/// Index of `$f0` among the registers of [`Target::registers`].
pub const FPR: usize = 67;
/// Index of the condition flags of coprocessor 1 (a byte) among the registers of [`Target::registers`].
pub const FCC: usize = FPR + 32;

/// Registers of debugger expressions: `$zero`..`$ra` (indices 0..32), `$0`..`$31` (32..64),
/// then PC, HI, LO (64..67), the floating-point registers `$f0`..`$f31` (67..99) and the 8 condition
/// flags of coprocessor 1 as one byte, `FCC` (99; bit n is flag n).
static REGISTERS: std::sync::LazyLock<Vec<Register>> = std::sync::LazyLock::new(|| {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let mut v: Vec<Register> =
        REG_NAMES.iter().map(|n| Register { name: leak(format!("${n}")), bits: 32, kind: RegKind::General }).collect();
    v.extend((0..32).map(|i| Register { name: leak(format!("${i}")), bits: 32, kind: RegKind::General }));
    v.push(Register { name: "PC", bits: 32, kind: RegKind::Pointer });
    v.push(Register { name: "HI", bits: 32, kind: RegKind::General });
    v.push(Register { name: "LO", bits: 32, kind: RegKind::General });
    v.extend((0..32).map(|i| Register { name: leak(format!("$f{i}")), bits: 32, kind: RegKind::General }));
    v.push(Register { name: "FCC", bits: 8, kind: RegKind::General });
    v
});

/// Calls (jal, jalr, bgezal, bltzal) and returns (jr $ra).
fn flow(word: u32, addr: u32) -> Flow {
    match disasm::decode(word, addr) {
        Some(d) if matches!(d.mnemonic, "jal" | "jalr" | "bgezal" | "bltzal") => Flow::Call,
        Some(d) if d.mnemonic == "jr" && word >> 21 & 31 == RA as u32 => Flow::Return,
        _ => Flow::Other,
    }
}

impl Machine {
    /// Whether the instruction before `ret` is a call (so `ret` is a return address).
    fn called_from(&self, ret: u32) -> Option<Frame> {
        let at = ret.checked_sub(4)?;
        if !self.mem.has_code(at) || flow(self.mem.peek32(at), at) != Flow::Call {
            return None;
        }
        let target = match disasm::decode(self.mem.peek32(at), at)?.args.last()? {
            disasm::Arg::Jump(t) | disasm::Arg::Branch(_, t) => Some(*t),
            _ => None,
        };
        Some(Frame { kind: FrameKind::Call, return_addr: ret, call_addr: at, target, stack_offset: 0 })
    }
}

impl Target for Machine {
    fn arch(&self) -> Arch {
        Arch::Mips32
    }

    fn step(&mut self) -> Event {
        Machine::step(self)
    }

    fn run(&mut self, budget: u64) -> Event {
        Machine::run(self, budget)
    }

    fn run_counted(&mut self, budget: u64) -> (Event, u64) {
        Machine::run_counted(self, budget)
    }

    fn step_back(&mut self) -> bool {
        Machine::step_back(self)
    }

    fn history_len(&self) -> usize {
        Machine::history_len(self)
    }

    fn clear_history(&mut self) {
        Machine::clear_history(self)
    }

    fn set_history(&mut self, on: bool) {
        self.history_enabled = on;
    }

    fn breakpoints(&self) -> &HashSet<u32> {
        &self.breakpoints
    }

    fn breakpoints_mut(&mut self) -> &mut HashSet<u32> {
        &mut self.breakpoints
    }

    fn instr_count(&self) -> u64 {
        self.cpu.instr_count
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn image_base(&self) -> Option<u32> {
        self.loaded.then_some(memory::TEXT_BASE)
    }

    fn terminated(&self) -> Option<u8> {
        self.finished.map(Finish::code)
    }

    fn output(&self) -> &[u8] {
        self.console.bytes()
    }

    fn output_dropped(&self) -> usize {
        self.console.dropped()
    }

    fn is_waiting_input(&self) -> bool {
        Machine::is_waiting_input(self)
    }

    fn has_pending_input(&self) -> bool {
        !self.input.is_empty()
    }

    fn push_char(&mut self, c: u8) {
        self.push_byte(c)
    }

    fn reset(&mut self) {
        Machine::reset(self)
    }

    fn pc(&self) -> u32 {
        self.cpu.pc
    }

    fn pc_name(&self) -> &'static str {
        "PC"
    }

    fn pc_text(&self) -> String {
        memory::hex(self.cpu.pc)
    }

    fn system_caller(&self) -> Option<u32> {
        // System services complete within the syscall's step.
        None
    }

    fn flow_at(&self, addr: u32) -> (Flow, u32) {
        (flow(self.mem.peek32(addr), addr), 4)
    }

    fn step_over_stop(&self) -> Option<u32> {
        let pc = self.cpu.pc;
        (flow(self.mem.peek32(pc), pc) == Flow::Call).then(|| pc.wrapping_add(4))
    }

    fn call_stack(&self) -> Vec<Frame> {
        let mut out = Vec::new();
        let ra = self.cpu.reg(RA);
        if let Some(mut f) = self.called_from(ra) {
            f.stack_offset = self.cpu.reg(SP);
            out.push(f);
        }
        // Return addresses saved on the stack (sw $ra, n($sp)).
        let mut a = self.cpu.reg(SP) & !3;
        while Segment::of(a) == Some(Segment::Stack) && a <= memory::STACK_BASE && out.len() < 64 {
            let w = self.mem.peek32(a);
            if w != ra
                && let Some(mut f) = self.called_from(w)
            {
                f.stack_offset = a;
                out.push(f);
            }
            a = match a.checked_add(4) {
                Some(n) if n.wrapping_sub(self.cpu.reg(SP)) <= 4096 => n,
                _ => break,
            };
        }
        out
    }

    fn code_segment(&self) -> (u32, u32) {
        // Flat addresses: the offset is the address.
        (0, u32::MAX)
    }

    fn disassemble(&self, addr: u32, _offset: u32) -> Instruction {
        Instruction { len: 4, text: disasm::disassemble(self.mem.peek32(addr), addr, Style::Named) }
    }

    fn read8(&self, addr: u32) -> u8 {
        self.mem.peek8(addr)
    }

    fn write8(&mut self, addr: u32, value: u8) {
        self.mem.load(addr, &[value])
    }

    fn read_bytes(&self, addr: u32, len: usize) -> Vec<u8> {
        self.mem.read_bytes(addr, len)
    }

    fn data_address(&self, _seg: Option<u64>, off: u64) -> u32 {
        off as u32
    }

    fn registers(&self) -> &'static [Register] {
        &REGISTERS
    }

    fn register(&self, index: usize) -> u64 {
        (match index {
            0..32 => self.cpu.reg(index),
            32..GPR_ENTRIES => self.cpu.reg(index - 32),
            64 => self.cpu.pc,
            65 => self.cpu.hi,
            66 => self.cpu.lo,
            FPR..FCC => self.cpu.fpr[index - FPR],
            FCC => self.cpu.fcc as u32,
            _ => 0,
        }) as u64
    }

    fn set_register(&mut self, index: usize, value: u64) {
        let v = value as u32;
        match index {
            0..32 => self.cpu.set_reg(index, v),
            32..GPR_ENTRIES => self.cpu.set_reg(index - 32, v),
            64 => self.cpu.pc = v,
            65 => self.cpu.hi = v,
            66 => self.cpu.lo = v,
            FPR..FCC => self.cpu.fpr[index - FPR] = v,
            FCC => self.cpu.fcc = v as u8,
            _ => {}
        }
    }

    fn checkpoint(&self) -> State {
        State::new(Checkpoint { cpu: self.cpu.clone() })
    }

    fn save_state(&self) -> State {
        State::new(Snapshot {
            cpu: self.cpu.clone(),
            mem: self.mem.clone(),
            console: self.console.clone(),
            input: self.input.clone(),
            heap: self.heap,
            random: self.random.clone(),
            mmio: self.mmio.clone(),
            lab: self.lab.clone(),
            finished: self.finished,
            loaded: self.loaded,
            waiting: self.waiting,
        })
    }

    fn restore_state(&mut self, state: &State) -> bool {
        let Some(s) = state.get::<Snapshot>() else { return false };
        let s = s.clone();
        self.cpu = s.cpu;
        self.mem = s.mem;
        self.console = s.console;
        self.input = s.input;
        self.heap = s.heap;
        self.random = s.random;
        // The device's settings stay.
        self.lab = s.lab;
        self.mmio = crate::mmio::KeyboardDisplay {
            delay: self.mmio.delay,
            display_after_delay: self.mmio.display_after_delay,
            technique: self.mmio.technique,
            terminal_size: self.mmio.terminal_size,
            ..s.mmio
        };
        self.finished = s.finished;
        self.loaded = s.loaded;
        self.waiting = s.waiting;
        self.sleep_until = None;
        self.clear_history();
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Reaching the MIPS [`Machine`] behind a [`Target`].
pub trait AsMips {
    /// The MIPS machine, when the target is one.
    fn mips(&self) -> Option<&Machine>;
    /// The MIPS machine, when the target is one, to change it.
    fn mips_mut(&mut self) -> Option<&mut Machine>;
}

impl AsMips for dyn Target {
    fn mips(&self) -> Option<&Machine> {
        self.as_any().downcast_ref()
    }

    fn mips_mut(&mut self) -> Option<&mut Machine> {
        self.as_any_mut().downcast_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::assemble;

    fn machine(src: &str) -> Machine {
        let mut m = Machine::new();
        m.instant_waits = true;
        m.load(&assemble(src).unwrap().image).unwrap();
        m
    }

    #[test]
    fn registers_by_name_and_number() {
        let mut m = machine(".text\nli $t0, 5\n");
        let t: &mut dyn Target = &mut m;
        t.step();
        let i = |t: &dyn Target, n: &str| t.register_index(n).unwrap();
        assert_eq!(t.register(i(t, "$t0")), 5);
        assert_eq!(t.register(i(t, "$8")), 5);
        assert_eq!(t.register(i(t, "pc")), 0x0040_0004);
        t.set_register(i(t, "$zero"), 7);
        assert_eq!(t.register(i(t, "$0")), 0, "$zero stays 0");
        t.set_register(i(t, "HI"), 9);
        assert_eq!(t.mips().unwrap().cpu.hi, 9);
        t.set_register(i(t, "$f3"), 1.5f32.to_bits() as u64);
        assert_eq!(t.mips().unwrap().cpu.fpr[3], 0x3fc0_0000);
        t.set_register(i(t, "fcc"), 0x81);
        assert!(t.mips().unwrap().cpu.fcc(7) && t.mips().unwrap().cpu.fcc(0));
        assert_eq!((t.pc_name(), t.pc_text()), ("PC", "0x00400004".to_string()));
    }

    #[test]
    fn calls_returns_and_the_call_stack() {
        let mut m = machine(
            ".text\nmain: jal f\nli $v0, 10\nsyscall\nf: addiu $sp, $sp, -4\nsw $ra, 0($sp)\njal g\nlw $ra, 0($sp)\njr $ra\ng: jr $ra\n",
        );
        let t: &mut dyn Target = &mut m;
        assert_eq!(t.flow_at(0x0040_0000), (Flow::Call, 4));
        assert_eq!(t.step_over_stop(), Some(0x0040_0004));
        for _ in 0..4 {
            t.step();
        }
        // In g: $ra -> after "jal g", the stack -> after "jal f".
        let frames = t.call_stack();
        let rets: Vec<u32> = frames.iter().map(|f| f.return_addr).collect();
        assert_eq!(rets, [0x0040_0018, 0x0040_0004]);
        assert_eq!(frames[0].target, Some(0x0040_0020));
        assert_eq!(t.flow_at(t.pc()), (Flow::Return, 4));
        assert_eq!(t.disassemble(0x0040_0000, 0).text, "jal 0x0040000c");
    }

    #[test]
    fn snapshots() {
        let mut m = machine(".text\nli $v0, 1\nli $a0, 42\nsyscall\nli $t0, 1\n");
        let t: &mut dyn Target = &mut m;
        t.step();
        let saved = t.save_state();
        t.run(10);
        assert_eq!(t.output(), b"42");
        assert!(t.restore_state(&saved));
        assert_eq!((t.output(), t.pc()), (&b""[..], 0x0040_0004));
        assert_eq!(t.terminated(), None);
    }
}
