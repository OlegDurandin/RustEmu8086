//! The assembler of MARS's dialect: source text to an [`Image`] for the [`Machine`](crate::Machine)
//! and the debug information of the program.
//!
//! - Statements: `label: mnemonic operands # comment`; registers `$t0` or `$8`; integers in
//!   decimal, `0x` hex, `'c'`; `label+n` in loads, stores and `la`.
//! - Instructions: MARS's basic instructions and pseudo-instructions, expanded as MARS expands
//!   them (`li`, `la`, `move`, `blt`, `mul $t0, $t1, 5`, `lw $t0, label`, ...); a mnemonic's forms
//!   are tried in MARS's order and the first whose operands fit is taken.
//! - Directives: `.text` / `.data` / `.ktext` / `.kdata` (with an optional address), `.word`,
//!   `.half`, `.byte`, `.float`, `.double` (also `value : count`), `.ascii`, `.asciiz`, `.space`,
//!   `.align`, `.extern`, `.globl`, `.eqv`; data is aligned to its size, as in MARS, and a label
//!   just before it moves with it.
//! - `%hi(label)` / `%lo(label)` (an extension over MARS, as in GNU as): `lui $t0, %hi(x)` +
//!   `addiu $t0, $t0, %lo(x)` (or `lw $t1, %lo(x)($t0)`) load the address of `x`.
//!
//! Strings are stored as UTF-8 (MARS keeps only the low byte of each character). Macros,
//! `.include` and the floating-point instructions are not supported yet.

mod basic;
mod lexer;
mod table;

use std::collections::{BTreeMap, HashMap};

pub use basic::{BASIC, Basic, Format};
pub use lexer::{DIRECTIVES, IntClass, Kind, Token, parse_int, tokenize};
pub use table::{Def, Form, Operand, Table};

use crate::machine::{Chunk, Image};
use crate::memory::{self, Segment};

/// An error (or a warning) of the source: 1-based line and column, and MARS's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmError {
    /// 1-based line.
    pub line: usize,
    /// 1-based column (0: the whole line).
    pub col: usize,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} column {}: {}", self.line, self.col, self.message)
    }
}

/// The machine instructions of a source statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// 1-based line of the source.
    pub line: usize,
    /// Address of its first instruction.
    pub addr: u32,
    /// The instructions (one, or the expansion of a pseudo-instruction).
    pub words: Vec<u32>,
    /// Each instruction as MARS shows it in the Basic column: `lui $1,4097`, `beq $8,$0,3`.
    pub basic: Vec<String>,
}

/// Data a source line put into memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLine {
    /// 1-based line.
    pub line: usize,
    /// Address of its first byte (after alignment).
    pub addr: u32,
    /// Number of bytes (a `.space` too).
    pub len: u32,
    /// Size of an element: 1 (`.byte`, strings, `.space`), 2, 4, 8 (`.double`).
    pub unit: u8,
}

/// A label of the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The name (case-sensitive, as in MARS).
    pub name: String,
    /// Its address.
    pub addr: u32,
    /// It labels data (otherwise code).
    pub data: bool,
}

/// An assembled program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// What the machine loads.
    pub image: Image,
    /// The instructions by source statement, in address order.
    pub statements: Vec<Statement>,
    /// The data by source line.
    pub data: Vec<DataLine>,
    /// The labels, in address order.
    pub symbols: Vec<Symbol>,
    /// Warnings (MARS's: a `.byte` value out of range, `.set` ignored).
    pub warnings: Vec<AsmError>,
}

/// A data directive: the size of its elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataKind {
    Byte,
    Half,
    Word,
    Float,
    Double,
    Ascii,
    Asciiz,
}

impl DataKind {
    fn of(name: &str) -> Option<DataKind> {
        Some(match name {
            ".byte" => DataKind::Byte,
            ".half" => DataKind::Half,
            ".word" => DataKind::Word,
            ".float" => DataKind::Float,
            ".double" => DataKind::Double,
            ".ascii" => DataKind::Ascii,
            ".asciiz" => DataKind::Asciiz,
            _ => return None,
        })
    }

