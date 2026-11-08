//! The MIPS machine: the core with its memory, the text console, files and MARS's system
//! services, and the step-back history.
//!
//! A step runs one instruction. `syscall` performs a service of MARS (see [`crate::syscall`]); a
//! service that needs input which has not been typed yet puts the program counter back on the
//! `syscall` and reports [`Event::WaitingInput`]: the step is repeated once keys arrive, and such
//! empty retries are not recorded in the history.
//!
//! Step back undoes the instruction (registers, memory, and for `syscall` the console, the input,
//! the heap and the random generators). File operations cannot be undone: the history stops there.
//! The history keeps at most [`HISTORY_DEPTH`] steps and [`HISTORY_BYTES`] of accounted snapshots and memory journal.

use std::collections::{BTreeMap, HashSet, VecDeque};

use rustemu_target::Event;

use crate::console::Console;
use crate::cpu::{Cpu, Step};
use crate::files::{Files, Storage};
use crate::memory::{self, Memory, Segment};
use crate::mmio::{DeviceState, DigitalLab, DisplayUndo, KeyboardDisplay, LabState};
use crate::random::JavaRandom;
use crate::settings::Settings;

/// Instructions step back can undo.
pub const HISTORY_DEPTH: usize =
    rustemu_target::resources::ResourceBudget::limit(rustemu_target::resources::Resource::HistorySteps);
/// Bytes of snapshots and their allocated payloads kept at most (older steps are forgotten).
pub const HISTORY_BYTES: usize =
    rustemu_target::resources::ResourceBudget::limit(rustemu_target::resources::Resource::HistoryBytes);
/// Maximum independent random streams in a machine (syscalls 40..44).
pub const MAX_RANDOM_STREAMS: usize =
    rustemu_target::resources::ResourceBudget::limit(rustemu_target::resources::Resource::RandomStreams);

/// How the program ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// `exit` (syscall 10, code 0) or `exit2` (syscall 17 with the code in `$a0`).
    Exit(i32),
    /// Execution ran past the last instruction (MARS: "dropped off bottom"), a normal end.
    DroppedOff,
}

impl Finish {
    /// The exit code (truncated to a byte, as a process exit code).
    pub fn code(self) -> u8 {
        match self {
            Finish::Exit(c) => c as u8,
            Finish::DroppedOff => 0,
        }
    }
}

/// A part of a program to load: bytes at an address, code or data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
    /// Where the bytes go.
    pub addr: u32,
    /// The bytes (instructions are little-endian words).
    pub bytes: Vec<u8>,
    /// The bytes are instructions (in the text or kernel text segment).
    pub code: bool,
}

/// A program ready to load: its parts and where execution starts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Image {
    /// Code and data.
    pub chunks: Vec<Chunk>,
    /// The first instruction executed (usually the start of `.text`).
    pub entry: u32,
    /// The settings the program was assembled with; the machine runs it with them (memory
    /// configuration, delayed branching, self-modifying code).
    pub settings: Settings,
}

/// What a `syscall` changes outside registers and memory, saved to step back over it.
#[derive(Debug, Clone)]
struct SysState {
    console_len: usize,
    input: Vec<u8>,
    heap: u32,
    random: Option<(i32, Option<JavaRandom>)>,
    waiting: bool,
}

/// The keyboard and display before a step that changed them.
#[derive(Debug, Clone)]
struct DeviceUndo {
    state: DeviceState,
    lab: LabState,
    /// How to undo the step's change of the display text.
    display: Option<DisplayUndo>,
}

/// One instruction of the history.
#[derive(Debug, Clone)]
struct Snapshot {
    cpu: Cpu,
    /// Number of its entries at the end of [`Machine::journal`] (no allocation per step).
    undo: usize,
    finished: Option<Finish>,
    sys: Option<Box<SysState>>,
    /// The keyboard and display before the step, when the step changed them.
    device: Option<Box<DeviceUndo>>,
    /// Number of the memory's code ranges before the step (a self-modifying store adds one).
    code: usize,
}

