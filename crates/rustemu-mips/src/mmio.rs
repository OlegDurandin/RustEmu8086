//! MARS's tools for memory-mapped I/O: the "Keyboard and Display MMIO Simulator" ([`KeyboardDisplay`]:
//! a keyboard and a character display in four memory words at the start of the memory-mapped I/O
//! segment) and the "Digital Lab Sim" ([`DigitalLab`]: seven-segment displays, a hexadecimal
//! keyboard and a counter, from `0xffff0010`).
//!
//! | address | register | |
//! |---|---|---|
//! | `0xffff0000` | receiver control | bit 0: a key is ready, bit 1: interrupt enable |
//! | `0xffff0004` | receiver data | the key's character in the low byte |
//! | `0xffff0008` | transmitter control | bit 0: the display is ready, bit 1: interrupt enable |
//! | `0xffff000c` | transmitter data | the character to display in the low byte |
//!
//! A typed key goes to the receiver data and sets the ready bit; a load at exactly the receiver
//! data address clears it. A store at exactly the transmitter data address while the display is
//! ready clears its ready bit; the character is shown after a delay counted in executed
//! instructions (MARS: "Display After Delay"), then the ready bit is set again. With interrupt
//! enable set in the control register and in Status (bit 0, exception level 0), a ready bit set
//! by the device raises an external interrupt before the next instruction: Cause gets code 0
//! with bit 8 (keyboard) or 9 (display), EPC the instruction's address.
//!
//! The display, as MARS's text area, is text and a caret. Form feed (12) clears it; bell (7)
//! switches it to a terminal of [`KeyboardDisplay::terminal_size`] (lines of spaces) and puts
//! the caret at column bits 20..31, row bits 8..19 of the stored word. MARS's quirks are kept: in
//! terminal mode a character replaces the one at the caret, skipping a line end; at the very end
//! the last one is overwritten.
//!
//! The device is always connected (MARS: the tool's "Connect to MIPS"): loading a program sets the
//! transmitter's ready bit. Its registers are ordinary memory, so the program may change them.

use crate::cpu::{ExcCode, cp0};
use crate::memory::{MMIO_BASE, Memory};
use crate::random::JavaRandom;
use std::sync::atomic::Ordering::Relaxed;

/// Receiver (keyboard) control: bit 0 ready, bit 1 interrupt enable.
pub const RECEIVER_CONTROL: u32 = MMIO_BASE;
/// Receiver (keyboard) data: the typed character in the low byte.
pub const RECEIVER_DATA: u32 = MMIO_BASE + 4;
/// Transmitter (display) control: bit 0 ready, bit 1 interrupt enable.
pub const TRANSMITTER_CONTROL: u32 = MMIO_BASE + 8;
/// Transmitter (display) data: the character to display in the low byte.
pub const TRANSMITTER_DATA: u32 = MMIO_BASE + 12;

/// The transmitter delays MARS's slider offers, in instructions.
pub const DELAYS: [u32; 41] = [
    1, 2, 3, 4, 5, 10, 20, 30, 40, 50, 100, 150, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1500, 2000, 3000, 4000,
    5000, 6000, 7000, 8000, 9000, 10000, 20000, 40000, 60000, 80000, 100000, 200000, 400000, 600000, 800000, 1000000,
];
/// MARS's default transmitter delay, in instructions.
pub const DEFAULT_DELAY: u32 = 5;
/// Terminal size (columns, rows) until the interface sets one.
pub const DEFAULT_TERMINAL: (u32, u32) = (80, 25);
/// Bytes the display keeps (older text is dropped).
pub const DISPLAY_LIMIT: usize = 1 << 20;

/// ASCII form feed: clears the display.
const CLEAR_SCREEN: u8 = 12;
/// ASCII bell: sets the caret to (x, y).
const SET_CURSOR: u8 = 7;

/// How the transmitter's delay is chosen for each character (MARS's three techniques).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelayTechnique {
    /// Always [`KeyboardDisplay::delay`] instructions.
    #[default]
    Fixed,
    /// Uniformly from 1 to the delay.
    Uniform,
    /// `floor(abs(N(0,1) * delay) + 1)`.
    Normal,
}

