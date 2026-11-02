//! The boundary between an emulated processor and the rest of RustEmu8086.
//!
//! The session (source-level debugging) and the architecture-neutral parts of the interface (run
//! loop, breakpoints, watches, call stack, disassembly list) work with a [`Target`]: whatever the
//! processor is, it steps, keeps a step-back history, stops at breakpoint addresses, has memory at
//! 32-bit addresses and registers with names. Everything specific to a processor — how a program
//! is loaded, its services (BIOS/DOS for the 8086), devices, the windows that show its registers —
//! stays with that processor's crate and is reached by downcasting ([`Target::as_any`]).
//!
//! Addresses are physical: the 8086 maps `segment:offset` to a 20-bit address, a flat 32-bit
//! architecture uses its addresses as they are.

#![warn(missing_docs)]

use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;

/// Processor architectures RustEmu8086 emulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    /// Intel 8086 (with the 80186 extensions Emu8086 programs use) and the 8087, running under
    /// emulated BIOS and DOS services.
    I8086,
}

impl Arch {
    /// Short name for the user ("8086").
    pub fn name(self) -> &'static str {
        match self {
            Arch::I8086 => "8086",
        }
    }
}

/// Result of [`Target::step`] / [`Target::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Instruction executed, nothing special.
    Continue,
    /// Execution is blocked until a key is pressed.
    WaitingInput,
    /// A timed wait (8086: INT 15h/86h) is in progress; run again later.
    Sleeping,
    /// The processor halted (8086: HLT, waiting for an interrupt).
    Halted,
    /// Program terminated with an exit code.
    Terminated(u8),
    /// Breakpoint reached (or a breakpoint instruction executed, 8086: INT 3).
    Breakpoint,
    /// Instruction budget of [`Target::run`] used up.
    Budget,
    /// Fatal error (divide error, unsupported service...); execution stops.
    Error(String),
}

/// How an instruction changes the flow of control, for Step Over / Step Out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Enters a subroutine or an interrupt handler and comes back after it (8086: CALL, INT).
    Call,
    /// Returns from a subroutine or handler (8086: RET, RETF, IRET).
    Return,
    /// Anything else.
    Other,
}

/// A decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Length in bytes.
    pub len: u32,
    /// Mnemonic with operands in the architecture's notation, e.g. `MOV AX, 0005h`.
    pub text: String,
}

/// What a register is, for debugger expressions and register views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegKind {
    /// A general-purpose register (or a part of one, like AL).
    General,
    /// A segment register: it can prefix a memory reference (`es:[di]`).
    Segment,
    /// The instruction pointer / program counter.
    Pointer,
    /// The whole flags (status) register.
    Flags,
    /// One flag (its value is 0 or 1).
    Flag,
}

/// A register the debugger reads and writes by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register {
    /// Upper-case name, as typed in expressions (`AX`, `AL`, `CF`).
    pub name: &'static str,
    /// Width in bits (1 for a flag).
    pub bits: u8,
    /// What the register is.
    pub kind: RegKind,
}

/// A return address found on the stack (see [`Target::call_stack`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// A call or an interrupt.
    pub kind: FrameKind,
    /// Where execution continues after the return.
    pub return_addr: u32,
    /// The calling instruction.
    pub call_addr: u32,
    /// The called address when it is known (direct calls).
    pub target: Option<u32>,
    /// Where the frame is on the stack (8086: the SP offset in the stack segment).
    pub stack_offset: u32,
}

/// How a [`Frame`] was entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// A subroutine call (8086: near or far CALL).
    Call,
    /// An interrupt or system call with its number (8086: INT `n`).
    Interrupt(u8),
}

/// A value saved by a target and given back to it (machine snapshots, the state before a step).
/// Its contents are known only to the target that made it ([`State::get`]).
#[derive(Clone)]
pub struct State(Arc<dyn Any + Send + Sync>);

impl State {
    /// Wraps a target's own value.
    pub fn new<T: Any + Send + Sync>(value: T) -> State {
        State(Arc::new(value))
    }

    /// The value, when it is a `T`.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("State(..)")
    }
}

/// An emulated computer: a processor with its memory and whatever runs around it.
///
/// Implemented by each architecture (`rustemu_core::Machine` for the 8086). Code that needs more
/// than this reaches the concrete machine with [`Target::as_any`].
pub trait Target: Any + Send {
    /// Which processor this is.
    fn arch(&self) -> Arch;

    // ---- execution ----

