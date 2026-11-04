//! Memory of the MIPS machine: a sparse 32-bit address space in 4 KB pages, little-endian, with
//! the segments and access rules of a MARS memory configuration ([`Memory::layout`], see
//! [`crate::settings`]) and a journal of overwritten bytes for step back.
//!
//! The segments of the default configuration are (MARS's effective ones: its tables hold 4 MB
//! per segment):
//!
//! | segment | addresses | program access |
//! |---|---|---|
//! | text | `0x00400000..0x00800000` | instruction fetch; loads and stores only with [`Memory::self_modifying`] |
//! | data | `0x10000000..0x10400000` | loads and stores |
//! | stack | `0x7fbffffd..=0x7ffffffc` (start address) | loads and stores |
//! | kernel text | `0x80000000..0x80400000` | instruction fetch |
//! | kernel data | `0x90000000..0x90400000` | loads and stores |
//! | MMIO | `0xffff0000..0xffffffff` | loads and stores |
//!
//! Anything else is "address out of range". With self-modifying code the text segment takes
//! loads and stores as MARS's does: by word (a byte or halfword store replaces the whole word,
//! a byte or halfword load reads the low bits of the word; at an address that is not a word's,
//! MARS's statement lookup fails with an address error), and a stored word becomes an
//! instruction. The loader and the debugger use the raw accessors ([`Memory::peek8`],
//! [`Memory::load`]), which reach every address.

use std::sync::atomic::Ordering::Relaxed;

/// Bytes per page.
pub const PAGE_SIZE: usize = 4096;
/// Pages in a block of the page directory (a block covers 4 MB, the size of a segment).
const BLOCK_PAGES: usize = 1024;

type Page = [u8; PAGE_SIZE];
/// 1024 pages: 4 MB of the address space.
type Block = [Option<Box<Page>>; BLOCK_PAGES];
/// Size MARS gives each segment (its block tables hold 4 MB).
const SEGMENT_SIZE: u32 = 4 * 1024 * 1024;

// The addresses of MARS's default memory configuration (see [`crate::settings::DEFAULT`] and
// the other configurations there).

/// Base of the text segment (`.text`, where the program counter starts).
pub const TEXT_BASE: u32 = 0x0040_0000;
/// End (exclusive) of the text segment.
pub const TEXT_LIMIT: u32 = TEXT_BASE + SEGMENT_SIZE;
/// Base of the data segment (`.extern` is here).
pub const DATA_SEGMENT_BASE: u32 = 0x1000_0000;
/// End (exclusive) of the data segment.
pub const DATA_LIMIT: u32 = DATA_SEGMENT_BASE + SEGMENT_SIZE;
/// Where `.data` starts.
pub const DATA_BASE: u32 = 0x1001_0000;
/// Where the heap (`sbrk`) starts.
pub const HEAP_BASE: u32 = 0x1004_0000;
/// Initial global pointer `$gp`.
pub const GP_INIT: u32 = 0x1000_8000;
/// Initial stack pointer `$sp`.
pub const SP_INIT: u32 = 0x7fff_effc;
/// Highest (start) address of the stack segment; the stack grows down from here.
pub const STACK_BASE: u32 = 0x7fff_fffc;
/// The stack segment holds start addresses above this one.
pub const STACK_LIMIT: u32 = STACK_BASE - SEGMENT_SIZE;
/// Base of the kernel text segment (`.ktext`).
pub const KTEXT_BASE: u32 = 0x8000_0000;
/// End (exclusive) of the kernel text segment.
pub const KTEXT_LIMIT: u32 = KTEXT_BASE + SEGMENT_SIZE;
/// Where the exception handler is: exceptions jump here when it holds code.
pub const EXCEPTION_HANDLER: u32 = 0x8000_0180;
/// Base of the kernel data segment (`.kdata`).
pub const KDATA_BASE: u32 = 0x9000_0000;
/// End (exclusive) of the kernel data segment.
pub const KDATA_LIMIT: u32 = KDATA_BASE + SEGMENT_SIZE;
/// Base of memory-mapped I/O (keyboard and display of MARS's MMIO simulator).
pub const MMIO_BASE: u32 = 0xffff_0000;