impl DelayTechnique {
    /// MARS's description of the technique.
    pub fn title(self) -> &'static str {
        match self {
            DelayTechnique::Fixed => "Fixed transmitter delay, select using slider",
            DelayTechnique::Uniform => "Uniformly distributed delay, min=1, max=slider",
            DelayTechnique::Normal => "'Normally' distributed delay: floor(abs(N(0,1)*slider)+1)",
        }
    }
}

/// The device's state besides the display text (saved by every step of the history).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct DeviceState {
    /// The transmitter is busy: instructions are being counted.
    counting: bool,
    /// Instructions counted since the character was stored.
    count: u32,
    /// The delay of the character being transmitted.
    limit: u32,
    /// The word stored to the transmitter data.
    pending: u32,
    /// The display is a terminal (after the first bell).
    random_access: bool,
    /// Terminal size, fixed when it was set up.
    columns: u32,
    rows: u32,
    /// Caret: an index in the display text.
    caret: usize,
    /// An interrupt waits for the next instruction.
    interrupt: Option<ExcCode>,
    /// The generator of random delays (MARS: an unseeded `java.util.Random`).
    random: JavaRandom,
}

/// How to undo a change of the display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DisplayUndo {
    /// Characters were appended: back to this length.
    Len(usize),
    /// The byte at this index was replaced.
    Byte(usize, u8),
    /// The whole text was replaced.
    Text(Vec<u8>),
}

/// The keyboard and display (see the module documentation).
#[derive(Debug, Clone)]
pub struct KeyboardDisplay {
    /// Transmitter delay, in instructions (at least 1).
    pub delay: u32,
    /// A character is shown when its delay ends (MARS's "DAD"); otherwise when it is stored.
    pub display_after_delay: bool,
    /// How the delay of each character is chosen.
    pub technique: DelayTechnique,
    /// Columns and rows of the terminal the next bell (or reset) sets up: MARS takes them from
    /// the size of its window.
    pub terminal_size: (u32, u32),
    pub(crate) state: DeviceState,
    pub(crate) display: Vec<u8>,
    /// The display change of the running step.
    pub(crate) undo: Option<DisplayUndo>,
}

impl Default for KeyboardDisplay {
    fn default() -> Self {
        KeyboardDisplay::new()
    }
}

impl KeyboardDisplay {
    /// A device with MARS's settings and an empty display.
    pub fn new() -> KeyboardDisplay {
        KeyboardDisplay {
            delay: DEFAULT_DELAY,
            display_after_delay: true,
            technique: DelayTechnique::Fixed,
            terminal_size: DEFAULT_TERMINAL,
            state: DeviceState::default(),
            display: Vec::new(),
            undo: None,
        }
    }

