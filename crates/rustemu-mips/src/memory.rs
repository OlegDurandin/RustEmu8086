//! Memory of the MIPS machine: a sparse 32-bit address space in 4 KB pages, little-endian, with
//! the segments and access rules of MARS's default memory configuration and a journal of
//! overwritten bytes for step back.
//!
//! The segments are MARS's effective ones (its tables hold 4 MB per segment):
//!
//! | segment | addresses | program access |
//! |---|---|---|
//! | text | `0x00400000..0x00800000` | instruction fetch; loads and stores only with [`Memory::self_modifying`] |
//! | data | `0x10000000..0x10400000` | loads and stores |
//! | stack | `0x7fbffffd..=0x7ffffffc` (start address) | loads and stores |
//! | kernel text | `0x80000000..0x80400000` | instruction fetch |
//! | kernel data | `0x90000000..0x90400000` | loads and stores |
//! | MMIO | `0xffff0000..=0xffffffff` | loads and stores |
//!
//! Anything else is "address out of range". The loader and the debugger use the raw accessors
//! ([`Memory::peek8`], [`Memory::load`]), which reach every address.

use std::collections::HashMap;

/// Bytes per page.
pub const PAGE_SIZE: usize = 4096;
/// Size MARS gives each segment (its block tables hold 4 MB).
const SEGMENT_SIZE: u32 = 4 * 1024 * 1024;

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
    /// The segment of `addr` (for a multi-byte access: of its first byte), None when the address
    /// is outside every segment.
    pub fn of(addr: u32) -> Option<Segment> {
        Some(match addr {
            DATA_SEGMENT_BASE..DATA_LIMIT => Segment::Data,
            a if a > STACK_LIMIT && a <= STACK_BASE => Segment::Stack,
            MMIO_BASE..=u32::MAX => Segment::Mmio,
            TEXT_BASE..TEXT_LIMIT => Segment::Text,
            KDATA_BASE..KDATA_LIMIT => Segment::KernelData,
            KTEXT_BASE..KTEXT_LIMIT => Segment::KernelText,
            _ => return None,
        })
    }

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

/// The memory of the machine.
#[derive(Clone, Default)]
pub struct Memory {
    /// Pages that were written, by page number (address / [`PAGE_SIZE`]); the rest reads as 0.
    pages: HashMap<u32, Box<[u8; PAGE_SIZE]>>,
    /// While executing: `(address, old value)` of every byte a store changed.
    journal: Option<Vec<(u32, u8)>>,
    /// Address ranges `[start, end)` that hold loaded code (fetching elsewhere in a code segment
    /// finds no instruction: the program "dropped off the bottom").
    code: Vec<(u32, u32)>,
    /// Programs may load and store in the text segment (MARS setting "self-modifying code",
    /// off by default).
    pub self_modifying: bool,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memory").field("pages", &self.pages.len()).field("code", &self.code).finish()
    }
}

impl Memory {
    /// Empty memory: every byte reads as 0, nothing is code.
    pub fn new() -> Memory {
        Memory::default()
    }

    // ---- raw access (loader, debugger) ----

    /// The byte at `addr`, whatever the segment (unwritten memory is 0).
    pub fn peek8(&self, addr: u32) -> u8 {
        self.pages.get(&(addr / PAGE_SIZE as u32)).map_or(0, |p| p[addr as usize % PAGE_SIZE])
    }