pub use crate::settings::Layout;

/// A region of the address space with its own access rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    /// User program code.
    Text,
    /// User data, `.extern`, the heap.
    Data,
    /// The user stack.
    Stack,
    /// Kernel code (the exception handler).
    KernelText,
    /// Kernel data.
    KernelData,
    /// Memory-mapped I/O.
    Mmio,
}

impl Segment {
    /// Code segments: instructions are fetched from them.
    pub fn is_code(self) -> bool {
        matches!(self, Segment::Text | Segment::KernelText)
    }
}

/// A load or a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Reading memory (also an instruction fetch).
    Load,
    /// Writing memory.
    Store,
}

/// A refused memory access: what MARS reports as an address exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessError {
    /// Load or store (address exception cause 4 or 5).
    pub access: Access,
    /// The address that was refused (it goes to the BadVAddr register).
    pub addr: u32,
    /// MARS's message, the address included: `address out of range 0x00000000`.
    pub message: String,
}

impl AccessError {
    fn new(access: Access, addr: u32, text: &str) -> AccessError {
        AccessError { access, addr, message: format!("{text}{}", hex(addr)) }
    }
}

/// `0x0040001c`: a 32-bit value as MARS prints addresses.
pub fn hex(v: u32) -> String {
    format!("0x{v:08x}")
}

/// What an instruction did to the keyboard and display registers (MARS tells its MMIO tool of
/// every access; only these two matter to it).
#[derive(Debug, Default)]
pub(crate) struct MmioTouch {
    /// The receiver data register was read (a load at exactly its address).
    pub(crate) read_data: std::sync::atomic::AtomicBool,
    /// The value stored at exactly the transmitter data address (the byte of `sb`, the word of `sw`).
    pub(crate) written: Option<u32>,
    /// Loads and stores at the Digital Lab Sim's addresses (`0xffff0010..=0xffff0013`), in
    /// order: the address and the value loaded or stored (MARS tells its tool of each).
    pub(crate) lab: std::sync::Mutex<Vec<(u32, u32)>>,
    /// `lab` is not empty.
    pub(crate) lab_touched: std::sync::atomic::AtomicBool,
}

impl MmioTouch {
    /// Either register was touched.
    #[inline]
    pub(crate) fn any(&self) -> bool {
        self.written.is_some() || self.read_data.load(Relaxed) || self.lab_touched.load(Relaxed)
    }
}

impl Clone for MmioTouch {
    fn clone(&self) -> Self {
        MmioTouch {
            read_data: self.read_data.load(Relaxed).into(),
            written: self.written,
            lab: self.lab.lock().map(|l| l.clone()).unwrap_or_default().into(),
            lab_touched: self.lab_touched.load(Relaxed).into(),
        }
    }
}

/// The memory of the machine.
#[derive(Clone, Default)]
pub struct Memory {
    /// Pages that were written, in a two-level directory: block = address bits 22..31, page in
    /// the block = bits 12..21 (two index operations per access, no hashing); the rest reads as 0.
    blocks: Vec<Option<Box<Block>>>,
    /// Number of pages allocated.
    pages: usize,
    /// While executing: `(address, old value)` of every byte a store changed.
    journal: Option<Vec<(u32, u8)>>,
    /// Address ranges `[start, end)` that hold loaded code (fetching elsewhere in a code segment
    /// finds no instruction: the program "dropped off the bottom").
    code: Vec<(u32, u32)>,
    /// Programs may load and store in the text segment (MARS setting "self-modifying code",
    /// off by default).
    pub self_modifying: bool,
    /// The memory configuration: where the segments are.
    pub layout: Layout,
    /// The MMIO registers of the keyboard and display the running instruction touched (see
    /// [`crate::mmio`]).
    pub(crate) mmio: MmioTouch,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memory").field("pages", &self.pages).field("code", &self.code).finish()
    }
}