    /// MARS's tool reset: the display empty and not a terminal, nothing in transmission, the
    /// transmitter ready (the settings stay).
    pub fn reset(&mut self, mem: &mut Memory) {
        // Each run gets other random delays, as in MARS.
        let seed = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64);
        self.state = DeviceState { random: JavaRandom::new(seed), ..DeviceState::default() };
        self.display.clear();
        self.undo = None;
        let tc = mem.peek32(TRANSMITTER_CONTROL) | 1;
        mem.load(TRANSMITTER_CONTROL, &tc.to_le_bytes());
    }

    /// The display text: bytes, shown as Latin-1 (MARS shows each byte as a character); in
    /// terminal mode rows of [`KeyboardDisplay::size`] columns separated by `\n`.
    pub fn text(&self) -> &[u8] {
        &self.display
    }

    /// The display is a terminal (a bell was transmitted).
    pub fn is_terminal(&self) -> bool {
        self.state.random_access
    }

    /// Columns and rows of the terminal (when [`KeyboardDisplay::is_terminal`]).
    pub fn size(&self) -> (u32, u32) {
        (self.state.columns, self.state.rows)
    }

    /// The caret as MARS's window title shows it: `(x,y)` in a terminal, otherwise the index in
    /// the text.
    pub fn cursor_text(&self) -> String {
        let s = &self.state;
        let p = s.caret;
        if !s.random_access {
            return p.to_string();
        }
        let w = s.columns as usize + 1;
        if !(p + 1).is_multiple_of(w) {
            format!("({},{})", p % w, p / w)
        } else if p / w + 1 == s.rows as usize {
            format!("({},{})", p % w - 1, p / w)
        } else {
            format!("(0,{})", p / w + 1)
        }
    }

    /// The transmitter is busy with a character.
    #[inline]
    pub fn is_transmitting(&self) -> bool {
        self.state.counting
    }

    /// A key typed on the keyboard: its character goes to the receiver data, the ready bit is
    /// set (the previous key, if not read, is lost); `status` is coprocessor 0's Status. The
    /// registers are written outside the history: step back does not take a key back.
    pub fn key(&mut self, mem: &mut Memory, status: u32, char: u8) {
        let rc = mem.peek32(RECEIVER_CONTROL) | 1;
        mem.load(RECEIVER_CONTROL, &rc.to_le_bytes());
        mem.load(RECEIVER_DATA, &(char as u32).to_le_bytes());
        if rc != 1 && interrupts_on(status) {
            self.state.interrupt = Some(ExcCode::KeyboardInterrupt);
        }
    }

    /// Takes the interrupt waiting for the next instruction.
    #[inline]
    pub(crate) fn take_interrupt(&mut self) -> Option<ExcCode> {
        self.state.interrupt.take()
    }

    /// What the device does after an instruction (MARS's notices of its loads and stores, then
    /// of the next fetch); `fetched` is false when no next instruction is fetched (a blocked
    /// system call).
    pub(crate) fn after_instruction(&mut self, mem: &mut Memory, status: u32, fetched: bool) {
        self.undo = None;
        if mem.mmio.read_data.load(Relaxed) {
            mem.mmio.read_data.store(false, Relaxed);
            let rc = mem.peek32(RECEIVER_CONTROL) & 2;
            store(mem, RECEIVER_CONTROL, rc);
        }
        if let Some(v) = mem.mmio.written.take()
            && mem.peek32(TRANSMITTER_CONTROL) & 1 == 1
        {
            let tc = mem.peek32(TRANSMITTER_CONTROL) & 2;
            store(mem, TRANSMITTER_CONTROL, tc);
            self.state.pending = v;
            if !self.display_after_delay {
                self.show(v);
            }
            self.state.counting = true;
            self.state.count = 0;
            self.state.limit = self.generate_delay();
        }
        if fetched && self.state.counting {
            self.state.count += 1;
            if self.state.count >= self.state.limit {
                if self.display_after_delay {
                    self.show(self.state.pending);
                }
                self.state.counting = false;
                let tc = mem.peek32(TRANSMITTER_CONTROL) | 1;
                store(mem, TRANSMITTER_CONTROL, tc);
                if tc != 1 && interrupts_on(status) {
                    self.state.interrupt = Some(ExcCode::DisplayInterrupt);
                }
            }
        }
    }

    /// The delay of the next character, in instructions (MARS's `generateDelay`).
    fn generate_delay(&mut self) -> u32 {
        let delay = self.delay.max(1);
        let r = &mut self.state.random;
        match self.technique {
            DelayTechnique::Fixed => delay,
            DelayTechnique::Uniform => r.next_int_below(delay as i32).unwrap_or(0) as u32 + 1,
            DelayTechnique::Normal => ((r.next_gaussian() * delay as f64).abs() + 1.0) as u32,
        }
    }

    /// Displays the character of a transmitted word (MARS's `displayCharacter`).
    fn show(&mut self, word: u32) {
        let c = word as u8;
        if c == CLEAR_SCREEN {
            self.undo = Some(DisplayUndo::Text(std::mem::take(&mut self.display)));
            self.init_display();
        } else if c == SET_CURSOR {
            if !self.state.random_access {
                self.state.random_access = true;
                self.undo = Some(DisplayUndo::Text(std::mem::take(&mut self.display)));
                self.init_display();
            }
            let s = &mut self.state;
            let x = (word >> 20).min(s.columns - 1);
            let y = ((word >> 8) & 0xFFF).min(s.rows - 1);
            s.caret = (y * (s.columns + 1) + x) as usize;
        } else if self.state.random_access {
            let s = &mut self.state;
            let len = self.display.len();
            let mut p = s.caret;
            if (p + 1).is_multiple_of(s.columns as usize + 1) {
                p += 1;
            }
            if p >= len {
                // Java's text area refuses the position: the character before the caret is
                // replaced.
                p = s.caret.max(1) - 1;
            }
            if p < len {
                self.undo = Some(DisplayUndo::Byte(p, self.display[p]));
                self.display[p] = c;
                s.caret = p + 1;
            }
        } else {
            if self.display.len() >= DISPLAY_LIMIT {
                self.undo = Some(DisplayUndo::Text(self.display.clone()));
                self.display.drain(..DISPLAY_LIMIT / 4);
            } else {
                self.undo = Some(DisplayUndo::Len(self.display.len()));
            }
            self.display.push(c);
            self.state.caret = self.display.len();
        }
    }

    /// Clears the display: empty, or a terminal of spaces (its size taken now).
    fn init_display(&mut self) {
        let s = &mut self.state;
        self.display.clear();
        if s.random_access {
            s.columns = self.terminal_size.0.clamp(1, 4096);
            s.rows = self.terminal_size.1.clamp(1, 4096);
            for r in 0..s.rows {
                if r > 0 {
                    self.display.push(b'\n');
                }
                self.display.extend(std::iter::repeat_n(b' ', s.columns as usize));
            }
        }
        s.caret = 0;
    }

    /// Undoes a display change of a step.
    pub(crate) fn undo_display(&mut self, undo: DisplayUndo) {
        match undo {
            DisplayUndo::Len(n) => self.display.truncate(n),
            DisplayUndo::Byte(p, b) => {
                if let Some(c) = self.display.get_mut(p) {
                    *c = b;
                }
            }
            DisplayUndo::Text(t) => self.display = t,
        }
    }
}

