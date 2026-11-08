//! MARS's settings that change how a program assembles and runs (its Settings menu and command
//! line options `db`, `smc`, `sm`, `np`, `we`, `mc`), and its memory configurations.
//!
//! The assembler takes [`Settings`] and records them in the [`Image`](crate::Image) it makes;
//! [`Machine::load`](crate::Machine::load) sets the memory and the processor up from them, so a
//! program always runs with the settings it was assembled with.

use crate::memory::Segment;

/// MARS's memory configurations (Settings → Memory Configuration, command line `mc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize)]
pub enum MemoryConfig {
    /// The 32-bit address space: text at 0x00400000, data at 0x10000000 (MARS's default).
    #[default]
    Default,
    /// A 32 KB space, data from address 0, text at 0x3000.
    CompactDataAtZero,
    /// A 32 KB space, text from address 0, data at 0x1000.
    CompactTextAtZero,
}

impl MemoryConfig {
    /// Every configuration, in MARS's order.
    pub const ALL: [MemoryConfig; 3] =
        [MemoryConfig::Default, MemoryConfig::CompactDataAtZero, MemoryConfig::CompactTextAtZero];

    /// MARS's name in its settings dialog.
    pub fn title(self) -> &'static str {
        match self {
            MemoryConfig::Default => "Default",
            MemoryConfig::CompactDataAtZero => "Compact, Data at Address 0",
            MemoryConfig::CompactTextAtZero => "Compact, Text at Address 0",
        }
    }

    /// MARS's identifier (the argument of its `mc` option).
    pub fn id(self) -> &'static str {
        match self {
            MemoryConfig::Default => "Default",
            MemoryConfig::CompactDataAtZero => "CompactDataAtZero",
            MemoryConfig::CompactTextAtZero => "CompactTextAtZero",
        }
    }

    /// The configuration of MARS identifier `id` (case-insensitive).
    pub fn from_id(id: &str) -> Option<MemoryConfig> {
        MemoryConfig::ALL.into_iter().find(|c| c.id().eq_ignore_ascii_case(id))
    }

    /// Its addresses.
    pub fn layout(self) -> &'static Layout {
        match self {
            MemoryConfig::Default => &DEFAULT,
            MemoryConfig::CompactDataAtZero => &DATA_AT_ZERO,
            MemoryConfig::CompactTextAtZero => &TEXT_AT_ZERO,
        }
    }
}

/// The addresses of a memory configuration. The segments are MARS's effective ones: each is
/// limited by the configuration and by MARS's tables (4 MB a segment, 64 KB of MMIO), computed
/// as MARS computes them, in signed 32-bit arithmetic. Limits are exclusive except the stack's
/// base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// The configuration.
    pub config: MemoryConfig,
    /// Base of the text segment: where the program counter starts.
    pub text_base: u32,
    /// End (exclusive) of the text segment.
    pub text_limit: u32,
    /// Base of the data segment (where `.extern` data is).
    pub data_segment_base: u32,
    /// End (exclusive) of the data segment.
    pub data_limit: u32,
    /// Where `.extern` data starts.
    pub extern_base: u32,
    /// Initial global pointer `$gp`.
    pub gp: u32,
    /// Where `.data` starts.
    pub data_base: u32,
    /// Where the heap (`sbrk`) starts.
    pub heap_base: u32,
    /// Initial stack pointer `$sp`.
    pub sp: u32,
    /// Highest address of the stack segment (the stack grows down from it).
    pub stack_base: u32,
    /// The stack segment holds the addresses above this one.
    pub stack_limit: u32,
    /// Base of the kernel text segment (`.ktext`).
    pub ktext_base: u32,
    /// End (exclusive) of the kernel text segment.
    pub ktext_limit: u32,
    /// The exception handler: exceptions jump here when it holds code.
    pub exception_handler: u32,
    /// Base of the kernel data segment (`.kdata`).
    pub kdata_base: u32,
    /// End (exclusive) of the kernel data segment.
    pub kdata_limit: u32,
    /// Base of memory-mapped I/O (MARS's devices are here).
    pub mmio_base: u32,
    /// End (exclusive) of memory-mapped I/O.
    pub mmio_limit: u32,
}

/// Bytes MARS's tables hold for a segment.
const TABLE: i32 = 4 << 20;
/// Bytes MARS's table holds for memory-mapped I/O.
const MMIO_TABLE: i32 = 64 << 10;

const fn min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

const fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

