//! The MIPS side of the session: a session with the MIPS machine, loading a program assembled by
//! `rustemu_mips::asm`, and its debug information in the session's common form (source lines and
//! addresses, symbols) so that source-level debugging works as for the 8086.

use rustemu_asm::{Dialect, EmuDirectives, Format, LineInfo, Symbol, SymbolKind};
use rustemu_mips::AsMips;
use rustemu_mips::memory::TEXT_BASE;

use crate::Session;

/// Error of a MIPS operation on a session with another processor.
const NOT_MIPS: &str = "the loaded machine is not a MIPS";

impl Session {
    /// A session with a MIPS machine after a reset.
    pub fn new_mips() -> Session {
        let mut s = Session::new();
        s.target = Box::new(rustemu_mips::Machine::new());
        s
    }

    /// The MIPS machine, when the session emulates one.
    pub fn mips(&self) -> Option<&rustemu_mips::Machine> {
        self.target.mips()
    }

    /// The MIPS machine, when the session emulates one, to change it.
    pub fn mips_mut(&mut self) -> Option<&mut rustemu_mips::Machine> {
        self.target.mips_mut()
    }

    /// Loads a MIPS program assembled from file `name`, with its debug information.
    pub fn load_mips(&mut self, prog: rustemu_mips::asm::Program, name: &str) -> Result<(), String> {
        let m = self.mips_mut().ok_or(NOT_MIPS)?;
        m.load(&prog.image)?;
        self.program = Some(debug_info(&prog, name));
        self.mips_program = Some(prog);
        self.loaded = None;
        self.index_symbols();
        self.index_lines();
        Ok(())
    }

    /// Reload of a MIPS program: the same image on a fresh machine.
    pub(crate) fn mips_reload(&mut self) -> Result<(), String> {
        let prog = self.mips_program.clone().ok_or("nothing loaded")?;
        let m = self.mips_mut().ok_or(NOT_MIPS)?;
        m.load(&prog.image)
    }

    /// Assembles MIPS source `text` of file `name` and loads it.
    pub fn load_mips_source(&mut self, name: &str, text: &str) -> Result<(), String> {
        let prog = rustemu_mips::asm::assemble(text)
            .map_err(|e| e.iter().map(|e| format!("{name}: {e}")).collect::<Vec<_>>().join("\n"))?;
        self.load_mips(prog, name)
    }
}

/// The debug information of a MIPS program in the session's common form: statements and data by
/// source line (offsets from the text segment base), labels as symbols.
fn debug_info(prog: &rustemu_mips::asm::Program, name: &str) -> rustemu_asm::Program {
    let line = |file_line: usize| file_line as u32;
    let mut lines: Vec<LineInfo> = prog
        .statements
        .iter()
        .map(|s| LineInfo {
            file: 0,
            line: line(s.line),
            image_offset: s.addr.wrapping_sub(TEXT_BASE),
            len: s.words.len() as u32 * 4,
            is_code: true,
            data_size: 0,
            data_float: false,
            origin: None,
            chain: Vec::new(),
        })
        .collect();
    lines.extend(prog.data.iter().filter(|d| d.len > 0).map(|d| LineInfo {
        file: 0,
        line: line(d.line),
        image_offset: d.addr.wrapping_sub(TEXT_BASE),
        len: d.len,
        is_code: false,
        data_size: d.unit,
        data_float: false,
        origin: None,
        chain: Vec::new(),
    }));
    // A data label's size is the unit of its data (`.byte` / `.asciiz` 1, `.half` 2, `.word` 4).
    let unit = |addr: u32| prog.data.iter().find(|d| d.addr == addr && d.len > 0).map_or(4, |d| d.unit.max(1));
    let symbols = prog
        .symbols
        .iter()
        .map(|s| Symbol {
            name: s.name.clone(),
            kind: if s.data { SymbolKind::Variable } else { SymbolKind::Label },
            segment: None,
            value: s.addr as i64,
            size: s.data.then(|| unit(s.addr)),
        })
        .collect();
    rustemu_asm::Program {
        format: Format::Bin,
        image: Vec::new(),
        origin: 0,
        relocs: Vec::new(),
        entry: (0, 0),
        stack: (0, 0),
        lines,
        listing: Vec::new(),
        symbols,
        segments: Vec::new(),
        files: vec![name.to_string()],
        emu: EmuDirectives::default(),
        dialect: Dialect::default(),
    }
}