    fn size(self) -> u32 {
        match self {
            DataKind::Byte | DataKind::Ascii | DataKind::Asciiz => 1,
            DataKind::Half => 2,
            DataKind::Word | DataKind::Float => 4,
            DataKind::Double => 8,
        }
    }
}

/// A statement waiting for the second pass.
struct Pending {
    line: usize,
    tokens: Vec<Token>,
    form: &'static Form,
    addr: u32,
}

/// Index of the user / kernel address space.
const USER: usize = 0;
const KERNEL: usize = 1;

/// The state of the assembly.
struct Asm {
    errors: Vec<AsmError>,
    warnings: Vec<AsmError>,
    symbols: HashMap<String, (u32, bool)>,
    eqv: HashMap<String, Vec<Token>>,
    in_data: bool,
    text_space: usize,
    data_space: usize,
    text_addr: [u32; 2],
    data_addr: [u32; 2],
    extern_addr: u32,
    auto_align: bool,
    last_data: Option<DataKind>,
    data: BTreeMap<u32, u8>,
    data_lines: Vec<DataLine>,
    forward: Vec<(u32, u32, Token, usize)>,
    pending: Vec<Pending>,
    line: usize,
    /// Address of the first byte the current data line stored.
    first_write: Option<u32>,
}

impl Asm {
    fn new() -> Asm {
        Asm {
            errors: Vec::new(),
            warnings: Vec::new(),
            symbols: HashMap::new(),
            eqv: HashMap::new(),
            in_data: false,
            text_space: USER,
            data_space: USER,
            text_addr: [memory::TEXT_BASE, memory::KTEXT_BASE],
            data_addr: [memory::DATA_BASE, memory::KDATA_BASE],
            extern_addr: memory::DATA_SEGMENT_BASE,
            auto_align: true,
            last_data: None,
            data: BTreeMap::new(),
            data_lines: Vec::new(),
            forward: Vec::new(),
            pending: Vec::new(),
            line: 0,
            first_write: None,
        }
    }

    fn error(&mut self, col: usize, message: impl Into<String>) {
        self.errors.push(AsmError { line: self.line, col, message: message.into() });
    }

    fn warning(&mut self, col: usize, message: impl Into<String>) {
        self.warnings.push(AsmError { line: self.line, col, message: message.into() });
    }

    fn data_addr(&self) -> u32 {
        self.data_addr[self.data_space]
    }

    fn set_data_addr(&mut self, a: u32) {
        self.data_addr[self.data_space] = a;
    }

    // ---- the first pass ----

    fn line(&mut self, text: &str) {
        let mut tokens = match tokenize(text) {
            Ok(t) => t,
            Err((col, m)) => return self.error(col, m),
        };
        // .eqv substitution (not of the name being defined).
        let defining = tokens.first().is_some_and(|t| t.kind == Kind::Directive && t.text.eq_ignore_ascii_case(".eqv"));
        let mut i = if defining { 2 } else { 0 };
        while i < tokens.len() {
            if tokens[i].kind == Kind::Ident
                && let Some(rep) = self.eqv.get(&tokens[i].text)
            {
                let col = tokens[i].col;
                let rep: Vec<Token> = rep.iter().map(|t| Token { col, ..t.clone() }).collect();
                let n = rep.len();
                tokens.splice(i..=i, rep);
                i += n;
            } else {
                i += 1;
            }
        }
        while tokens.len() >= 2 && tokens[0].kind == Kind::Ident && tokens[1].kind == Kind::Colon {
            let label = tokens.remove(0);
            tokens.remove(0);
            let addr = if self.in_data { self.data_addr() } else { self.text_addr[self.text_space] };
            self.define(&label, addr, self.in_data);
        }
        let Some(first) = tokens.first().cloned() else { return };
        if first.kind == Kind::Directive {
            self.directive(&first, &tokens[1..]);
        } else if self.in_data {
            match self.last_data {
                Some(kind) => self.store(kind, &tokens),
                None => self
                    .error(first.col, format!("\"{}\": instructions belong in the text segment (.text)", first.text)),
            }
        } else {
            self.instruction(tokens);
        }
    }