/// First address of the Digital Lab Sim: the right seven-segment display.
pub const LAB_BASE: u32 = MMIO_BASE + 0x10;
/// The left seven-segment display (bit 0: segment a … bit 6: g, bit 7: the point).
pub const LAB_LEFT: u32 = MMIO_BASE + 0x11;
/// The hexadecimal keyboard's row to scan (bits 0..3) and interrupt enable (bits 4..7).
pub const LAB_ROW: u32 = MMIO_BASE + 0x12;
/// The counter: a non-zero value turns its interrupt on.
pub const LAB_COUNTER: u32 = MMIO_BASE + 0x13;
/// The key found by the scan: its row (bits 0..3) and column (bits 4..7), 0 when none.
pub const LAB_KEY: u32 = MMIO_BASE + 0x14;
/// Notices between two counter interrupts, less one (MARS's `CounterValueMax`).
pub const COUNTER_MAX: u32 = 30;

/// The Digital Lab Sim's state (saved by the steps of the history that change it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LabState {
    /// Segments of the right and the left display.
    segments: [u8; 2],
    /// The key pressed (0..15), until the next click.
    key: Option<u8>,
    /// A click raises an interrupt (a row byte with bits 4..7).
    key_interrupts: bool,
    /// The counter counts.
    counter_on: bool,
    /// Notices left before the next counter interrupt.
    counter: u32,
    /// An interrupt waits for the next instruction.
    interrupt: Option<ExcCode>,
}

impl Default for LabState {
    fn default() -> Self {
        LabState {
            segments: [0; 2],
            key: None,
            key_interrupts: false,
            counter_on: false,
            counter: COUNTER_MAX,
            interrupt: None,
        }
    }
}

/// MARS's Digital Lab Sim: two seven-segment displays, a 4 x 4 hexadecimal keyboard and a counter,
/// in the bytes `0xffff0010..=0xffff0014`.
///
/// As in MARS, the tool sees every load and store at `0xffff0010..=0xffff0013` (the value loaded
/// counts as well as the value stored) and every instruction fetched from the text segment:
///
/// - a byte at `0xffff0010` / `0xffff0011` lights the segments of the right / left display;
/// - a byte at `0xffff0012` scans a row: when the key pressed is in that row (bits 0..3, exactly
///   one bit), `0xffff0014` gets its row and column bits (key 2: `0x41`), otherwise 0; bits 4..7
///   enable the keyboard interrupt (Cause bit 11) of the next click;
/// - a non-zero byte at `0xffff0013` starts the counter: each of these notices (instruction
///   fetches from `.text` — not the handler's in `.ktext` — and accesses to the four bytes) counts
///   down from 30, and after 0 comes an interrupt (Cause bit 10).
///
/// Its interrupts look only at Status's exception level, not at interrupt enable; MARS's Cause
/// mask leaves bits 10 and 11 set afterwards.
#[derive(Debug, Clone, Default)]
pub struct DigitalLab {
    pub(crate) state: LabState,
}

