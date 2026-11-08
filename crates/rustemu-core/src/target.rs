//! The 8086 machine as a [`Target`]: what the session and the architecture-neutral interface use,
//! plus the 8086 knowledge they used to have themselves (the INT that called a BIOS service, the
//! CALL / INT frames on the stack, which instructions Step Over runs to completion).

use std::any::Any;
use std::collections::HashSet;

use rustemu_target::{Arch, Event, Flow, Frame, FrameKind, Instruction, RegKind, Register, State, Target};

use crate::bus::linear;
use crate::cpu::{self, Cpu, flags};
use crate::fpu::Fpu;
use crate::machine::{Machine, MachineState, ROM_SEG};

/// The 8086 state an execution command starts from ([`Target::checkpoint`]): the interface compares
/// it with the state after the command to highlight what changed.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// Registers and flags.
    pub cpu: Cpu,
    /// The 8087.
    pub fpu: Fpu,
}

/// Registers of debugger expressions. Indices: 0..8 the word registers in [`cpu::REG16_NAMES`]
/// order, 8..16 the byte registers ([`cpu::REG8_NAMES`]), 16..20 the segment registers
/// ([`cpu::SREG_NAMES`]), then IP, FLAGS (also `FL`) and the nine flags.
const REGISTERS: &[Register] = &[
    Register { name: "AX", bits: 16, kind: RegKind::General },
    Register { name: "CX", bits: 16, kind: RegKind::General },
    Register { name: "DX", bits: 16, kind: RegKind::General },
    Register { name: "BX", bits: 16, kind: RegKind::General },
    Register { name: "SP", bits: 16, kind: RegKind::General },
    Register { name: "BP", bits: 16, kind: RegKind::General },
    Register { name: "SI", bits: 16, kind: RegKind::General },
    Register { name: "DI", bits: 16, kind: RegKind::General },
    Register { name: "AL", bits: 8, kind: RegKind::General },
    Register { name: "CL", bits: 8, kind: RegKind::General },
    Register { name: "DL", bits: 8, kind: RegKind::General },
    Register { name: "BL", bits: 8, kind: RegKind::General },
    Register { name: "AH", bits: 8, kind: RegKind::General },
    Register { name: "CH", bits: 8, kind: RegKind::General },
    Register { name: "DH", bits: 8, kind: RegKind::General },
    Register { name: "BH", bits: 8, kind: RegKind::General },
    Register { name: "ES", bits: 16, kind: RegKind::Segment },
    Register { name: "CS", bits: 16, kind: RegKind::Segment },
    Register { name: "SS", bits: 16, kind: RegKind::Segment },
    Register { name: "DS", bits: 16, kind: RegKind::Segment },
    Register { name: "IP", bits: 16, kind: RegKind::Pointer },
    Register { name: "FLAGS", bits: 16, kind: RegKind::Flags },
    Register { name: "FL", bits: 16, kind: RegKind::Flags },
    Register { name: "CF", bits: 1, kind: RegKind::Flag },
    Register { name: "PF", bits: 1, kind: RegKind::Flag },
    Register { name: "AF", bits: 1, kind: RegKind::Flag },
    Register { name: "ZF", bits: 1, kind: RegKind::Flag },
    Register { name: "SF", bits: 1, kind: RegKind::Flag },
    Register { name: "TF", bits: 1, kind: RegKind::Flag },
    Register { name: "IF", bits: 1, kind: RegKind::Flag },
    Register { name: "DF", bits: 1, kind: RegKind::Flag },
    Register { name: "OF", bits: 1, kind: RegKind::Flag },
];

/// Flag masks of the flag entries of [`REGISTERS`], in order.
const FLAG_MASKS: [u16; 9] =
    [flags::CF, flags::PF, flags::AF, flags::ZF, flags::SF, flags::TF, flags::IF, flags::DF, flags::OF];
/// Index of the first flag in [`REGISTERS`].
const FIRST_FLAG: usize = 23;

