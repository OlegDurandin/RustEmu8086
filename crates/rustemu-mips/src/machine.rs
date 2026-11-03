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
//! The history keeps at most [`HISTORY_DEPTH`] steps and [`HISTORY_BYTES`] of memory journal.

use std::collections::{BTreeMap, HashSet, VecDeque};

use rustemu_target::Event;

use crate::console::Console;
use crate::cpu::{Cpu, Step};
use crate::files::{Files, Storage};
use crate::memory::{self, Memory, Segment};
use crate::random::JavaRandom;

/// Instructions step back can undo.
pub const HISTORY_DEPTH: usize = 2000;
/// Bytes of memory journal the history keeps at most (older steps are forgotten).
pub const HISTORY_BYTES: usize = 64 << 20;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Where the bytes go.
    pub addr: u32,
    /// The bytes (instructions are little-endian words).
    pub bytes: Vec<u8>,
    /// The bytes are instructions (in the text or kernel text segment).
    pub code: bool,
}

/// A program ready to load: its parts and where execution starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Code and data.
    pub chunks: Vec<Chunk>,
    /// The first instruction executed (usually the start of `.text`).
    pub entry: u32,
}

/// What a `syscall` changes outside registers and memory, saved to step back over it.
#[derive(Debug, Clone)]
struct SysState {
    console_len: usize,
    input: VecDeque<u8>,
    heap: u32,
    random: BTreeMap<i32, JavaRandom>,
    waiting: bool,
}

/// One instruction of the history.
#[derive(Debug, Clone)]
struct Snapshot {
    cpu: Cpu,
    undo: Vec<(u32, u8)>,
    finished: Option<Finish>,
    sys: Option<Box<SysState>>,
}

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
    /// How the program ended, once it has.
    pub finished: Option<Finish>,
    /// A program is loaded.
    pub loaded: bool,
    /// Physical addresses where execution stops.
    pub breakpoints: HashSet<u32>,
    /// Steps are recorded for step back (off: faster).
    pub history_enabled: bool,
    history: VecDeque<Snapshot>,
    /// Journal bytes in the history (see [`HISTORY_BYTES`]).
    history_bytes: usize,
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
            finished: None,
            loaded: false,
            breakpoints: HashSet::new(),
            history_enabled: true,
            history: VecDeque::new(),
            history_bytes: 0,
            barrier: false,
            waiting: false,
            sleep_until: None,
            instant_waits: false,
            fixed_time: None,
        }
    }

    /// Powers the machine on again: nothing loaded, the console empty, files closed (the storage,
    /// the breakpoints and the settings stay).
    pub fn reset(&mut self) {
        self.cpu = Cpu::new();
        self.mem.clear();
        self.console.clear();
        self.input.clear();
        self.heap = memory::HEAP_BASE;
        self.files.reset();
        self.file_error = None;
        self.random.clear();
        self.finished = None;
        self.loaded = false;
        self.clear_history();
        self.waiting = false;
        self.sleep_until = None;
    }

    /// Loads a program after a reset.
    pub fn load(&mut self, image: &Image) -> Result<(), String> {
        for c in &image.chunks {
            let end = c.addr.checked_add(c.bytes.len() as u32).ok_or("the program does not fit in memory")?;
            let seg = Segment::of(c.addr);
            let fits = c.bytes.is_empty() || (seg.is_some() && Segment::of(end - 1) == seg);
            if !fits || (c.code && !seg.is_some_and(Segment::is_code)) {
                return Err(format!(
                    "{} bytes at {} are outside the {} segment",
                    c.bytes.len(),
                    memory::hex(c.addr),
                    if c.code { "text" } else { "data" }
                ));
            }
        }
        self.reset();
        for c in &image.chunks {
            self.mem.load(c.addr, &c.bytes);
            if c.code {
                self.mem.add_code(c.addr, c.addr + c.bytes.len() as u32);
            }
        }
        self.cpu.pc = image.entry;
        self.loaded = true;
        Ok(())
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
        self.waiting
    }

    // ---- execution ----

    /// The instruction at the program counter is a `syscall`.
    fn at_syscall(&self) -> bool {
        self.mem.has_code(self.cpu.pc) && self.mem.peek32(self.cpu.pc) & 0xFC00_003F == 0x0000_000C
    }

    fn sys_state(&self) -> SysState {
        SysState {
            console_len: self.console.len(),
            input: self.input.clone(),
            heap: self.heap,
            random: self.random.clone(),
            waiting: self.waiting,
        }
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
            return self.step_inner();
        }
        let snap = Snapshot {
            cpu: self.cpu.clone(),
            undo: Vec::new(),
            finished: self.finished,
            sys: self.at_syscall().then(|| Box::new(self.sys_state())),
        };
        self.mem.set_journaling(true);
        self.barrier = false;
        let ev = self.step_inner();
        let undo = self.mem.take_journal();
        if self.barrier {
            self.clear_history();
            return ev;
        }
        // Retrying an input syscall that still has no input changes nothing.
        let idle = ev == Event::WaitingInput && undo.is_empty() && self.cpu == snap.cpu;
        if !idle {
            self.history_bytes += undo.len() * std::mem::size_of::<(u32, u8)>();
            self.history.push_back(Snapshot { undo, ..snap });
            while self.history.len() > HISTORY_DEPTH || self.history_bytes > HISTORY_BYTES {
                let Some(old) = self.history.pop_front() else { break };
                self.history_bytes -= old.undo.len() * std::mem::size_of::<(u32, u8)>();
            }
        }
        ev
    }

    fn step_inner(&mut self) -> Event {
        match self.cpu.step(&mut self.mem) {
            Step::Ok => Event::Continue,
            Step::DroppedOff => {
                self.finished = Some(Finish::DroppedOff);
                Event::Terminated(0)
            }
            Step::Exception(e) if e.handled => Event::Continue,
            Step::Exception(e) => Event::Error(e.report()),
            Step::Syscall => self.syscall(),
        }
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
            if self.breakpoints.contains(&self.cpu.pc) {
                return (Event::Breakpoint, n + 1);
            }
        }
        (Event::Budget, budget)
    }

    /// Undoes the last instruction; false when the history is empty.
    pub fn step_back(&mut self) -> bool {
        let Some(s) = self.history.pop_back() else { return false };
        self.history_bytes -= s.undo.len() * std::mem::size_of::<(u32, u8)>();
        self.mem.undo(&s.undo);
        self.cpu = s.cpu;
        self.finished = s.finished;
        self.sleep_until = None;
        if let Some(sys) = s.sys {
            self.console.truncate(sys.console_len);
            self.input = sys.input;
            self.heap = sys.heap;
            self.random = sys.random;
            self.waiting = sys.waiting;
        }
        true
    }

    /// Number of instructions that can be undone.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Forgets the step-back history.
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.history_bytes = 0;
    }

    /// The registers before each recorded step, oldest first (the execution trace).
    pub fn history_cpus(&self) -> impl DoubleEndedIterator<Item = &Cpu> + ExactSizeIterator {
        self.history.iter().map(|s| &s.cpu)
    }

    /// The last executed instruction: the registers before it and the bytes it overwrote.
    pub fn last_step(&self) -> Option<(&Cpu, &[(u32, u8)])> {
        self.history.back().map(|s| (&s.cpu, &s.undo[..]))
    }
}