    /// The little-endian word at `addr`, whatever the segment.
    pub fn peek32(&self, addr: u32) -> u32 {
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

    /// Forgets all contents and code (a fresh machine).
    pub fn clear(&mut self) {
        self.pages.clear();
        self.code.clear();
        if let Some(j) = &mut self.journal {
            j.clear();
        }
    }

    /// Number of pages in use (each [`PAGE_SIZE`] bytes).
    pub fn pages_in_use(&self) -> usize {
        self.pages.len()
    }

    fn page_mut(&mut self, addr: u32) -> &mut [u8; PAGE_SIZE] {
        self.pages.entry(addr / PAGE_SIZE as u32).or_insert_with(|| Box::new([0; PAGE_SIZE]))
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

    /// Restores bytes recorded in a journal (in reverse order).
    pub fn undo(&mut self, entries: &[(u32, u8)]) {
        for &(a, v) in entries.iter().rev() {
            self.page_mut(a)[a as usize % PAGE_SIZE] = v;
        }
    }

    // ---- program access ----

    /// Checks that the program may load or store at `addr` (alignment aside).
    pub fn check_access(&self, access: Access, addr: u32) -> Result<(), AccessError> {
        match Segment::of(addr) {
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
                    Access::Load => "Cannot read directly from kernel text segment!",
                    Access::Store => "Cannot write directly to kernel text segment!",
                },
            )),
            Some(_) => Ok(()),
            None => Err(AccessError::new(access, addr, "address out of range ")),
        }
    }

    fn read(&self, addr: u32, len: u32) -> u32 {
        (0..len).rev().fold(0, |v, i| (v << 8) | self.peek8(addr.wrapping_add(i)) as u32)
    }

    fn write(&mut self, addr: u32, len: u32, value: u32) {
        for i in 0..len {
            let a = addr.wrapping_add(i);
            let b = (value >> (8 * i)) as u8;
            let page = self.pages.entry(a / PAGE_SIZE as u32).or_insert_with(|| Box::new([0; PAGE_SIZE]));
            let cell = &mut page[a as usize % PAGE_SIZE];
            if let Some(j) = &mut self.journal {
                j.push((a, *cell));
            }
            *cell = b;
        }
    }

    /// A byte the program loads (`lb`, `lbu`, and each byte of `lwl` / `lwr`).
    pub fn load8(&self, addr: u32) -> Result<u8, AccessError> {
        self.check_access(Access::Load, addr)?;
        Ok(self.peek8(addr))
    }

    /// A halfword the program loads (`lh`, `lhu`); it must be aligned.
    pub fn load16(&self, addr: u32) -> Result<u16, AccessError> {
        if !addr.is_multiple_of(2) {
            return Err(AccessError::new(Access::Load, addr, "fetch address not aligned on halfword boundary "));
        }
        self.check_access(Access::Load, addr)?;
        Ok(self.read(addr, 2) as u16)
    }

    /// A word the program loads (`lw`, `ll`); it must be aligned.
    pub fn load32(&self, addr: u32) -> Result<u32, AccessError> {
        if !addr.is_multiple_of(4) {
            return Err(AccessError::new(Access::Load, addr, "fetch address not aligned on word boundary "));
        }
        self.check_access(Access::Load, addr)?;
        Ok(self.read(addr, 4))
    }

    /// Stores a byte (`sb`, and each byte of `swl` / `swr`).
    pub fn store8(&mut self, addr: u32, value: u8) -> Result<(), AccessError> {
        self.check_access(Access::Store, addr)?;
        self.write(addr, 1, value as u32);
        Ok(())
    }

    /// Stores an aligned halfword (`sh`).
    pub fn store16(&mut self, addr: u32, value: u16) -> Result<(), AccessError> {
        if !addr.is_multiple_of(2) {
            return Err(AccessError::new(Access::Store, addr, "store address not aligned on halfword boundary "));
        }
        self.check_access(Access::Store, addr)?;
        self.write(addr, 2, value as u32);
        Ok(())
    }

    /// Stores an aligned word (`sw`, `sc`).
    pub fn store32(&mut self, addr: u32, value: u32) -> Result<(), AccessError> {
        if !addr.is_multiple_of(4) {
            return Err(AccessError::new(Access::Store, addr, "store address not aligned on word boundary "));
        }
        self.check_access(Access::Store, addr)?;
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
        match Segment::of(pc) {
            Some(s) if s.is_code() => Ok(self.has_code(pc).then(|| self.peek32(pc))),
            Some(_) if self.self_modifying => Ok(Some(self.peek32(pc))),
            _ => Err(AccessError::new(Access::Load, pc, "fetch address for text segment out of range ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_follow_mars() {
        assert_eq!(Segment::of(0x0040_0000), Some(Segment::Text));
        assert_eq!(Segment::of(0x007f_fffc), Some(Segment::Text));
        assert_eq!(Segment::of(0x0080_0000), None);
        assert_eq!(Segment::of(DATA_BASE), Some(Segment::Data));
        assert_eq!(Segment::of(0x103f_ffff), Some(Segment::Data));
        assert_eq!(Segment::of(0x1040_0000), None);
        assert_eq!(Segment::of(SP_INIT), Some(Segment::Stack));
        assert_eq!(Segment::of(STACK_BASE), Some(Segment::Stack));
        assert_eq!(Segment::of(STACK_BASE + 1), None);
        assert_eq!(Segment::of(STACK_LIMIT), None);
        assert_eq!(Segment::of(EXCEPTION_HANDLER), Some(Segment::KernelText));
        assert_eq!(Segment::of(KDATA_BASE), Some(Segment::KernelData));
        assert_eq!(Segment::of(0xffff_000c), Some(Segment::Mmio));
        assert_eq!(Segment::of(0), None);
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
        m.self_modifying = true;
        assert_eq!(m.load32(TEXT_BASE), Ok(0));
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