impl Machine {
    /// While CS:IP is in the BIOS/DOS code: CS:IP of the `INT nn` that called it (the return
    /// address on the stack, less the two bytes of the instruction).
    pub fn bios_caller(&self) -> Option<(u16, u16)> {
        if self.cpu.cs() != ROM_SEG {
            return None;
        }
        let (ss, sp) = (self.cpu.sregs[cpu::SS], self.cpu.regs[cpu::SP]);
        let ip = self.sys.mem.read16(linear(ss, sp));
        let cs = self.sys.mem.read16(linear(ss, sp.wrapping_add(2)));
        Some((cs, ip.wrapping_sub(2)))
    }

    /// Return addresses on the stack, innermost first (a heuristic walk: a word counts as a return
    /// address when the bytes before it are a CALL, or an `INT n` for an interrupt frame).
    pub fn call_frames(&self) -> Vec<Frame> {
        let mem = &self.sys.mem;
        let ss = self.cpu.sregs[cpu::SS];
        let sp = self.cpu.regs[cpu::SP];
        let (mut inner_cs, mut inner_ip, sp) = if self.cpu.cs() == ROM_SEG {
            // Inside a BIOS/DOS service: the INT frame on top is the current position.
            (mem.read16(linear(ss, sp.wrapping_add(2))), mem.read16(linear(ss, sp)).wrapping_sub(2), sp.wrapping_add(6))
        } else {
            (self.cpu.cs(), self.cpu.ip, sp)
        };
        let mut out = Vec::new();
        let mut off = sp;
        while off < 0xFFFE && out.len() < 64 {
            let w = mem.read16(linear(ss, off));
            let b = |cs: u16, ip: u16, back: u16| mem.read8(linear(cs, ip.wrapping_sub(back)));
            let far_cs = mem.read16(linear(ss, off.wrapping_add(2)));
            // Near CALL rel16: the target must be at or before the code that runs inside.
            let frame = if b(inner_cs, w, 3) == 0xE8 && {
                let target = w.wrapping_add(mem.read16(linear(inner_cs, w.wrapping_sub(2))));
                target <= inner_ip && inner_ip - target < 0x4000
            } {
                let target = w.wrapping_add(mem.read16(linear(inner_cs, w.wrapping_sub(2))));
                Some((FrameKind::Call, inner_cs, w, 3, Some(linear(inner_cs, target)), 2))
            } else if b(inner_cs, w, 2) == 0xFF && (b(inner_cs, w, 1) & 0xF8) == 0xD0 {
                // CALL reg16
                Some((FrameKind::Call, inner_cs, w, 2, None, 2))
            } else if off < 0xFFFA
                && b(far_cs, w, 2) == 0xCD
                && mem.read16(linear(ss, off.wrapping_add(4))) & 0x0002 != 0
            {
                Some((FrameKind::Interrupt(b(far_cs, w, 1)), far_cs, w, 2, None, 6))
            } else if b(far_cs, w, 5) == 0x9A && mem.read16(linear(far_cs, w.wrapping_sub(2))) == inner_cs {
                let t = mem.read16(linear(far_cs, w.wrapping_sub(4)));
                Some((FrameKind::Call, far_cs, w, 5, Some(linear(inner_cs, t)), 4))
            } else {
                None
            };
            match frame {
                Some((kind, cs, ret, len, target, size)) => {
                    out.push(Frame {
                        kind,
                        return_addr: linear(cs, ret),
                        call_addr: linear(cs, ret.wrapping_sub(len)),
                        target,
                        stack_offset: off as u32,
                    });
                    inner_cs = cs;
                    inner_ip = ret.wrapping_sub(len);
                    off = off.wrapping_add(size);
                }
                None => off = off.wrapping_add(2),
            }
        }
        out
    }