impl Snapshot {
    /// Accounts inline state and heap allocations owned by this step.
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.undo * ENTRY
            + self.sys.as_ref().map_or(0, |s| std::mem::size_of::<SysState>() + s.input.capacity())
            + self.device.as_ref().map_or(0, |d| {
                std::mem::size_of::<DeviceUndo>()
                    + match &d.display {
                        Some(DisplayUndo::Text(bytes)) => bytes.capacity(),
                        _ => 0,
                    }
            })
    }
}

/// Bytes of one journal entry.
const ENTRY: usize = std::mem::size_of::<(u32, u8)>();

/// The emulated MIPS computer.
#[derive(Debug)]
pub struct Machine {
    /// The processor.
    pub cpu: Cpu,
    /// The memory.
    pub mem: Memory,
    /// The text console: output and echoed input.
    pub console: Console,
    /// Keys typed and not read yet (UTF-8 bytes; Enter is `\n`).
    pub(crate) input: VecDeque<u8>,
    /// The next free address of the heap (`sbrk`, syscall 9).
    pub heap: u32,
    /// Files of the program (syscalls 13..16).
    pub files: Files,
    /// Why the last file service returned −1 (MARS keeps this message to itself; the interface
    /// shows it).
    pub file_error: Option<String>,
    /// Random generators by stream number (syscalls 40..44).
    pub(crate) random: BTreeMap<i32, JavaRandom>,
    /// MARS's keyboard and display in memory-mapped I/O.
    pub mmio: KeyboardDisplay,
    /// MARS's Digital Lab Sim in memory-mapped I/O.
    pub lab: DigitalLab,
    /// How the program ended, once it has.
    pub finished: Option<Finish>,
    /// A program is loaded.
    pub loaded: bool,
    /// Physical addresses where execution stops.
    pub breakpoints: HashSet<u32>,
    /// Steps are recorded for step back (off: faster).
    pub history_enabled: bool,
    history: VecDeque<Snapshot>,
    history_bytes: usize,
    /// Consumed bytes recorded only during the current instruction.
    input_undo: Vec<u8>,
    /// The overwritten bytes of every step in the history, oldest first: each [`Snapshot`] owns
    /// its last `undo` entries.
    journal: VecDeque<(u32, u8)>,
    /// A step did something that cannot be undone: the history is cleared after it.
    pub(crate) barrier: bool,
    /// The program waits for input (an input `syscall` found no complete input).
    pub(crate) waiting: bool,
    /// A `sleep` (syscall 32) lasts until then.
    pub(crate) sleep_until: Option<web_time::Instant>,
    /// Sleeps end at once (tests, the command line).
    pub instant_waits: bool,
    /// The time syscall 30 reports, in milliseconds since 1970 (None: the computer's clock).
    pub fixed_time: Option<u64>,
    /// The settings of the loaded program (see [`Image::settings`]).
    pub settings: Settings,
    /// The dialog (syscall 50..59) the program waits on (see [`crate::dialog`]).
    pub(crate) dialog: Option<crate::dialog::Dialog>,
    /// The answer to it, for the repeated step.
    pub(crate) dialog_answer: Option<crate::dialog::DialogAnswer>,
    /// Dialogs without windows: messages to the console, answers from lines of input (the
    /// command line).
    pub dialogs_in_console: bool,
    /// Arguments given to the program (MARS's "program arguments"): stored under the stack at
    /// each load, `$a0` = their number, `$a1` = the address of the array of their addresses.
    pub program_arguments: Vec<String>,
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

impl Machine {
    /// A machine after a reset, with no files.
    pub fn new() -> Machine {
        Machine::with_storage(Storage::None)
    }

