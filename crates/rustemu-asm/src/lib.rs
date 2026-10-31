//! 8086 assembler: the Emu8086 / MASM dialect and the NASM dialect (`-f bin` programs).
//!
//! ```
//! let prog = rustemu_asm::assemble_str("org 100h\nmov ax, 5\nret\n").unwrap();
//! assert_eq!(prog.image, [0xB8, 0x05, 0x00, 0xC3]);
//! ```

mod asm;
pub mod encode;
mod errpos;
pub mod expr;
pub mod lexer;
pub mod library;
pub mod parse;
pub mod source;

use std::path::{Path, PathBuf};

pub use source::FileLoader;

/// Built-in include file with the Emu8086-compatible macro library.
pub const EMU8086_INC: &str = include_str!("../inc/emu8086.inc");

/// Source syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialect {
    /// Emu8086 / MASM: `mov ax, var` reads memory, `byte ptr`, `seg segment ... ends`, `MACRO`.
    #[default]
    Masm,
    /// NASM `-f bin`: `mov ax, var` loads the address, `[var]` reads memory, `byte [x]`, `section`,
    /// `times`, `resb`, `%macro`, local labels `.name`.
    Nasm,
}

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Masm => "Emu8086/MASM",
            Dialect::Nasm => "NASM",
        }
    }
}

/// Guesses the dialect of a source file: `.nasm` files, an explicit `#nasm#` line, or NASM-only
/// constructs (`bits 16`, `[org ..]`, `section .text`, `%define`, `%macro`, `times`, `resb`, `$$`)
/// select NASM; everything else is Emu8086/MASM.
pub fn detect_dialect(name: &str, text: &str) -> Dialect {
    if name.to_ascii_lowercase().ends_with(".nasm") {
        return Dialect::Nasm;
    }
    for line in text.lines() {
        let code = code_without_strings(line).trim().to_ascii_lowercase();
        if code.is_empty() {
            continue;
        }
        let first = code.split_whitespace().next().unwrap_or("");
        let nasm_line = code.starts_with("#nasm")
            || code.starts_with("[bits")
            || code.starts_with("[org")
            || code.starts_with("[section")
            || code.starts_with("[cpu")
            || matches!(
                first,
                "bits" | "use16" | "section" | "cpu" | "global" | "extern" | "struc" | "endstruc" | "iend"
            )
            || code.split_whitespace().nth(1) == Some("istruc")
            || (first.starts_with('%')
                && matches!(
                    first,
                    "%define" | "%macro" | "%include" | "%assign" | "%rep" | "%if" | "%ifdef" | "%ifndef"
                ))
            || code.contains("$$")
            || nasm_word_directive(&code)
            || size_without_ptr(&code);
        if nasm_line {
            return Dialect::Nasm;
        }
        if code.starts_with("#fasm") {
            return Dialect::Masm;
        }
    }
    Dialect::Masm
}

/// The code part of a line: strings are blanked out and the comment is removed.
fn code_without_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => out.push(' '),
            None if matches!(c, '\'' | '"' | '`') => quote = Some(c),
            None if c == ';' => break,
            None => out.push(c),
        }
    }
    out
}

/// `byte [..]`, `word [..]`, `dword [..]` without PTR (MASM needs `byte ptr`).
fn size_without_ptr(code: &str) -> bool {
    let w: Vec<&str> = code.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).collect();
    w.windows(2).any(|p| matches!(p[0], "byte" | "word" | "dword") && p[1].starts_with('['))
}

/// `times N ...` or `[label] resb N` (and not a MASM variable that happens to be named so).
fn nasm_word_directive(code: &str) -> bool {
    let w: Vec<&str> = code.split_whitespace().collect();
    let data = ["db", "dw", "dd", "dq", "dt", "equ", "=", "label", "proc", "macro", "segment"];
    w.iter().enumerate().any(|(i, word)| {
        let word = word.trim_end_matches(':');
        let next = w.get(i + 1).copied().unwrap_or("");
        matches!(word, "times" | "resb" | "resw" | "resd" | "resq") && !next.is_empty() && !data.contains(&next)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Com,
    Exe,
    Bin,
    Boot,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Com => "COM",
            Format::Exe => "EXE",
            Format::Bin => "BIN",
            Format::Boot => "BOOT",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Com => "com",
            Format::Exe => "exe",
            Format::Bin => "bin",
            Format::Boot => "boot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmError {
    /// Index into [`Program::files`] / the file list returned with errors.
    pub file: usize,
    /// 1-based line number (0 = not tied to a line).
    pub line: u32,
    /// The part of the line the error is about: 0-based column and length, in characters.
    pub col: Option<usize>,
    pub len: usize,
    pub msg: String,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.col) {
            (0, _) => write!(f, "{}", self.msg),
            (line, Some(col)) => write!(f, "({line}:{}) {}", col + 1, self.msg),
            (line, None) => write!(f, "({line}) {}", self.msg),
        }
    }
}