impl Memory {
    /// Empty memory: every byte reads as 0, nothing is code.
    pub fn new() -> Memory {
        Memory::default()
    }

    // ---- raw access (loader, debugger) ----

    /// The page holding `addr`, if it was written.
    #[inline]
    fn page(&self, addr: u32) -> Option<&Page> {
        let block = self.blocks.get((addr >> 22) as usize)?.as_deref()?;
        block[(addr >> 12) as usize & (BLOCK_PAGES - 1)].as_deref()
    }

    /// The byte at `addr`, whatever the segment (unwritten memory is 0).
    #[inline]
    pub fn peek8(&self, addr: u32) -> u8 {
        self.page(addr).map_or(0, |p| p[addr as usize % PAGE_SIZE])
    }

    /// The little-endian word at `addr`, whatever the segment.
    #[inline]
    pub fn peek32(&self, addr: u32) -> u32 {
        let at = addr as usize % PAGE_SIZE;
        if at <= PAGE_SIZE - 4 {
            // Within one page (always so for an aligned word).
            return self.page(addr).map_or(0, |p| u32::from_le_bytes(p[at..at + 4].try_into().unwrap()));
        }
        u32::from_le_bytes(std::array::from_fn(|i| self.peek8(addr.wrapping_add(i as u32))))
    }

    /// Up to `len` bytes from `addr` (shorter at the end of the address space).
    pub fn read_bytes(&self, addr: u32, len: usize) -> Vec<u8> {
        let len = len.min((u32::MAX - addr) as usize + 1);
        (0..len).map(|i| self.peek8(addr + i as u32)).collect()
    }

    /// Writes `bytes` from `addr` whatever the segment, bypassing the journal (loading a program,
    /// editing memory in the debugger).
    pub fn load(&mut self, addr: u32, bytes: &[u8]) {
        for (i, &b) in bytes.iter().enumerate() {
            let a = addr.wrapping_add(i as u32);
            self.page_mut(a)[a as usize % PAGE_SIZE] = b;
        }
    }

    /// Marks `[start, end)` as loaded code.
    pub fn add_code(&mut self, start: u32, end: u32) {
        if start < end {
            self.code.push((start, end));
        }
    }

    /// Whether `addr` holds loaded code.
    pub fn has_code(&self, addr: u32) -> bool {
        self.code.iter().any(|&(s, e)| (s..e).contains(&addr))
    }

    /// Number of code ranges ([`Memory::truncate_code`] goes back to it: step back over a
    /// self-modifying store).
    pub fn code_ranges(&self) -> usize {
        self.code.len()
    }

    /// Forgets the code ranges added after the first `n`.
    pub fn truncate_code(&mut self, n: usize) {
        self.code.truncate(n);
    }