    /// A machine whose programs keep their files in `storage`.
    pub fn with_storage(storage: Storage) -> Machine {
        Machine {
            cpu: Cpu::new(),
            mem: Memory::new(),
            console: Console::new(),
            input: VecDeque::new(),
            heap: memory::HEAP_BASE,
            files: Files::new(storage),
            file_error: None,
            random: BTreeMap::new(),
            mmio: KeyboardDisplay::new(),
            lab: DigitalLab::default(),
            finished: None,
            loaded: false,
            breakpoints: HashSet::new(),
            history_enabled: true,
            history: VecDeque::new(),
            history_bytes: 0,
            input_undo: Vec::new(),
            journal: VecDeque::new(),
            barrier: false,
            waiting: false,
            sleep_until: None,
            instant_waits: false,
            fixed_time: None,
            settings: Settings::default(),
            dialog: None,
            dialog_answer: None,
            dialogs_in_console: false,
            program_arguments: Vec::new(),
        }
    }

    /// Powers the machine on again: nothing loaded, the console empty, files closed (the storage,
    /// the breakpoints and the settings stay).
    pub fn reset(&mut self) {
        let layout = self.settings.layout();
        self.cpu = Cpu::with_layout(layout);
        self.cpu.delayed_branching = self.settings.delayed_branching;
        self.mem.clear();
        self.mem.layout = *layout;
        self.mem.self_modifying = self.settings.self_modifying;
        self.console.clear();
        self.input.clear();
        self.heap = layout.heap_base;
        self.files.reset();
        self.file_error = None;
        self.random.clear();
        self.finished = None;
        self.loaded = false;
        self.clear_history();
        self.waiting = false;
        self.sleep_until = None;
        self.dialog = None;
        self.dialog_answer = None;
        self.mmio.reset(&mut self.mem);
        self.lab.reset();
    }

    /// Loads a program after a reset, with the settings it was assembled with.
    pub fn load(&mut self, image: &Image) -> Result<(), String> {
        let layout = image.settings.layout();
        for c in &image.chunks {
            let end = c.addr.checked_add(c.bytes.len() as u32).ok_or("the program does not fit in memory")?;
            let seg = layout.segment(c.addr);
            let fits = c.bytes.is_empty() || (seg.is_some() && layout.segment(end - 1) == seg);
            if !fits || (c.code && !seg.is_some_and(Segment::is_code)) {
                return Err(format!(
                    "{} bytes at {} are outside the {} segment",
                    c.bytes.len(),
                    memory::hex(c.addr),
                    if c.code { "text" } else { "data" }
                ));
            }
        }
        self.settings = image.settings;
        self.reset();
        for c in &image.chunks {
            self.mem.load(c.addr, &c.bytes);
            if c.code {
                self.mem.add_code(c.addr, c.addr + c.bytes.len() as u32);
            }
        }
        self.cpu.pc = image.entry;
        self.store_program_arguments();
        self.loaded = true;
        Ok(())
    }

    /// Stores [`Machine::program_arguments`] as MARS's `ProgramArgumentList`: the strings from
    /// the stack base down (each with its zero byte), below them (or below `$sp` when they fit
    /// above it) the array of their addresses ending with 0 and their number; `$sp` points to the
    /// number, `$a0` holds it, `$a1` the address of the array.
    fn store_program_arguments(&mut self) {
        if self.program_arguments.is_empty() {
            return;
        }
        let layout = self.settings.layout();
        // (MARS's int arithmetic.)
        let mut high = layout.stack_base as i32;
        let mut starts = Vec::new();
        for arg in &self.program_arguments {
            self.mem.load(high as u32, &[0]);
            high -= 1;
            for &b in arg.as_bytes().iter().rev() {
                self.mem.load(high as u32, &[b]);
                high -= 1;
            }
            starts.push(high + 1);
        }
        let mut sp = layout.sp as i32;
        if high < sp {
            sp = high - high % 4 - 4;
        }
        let word = |mem: &mut Memory, at: i32, v: u32| mem.load(at as u32, &v.to_le_bytes());
        word(&mut self.mem, sp, 0);
        sp -= 4;
        for &s in starts.iter().rev() {
            word(&mut self.mem, sp, s as u32);
            sp -= 4;
        }
        word(&mut self.mem, sp, starts.len() as u32);
        sp -= 4;
        self.cpu.set_reg(crate::cpu::SP, (sp + 4) as u32);
        self.cpu.set_reg(4, starts.len() as u32);
        self.cpu.set_reg(5, (sp + 8) as u32);
    }