    fn define(&mut self, label: &Token, addr: u32, data: bool) {
        if self.symbols.contains_key(&label.text) {
            return self.error(label.col, format!("label \"{}\" already defined", label.text));
        }
        self.symbols.insert(label.text.clone(), (addr, data));
    }

    fn instruction(&mut self, tokens: Vec<Token>) {
        let m = &tokens[0];
        if m.kind != Kind::Ident {
            return self.error(m.col, format!("\"{}\" is not a recognized operator", m.text));
        }
        if let Some(fp) = tokens.iter().find(|t| matches!(t.kind, Kind::FpReg(_))) {
            return self.error(fp.col, "floating-point instructions (coprocessor 1) are not supported yet");
        }
        let Some(forms) = Table::get().forms(&m.text) else {
            let mnemonic = m.text.to_ascii_lowercase();
            if mnemonic.contains('.') || mnemonic.starts_with("bc1") || mnemonic.ends_with("c1") {
                return self.error(m.col, "floating-point instructions (coprocessor 1) are not supported yet");
            }
            return self.error(m.col, format!("\"{}\" is not a recognized operator", m.text));
        };
        let form = match table::choose(&tokens[1..], forms) {
            Ok(f) => f,
            Err((i, msg)) => {
                let col = i.map_or(m.col, |i| tokens[i + 1].col);
                return self.error(col, msg);
            }
        };
        let addr = self.text_addr[self.text_space];
        let end = addr.wrapping_add(form.words() * 4).wrapping_sub(1);
        if !Segment::of(addr).is_some_and(Segment::is_code) || Segment::of(end) != Segment::of(addr) {
            return self.error(m.col, format!("\"{}\" is not a valid text segment address", addr as i32));
        }
        self.text_addr[self.text_space] = addr.wrapping_add(form.words() * 4);
        self.pending.push(Pending { line: self.line, tokens, form, addr });
    }

    fn need_data(&mut self, d: &Token) -> bool {
        if !self.in_data {
            self.error(d.col, format!("\"{}\" directive cannot appear in text segment", d.text));
        }
        self.in_data
    }

    fn directive(&mut self, d: &Token, args: &[Token]) {
        let name = d.text.to_ascii_lowercase();
        let int_arg = |a: &[Token]| match a {
            [Token { kind: Kind::Int(v), .. }] => Some(*v),
            _ => None,
        };
        match name.as_str() {
            ".text" | ".ktext" => {
                self.in_data = false;
                self.text_space = if name == ".text" { USER } else { KERNEL };
                if let Some(Token { kind: Kind::Int(v), .. }) = args.first() {
                    self.text_addr[self.text_space] = *v as u32;
                }
            }
            ".data" | ".kdata" => {
                self.in_data = true;
                self.auto_align = true;
                self.data_space = if name == ".data" { USER } else { KERNEL };
                if let Some(Token { kind: Kind::Int(v), .. }) = args.first() {
                    self.set_data_addr(*v as u32);
                }
            }
            ".align" | ".space" => {
                if !self.need_data(d) {
                    return;
                }
                if args.len() != 1 {
                    return self.error(d.col, format!("\"{}\" requires one operand", d.text));
                }
                let Some(n) = int_arg(args).filter(|&n| n >= 0) else {
                    return self.error(d.col, format!("\"{}\" requires a non-negative integer", d.text));
                };
                if name == ".align" {
                    match n {
                        0 => self.auto_align = false,
                        1..=30 => self.align(1 << n),
                        _ => self.error(d.col, format!("\"{}\" requires a non-negative integer", d.text)),
                    }
                } else {
                    let start = self.data_addr();
                    if self.check_data(start, n as u32, d.col) {
                        self.data_lines.push(DataLine { line: self.line, addr: start, len: n as u32, unit: 1 });
                        self.set_data_addr(start.wrapping_add(n as u32));
                    }
                }
            }
            ".extern" => {
                let [label, size] = args else {
                    return self
                        .error(d.col, format!("\"{}\" directive requires two operands (label and size).", d.text));
                };
                let Some(size) = int_arg(std::slice::from_ref(size)).filter(|&n| n >= 0) else {
                    return self.error(d.col, format!("\"{}\" requires a non-negative integer size", d.text));
                };
                if !self.symbols.contains_key(&label.text) {
                    let addr = self.extern_addr;
                    self.define(label, addr, true);
                    self.extern_addr = self.extern_addr.wrapping_add(size as u32);
                }
            }
            ".globl" | ".global" => {
                if args.is_empty() {
                    return self.error(d.col, format!("\"{}\" directive requires at least one argument.", d.text));
                }
                if let Some(a) = args.iter().find(|a| a.kind != Kind::Ident) {
                    self.error(a.col, format!("\"{}\" directive argument must be label.", d.text));
                }
            }
            ".eqv" => match args {
                [n, rest @ ..] if n.kind == Kind::Ident && !rest.is_empty() => {
                    self.eqv.insert(n.text.clone(), rest.to_vec());
                }
                _ => self.error(d.col, "Too few operands for .eqv directive."),
            },
            ".set" => self.warning(d.col, "MARS currently ignores the .set directive."),
            ".macro" | ".end_macro" | ".include" => {
                self.error(d.col, format!("\"{}\": macros and .include are not supported yet", d.text))
            }
            _ => {
                let Some(kind) = DataKind::of(&name) else {
                    return self
                        .error(d.col, format!("\"{}\" directive is invalid or not implemented in MARS", d.text));
                };
                if self.need_data(d) {
                    self.last_data = Some(kind);
                    self.store(kind, args);
                }
            }
        }
    }