/// A layout from MARS's `MemoryConfiguration` values, in its order (`MemoryConfigurations.java`),
/// limited as MARS's `Memory.initialize` limits them (Java ints).
const fn layout(config: MemoryConfig, v: [u32; 21]) -> Layout {
    Layout {
        config,
        text_base: v[0],
        text_limit: min(v[16] as i32, (v[0] as i32).wrapping_add(TABLE)) as u32,
        data_segment_base: v[1],
        data_limit: min(v[15] as i32, (v[1] as i32).wrapping_add(TABLE)) as u32,
        extern_base: v[2],
        gp: v[3],
        data_base: v[4],
        heap_base: v[5],
        sp: v[6],
        stack_base: v[7],
        stack_limit: max(v[19] as i32, (v[7] as i32).wrapping_sub(TABLE)) as u32,
        ktext_base: v[10],
        ktext_limit: min(v[18] as i32, (v[10] as i32).wrapping_add(TABLE)) as u32,
        exception_handler: v[11],
        kdata_base: v[12],
        kdata_limit: min(v[17] as i32, (v[12] as i32).wrapping_add(TABLE)) as u32,
        mmio_base: v[13],
        mmio_limit: min(v[20] as i32, (v[13] as i32).wrapping_add(MMIO_TABLE)) as u32,
    }
}

/// MARS's default configuration.
pub static DEFAULT: Layout = layout(
    MemoryConfig::Default,
    [
        0x0040_0000, // .text base
        0x1000_0000, // data segment base
        0x1000_0000, // .extern base
        0x1000_8000, // $gp
        0x1001_0000, // .data base
        0x1004_0000, // heap base
        0x7fff_effc, // $sp
        0x7fff_fffc, // stack base
        0x7fff_ffff, // user space high address
        0x8000_0000, // kernel space base
        0x8000_0000, // .ktext base
        0x8000_0180, // exception handler
        0x9000_0000, // .kdata base
        0xffff_0000, // MMIO base
        0xffff_ffff, // kernel space high address
        0x7fff_ffff, // data segment limit
        0x0fff_fffc, // text limit
        0xfffe_ffff, // kernel data limit
        0x8fff_fffc, // kernel text limit
        0x1004_0000, // stack limit
        0xffff_ffff, // memory map limit
    ],
);

/// MARS's compact configuration with data at address 0.
pub static DATA_AT_ZERO: Layout = layout(
    MemoryConfig::CompactDataAtZero,
    [
        0x0000_3000,
        0x0000_0000,
        0x0000_1000,
        0x0000_1800,
        0x0000_0000,
        0x0000_2000,
        0x0000_2ffc,
        0x0000_2ffc,
        0x0000_3fff,
        0x0000_4000,
        0x0000_4000,
        0x0000_4180,
        0x0000_5000,
        0x0000_7f00,
        0x0000_7fff,
        0x0000_2fff,
        0x0000_3ffc,
        0x0000_7eff,
        0x0000_4ffc,
        0x0000_2000,
        0x0000_7fff,
    ],
);

/// MARS's compact configuration with text at address 0.
pub static TEXT_AT_ZERO: Layout = layout(
    MemoryConfig::CompactTextAtZero,
    [
        0x0000_0000,
        0x0000_1000,
        0x0000_1000,
        0x0000_1800,
        0x0000_2000,
        0x0000_3000,
        0x0000_3ffc,
        0x0000_3ffc,
        0x0000_3fff,
        0x0000_4000,
        0x0000_4000,
        0x0000_4180,
        0x0000_5000,
        0x0000_7f00,
        0x0000_7fff,
        0x0000_3fff,
        0x0000_0ffc,
        0x0000_7eff,
        0x0000_4ffc,
        0x0000_3000,
        0x0000_7fff,
    ],
);

impl Default for Layout {
    fn default() -> Self {
        DEFAULT
    }
}

impl Layout {
    /// The segment of `addr` (for a multi-byte access: of its first byte), None when the address
    /// is outside every segment. Checked in MARS's order: data, stack, MMIO, text, kernel data,
    /// kernel text (in the compact configurations the stack lies within the data segment).
    pub fn segment(&self, addr: u32) -> Option<Segment> {
        Some(if (self.data_segment_base..self.data_limit).contains(&addr) {
            Segment::Data
        } else if self.in_stack(addr) {
            Segment::Stack
        } else if (self.mmio_base..self.mmio_limit).contains(&addr) {
            Segment::Mmio
        } else if (self.text_base..self.text_limit).contains(&addr) {
            Segment::Text
        } else if (self.kdata_base..self.kdata_limit).contains(&addr) {
            Segment::KernelData
        } else if (self.ktext_base..self.ktext_limit).contains(&addr) {
            Segment::KernelText
        } else {
            return None;
        })
    }

    /// Whether `addr` is in the stack's range (also when it lies within the data segment).
    pub fn in_stack(&self, addr: u32) -> bool {
        addr > self.stack_limit && addr <= self.stack_base
    }