    // ---- input ----

    /// Types a byte (UTF-8 of a character; Enter is `\n`). Backspace (8) takes back the last byte
    /// of an unfinished line.
    pub fn push_byte(&mut self, b: u8) {
        if b == 8 {
            if self.input.back().is_some_and(|&l| l != b'\n') {
                // A whole UTF-8 character.
                while let Some(l) = self.input.pop_back() {
                    if l & 0xC0 != 0x80 {
                        break;
                    }
                }
            }
        } else if self.input.len() < 4096 {
            self.input.push_back(b);
        }
    }

    /// Types `text` (new lines are Enter).
    pub fn push_text(&mut self, text: &str) {
        for b in text.replace("\r\n", "\n").bytes() {
            self.push_byte(if b == b'\r' { b'\n' } else { b });
        }
    }

    /// Input typed and not read yet (shown after the console text while the program waits).
    pub fn pending_input(&self) -> Vec<u8> {
        self.input.iter().copied().collect()
    }

    /// The program waits for input.
    pub fn is_waiting_input(&self) -> bool {
        // An answered dialog is input that is there.
        self.waiting && self.dialog_answer.is_none()
    }

    // ---- execution ----

    /// The instruction at the program counter is a `syscall`.
    fn at_syscall(&self) -> bool {
        self.mem.has_code(self.cpu.pc) && self.mem.peek32(self.cpu.pc) & 0xFC00_003F == 0x0000_000C
    }

    fn sys_state(&self) -> SysState {
        SysState {
            console_len: self.console.len(),
            input: Vec::new(),
            heap: self.heap,
            random: (40..=44).contains(&self.cpu.regs[2]).then(|| {
                let id = self.cpu.regs[4] as i32;
                (id, self.random.get(&id).copied())
            }),
            waiting: self.waiting,
        }
    }

    /// Consumes input, journaling only the removed bytes rather than the entire queue.
    pub(crate) fn consume_input(&mut self, n: usize) -> Vec<u8> {
        let bytes: Vec<u8> = self.input.drain(..n).collect();
        if self.history_enabled && !self.barrier {
            if self.input_undo.len().saturating_add(bytes.len()) > HISTORY_BYTES {
                // The operation still succeeds; its oversized undo is a history barrier.
                self.barrier = true;
                self.input_undo.clear();
            } else {
                self.input_undo.extend_from_slice(&bytes);
            }
        }
        bytes
    }