    /// Classifies the instruction at `pc` (skipping prefixes) and returns its length.
    pub fn flow_at(&self, pc: u32) -> (Flow, u32) {
        let bytes = self.sys.mem.slice(pc, 8);
        let ins = crate::disasm::disassemble(bytes, self.cpu.ip);
        let op =
            bytes.iter().copied().find(|b| !matches!(b, 0x26 | 0x2E | 0x36 | 0x3E | 0xF0 | 0xF2 | 0xF3)).unwrap_or(0);
        let modrm_reg = bytes.iter().position(|&b| b == op).and_then(|i| bytes.get(i + 1)).map(|m| (m >> 3) & 7);
        let flow = match op {
            0xE8 | 0x9A | 0xCC | 0xCD | 0xCE => Flow::Call,
            0xFF if matches!(modrm_reg, Some(2) | Some(3)) => Flow::Call,
            0xC0..=0xC3 | 0xC8..=0xCB | 0xCF => Flow::Return,
            _ => Flow::Other,
        };
        (flow, ins.len as u32)
    }
}

impl Target for Machine {
    fn resources(&self) -> rustemu_target::resources::ResourceBudget {
        use rustemu_target::resources::{Resource, ResourceBudget};
        let mut b = ResourceBudget::default();
        b.record(Resource::HistoryBytes, self.history_bytes());
        b.record(Resource::HistorySteps, self.history_len());
        b.record(Resource::OutputBytes, self.output.bytes().len());
        b.record(Resource::DeviceBytes, self.sys.dev.printer.bytes().len());
        b.record(Resource::FileWriteBytes, self.dos.host_written as usize);
        b.record(Resource::CreatedFiles, self.dos.host_created as usize);
        if let Some(d) = self.dos_memory_drive() {
            b.record(Resource::DriveBytes, d.used());
            b.record(Resource::DriveEntries, d.files().count() + d.dirs().count());
        }
        b
    }

    fn arch(&self) -> Arch {
        Arch::I8086
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
        self.loaded.is_some()
    }

    fn image_base(&self) -> Option<u32> {
        self.loaded.as_ref().map(|l| l.image_base)
    }

    fn terminated(&self) -> Option<u8> {
        self.terminated
    }

    fn output(&self) -> &[u8] {
        self.output.bytes()
    }

    fn output_dropped(&self) -> usize {
        self.output.dropped()
    }

    fn is_waiting_input(&self) -> bool {
        Machine::is_waiting_input(self)
    }

    fn has_pending_input(&self) -> bool {
        !self.sys.keys.is_empty()
    }

    fn push_char(&mut self, c: u8) {
        Machine::push_char(self, c)
    }

    fn reset(&mut self) {
        Machine::reset(self)
    }

    fn pc(&self) -> u32 {
        self.cpu.pc()
    }