/// Maps a range of the load image back to a source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineInfo {
    pub file: usize,
    pub line: u32,
    pub image_offset: u32,
    pub len: u32,
    /// True for instructions, false for data.
    pub is_code: bool,
    /// Element size of DB/DW/DD/DQ/DT data (0 for instructions and other statements).
    pub data_size: u8,
    /// For code generated by a macro: (file, line) inside the macro definition.
    pub origin: Option<(usize, u32)>,
    /// Macro nesting: body line in each macro definition, outermost first (`origin` is the last).
    pub chain: Vec<(usize, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingLine {
    pub file: usize,
    pub line: u32,
    /// Segment paragraph (relative to the image) and offset of the emitted bytes.
    pub segment: Option<u16>,
    pub offset: Option<u16>,
    pub bytes: Vec<u8>,
    pub text: String,
    pub macro_depth: u32,
    pub origin: Option<(usize, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Label,
    Variable,
    Constant,
    Segment,
    Proc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// Segment paragraph relative to the image (labels and variables).
    pub segment: Option<u16>,
    pub value: i64,
    pub size: Option<u8>,
}

impl Symbol {
    /// Labels declared with LOCAL inside a macro are renamed to `name??NNNN`.
    pub fn is_macro_local(&self) -> bool {
        self.name.contains("??")
    }

    /// Name as written in the source (`NEXT_KEY??0003` -> `NEXT_KEY`).
    pub fn display_name(&self) -> &str {
        self.name.split("??").next().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentInfo {
    pub name: String,
    pub paragraph: u16,
    pub size: u32,
}

/// Emu8086 `#...#` directives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmuDirectives {
    pub load_segment: Option<u16>,
    pub load_offset: Option<u16>,
    /// Initial register values (`#AX=1234h#`).
    pub regs: Vec<(String, u16)>,
    /// Devices / programs to start (`#start=traffic_lights.exe#`).
    pub start: Vec<String>,
    /// Memory preset with `#MEM=...#`: (physical address, bytes).
    pub mem: Vec<(u32, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub format: Format,
    /// Load module: what is placed in memory (for EXE: without the MZ header).
    pub image: Vec<u8>,
    /// Offset of `image[0]` in its segment (COM: 100h, BIN/BOOT: lowest ORG).
    pub origin: u16,
    /// EXE: image offsets of words that hold segment paragraphs (load segment must be added).
    pub relocs: Vec<u32>,
    /// Initial CS (paragraph relative to the image start) and IP.
    pub entry: (u16, u16),
    /// Initial SS (relative) and SP.
    pub stack: (u16, u16),
    pub lines: Vec<LineInfo>,
    pub listing: Vec<ListingLine>,
    pub symbols: Vec<Symbol>,
    pub segments: Vec<SegmentInfo>,
    pub files: Vec<String>,
    pub emu: EmuDirectives,
    pub dialect: Dialect,
}

impl Program {
    /// Bytes of the output file (.com / .exe / .bin / boot sector).
    pub fn file_bytes(&self) -> Vec<u8> {
        match self.format {
            Format::Exe => self.mz(),
            Format::Boot => {
                let mut v = self.image.clone();
                if v.len() < 512 {
                    v.resize(512, 0);
                }
                v
            }
            _ => self.image.clone(),
        }
    }

    fn mz(&self) -> Vec<u8> {
        let header_len = (0x1C + self.relocs.len() * 4).div_ceil(16) * 16;
        let total = header_len + self.image.len();
        let mut h = vec![0u8; header_len];
        let w = |h: &mut Vec<u8>, at: usize, v: u16| h[at..at + 2].copy_from_slice(&v.to_le_bytes());
        h[0] = b'M';
        h[1] = b'Z';
        w(&mut h, 2, (total % 512) as u16);
        w(&mut h, 4, total.div_ceil(512) as u16);
        w(&mut h, 6, self.relocs.len() as u16);
        w(&mut h, 8, (header_len / 16) as u16);
        // Extra memory: room for the stack if it is beyond the image.
        let image_paras = self.image.len().div_ceil(16) as u32;
        let stack_end = self.stack.0 as u32 + (self.stack.1 as u32).div_ceil(16);
        w(&mut h, 0x0A, stack_end.saturating_sub(image_paras).max(0x10) as u16);
        w(&mut h, 0x0C, 0xFFFF);
        w(&mut h, 0x0E, self.stack.0);
        w(&mut h, 0x10, self.stack.1);
        w(&mut h, 0x14, self.entry.1);
        w(&mut h, 0x16, self.entry.0);
        w(&mut h, 0x18, 0x1C);
        for (i, &r) in self.relocs.iter().enumerate() {
            let at = 0x1C + i * 4;
            w(&mut h, at, (r & 0xF) as u16);
            w(&mut h, at + 2, (r >> 4) as u16);
        }
        h.extend_from_slice(&self.image);
        h
    }

    /// Finds the source line that produced the byte at `image_offset`.
    pub fn line_at(&self, image_offset: u32) -> Option<&LineInfo> {
        self.lines.iter().find(|l| l.image_offset <= image_offset && image_offset < l.image_offset + l.len)
    }

    /// Text listing (address, bytes, source).
    pub fn listing_text(&self) -> String {
        let mut out = String::new();
        for l in &self.listing {
            let addr = match (l.segment, l.offset) {
                (Some(s), Some(o)) => format!("{s:04X}:{o:04X}"),
                _ => " ".repeat(9),
            };
            let mut chunks = l.bytes.chunks(6);
            let first = chunks.next().map(hex_bytes).unwrap_or_default();
            // `+` marks lines generated by macro expansion.
            let mark = if l.macro_depth > 0 { '+' } else { ' ' };
            out.push_str(&format!("{:>5} {addr}  {first:<18}{mark}{}\n", l.line, l.text));
            for c in chunks {
                out.push_str(&format!("{:>5} {}  {}\n", "", " ".repeat(9), hex_bytes(c)));
            }
        }
        out
    }

    /// Symbol table as text (labels local to macro expansions are left out).
    pub fn symbols_text(&self) -> String {
        let mut out = format!("{:<24} {:<9} {:<9} {:<5}\n", "Name", "Type", "Value", "Size");
        for s in self.symbols.iter().filter(|s| !s.is_macro_local()) {
            let kind = match s.kind {
                SymbolKind::Label => "LABEL",
                SymbolKind::Variable => "VAR",
                SymbolKind::Constant => "CONST",
                SymbolKind::Segment => "SEGMENT",
                SymbolKind::Proc => "PROC",
            };
            let val = match s.segment {
                Some(seg) => format!("{seg:04X}:{:04X}", s.value as u16),
                None => format!("{:04X}", s.value as u16),
            };
            let size = s.size.map(|z| z.to_string()).unwrap_or_default();
            out.push_str(&format!("{:<24} {kind:<9} {val:<9} {size:<5}\n", s.name));
        }
        out
    }
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

/// Loads include files from disk (relative to the including file and extra directories),
/// falling back to the built-in `emu8086.inc`.
pub struct FsLoader {
    pub include_dirs: Vec<PathBuf>,
}

impl FileLoader for FsLoader {
    fn load(&mut self, name: &str, from_file: &str) -> Option<(String, String)> {
        let mut candidates = Vec::new();
        if let Some(dir) = Path::new(from_file).parent() {
            candidates.push(dir.join(name));
        }
        for d in &self.include_dirs {
            candidates.push(d.join(name));
        }
        for c in candidates {
            if let Ok(t) = std::fs::read(&c) {
                return Some((c.to_string_lossy().into_owned(), String::from_utf8_lossy(&t).into_owned()));
            }
        }
        builtin_include(name)
    }
}

fn builtin_include(name: &str) -> Option<(String, String)> {
    if name.eq_ignore_ascii_case("emu8086.inc") { Some(("emu8086.inc".into(), EMU8086_INC.into())) } else { None }
}

struct BuiltinLoader;

impl FileLoader for BuiltinLoader {
    fn load(&mut self, name: &str, _from: &str) -> Option<(String, String)> {
        builtin_include(name)
    }
}

/// Error list paired with the file table it refers to.
#[derive(Debug, Clone)]
pub struct AsmErrors {
    pub errors: Vec<AsmError>,
    pub files: Vec<String>,
}

impl std::fmt::Display for AsmErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for e in &self.errors {
            let file = self.files.get(e.file).map(String::as_str).unwrap_or("");
            writeln!(f, "{file}{e}")?;
        }
        Ok(())
    }
}

/// Assembles source text with a custom include loader; the dialect is detected
/// ([`detect_dialect`]).
pub fn assemble(name: &str, source: &str, loader: &mut dyn FileLoader) -> Result<Program, AsmErrors> {
    assemble_dialect(name, source, loader, None)
}

/// Assembles source text in the given dialect (`None`: detect it). A detected dialect is a guess:
/// when the program does not assemble in it but does in the other one, that one is used; when
/// neither works, the errors are those of the detected dialect.
pub fn assemble_dialect(
    name: &str,
    source: &str,
    loader: &mut dyn FileLoader,
    dialect: Option<Dialect>,
) -> Result<Program, AsmErrors> {
    let run = |loader: &mut dyn FileLoader, d| {
        asm::assemble(name, source, loader, d).map_err(|(errors, files)| AsmErrors { errors, files })
    };
    match dialect {
        Some(d) => run(loader, d),
        None => {
            let guess = detect_dialect(name, source);
            let other = if guess == Dialect::Masm { Dialect::Nasm } else { Dialect::Masm };
            run(loader, guess).or_else(|errors| run(loader, other).map_err(|_| errors))
        }
    }
}

/// Assembles source text; only the built-in include files are available.
pub fn assemble_str(source: &str) -> Result<Program, AsmErrors> {
    assemble("source.asm", source, &mut BuiltinLoader)
}

/// Assembles a file from disk.
pub fn assemble_file(path: &Path) -> std::io::Result<Result<Program, AsmErrors>> {
    let text = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
    let mut loader = FsLoader { include_dirs: Vec::new() };
    Ok(assemble(&path.to_string_lossy(), &text, &mut loader))
}

/// Evaluates a constant expression in assembler syntax (`0FFh + 'A' * 2`, `1010b shl 3`, ...).
pub fn evaluate(text: &str) -> Result<i64, String> {
    struct NoSymbols;
    impl expr::Env for NoSymbols {
        fn lookup(&self, _: &str) -> Option<expr::SymVal> {
            None
        }
        fn is_forward(&self, _: &str) -> Option<expr::ForwardKind> {
            None
        }
        fn here(&self) -> (expr::SegId, i64) {
            (0, 0)
        }
        fn seg_paragraph(&self, _: expr::SegId) -> i64 {
            0
        }
    }
    let toks = lexer::tokenize(text).map_err(|(_, m)| m)?;
    if toks.is_empty() {
        return Err("empty expression".into());
    }
    let mut p = expr::Parser::new(&toks);
    let e = p.expr().map_err(|(_, m)| m)?;
    if !p.at_end() {
        return Err("unexpected text after the expression".into());
    }
    let v = expr::eval(&e, &NoSymbols)?;
    if !v.is_plain() {
        return Err("registers and memory are not allowed here".into());
    }
    Ok(v.num)
}

#[cfg(test)]
mod evaluate_tests {
    #[test]
    fn expressions() {
        assert_eq!(super::evaluate("0FFh + 1"), Ok(256));
        assert_eq!(super::evaluate("'A' * 2"), Ok(130));
        assert_eq!(super::evaluate("1010b shl 3 or 1"), Ok(81));
        assert_eq!(super::evaluate("(10 - 25) / 5"), Ok(-3));
        assert_eq!(super::evaluate("7 mod 3 + 1 << 4"), Ok(32));
        assert!(super::evaluate("ax + 1").is_err());
        assert!(super::evaluate("").is_err());
    }
}