    /// Aligns the data address to `boundary`; labels at the old address move with it (MARS's
    /// `fixSymbolTableAddress`).
    fn align(&mut self, boundary: u32) {
        let a = self.data_addr();
        let rem = a % boundary;
        if rem != 0 {
            let new = a + boundary - rem;
            for v in self.symbols.values_mut() {
                if v.0 == a && v.1 {
                    v.0 = new;
                }
            }
            self.set_data_addr(new);
        }
    }

    /// Whether `len` bytes at `addr` are in a data segment.
    fn check_data(&mut self, addr: u32, len: u32, col: usize) -> bool {
        let seg = Segment::of(addr);
        let ok = matches!(seg, Some(Segment::Data | Segment::Stack | Segment::KernelData | Segment::Mmio))
            && (len == 0 || Segment::of(addr.wrapping_add(len - 1)) == seg);
        if !ok {
            self.error(col, format!("\"{}\" is not a valid data segment address", addr as i32));
        }
        ok
    }

    /// Writes `len` bytes of `value` (little-endian) at the data address, aligned to `len`.
    fn write(&mut self, value: u64, len: u32, col: usize) -> Option<u32> {
        if self.auto_align {
            self.align(len);
        }
        let a = self.data_addr();
        if !self.check_data(a, len, col) {
            return None;
        }
        for i in 0..len {
            self.data.insert(a + i, (value >> (8 * i)) as u8);
        }
        self.first_write.get_or_insert(a);
        self.set_data_addr(a + len);
        Some(a)
    }

    /// Stores the operands of a data directive (or of a continuation line).
    fn store(&mut self, kind: DataKind, args: &[Token]) {
        self.first_write = None;
        match kind {
            DataKind::Ascii | DataKind::Asciiz => {
                for t in args {
                    if t.kind != Kind::Str {
                        self.error(t.col, format!("\"{}\" is not a valid character string", t.text));
                        continue;
                    }
                    let mut bytes = unescape(&t.text[1..t.text.len() - 1]);
                    if kind == DataKind::Asciiz {
                        bytes.push(0);
                    }
                    let a = self.data_addr();
                    if self.check_data(a, bytes.len() as u32, t.col) {
                        for (i, b) in bytes.iter().enumerate() {
                            self.data.insert(a + i as u32, *b);
                        }
                        self.first_write.get_or_insert(a);
                        self.set_data_addr(a + bytes.len() as u32);
                    }
                }
            }
            _ => {
                if let [v, Token { kind: Kind::Colon, .. }, count] = args {
                    let Some(n) = int_value(count).filter(|&n| n >= 0) else {
                        return self.error(
                            count.col,
                            format!("\"{}\": repetition count must be a non-negative integer", count.text),
                        );
                    };
                    for _ in 0..n {
                        if !self.value(kind, v) {
                            break;
                        }
                    }
                } else {
                    for t in args {
                        self.value(kind, t);
                    }
                }
            }
        }
        // One data line for what this source line stored.
        if let Some(begin) = self.first_write {
            let len = self.data_addr().wrapping_sub(begin);
            self.data_lines.push(DataLine { line: self.line, addr: begin, len, unit: kind.size() as u8 });
        }
    }