    /// Executes one instruction (a call into the emulator's own system code completes within it).
    fn step(&mut self) -> Event;
    /// Runs up to `budget` instructions, stopping at breakpoints and events.
    fn run(&mut self, budget: u64) -> Event;
    /// Like [`Target::run`], also returning the number of instructions executed.
    fn run_counted(&mut self, budget: u64) -> (Event, u64);
    /// Undoes the last instruction; false when the history is empty.
    fn step_back(&mut self) -> bool;
    /// Number of instructions that can be undone.
    fn history_len(&self) -> usize;
    /// Forgets the step-back history.
    fn clear_history(&mut self);
    /// Turns recording of the step-back history on or off (off: faster, nothing to undo).
    fn set_history(&mut self, on: bool);
    /// Physical addresses where execution stops.
    fn breakpoints(&self) -> &HashSet<u32>;
    /// Physical addresses where execution stops, to change them.
    fn breakpoints_mut(&mut self) -> &mut HashSet<u32>;
    /// Instructions executed since the program was loaded.
    fn instr_count(&self) -> u64;

    // ---- the program ----

    /// A program is loaded.
    fn is_loaded(&self) -> bool;
    /// Physical address of the first byte of the loaded image.
    fn image_base(&self) -> Option<u32>;
    /// The exit code once the program has terminated.
    fn terminated(&self) -> Option<u8>;
    /// Everything the program has written to its console, in the machine's character set.
    fn output(&self) -> &[u8];
    /// The program is blocked waiting for keyboard input.
    fn is_waiting_input(&self) -> bool;
    /// Keys are queued that the program has not read yet.
    fn has_pending_input(&self) -> bool;
    /// Types a character (in the machine's character set) on the keyboard.
    fn push_char(&mut self, c: u8);
    /// Powers the machine on again with nothing loaded.
    fn reset(&mut self);

    // ---- code ----

    /// Physical address of the next instruction.
    fn pc(&self) -> u32;
    /// Name of the instruction pointer for the user (`CS:IP`).
    fn pc_name(&self) -> &'static str;
    /// The instruction pointer as the user reads it (`0700:0100`).
    fn pc_text(&self) -> String;
    /// While the emulator's own system code runs (8086: a BIOS/DOS service, e.g. waiting for a
    /// key), the address of the program instruction that called it.
    fn system_caller(&self) -> Option<u32>;
    /// How the instruction at `addr` changes the flow of control, and its length.
    fn flow_at(&self, addr: u32) -> (Flow, u32);
    /// Where Step Over without debug information stops: the address after the current
    /// instruction when it should run to completion (8086: CALL, INT, LOOP, REP), else None.
    fn step_over_stop(&self) -> Option<u32>;
    /// Return addresses on the stack, innermost first (a heuristic walk).
    fn call_stack(&self) -> Vec<Frame>;
    /// Where the disassembly list scrolls: the physical address of offset 0 and the largest
    /// offset (8086: the code segment, offsets 0..FFFFh).
    fn code_segment(&self) -> (u32, u32);
    /// Decodes the instruction at `addr`. `offset` is the address as the program sees it, used
    /// for jump targets (8086: the offset in the code segment).
    fn disassemble(&self, addr: u32, offset: u32) -> Instruction;

    // ---- memory ----

    /// The byte at a physical address.
    fn read8(&self, addr: u32) -> u8;
    /// Writes a byte at a physical address (recorded for step back while executing).
    fn write8(&mut self, addr: u32, value: u8);
    /// Up to `len` bytes from `addr` (shorter at the end of memory).
    fn read_bytes(&self, addr: u32, len: usize) -> Vec<u8>;
    /// Physical address of a debugger memory reference: `[off]` (`seg` None) or `seg:[off]`.
    /// The 8086 uses DS when no segment is given.
    fn data_address(&self, seg: Option<u64>, off: u64) -> u32;

    // ---- registers ----

    /// Registers the debugger knows by name, including parts of registers and single flags.
    fn registers(&self) -> &'static [Register];
    /// Value of register `index` of [`Target::registers`].
    fn register(&self, index: usize) -> u64;
    /// Sets register `index` of [`Target::registers`] (a flag takes bit 0 of `value`).
    fn set_register(&mut self, index: usize, value: u64);

    // ---- snapshots ----

    /// What the interface compares after an execution command to highlight changes (registers,
    /// coprocessor).
    fn checkpoint(&self) -> State;
    /// The whole machine, to come back to it with [`Target::restore_state`].
    fn save_state(&self) -> State;
    /// Returns to a [`Target::save_state`] snapshot (breakpoints are kept); false when the
    /// snapshot belongs to another kind of machine.
    fn restore_state(&mut self, state: &State) -> bool;

    // ---- the concrete machine ----

    /// The machine itself, to downcast to its type.
    fn as_any(&self) -> &dyn Any;
    /// The machine itself, to downcast to its type and change it.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl dyn Target {
    /// Index of the register named `name` (case-insensitive) in [`Target::registers`].
    pub fn register_index(&self, name: &str) -> Option<usize> {
        self.registers().iter().position(|r| r.name.eq_ignore_ascii_case(name))
    }
}