impl DigitalLab {
    /// The tool's reset: displays dark, no key pressed, counter off.
    pub fn reset(&mut self) {
        self.state = LabState::default();
    }

    /// The segments of the left and the right display (bit 0: segment a … bit 7: the point).
    pub fn segments(&self) -> [u8; 2] {
        [self.state.segments[1], self.state.segments[0]]
    }

    /// The key pressed on the hexadecimal keyboard.
    pub fn pressed(&self) -> Option<u8> {
        self.state.key
    }

    /// Notices left before the next counter interrupt, while the counter counts.
    pub fn counter(&self) -> Option<u32> {
        self.state.counter_on.then_some(self.state.counter)
    }

    /// The counter counts.
    #[inline]
    pub fn is_counting(&self) -> bool {
        self.state.counter_on
    }

    /// A click on key `key` (0..15): it is pressed, or, when a key is pressed already, that one is
    /// released (MARS's buttons toggle). Pressing raises the keyboard interrupt when enabled.
    pub fn click(&mut self, mem: &mut Memory, status: u32, key: u8) {
        if self.state.key.is_some() {
            self.state.key = None;
            mem.load(LAB_KEY, &[0]);
        } else {
            self.state.key = Some(key & 15);
            if self.state.key_interrupts && status & cp0::EXL == 0 {
                self.state.interrupt = Some(ExcCode::HexKeyboardInterrupt);
            }
        }
    }

    /// Takes the interrupt waiting for the next instruction.
    #[inline]
    pub(crate) fn take_interrupt(&mut self) -> Option<ExcCode> {
        self.state.interrupt.take()
    }

    /// What the tool does after an instruction: its notices of the loads and stores at its
    /// addresses, then of the next fetch when it is in the text segment (`text_fetch`).
    pub(crate) fn after_instruction(&mut self, mem: &mut Memory, status: u32, text_fetch: bool) {
        if mem.mmio.lab_touched.swap(false, Relaxed) {
            let accesses = mem.mmio.lab.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
            for (addr, value) in accesses {
                self.access(mem, addr, value as u16);
                self.tick(status);
            }
        }
        if text_fetch {
            self.tick(status);
        }
    }

    /// A load or store at one of the four bytes (MARS takes the value as a Java `char`).
    fn access(&mut self, mem: &mut Memory, addr: u32, value: u16) {
        let s = &mut self.state;
        match addr {
            LAB_BASE => s.segments[0] = value as u8,
            LAB_LEFT => s.segments[1] = value as u8,
            LAB_ROW => {
                let found = match s.key {
                    Some(k) if 1 << (k / 4) == value & 0xF => (1 << (k / 4)) | (1 << (4 + k % 4)),
                    _ => 0,
                };
                // A store of the tool: part of the step, as the program's.
                let _ = mem.store8(LAB_KEY, found as u8);
                s.key_interrupts = value & 0xF0 != 0;
            }
            _ => {
                s.counter_on = value != 0;
                if s.counter_on {
                    s.counter = COUNTER_MAX;
                }
            }
        }
    }

    /// One notice for the counter.
    fn tick(&mut self, status: u32) {
        let s = &mut self.state;
        if s.counter_on {
            if s.counter > 0 {
                s.counter -= 1;
            } else {
                s.counter = COUNTER_MAX;
                if status & cp0::EXL == 0 {
                    s.interrupt = Some(ExcCode::TimerInterrupt);
                }
            }
        }
    }
}

/// External interrupts are taken: Status interrupt enable on, exception level off.
fn interrupts_on(status: u32) -> bool {
    status & cp0::EXL == 0 && status & 1 == 1
}