    /// Stores one value of a numeric directive; false after an error.
    fn value(&mut self, kind: DataKind, t: &Token) -> bool {
        let len = kind.size();
        match kind {
            DataKind::Float | DataKind::Double => {
                let v = match t.kind {
                    Kind::Int(i) => i as f64,
                    Kind::Real(r) => r,
                    _ => {
                        self.error(t.col, format!("\"{}\" is not a valid floating point constant", t.text));
                        return false;
                    }
                };
                if kind == DataKind::Float {
                    if v.is_finite() && v.abs() > f32::MAX as f64 {
                        self.error(t.col, format!("\"{}\" is an out-of-range value", t.text));
                        return false;
                    }
                    self.write((v as f32).to_bits() as u64, 4, t.col).is_some()
                } else {
                    self.write(v.to_bits(), 8, t.col).is_some()
                }
            }
            _ => match &t.kind {
                Kind::Int(v) => {
                    let out = match kind {
                        DataKind::Byte => !(-128..=127).contains(v),
                        DataKind::Half => !(-32768..=32767).contains(v),
                        _ => false,
                    };
                    if out {
                        self.warning(
                            t.col,
                            format!("\"{}\" is out-of-range for a signed value and possibly truncated", t.text),
                        );
                    }
                    self.write(*v as u32 as u64, len, t.col).is_some()
                }
                Kind::Ident => {
                    let known = self.symbols.get(&t.text).map(|s| s.0);
                    let Some(at) = self.write(known.unwrap_or(0) as u64, len, t.col) else { return false };
                    if known.is_none() {
                        self.forward.push((at, len, t.clone(), self.line));
                    }
                    true
                }
                _ => {
                    self.error(t.col, format!("\"{}\" is not a valid integer constant or label", t.text));
                    false
                }
            },
        }
    }

    // ---- the second pass ----

    /// The value of a label operand or a `%hi` / `%lo`, as a token text (MARS replaces labels by
    /// their addresses before expanding pseudo-instructions).
    fn resolve(&mut self, t: &Token) -> Option<Token> {
        let lookup = |s: &Asm, name: &str| s.symbols.get(name).map(|v| v.0);
        match &t.kind {
            Kind::Ident => match lookup(self, &t.text) {
                Some(a) => Some(Token { kind: Kind::Int(a as i32), text: (a as i32).to_string(), col: t.col }),
                None => {
                    self.error(t.col, format!("Symbol \"{}\" not found in symbol table.", t.text));
                    None
                }
            },
            Kind::Reloc { hi, label, offset } => match lookup(self, label) {
                Some(a) => {
                    let a = a.wrapping_add(*offset as u32);
                    let v = if *hi { (a.wrapping_add(0x8000) >> 16) as i32 } else { a as i16 as i32 };
                    Some(Token { kind: Kind::Int(v), text: v.to_string(), col: t.col })
                }
                None => {
                    self.error(t.col, format!("Symbol \"{label}\" not found in symbol table."));
                    None
                }
            },
            _ => Some(t.clone()),
        }
    }