    fn pc_name(&self) -> &'static str {
        "CS:IP"
    }

    fn pc_text(&self) -> String {
        format!("{:04X}:{:04X}", self.cpu.cs(), self.cpu.ip)
    }

    fn system_caller(&self) -> Option<u32> {
        self.bios_caller().map(|(cs, ip)| linear(cs, ip))
    }

    fn flow_at(&self, addr: u32) -> (Flow, u32) {
        Machine::flow_at(self, addr)
    }

    fn step_over_stop(&self) -> Option<u32> {
        let ins = crate::disasm::disassemble(self.sys.mem.slice(self.cpu.pc(), 8), self.cpu.ip);
        let t = ins.text.as_str();
        let over = t.starts_with("CALL") || t.starts_with("INT") || t.starts_with("LOOP") || t.starts_with("REP");
        over.then(|| linear(self.cpu.cs(), self.cpu.ip.wrapping_add(ins.len as u16)))
    }

    fn call_stack(&self) -> Vec<Frame> {
        self.call_frames()
    }

    fn code_segment(&self) -> (u32, u32) {
        (linear(self.cpu.cs(), 0), 0xFFFF)
    }

    fn disassemble(&self, addr: u32, offset: u32) -> Instruction {
        let ins = crate::disasm::disassemble(self.sys.mem.slice(addr, 8), offset as u16);
        Instruction { len: ins.len as u32, text: ins.text }
    }

    fn read8(&self, addr: u32) -> u8 {
        self.sys.mem.read8(addr)
    }

    fn write8(&mut self, addr: u32, value: u8) {
        self.sys.mem.write8(addr, value)
    }

    fn read_bytes(&self, addr: u32, len: usize) -> Vec<u8> {
        self.sys.mem.slice(addr, len).to_vec()
    }

    fn data_address(&self, seg: Option<u64>, off: u64) -> u32 {
        let seg = seg.map_or(self.cpu.sregs[cpu::DS], |s| s as u16);
        linear(seg, off as u16)
    }

    fn registers(&self) -> &'static [Register] {
        REGISTERS
    }

    fn register(&self, index: usize) -> u64 {
        let c = &self.cpu;
        (match index {
            0..8 => c.regs[index],
            8..16 => c.reg8((index - 8) as u8) as u16,
            16..20 => c.sregs[index - 16],
            20 => c.ip,
            21 | 22 => c.flags,
            _ => match FLAG_MASKS.get(index - FIRST_FLAG) {
                Some(&f) => (c.flags & f != 0) as u16,
                None => 0,
            },
        }) as u64
    }

    fn set_register(&mut self, index: usize, value: u64) {
        let c = &mut self.cpu;
        let v = value as u16;
        match index {
            0..8 => c.regs[index] = v,
            8..16 => c.set_reg8((index - 8) as u8, v as u8),
            16..20 => c.sregs[index - 16] = v,
            20 => c.ip = v,
            21 | 22 => c.set_flags_word(v),
            _ => {
                if let Some(&f) = FLAG_MASKS.get(index - FIRST_FLAG) {
                    let word = if v & 1 != 0 { c.flags | f } else { c.flags & !f };
                    c.set_flags_word(word);
                }
            }
        }
    }

    fn checkpoint(&self) -> State {
        State::new(Checkpoint { cpu: self.cpu.clone(), fpu: self.sys.fpu.clone() })
    }

    fn save_state(&self) -> State {
        State::new(Machine::save_state(self))
    }

    fn restore_state(&mut self, state: &State) -> bool {
        let Some(s) = state.get::<MachineState>() else { return false };
        Machine::restore_state(self, s.clone());
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Reaching the 8086 [`Machine`] behind a [`Target`].
pub trait AsMachine {
    /// The 8086 machine, when the target is one.
    fn x86(&self) -> Option<&Machine>;
    /// The 8086 machine, when the target is one, to change it.
    fn x86_mut(&mut self) -> Option<&mut Machine>;
}

impl AsMachine for dyn Target {
    fn x86(&self) -> Option<&Machine> {
        self.as_any().downcast_ref()
    }

    fn x86_mut(&mut self) -> Option<&mut Machine> {
        self.as_any_mut().downcast_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_read_and_write_by_name() {
        let mut m = Machine::new();
        let t: &mut dyn Target = &mut m;
        let i = |t: &dyn Target, n: &str| t.register_index(n).unwrap();
        let ax = i(t, "ax");
        t.set_register(ax, 0x1234);
        assert_eq!(t.register(i(t, "AH")), 0x12);
        assert_eq!(t.register(i(t, "AL")), 0x34);
        t.set_register(i(t, "BH"), 0xAB);
        assert_eq!(t.x86().unwrap().cpu.regs[cpu::BX], 0xAB00);
        t.set_register(i(t, "ZF"), 1);
        assert_eq!(t.register(i(t, "ZF")), 1);
        assert_ne!(t.register(i(t, "FL")) as u16 & flags::ZF, 0);
        t.set_register(i(t, "ZF"), 0);
        assert_eq!(t.register(i(t, "FLAGS")) as u16 & flags::ZF, 0);
        t.set_register(i(t, "DS"), 0x0700);
        assert_eq!(t.data_address(None, 0x10), 0x7010);
        assert_eq!(t.data_address(Some(0x10), 0x1_0005), 0x105);
        assert_eq!(t.registers()[FIRST_FLAG].name, "CF");
        assert_eq!(REGISTERS.len(), FIRST_FLAG + FLAG_MASKS.len());
    }
}