    /// Executes one instruction.
    pub fn step(&mut self) -> Event {
        if let Some(f) = self.finished {
            return Event::Terminated(f.code());
        }
        if let Some(t) = self.sleep_until {
            if web_time::Instant::now() < t && !self.instant_waits {
                return Event::Sleeping;
            }
            self.sleep_until = None;
        }
        if !self.history_enabled {
            self.mem.set_journaling(false);
            self.barrier = false;
            let ev = self.step_inner();
            if self.mmio.undo.is_some() {
                self.mmio.undo = None;
            }
            return ev;
        }
        self.input_undo.clear();
        let mut snap = Snapshot {
            cpu: self.cpu.clone(),
            undo: 0,
            finished: self.finished,
            sys: self.at_syscall().then(|| Box::new(self.sys_state())),
            device: None,
            code: self.mem.code_ranges(),
        };
        let (device, lab) = (self.mmio.state, self.lab.state);
        self.mem.set_journaling(true);
        self.barrier = false;
        let ev = self.step_inner();
        let undo = self.mem.drain_journal_into(&mut self.journal);
        if let Some(sys) = &mut snap.sys {
            sys.input = std::mem::take(&mut self.input_undo);
        }
        if self.barrier {
            self.mmio.undo = None;
            self.clear_history();
            return ev;
        }
        // Retrying an input syscall that still has no input changes nothing.
        let idle = ev == Event::WaitingInput && undo == 0 && self.cpu == snap.cpu;
        if !idle {
            let display = self.mmio.undo.take();
            let device = (display.is_some() || self.mmio.state != device || self.lab.state != lab)
                .then(|| Box::new(DeviceUndo { state: device, lab, display }));
            let snap = Snapshot { undo, device, ..snap };
            self.history_bytes += snap.bytes();
            self.history.push_back(snap);
            while self.history.len() > HISTORY_DEPTH || self.history_bytes > HISTORY_BYTES {
                let Some(old) = self.history.pop_front() else { break };
                self.history_bytes -= old.bytes();
                self.journal.drain(..old.undo);
            }
        }
        ev
    }

    fn step_inner(&mut self) -> Event {
        let ev = match self.lab.take_interrupt().or_else(|| self.mmio.take_interrupt()) {
            // MARS takes an interrupt in place of the instruction.
            Some(code) => {
                let pc = self.cpu.pc;
                let e = self.cpu.raise(&self.mem, code, "External Interrupt".into(), pc, None, false);
                if e.handled {
                    self.cpu.end_step();
                    Event::Continue
                } else {
                    Event::Error(e.report())
                }
            }
            None => self.execute(),
        };
        // (Nothing to do for the devices in most steps.)
        if self.mmio.is_transmitting() || self.lab.is_counting() || self.mem.mmio.any() {
            let status = self.cpu.cp0[crate::cpu::cp0::STATUS];
            let fetched = ev != Event::WaitingInput;
            self.mmio.after_instruction(&mut self.mem, status, fetched);
            let text = fetched && self.mem.layout.segment(self.cpu.pc) == Some(Segment::Text);
            self.lab.after_instruction(&mut self.mem, status, text);
        }
        ev
    }

    fn execute(&mut self) -> Event {
        match self.cpu.step(&mut self.mem) {
            Step::Ok => Event::Continue,
            Step::DroppedOff => {
                self.finished = Some(Finish::DroppedOff);
                Event::Terminated(0)
            }
            Step::Exception(e) if e.handled => Event::Continue,
            Step::Exception(e) => Event::Error(e.report()),
            Step::Syscall => {
                let ev = self.syscall();
                // The service is over: the end of the step (not when it waits or stops).
                if matches!(ev, Event::Continue | Event::Sleeping) {
                    self.cpu.end_step();
                }
                ev
            }
        }
    }

    /// Clicks key `key` (0..15) of the Digital Lab Sim's hexadecimal keyboard (see
    /// [`crate::mmio::DigitalLab::click`]).
    pub fn lab_click(&mut self, key: u8) {
        let status = self.cpu.cp0[crate::cpu::cp0::STATUS];
        self.lab.click(&mut self.mem, status, key);
    }

    /// Types a key on MARS's MMIO keyboard (see [`crate::mmio`]).
    pub fn mmio_key(&mut self, char: u8) {
        let status = self.cpu.cp0[crate::cpu::cp0::STATUS];
        self.mmio.key(&mut self.mem, status, char);
    }

    /// Runs up to `budget` instructions, stopping at breakpoints and events.
    pub fn run(&mut self, budget: u64) -> Event {
        self.run_counted(budget).0
    }

    /// Like [`Machine::run`], also returning the number of instructions executed.
    pub fn run_counted(&mut self, budget: u64) -> (Event, u64) {
        for n in 0..budget {
            let ev = self.step();
            if ev != Event::Continue {
                return (ev, n);
            }
            // (No hashing per step when there is no breakpoint.)
            if !self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc) {
                return (Event::Breakpoint, n + 1);
            }
        }
        (Event::Budget, budget)
    }