    /// Encodes a basic instruction at `addr`: the word and its Basic text.
    fn encode(&self, b: &Basic, ops: &[Token], addr: u32) -> Result<(u32, String), String> {
        let mut values = Vec::new();
        let mut text = b.example.split(' ').next().unwrap_or("").to_string();
        let mut first = true;
        let mut prev_paren = false;
        for t in ops {
            let piece = match &t.kind {
                Kind::LParen | Kind::RParen => {
                    text.push_str(&t.text);
                    prev_paren = true;
                    continue;
                }
                Kind::RegName(r) | Kind::RegNum(r) => {
                    values.push(*r as i64);
                    format!("${r}")
                }
                Kind::Int(v) => {
                    values.push(*v as i64);
                    v.to_string()
                }
                Kind::Ident => {
                    let target = self
                        .symbols
                        .get(&t.text)
                        .map(|s| s.0)
                        .ok_or(format!("Symbol \"{}\" not found in symbol table.", t.text))?;
                    let v = match b.format {
                        Format::B => {
                            let rel = (target as i64 - (addr as i64 + 4)) >> 2;
                            if !(-32768..=32767).contains(&rel) {
                                return Err(format!("Branch target word address beyond 16-bit range: \"{}\"", t.text));
                            }
                            rel
                        }
                        _ => target as i64,
                    };
                    values.push(v);
                    (v as i32).to_string()
                }
                _ => return Err(format!("\"{}\": operand is of incorrect type", t.text)),
            };
            text.push_str(if first {
                " "
            } else if prev_paren {
                ""
            } else {
                ","
            });
            text.push_str(&piece);
            first = false;
            prev_paren = false;
        }
        if b.format == Format::J {
            let target = values[0] as u32;
            if target & 0xF000_0000 != addr & 0xF000_0000 {
                return Err("Jump target word address beyond 26-bit range".into());
            }
            values[0] = (target >> 2) as i64;
        }
        let mut word = 0u32;
        let mask = b.mask.as_bytes();
        for (k, letter) in b"fst".iter().copied().enumerate() {
            let width = mask.iter().filter(|&&c| c == letter).count() as u32;
            if width == 0 {
                continue;
            }
            let v = values.get(k).copied().unwrap_or(0) as u64 & ((1u64 << width) - 1);
            let mut bit = width;
            for (i, &c) in mask.iter().enumerate() {
                if c == letter {
                    bit -= 1;
                    word |= ((v >> bit) as u32 & 1) << (31 - i);
                }
            }
        }
        for (i, &c) in mask.iter().enumerate() {
            if c == b'1' {
                word |= 1 << (31 - i);
            }
        }
        Ok((word, text))
    }

    /// Builds the instructions of a statement.
    fn build(&mut self, p: &Pending) -> Option<Statement> {
        self.line = p.line;
        let ops: Vec<Token> = p.tokens[1..].iter().map(|t| self.resolve(t)).collect::<Option<_>>()?;
        let mut st = Statement { line: p.line, addr: p.addr, words: Vec::new(), basic: Vec::new() };
        match &p.form.def {
            Def::Basic(b) => {
                // Branch and jump labels stay labels: they are encoded relative to the address.
                let ops: Vec<Token> = p.tokens[1..]
                    .iter()
                    .zip(ops)
                    .map(|(orig, r)| {
                        if orig.kind == Kind::Ident && matches!(b.format, Format::B | Format::J) {
                            orig.clone()
                        } else {
                            r
                        }
                    })
                    .collect();
                match self.encode(b, &ops, p.addr) {
                    Ok((w, t)) => {
                        st.words.push(w);
                        st.basic.push(t);
                    }
                    Err(e) => {
                        self.error(p.tokens[0].col, e);
                        return None;
                    }
                }
            }
            Def::Pseudo { templates, .. } => {
                let texts: Vec<&str> =
                    std::iter::once(p.tokens[0].text.as_str()).chain(ops.iter().map(|t| t.text.as_str())).collect();
                let label = p.tokens.last().filter(|t| t.kind == Kind::Ident).map_or("", |t| t.text.as_str());
                let mut addr = p.addr;
                for tpl in templates {
                    let Some(line) = table::substitute(tpl, &texts, label) else { continue };
                    let built = tokenize(&line).ok().and_then(|toks| {
                        let (m, rest) = toks.split_first()?;
                        let forms = Table::get().forms(&m.text)?;
                        // As MARS: the first basic form that fits, otherwise the first one (an
                        // expansion's operands are not range-checked; the mask truncates them).
                        let basic: Vec<&Form> = forms.iter().filter(|f| matches!(f.def, Def::Basic(_))).collect();
                        let form = basic.iter().find(|f| table::check(rest, f).is_ok()).or(basic.first())?;
                        let Def::Basic(b) = &form.def else { return None };
                        Some(self.encode(b, rest, addr))
                    });
                    match built {
                        Some(Ok((w, t))) => {
                            st.words.push(w);
                            st.basic.push(t);
                        }
                        Some(Err(e)) => {
                            self.error(p.tokens[0].col, e);
                            return None;
                        }
                        None => {
                            let msg = format!(
                                "internal error: the expansion \"{line}\" of {} is not an instruction",
                                p.form.example
                            );
                            self.error(p.tokens[0].col, msg);
                            return None;
                        }
                    }
                    addr += 4;
                }
            }
        }
        Some(st)
    }
}