    /// Forgets all contents and code (a fresh machine).
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.pages = 0;
        self.code.clear();
        if let Some(j) = &mut self.journal {
            j.clear();
        }
    }

    /// Number of pages in use (each [`PAGE_SIZE`] bytes).
    pub fn pages_in_use(&self) -> usize {
        self.pages
    }

    /// The page holding `addr`, allocated (zeroed) on the first write.
    #[inline]
    fn page_mut(&mut self, addr: u32) -> &mut Page {
        let b = (addr >> 22) as usize;
        if self.blocks.len() <= b {
            self.blocks.resize_with(b + 1, || None);
        }
        let block = self.blocks[b].get_or_insert_with(|| {
            // Built on the heap: a block of 1024 pointers is 8 KB.
            vec![None; BLOCK_PAGES].into_boxed_slice().try_into().unwrap()
        });
        let slot = &mut block[(addr >> 12) as usize & (BLOCK_PAGES - 1)];
        if slot.is_none() {
            self.pages += 1;
        }
        slot.get_or_insert_with(|| Box::new([0; PAGE_SIZE]))
    }

    // ---- the journal ----

    /// Turns recording of overwritten bytes on or off.
    pub fn set_journaling(&mut self, on: bool) {
        match (on, &self.journal) {
            (true, None) => self.journal = Some(Vec::new()),
            (false, Some(_)) => self.journal = None,
            _ => {}
        }
    }

    /// Whether overwritten bytes are being recorded.
    pub fn is_journaling(&self) -> bool {
        self.journal.is_some()
    }

    /// Takes the recorded `(address, old value)` pairs, leaving the journal empty.
    pub fn take_journal(&mut self) -> Vec<(u32, u8)> {
        self.journal.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Moves the recorded `(address, old value)` pairs to the end of `history` (the journal
    /// keeps its buffer: no allocation per step); returns their number.
    pub fn drain_journal_into(&mut self, history: &mut std::collections::VecDeque<(u32, u8)>) -> usize {
        let Some(j) = &mut self.journal else { return 0 };
        let n = j.len();
        history.extend(j.drain(..));
        n
    }

    /// Puts back one byte recorded in a journal.
    #[inline]
    pub fn restore(&mut self, addr: u32, value: u8) {
        self.page_mut(addr)[addr as usize % PAGE_SIZE] = value;
    }

    /// Restores bytes recorded in a journal (in reverse order).
    pub fn undo(&mut self, entries: &[(u32, u8)]) {
        for &(a, v) in entries.iter().rev() {
            self.page_mut(a)[a as usize % PAGE_SIZE] = v;
        }
    }

    // ---- program access ----

    /// Checks that the program may load or store at `addr` (alignment aside).
    pub fn check_access(&self, access: Access, addr: u32) -> Result<(), AccessError> {
        match self.layout.segment(addr) {
            Some(Segment::Text) if !self.self_modifying => Err(AccessError::new(
                access,
                addr,
                match access {
                    Access::Load => "Cannot read directly from text segment!",
                    Access::Store => "Cannot write directly to text segment!",
                },
            )),
            Some(Segment::KernelText) => Err(AccessError::new(
                access,
                addr,
                match access {
                    Access::Load => "DEVELOPER: You must use getStatement() to read from kernel text segment!",
                    Access::Store => "DEVELOPER: You must use setStatement() to write to kernel text segment!",
                },
            )),
            Some(_) => Ok(()),
            None => Err(AccessError::new(access, addr, "address out of range ")),
        }
    }

    /// `len` (1, 2 or 4) bytes at an address aligned to `len`: within one page.
    #[inline]
    fn read(&self, addr: u32, len: u32) -> u32 {
        let at = addr as usize % PAGE_SIZE;
        self.page(addr).map_or(0, |p| p[at..at + len as usize].iter().rev().fold(0, |v, &b| (v << 8) | b as u32))
    }

    /// Writes `len` (1, 2 or 4) bytes at an address aligned to `len` (within one page), recording
    /// the old bytes in the journal.
    #[inline]
    fn write(&mut self, addr: u32, len: u32, value: u32) {
        let at = addr as usize % PAGE_SIZE;
        // (The journal is a separate field: borrowed beside the page.)
        let mut journal = self.journal.take();
        let page = self.page_mut(addr);
        for i in 0..len as usize {
            let cell = &mut page[at + i];
            if let Some(j) = &mut journal {
                j.push((addr + i as u32, *cell));
            }
            *cell = (value >> (8 * i)) as u8;
        }
        self.journal = journal;
    }

    /// Records a load the devices watch: the receiver data register, the Digital Lab Sim's bytes.
    #[inline]
    fn note_load(&self, addr: u32, value: u32) {
        let base = self.layout.mmio_base;
        if addr >= base {
            if addr == base + crate::mmio::RECEIVER_DATA_OFFSET {
                self.mmio.read_data.store(true, Relaxed);
            }
            self.note_lab(addr, value);
        }
    }

    /// Records a store the devices watch: the transmitter data register, the Digital Lab Sim's bytes.
    #[inline]
    fn note_store(&mut self, addr: u32, value: u32) {
        let base = self.layout.mmio_base;
        if addr >= base {
            if addr == base + crate::mmio::TRANSMITTER_DATA_OFFSET {
                self.mmio.written = Some(value);
            }
            self.note_lab(addr, value);
        }
    }

    fn note_lab(&self, addr: u32, value: u32) {
        if addr.wrapping_sub(self.layout.mmio_base + crate::mmio::LAB_OFFSET) < 4
            && let Ok(mut l) = self.mmio.lab.lock()
        {
            l.push((addr, value));
            self.mmio.lab_touched.store(true, Relaxed);
        }
    }

    /// With self-modifying code, `addr` is in the text segment: MARS reads and writes it by
    /// whole words.
    #[inline]
    fn text_word(&self, addr: u32) -> bool {
        self.self_modifying && (self.layout.text_base..self.layout.text_limit).contains(&addr)
    }

    /// The word of the text segment at `addr` as MARS reads it with self-modifying code (its
    /// statement: 0 where there is none); MARS looks the statement up first, so `addr` must
    /// be a word's.
    fn load_text(&self, addr: u32) -> Result<u32, AccessError> {
        if !addr.is_multiple_of(4) {
            return Err(AccessError::new(
                Access::Load,
                addr,
                "fetch address for text segment not aligned to word boundary ",
            ));
        }
        Ok(if self.has_code(addr) { self.read(addr, 4) } else { 0 })
    }

    /// A store into the text segment with self-modifying code, as MARS's: the value replaces
    /// the word at `addr` (a word's address: MARS reads the old statement first), which becomes
    /// an instruction.
    fn store_text(&mut self, addr: u32, value: u32) -> Result<(), AccessError> {
        self.load_text(addr)?;
        self.write(addr, 4, value);
        if !self.has_code(addr) {
            self.add_code(addr, addr + 4);
        }
        Ok(())
    }

    /// A byte the program loads (`lb`, `lbu`, and each byte of `lwl` / `lwr`).
    pub fn load8(&self, addr: u32) -> Result<u8, AccessError> {
        self.check_access(Access::Load, addr)?;
        if self.text_word(addr) {
            return Ok(self.load_text(addr)? as u8);
        }
        let v = self.peek8(addr);
        self.note_load(addr, v as u32);
        Ok(v)
    }

    /// A halfword the program loads (`lh`, `lhu`); it must be aligned.
    pub fn load16(&self, addr: u32) -> Result<u16, AccessError> {
        if !addr.is_multiple_of(2) {
            return Err(AccessError::new(Access::Load, addr, "fetch address not aligned on halfword boundary "));
        }
        self.check_access(Access::Load, addr)?;
        if self.text_word(addr) {
            return Ok(self.load_text(addr)? as u16);
        }
        let v = self.read(addr, 2);
        self.note_load(addr, v);
        Ok(v as u16)
    }

    /// A word the program loads (`lw`, `ll`); it must be aligned.
    pub fn load32(&self, addr: u32) -> Result<u32, AccessError> {
        if !addr.is_multiple_of(4) {
            return Err(AccessError::new(Access::Load, addr, "fetch address not aligned on word boundary "));
        }
        self.check_access(Access::Load, addr)?;
        if self.text_word(addr) {
            return self.load_text(addr);
        }
        let v = self.read(addr, 4);
        self.note_load(addr, v);
        Ok(v)
    }

    /// Stores a byte (`sb`, and each byte of `swl` / `swr`).
    pub fn store8(&mut self, addr: u32, value: u8) -> Result<(), AccessError> {
        self.check_access(Access::Store, addr)?;
        if self.text_word(addr) {
            return self.store_text(addr, value as u32);
        }
        self.note_store(addr, value as u32);
        self.write(addr, 1, value as u32);
        Ok(())
    }

    /// Stores an aligned halfword (`sh`).
    pub fn store16(&mut self, addr: u32, value: u16) -> Result<(), AccessError> {
        if !addr.is_multiple_of(2) {
            return Err(AccessError::new(Access::Store, addr, "store address not aligned on halfword boundary "));
        }
        self.check_access(Access::Store, addr)?;
        if self.text_word(addr) {
            return self.store_text(addr, value as u32);
        }
        self.note_store(addr, value as u32);
        self.write(addr, 2, value as u32);
        Ok(())
    }

    /// Stores an aligned word (`sw`, `sc`).
    pub fn store32(&mut self, addr: u32, value: u32) -> Result<(), AccessError> {
        if !addr.is_multiple_of(4) {
            return Err(AccessError::new(Access::Store, addr, "store address not aligned on word boundary "));
        }
        self.check_access(Access::Store, addr)?;
        if self.text_word(addr) {
            return self.store_text(addr, value);
        }
        self.note_store(addr, value);
        self.write(addr, 4, value);
        Ok(())
    }

    /// The instruction at `pc`: None when the code segment holds no instruction there (MARS:
    /// execution "dropped off the bottom" of the program). With self-modifying code, code
    /// outside the code segments is fetched as data.
    pub fn fetch(&self, pc: u32) -> Result<Option<u32>, AccessError> {
        if !pc.is_multiple_of(4) {
            return Err(AccessError::new(
                Access::Load,
                pc,
                "fetch address for text segment not aligned to word boundary ",
            ));
        }
        match self.layout.segment(pc) {
            Some(s) if s.is_code() => Ok(self.has_code(pc).then(|| self.peek32(pc))),
            Some(_) if self.self_modifying => Ok(Some(self.peek32(pc))),
            _ => Err(AccessError::new(Access::Load, pc, "fetch address for text segment out of range ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DEFAULT;

    #[test]
    fn segments_follow_mars() {
        assert_eq!(DEFAULT.segment(0x0040_0000), Some(Segment::Text));
        assert_eq!(DEFAULT.segment(0x007f_fffc), Some(Segment::Text));
        assert_eq!(DEFAULT.segment(0x0080_0000), None);
        assert_eq!(DEFAULT.segment(DATA_BASE), Some(Segment::Data));
        assert_eq!(DEFAULT.segment(0x103f_ffff), Some(Segment::Data));
        assert_eq!(DEFAULT.segment(0x1040_0000), None);
        assert_eq!(DEFAULT.segment(SP_INIT), Some(Segment::Stack));
        assert_eq!(DEFAULT.segment(STACK_BASE), Some(Segment::Stack));
        assert_eq!(DEFAULT.segment(STACK_BASE + 1), None);
        assert_eq!(DEFAULT.segment(STACK_LIMIT), None);
        assert_eq!(DEFAULT.segment(EXCEPTION_HANDLER), Some(Segment::KernelText));
        assert_eq!(DEFAULT.segment(KDATA_BASE), Some(Segment::KernelData));
        assert_eq!(DEFAULT.segment(0xffff_000c), Some(Segment::Mmio));
        assert_eq!(DEFAULT.segment(0xffff_ffff), None, "MARS's MMIO limit is exclusive");
        assert_eq!(DEFAULT.segment(0), None);
    }

    #[test]
    fn little_endian_loads_and_stores() {
        let mut m = Memory::new();
        m.store32(DATA_BASE, 0x1122_3344).unwrap();
        assert_eq!(m.load8(DATA_BASE).unwrap(), 0x44);
        assert_eq!(m.load16(DATA_BASE + 2).unwrap(), 0x1122);
        assert_eq!(m.load32(DATA_BASE).unwrap(), 0x1122_3344);
        assert_eq!(m.pages_in_use(), 1);
        // Unwritten memory is zero and takes no pages.
        assert_eq!(m.load32(DATA_BASE + 0x1000).unwrap(), 0);
        assert_eq!(m.pages_in_use(), 1);
    }

    #[test]
    fn refused_accesses_have_mars_messages() {
        let mut m = Memory::new();
        let e = m.load32(DATA_BASE + 2).unwrap_err();
        assert_eq!((e.access, e.addr), (Access::Load, DATA_BASE + 2));
        assert_eq!(e.message, "fetch address not aligned on word boundary 0x10010002");
        assert_eq!(
            m.store16(DATA_BASE + 1, 0).unwrap_err().message,
            "store address not aligned on halfword boundary 0x10010001"
        );
        assert_eq!(m.load8(0).unwrap_err().message, "address out of range 0x00000000");
        assert_eq!(m.store8(0x0080_0000, 0).unwrap_err().access, Access::Store);
        assert_eq!(m.load32(TEXT_BASE).unwrap_err().message, "Cannot read directly from text segment!0x00400000");
        assert!(m.load32(KTEXT_BASE).unwrap_err().message.starts_with("DEVELOPER: You must use getStatement()"));
        m.self_modifying = true;
        assert_eq!(m.load32(TEXT_BASE), Ok(0));
    }

    #[test]
    fn self_modifying_text_is_words() {
        let mut m = Memory::new();
        m.self_modifying = true;
        m.load(TEXT_BASE, &0x1122_3344u32.to_le_bytes());
        m.add_code(TEXT_BASE, TEXT_BASE + 4);
        // As MARS: a byte load reads the low byte of the word; not at a word's address, MARS's
        // statement lookup fails.
        assert_eq!((m.load8(TEXT_BASE).unwrap(), m.load16(TEXT_BASE).unwrap()), (0x44, 0x3344));
        let e = m.load8(TEXT_BASE + 2).unwrap_err();
        assert_eq!(
            (e.access, e.message.as_str()),
            (Access::Load, "fetch address for text segment not aligned to word boundary 0x00400002")
        );
        assert_eq!(m.store8(TEXT_BASE + 1, 1).unwrap_err().access, Access::Load);
        // A byte store replaces the word, and makes a new word code.
        m.store8(TEXT_BASE, 0xAB).unwrap();
        assert_eq!(m.peek32(TEXT_BASE), 0xAB);
        assert_eq!(m.load32(TEXT_BASE + 4), Ok(0), "no statement there");
        assert_eq!(m.fetch(TEXT_BASE + 8), Ok(None));
        let n = m.code_ranges();
        m.store32(TEXT_BASE + 8, 0x2408_0005).unwrap();
        assert_eq!(m.fetch(TEXT_BASE + 8), Ok(Some(0x2408_0005)));
        m.truncate_code(n);
        assert_eq!(m.fetch(TEXT_BASE + 8), Ok(None));
    }

    #[test]
    fn fetch_finds_loaded_code_only() {
        let mut m = Memory::new();
        m.load(TEXT_BASE, &0x2408_0005u32.to_le_bytes());
        m.add_code(TEXT_BASE, TEXT_BASE + 4);
        assert_eq!(m.fetch(TEXT_BASE), Ok(Some(0x2408_0005)));
        assert_eq!(m.fetch(TEXT_BASE + 4), Ok(None));
        assert_eq!(m.fetch(TEXT_BASE + 2).unwrap_err().addr, TEXT_BASE + 2);
        assert!(m.fetch(DATA_BASE).unwrap_err().message.contains("out of range"));
    }

    #[test]
    fn journal_undoes_stores() {
        let mut m = Memory::new();
        m.store32(DATA_BASE, 0xAABB_CCDD).unwrap();
        m.set_journaling(true);
        m.store16(DATA_BASE, 0x1234).unwrap();
        m.store8(SP_INIT, 7).unwrap();
        let j = m.take_journal();
        assert_eq!(j.len(), 3);
        m.undo(&j);
        assert_eq!(m.load32(DATA_BASE).unwrap(), 0xAABB_CCDD);
        assert_eq!(m.load8(SP_INIT).unwrap(), 0);
    }
}