    /// Whether `addr` is in the text or the kernel text segment.
    pub fn in_code(&self, addr: u32) -> bool {
        (self.text_base..self.text_limit).contains(&addr) || (self.ktext_base..self.ktext_limit).contains(&addr)
    }

    /// Every address fits in 15 bits (MARS's `usingCompactMemoryConfiguration`): pseudo-
    /// instructions with a compact form use it.
    pub fn is_compact(&self) -> bool {
        self.config != MemoryConfig::Default
    }
}

/// MARS's settings that change assembly and execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// Delayed branching: the instruction after a branch or jump (its delay slot) executes before
    /// the branch is taken (MARS's `db`; off by default).
    pub delayed_branching: bool,
    /// Self-modifying code: the program may load and store in the text segment and execute
    /// instructions from data (MARS's `smc`; off by default).
    pub self_modifying: bool,
    /// Execution starts at the global label `main` when there is one (MARS's `sm`; off by
    /// default: at the start of the text segment).
    pub start_at_main: bool,
    /// Pseudo-instructions and extended formats are allowed (MARS's default; its `np` turns
    /// this off).
    pub pseudo_instructions: bool,
    /// An assembly with warnings fails (MARS's `we`; off by default).
    pub warnings_are_errors: bool,
    /// The memory configuration (MARS's `mc`).
    pub memory: MemoryConfig,
    /// The reading services (5..8, 12) take their input from a dialog window instead of the
    /// Run I/O console (MARS's "popup dialog for input syscalls"; no command line option).
    pub popup_input: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            delayed_branching: false,
            self_modifying: false,
            start_at_main: false,
            pseudo_instructions: true,
            warnings_are_errors: false,
            memory: MemoryConfig::Default,
            popup_input: false,
        }
    }
}

impl Settings {
    /// The addresses of the memory configuration.
    pub fn layout(&self) -> &'static Layout {
        self.memory.layout()
    }

    /// MARS's command line options for these settings (`db`, `smc`, `sm`, `np`, `we`, `mc …`).
    pub fn mars_options(&self) -> Vec<String> {
        let mut o = Vec::new();
        for (on, name) in [
            (self.delayed_branching, "db"),
            (self.self_modifying, "smc"),
            (self.start_at_main, "sm"),
            (!self.pseudo_instructions, "np"),
            (self.warnings_are_errors, "we"),
        ] {
            if on {
                o.push(name.to_string());
            }
        }
        if self.memory != MemoryConfig::Default {
            o.extend(["mc".to_string(), self.memory.id().to_string()]);
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_segments_as_mars_computes_them() {
        let d = &DEFAULT;
        assert_eq!((d.text_base, d.text_limit), (0x0040_0000, 0x0080_0000));
        assert_eq!((d.data_segment_base, d.data_limit), (0x1000_0000, 0x1040_0000));
        assert_eq!(d.stack_limit, 0x7fbf_fffc);
        assert_eq!((d.ktext_limit, d.kdata_limit), (0x8040_0000, 0x9040_0000));
        // 0xffff0000 + 64 KB overflows to 0: the configured limit (-1) is the smaller.
        assert_eq!((d.mmio_base, d.mmio_limit), (0xffff_0000, 0xffff_ffff));
        let c = &DATA_AT_ZERO;
        assert_eq!((c.text_base, c.text_limit, c.data_limit, c.stack_limit), (0x3000, 0x3ffc, 0x2fff, 0x2000));
        assert_eq!((c.mmio_base, c.mmio_limit, c.exception_handler), (0x7f00, 0x7fff, 0x4180));
        let t = &TEXT_AT_ZERO;
        assert_eq!((t.text_base, t.text_limit, t.data_segment_base, t.data_limit), (0, 0x0ffc, 0x1000, 0x3fff));
    }

    #[test]
    fn segments_in_mars_order() {
        let c = &DATA_AT_ZERO;
        // The stack is within the data segment.
        assert_eq!(c.segment(c.sp), Some(Segment::Data));
        assert!(c.in_stack(c.sp));
        assert_eq!(c.segment(0x3000), Some(Segment::Text));
        assert_eq!(c.segment(0x3ffc), None, "MARS's text limit is exclusive");
        assert_eq!(c.segment(0x7f00), Some(Segment::Mmio));
        assert_eq!(DEFAULT.segment(0xffff_ffff), None);
        assert_eq!(DEFAULT.segment(0xffff_fffc), Some(Segment::Mmio));
    }

    #[test]
    fn options_for_mars() {
        let s = Settings { delayed_branching: true, memory: MemoryConfig::CompactTextAtZero, ..Settings::default() };
        assert_eq!(s.mars_options(), ["db", "mc", "CompactTextAtZero"]);
        assert_eq!(MemoryConfig::from_id("compactdataatzero"), Some(MemoryConfig::CompactDataAtZero));
    }
}
