//! System bus abstraction used by the CPU: memory and I/O port access.

/// Size of the 8086 physical address space (1 MiB).
pub const MEM_SIZE: usize = 0x10_0000;
/// Mask applied to physical addresses (20-bit address bus, wraps at 1 MiB).
pub const ADDR_MASK: u32 = 0xF_FFFF;

/// Computes the 20-bit physical address of `seg:off`.
#[inline]
pub fn linear(seg: u16, off: u16) -> u32 {
    ((seg as u32) << 4).wrapping_add(off as u32) & ADDR_MASK
}

/// Everything the CPU can talk to.
pub trait Bus {
    fn read8(&mut self, addr: u32) -> u8;
    fn write8(&mut self, addr: u32, val: u8);

    fn port_in8(&mut self, port: u16) -> u8;
    fn port_out8(&mut self, port: u16, val: u8);

    fn port_in16(&mut self, port: u16) -> u16 {
        let lo = self.port_in8(port) as u16;
        let hi = self.port_in8(port.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    fn port_out16(&mut self, port: u16, val: u16) {
        self.port_out8(port, val as u8);
        self.port_out8(port.wrapping_add(1), (val >> 8) as u8);
    }

    /// Returns the vector of a pending maskable hardware interrupt and acknowledges it.
    /// Called only when the CPU is able to accept interrupts (IF=1).
    fn take_irq(&mut self) -> Option<u8> {
        None
    }
}

/// Flat 1 MiB memory with an optional write journal (used for "step back").
pub struct Memory {
    data: Box<[u8]>,
    journal: Option<Vec<(u32, u8)>>,
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl Memory {
    pub fn new() -> Self {
        Self { data: vec![0u8; MEM_SIZE].into_boxed_slice(), journal: None }
    }

    #[inline]
    pub fn read8(&self, addr: u32) -> u8 {
        self.data[(addr & ADDR_MASK) as usize]
    }

    #[inline]
    pub fn write8(&mut self, addr: u32, val: u8) {
        let a = (addr & ADDR_MASK) as usize;
        if let Some(j) = &mut self.journal {
            j.push((a as u32, self.data[a]));
        }
        self.data[a] = val;
    }

    pub fn read16(&self, addr: u32) -> u16 {
        self.read8(addr) as u16 | ((self.read8(addr.wrapping_add(1)) as u16) << 8)
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        self.write8(addr, val as u8);
        self.write8(addr.wrapping_add(1), (val >> 8) as u8);
    }

    /// Copies `bytes` into memory starting at `addr` (bypasses the journal).
    pub fn load(&mut self, addr: u32, bytes: &[u8]) {
        for (i, &b) in bytes.iter().enumerate() {
            self.data[((addr as usize) + i) & ADDR_MASK as usize] = b;
        }
    }

    pub fn slice(&self, addr: u32, len: usize) -> &[u8] {
        let a = (addr & ADDR_MASK) as usize;
        &self.data[a..(a + len).min(MEM_SIZE)]
    }

    pub fn raw(&self) -> &[u8] {
        &self.data
    }

    pub fn raw_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Enables or disables recording of overwritten bytes (keeps an active journal).
    pub fn set_journaling(&mut self, on: bool) {
        match (on, &self.journal) {
            (true, None) => self.journal = Some(Vec::new()),
            (false, Some(_)) => self.journal = None,
            _ => {}
        }
    }

    pub fn is_journaling(&self) -> bool {
        self.journal.is_some()
    }

    /// Takes the recorded `(address, old value)` pairs, leaving the journal empty.
    pub fn take_journal(&mut self) -> Vec<(u32, u8)> {
        match &mut self.journal {
            Some(j) => std::mem::take(j),
            None => Vec::new(),
        }
    }

    /// Restores bytes recorded in a journal (in reverse order).
    pub fn undo(&mut self, entries: &[(u32, u8)]) {
        for &(a, v) in entries.iter().rev() {
            self.data[a as usize] = v;
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
        if let Some(j) = &mut self.journal {
            j.clear();
        }
    }
}