/// The bytes of a string's contents with MARS's escapes (`\n \t \r \\ \' \" \b \f \0`; another
/// escaped character stands for itself), UTF-8.
fn unescape(s: &str) -> Vec<u8> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('0') => out.push('\0'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out.into_bytes()
}

fn int_value(t: &Token) -> Option<i32> {
    match t.kind {
        Kind::Int(v) => Some(v),
        _ => None,
    }
}

/// Groups bytes by address into contiguous chunks.
fn chunks_of(bytes: impl Iterator<Item = (u32, u8)>, code: bool) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    for (a, b) in bytes {
        match out.last_mut() {
            Some(c) if c.addr.wrapping_add(c.bytes.len() as u32) == a && Segment::of(a) == Segment::of(c.addr) => {
                c.bytes.push(b)
            }
            _ => out.push(Chunk { addr: a, bytes: vec![b], code }),
        }
    }
    out
}

/// Assembles a MARS program.
pub fn assemble(source: &str) -> Result<Program, Vec<AsmError>> {
    let mut asm = Asm::new();
    for (i, line) in source.lines().enumerate() {
        asm.line = i + 1;
        asm.line(line);
    }
    for (at, len, t, line) in std::mem::take(&mut asm.forward) {
        asm.line = line;
        match asm.symbols.get(&t.text) {
            Some(&(v, _)) => {
                for i in 0..len {
                    asm.data.insert(at + i, (v >> (8 * i)) as u8);
                }
            }
            None => asm.error(t.col, format!("Symbol \"{}\" not found in symbol table.", t.text)),
        }
    }
    let pending = std::mem::take(&mut asm.pending);
    let statements: Vec<Statement> = pending.iter().filter_map(|p| asm.build(p)).collect();
    if !asm.errors.is_empty() {
        asm.errors.sort_by_key(|e| (e.line, e.col));
        return Err(asm.errors);
    }
    let mut statements = statements;
    statements.sort_by_key(|s| s.addr);
    let code = statements.iter().flat_map(|s| {
        s.words.iter().enumerate().flat_map(move |(k, w)| {
            let a = s.addr + 4 * k as u32;
            w.to_le_bytes().into_iter().enumerate().map(move |(i, b)| (a + i as u32, b))
        })
    });
    let mut chunks = chunks_of(code, true);
    chunks.extend(chunks_of(asm.data.iter().map(|(&a, &b)| (a, b)), false));
    let mut symbols: Vec<Symbol> =
        asm.symbols.iter().map(|(n, &(addr, data))| Symbol { name: n.clone(), addr, data }).collect();
    symbols.sort_by(|a, b| (a.addr, &a.name).cmp(&(b.addr, &b.name)));
    Ok(Program {
        image: Image { chunks, entry: memory::TEXT_BASE },
        statements,
        data: asm.data_lines,
        symbols,
        warnings: asm.warnings,
    })
}