/// A device write to one of its registers (MARS's `setRawWord`), recorded in the journal.
fn store(mem: &mut Memory, addr: u32, value: u32) {
    // The MMIO segment accepts stores: this cannot fail.
    let written = mem.mmio.written;
    let _ = mem.store32(addr, value);
    mem.mmio.written = written;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> (KeyboardDisplay, Memory) {
        let mut mem = Memory::new();
        let mut d = KeyboardDisplay::new();
        d.reset(&mut mem);
        (d, mem)
    }

    /// Stores `word` to the transmitter as `sw` would and runs `n` more instructions.
    fn transmit(d: &mut KeyboardDisplay, mem: &mut Memory, word: u32) {
        mem.store32(TRANSMITTER_DATA, word).unwrap();
        d.after_instruction(mem, 0, true);
        for _ in 1..d.delay {
            assert_eq!(mem.peek32(TRANSMITTER_CONTROL) & 1, 0, "busy");
            d.after_instruction(mem, 0, true);
        }
        assert_eq!(mem.peek32(TRANSMITTER_CONTROL) & 1, 1, "ready again");
    }

    #[test]
    fn characters_appear_after_the_delay() {
        let (mut d, mut mem) = device();
        assert_eq!(mem.peek32(TRANSMITTER_CONTROL), 1);
        mem.store8(TRANSMITTER_DATA, b'H').unwrap();
        d.after_instruction(&mut mem, 0, true);
        assert!(d.text().is_empty());
        for _ in 0..4 {
            d.after_instruction(&mut mem, 0, true);
        }
        assert_eq!(d.text(), b"H");
        // Stored while busy: ignored.
        transmit(&mut d, &mut mem, u32::from(b'i'));
        mem.store8(TRANSMITTER_DATA, b'!').unwrap();
        d.after_instruction(&mut mem, 0, true);
        mem.store8(TRANSMITTER_DATA, b'?').unwrap();
        d.after_instruction(&mut mem, 0, true);
        for _ in 0..5 {
            d.after_instruction(&mut mem, 0, true);
        }
        assert_eq!(d.text(), b"Hi!");
    }

    #[test]
    fn reading_the_key_clears_ready() {
        let (mut d, mut mem) = device();
        mem.store32(RECEIVER_CONTROL, 2).unwrap();
        d.key(&mut mem, 0, b'a');
        assert_eq!((mem.peek32(RECEIVER_CONTROL), mem.peek32(RECEIVER_DATA)), (3, 97));
        assert_eq!(d.take_interrupt(), None, "Status interrupt enable is off");
        d.key(&mut mem, cp0::STATUS_RESET, b'b');
        assert_eq!(d.take_interrupt(), Some(ExcCode::KeyboardInterrupt));
        assert_eq!(mem.load32(RECEIVER_DATA).unwrap(), 98);
        d.after_instruction(&mut mem, 0, true);
        assert_eq!(mem.peek32(RECEIVER_CONTROL), 2);
    }

    #[test]
    fn random_delays_stay_in_range() {
        let (mut d, _) = device();
        d.delay = 10;
        d.technique = DelayTechnique::Uniform;
        let uniform: Vec<u32> = (0..200).map(|_| d.generate_delay()).collect();
        assert!(uniform.iter().all(|v| (1..=10).contains(v)));
        assert!(uniform.contains(&1) && uniform.contains(&10));
        d.technique = DelayTechnique::Normal;
        assert!((0..200).map(|_| d.generate_delay()).all(|v| v >= 1));
    }

    #[test]
    fn bell_makes_a_terminal() {
        let (mut d, mut mem) = device();
        d.terminal_size = (4, 2);
        transmit(&mut d, &mut mem, u32::from(b'x'));
        transmit(&mut d, &mut mem, 2 << 20 | 1 << 8 | 7);
        assert_eq!(d.text(), b"    \n    ");
        assert_eq!(d.cursor_text(), "(2,1)");
        for c in *b"abc" {
            transmit(&mut d, &mut mem, u32::from(c));
        }
        // The last character is overwritten at the end.
        assert_eq!(d.text(), b"    \n  ac");
        transmit(&mut d, &mut mem, 3 << 20 | 7);
        transmit(&mut d, &mut mem, u32::from(b'1'));
        transmit(&mut d, &mut mem, u32::from(b'2'));
        assert_eq!(d.text(), b"   1\n2 ac", "a line end is skipped");
        transmit(&mut d, &mut mem, 12);
        assert_eq!(d.text(), b"    \n    ");
    }
}