    /// Undoes the last instruction; false when the history is empty.
    pub fn step_back(&mut self) -> bool {
        let Some(s) = self.history.pop_back() else { return false };
        self.history_bytes -= s.bytes();
        for _ in 0..s.undo {
            let (a, v) = self.journal.pop_back().expect("the journal holds the step's entries");
            self.mem.restore(a, v);
        }
        self.mem.truncate_code(s.code);
        self.cpu = s.cpu;
        self.finished = s.finished;
        self.sleep_until = None;
        self.dialog = None;
        self.dialog_answer = None;
        if let Some(d) = s.device {
            self.mmio.state = d.state;
            self.lab.state = d.lab;
            if let Some(u) = d.display {
                self.mmio.undo_display(u);
            }
        }
        if let Some(sys) = s.sys {
            self.console.truncate(sys.console_len);
            for b in sys.input.into_iter().rev() {
                self.input.push_front(b);
            }
            self.heap = sys.heap;
            if let Some((id, old)) = sys.random {
                if let Some(value) = old {
                    self.random.insert(id, value);
                } else {
                    self.random.remove(&id);
                }
            }
            self.waiting = sys.waiting;
        }
        true
    }

    /// Number of instructions that can be undone.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Accounted bytes retained by the step-back snapshots and their memory journal.
    pub fn history_bytes(&self) -> usize {
        self.history_bytes
    }

    /// Forgets the step-back history.
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.journal.clear();
        self.history_bytes = 0;
        self.input_undo = Vec::new();
    }

    /// The registers before each recorded step, oldest first (the execution trace).
    pub fn history_cpus(&self) -> impl DoubleEndedIterator<Item = &Cpu> + ExactSizeIterator {
        self.history.iter().map(|s| &s.cpu)
    }

    /// The last executed instruction: the registers before it and the bytes it overwrote
    /// (`(address, old value)`, in the order written).
    pub fn last_step(&self) -> Option<(&Cpu, Vec<(u32, u8)>)> {
        let s = self.history.back()?;
        Some((&s.cpu, self.journal.range(self.journal.len() - s.undo..).copied().collect()))
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    fn machine() -> Machine {
        let mut m = Machine::new();
        m.load(&crate::asm::assemble("syscall").unwrap().image).unwrap();
        m
    }

    fn service(m: &mut Machine, n: u32, a0: u32, a1: u32, a2: u32) -> Event {
        m.cpu.pc = m.mem.layout.text_base;
        m.cpu.regs[2] = n;
        m.cpu.regs[4..7].copy_from_slice(&[a0, a1, a2]);
        m.step()
    }

    #[test]
    fn random_history_is_a_delta_and_replays() {
        let mut m = machine();
        for id in 0..MAX_RANDOM_STREAMS as u32 {
            assert_eq!(service(&mut m, 40, id, 42, 0), Event::Continue);
        }
        assert_eq!(m.random.len(), MAX_RANDOM_STREAMS);
        assert!(m.history_bytes() < 2 << 20, "history must not clone the map");
        assert!(m.step_back());
        assert_eq!(m.random.len(), MAX_RANDOM_STREAMS - 1);
        assert_eq!(m.step(), Event::Continue);
        assert_eq!(m.random.len(), MAX_RANDOM_STREAMS);
        for n in 41..=44 {
            assert_eq!(service(&mut m, n, 0, 100, 0), Event::Continue);
            let cpu = m.cpu.clone();
            let state = m.random[&0];
            assert!(m.step_back());
            assert_eq!(m.step(), Event::Continue);
            assert_eq!(m.cpu, cpu);
            assert_eq!(m.random[&0], state);
        }
        assert!(matches!(service(&mut m, 40, MAX_RANDOM_STREAMS as u32, 0, 0), Event::Error(_)));
        assert_eq!(m.random.len(), MAX_RANDOM_STREAMS);
        assert_eq!(service(&mut m, 40, 0, 7, 0), Event::Continue, "existing streams still work");
        m.clear_history();
        assert_eq!(m.history_bytes(), 0);
        m.reset();
        assert!(m.random.is_empty());
    }

    #[test]
    fn input_history_keeps_only_consumed_bytes() {
        let mut m = machine();
        m.input.extend(std::iter::repeat_n(b'x', 1 << 20));
        for _ in 0..100 {
            assert_eq!(service(&mut m, 11, b'A' as u32, 0, 0), Event::Continue);
        }
        assert!(m.history_bytes() < 100_000, "printing must not copy pending input");
        assert_eq!(service(&mut m, 12, 0, 0, 0), Event::Continue);
        let size = m.history_bytes();
        assert_eq!(m.pending_input().len(), (1 << 20) - 1);
        assert!(m.step_back());
        assert_eq!(m.pending_input().len(), 1 << 20);
        assert!(size - m.history_bytes() < 1024);
    }

    #[test]
    fn consumed_input_counts_toward_the_history_budget() {
        let mut m = machine();
        // Invalid integers consume a whole line but do not write memory: only input undo
        // makes these snapshots large. Repeating them must evict the oldest payloads.
        let line = format!("{}\n", "x".repeat(1 << 20));
        for _ in 0..70 {
            m.input.extend(line.bytes());
            assert!(matches!(service(&mut m, 5, 0, 0, 0), Event::Error(_)));
            assert!(m.history_bytes() <= HISTORY_BYTES);
        }
        assert!(m.history_len() < 70);
        assert!(m.history_bytes() > HISTORY_BYTES / 2);
        let count = m.history_len();
        for _ in 0..count {
            assert!(m.step_back());
        }
        assert_eq!(m.history_bytes(), 0);
    }

    #[test]
    fn large_io_and_invalid_ranges_fail_without_large_buffers() {
        let mut m = machine();
        for fd in [1, 3, 99] {
            assert_eq!(service(&mut m, 15, fd, 0, i32::MAX as u32), Event::Continue);
            assert_eq!(m.cpu.regs[2], u32::MAX);
        }
        m.files = Files::new(Storage::Memory(BTreeMap::from([("data".into(), vec![9, 8])])));
        let fd = m.files.open("data", crate::files::O_RDONLY).unwrap();
        let dest = m.mem.layout.data_base;
        assert_eq!(service(&mut m, 14, fd, dest, i32::MAX as u32), Event::Continue);
        assert_eq!(m.cpu.regs[2], 2);
        assert_eq!(m.mem.peek8(dest), 9);
        assert_eq!(m.mem.peek8(dest + 1), 8);
        assert_eq!(service(&mut m, 14, fd, 0, i32::MAX as u32), Event::Continue, "EOF writes no memory");
        assert_eq!(m.cpu.regs[2], 0);
        m.files.close(fd);
        let fd = m.files.open("data", crate::files::O_RDONLY).unwrap();
        assert_eq!(service(&mut m, 14, fd, 0, 0), Event::Continue);
        assert!(matches!(service(&mut m, 14, fd, 0, 2), Event::Error(_)));
        // A valid destination/source is still required for nonempty transfers.
        assert!(matches!(service(&mut m, 15, 1, 0, 4), Event::Error(_)));
        assert_eq!(service(&mut m, 15, 1, 0, 0), Event::Continue);
        assert_eq!(m.cpu.regs[2], 0);
        assert_eq!(m.console.len(), 0);
        let boundary = m.mem.layout.data_limit - 1;
        assert!(matches!(service(&mut m, 15, 1, boundary, 2), Event::Error(_)));
        assert_eq!(m.console.len(), 0, "an invalid range must not print a prefix");
    }
}
