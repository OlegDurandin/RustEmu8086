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
//! - Several files ([`assemble_files`]), as MARS assembles an exception handler file or all the
//!   files of a folder: each has its own labels, macros and `.eqv`; a label declared `.globl` (and
//!   `.extern` data) is in the global table all files see; `.text` and `.data` go on from one
//!   file to the next.
//! - Macros (`.macro name (%a, %b)` … `.end_macro`) and `.include "file"`, by MARS's rules: a
//!   macro is chosen by its name and number of arguments (the last one defined wins), its body
//!   lines are substituted as text (MARS's `Macro.getSubstitutedLine`: each parameter token, from
//!   the last, replaces the first occurrence of its text), its labels become `label_M<n>` with a
//!   counter of expansions, a call of a macro being expanded is an error; `.include` replaces
//!   its whole line with the file (relative to the including file), and including a file a
//!   second time is MARS's "Recursive include" error.
//!
//! Strings are stored as UTF-8 (MARS keeps only the low byte of each character).
//!
//! MARS's settings ([`Settings`]) change the assembly: the memory configuration gives the
//! segment addresses (and in a compact configuration pseudo-instructions use their 16-bit
//! forms), delayed branching adds the delay-slot `nop`s of some pseudo-instructions, without
//! pseudo-instructions each one is an error, warnings can be errors, and execution can start
//! at the global label `main`.

mod basic;
mod lexer;
mod table;

use std::collections::{BTreeMap, HashMap};

pub use basic::{BASIC, Basic, Format};
pub use lexer::{DIRECTIVES, IntClass, Kind, Token, parse_int, tokenize};
pub use table::{Def, Form, Operand, Table};

use crate::machine::{Chunk, Image};
use crate::memory::Segment;
use crate::settings::{Layout, Settings};

/// An error (or a warning) of the source: its file, 1-based line and column, and MARS's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmError {
    /// The file (index into the files of [`AsmErrors`] / [`Program::files`]; 0: the program).
    pub file: usize,
    /// 1-based line (in a macro expansion: the line of the macro's body).
    pub line: usize,
    /// 1-based column (0: the whole line).
    pub col: usize,
    /// What is wrong.
    pub message: String,
    /// In a macro expansion: the lines of the calls, outermost first (MARS writes them as
    /// `line 12->5`).
    pub expansion: Vec<usize>,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line ")?;
        for l in &self.expansion {
            write!(f, "{l}->")?;
        }
        // As MARS: no column for an error of the whole line.
        match self.col {
            0 => write!(f, "{}: {}", self.line, self.message),
            col => write!(f, "{} column {col}: {}", self.line, self.message),
        }
    }
}

/// The errors of an assembly with the files their `file` indices refer to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmErrors {
    /// The errors, by file, line and column.
    pub errors: Vec<AsmError>,
    /// The names of the files (0: the program, then the included ones).
    pub files: Vec<String>,
}

impl std::fmt::Display for AsmErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            match self.files.get(e.file).filter(|n| !n.is_empty()) {
                Some(name) => write!(f, "{name}: {e}")?,
                None => write!(f, "{e}")?,
            }
        }
        Ok(())
    }
}

/// A source file of a program: the program itself or a file it includes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// The name (the program's as given; an included file's path as `.include` resolved it).
    pub name: String,
    /// The text.
    pub text: String,
}

impl Program {
    /// The listing, as MARS's text segment: address, machine word, basic instruction and the
    /// source line (at the first instruction of a statement; for a macro expansion also the body
    /// line, `<5> li $a0, 7`); then the data lines.
    pub fn listing(&self) -> String {
        let src = |file: usize, l: usize| {
            self.files.get(file).and_then(|f| f.text.lines().nth(l.wrapping_sub(1))).map_or("", str::trim_end)
        };
        let mut out = String::from("address    code       basic                      source\n");
        for s in &self.statements {
            for (k, (w, b)) in s.words.iter().zip(&s.basic).enumerate() {
                let source = match (&s.expanded, s.origin) {
                    (_, _) if k > 0 => String::new(),
                    (Some(text), Some((_, body))) => format!("{:>4}: <{body}> {text}", s.line),
                    _ => format!("{:>4}: {}", s.line, src(s.file, s.line)),
                };
                out += &format!("{:08x}   {w:08x}   {b:<26} {source}\n", s.addr + 4 * k as u32);
            }
        }
        if !self.data.is_empty() {
            out += "\ndata\n";
            for d in self.data.iter().filter(|d| d.len > 0) {
                out += &format!("{:08x}   {:>5} bytes  {:>4}: {}\n", d.addr, d.len, d.line, src(d.file, d.line));
            }
        }
        out
    }

    /// The labels, one a line: address, `text` or `data`, name.
    pub fn symbols_text(&self) -> String {
        self.symbols
            .iter()
            .map(|s| format!("{:08x}  {}  {}\n", s.addr, if s.data { "data" } else { "text" }, s.name))
            .collect()
    }

    /// The user text segment as bytes from its base (0x00400000 in the default memory
    /// configuration; little-endian words, gaps as zeros), as MARS's "dump .text Binary".
    pub fn text_bytes(&self) -> Vec<u8> {
        let layout = self.image.settings.layout();
        let in_text = |a: u32| (layout.text_base..layout.text_limit).contains(&a);
        let mut out = Vec::new();
        for s in &self.statements {
            for (k, w) in s.words.iter().enumerate() {
                let a = s.addr + 4 * k as u32;
                if in_text(a) {
                    let at = (a - layout.text_base) as usize;
                    if out.len() < at + 4 {
                        out.resize(at + 4, 0);
                    }
                    out[at..at + 4].copy_from_slice(&w.to_le_bytes());
                }
            }
        }
        out
    }
}

/// The machine instructions of a source statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Its file (index into [`Program::files`]).
    pub file: usize,
    /// 1-based line of the source (for a macro expansion: the line of the outermost call).
    pub line: usize,
    /// For a macro expansion: the body line it comes from (file, 1-based line).
    pub origin: Option<(usize, usize)>,
    /// For a macro expansion: the body line in each macro, outermost first (`origin` is the
    /// last): the calls of nested macros, then the line itself.
    pub chain: Vec<(usize, usize)>,
    /// For a macro expansion: the body line with the arguments substituted (trimmed).
    pub expanded: Option<String>,
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
    /// Its file (index into [`Program::files`]).
    pub file: usize,
    /// 1-based line (for a macro expansion: the line of the outermost call).
    pub line: usize,
    /// For a macro expansion: the body line it comes from (file, 1-based line).
    pub origin: Option<(usize, usize)>,
    /// For a macro expansion: the body line in each macro, outermost first.
    pub chain: Vec<(usize, usize)>,
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
    /// The source files: the program (0), then the included ones in the order of `.include`.
    pub files: Vec<SourceFile>,
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
    /// The file (unit of assembly) of the statement: its labels resolve the operands.
    unit: usize,
    place: Place,
    tokens: Vec<Token>,
    form: &'static Form,
    addr: u32,
}

/// Where the line being assembled comes from.
#[derive(Debug, Clone, Default)]
struct Place {
    /// Its file and 1-based line (in a macro expansion: of the body line).
    file: usize,
    line: usize,
    /// The macro calls being expanded, outermost first: (file, line) of each call.
    calls: Vec<(usize, usize)>,
    /// In a macro expansion: the body line after substitution.
    text: Option<String>,
}

impl Place {
    /// The source line a statement belongs to: the outermost call of a macro expansion.
    fn statement(&self) -> (usize, usize) {
        self.calls.first().copied().unwrap_or((self.file, self.line))
    }

    /// The body line of a macro expansion.
    fn origin(&self) -> Option<(usize, usize)> {
        (!self.calls.is_empty()).then_some((self.file, self.line))
    }

    /// The body line in each macro of an expansion, outermost first.
    fn chain(&self) -> Vec<(usize, usize)> {
        match self.calls.len() {
            0 => Vec::new(),
            _ => self.calls[1..].iter().copied().chain([(self.file, self.line)]).collect(),
        }
    }
}

/// A line of the program after the `.include`s.
struct SrcLine {
    file: usize,
    line: usize,
    text: String,
}

/// A macro: its body is lines `from + 1 .. to` of the program.
struct Macro {
    name: String,
    params: Vec<String>,
    /// Index of the `.macro` line among the program's lines.
    from: usize,
    /// Index of the `.end_macro` line.
    to: usize,
    /// The labels defined in the body (renamed `label_M<n>` in each expansion).
    labels: Vec<String>,
}

/// The most lines all macro expansions of a program may produce (a macro calling another
/// several times, which calls another... grows exponentially).
pub const MAX_EXPANDED_LINES: usize = 250_000;
/// The deepest nesting of macro calls and of `.include`s.
pub const MAX_NESTING: usize = 64;

/// Index of the user / kernel address space.
const USER: usize = 0;
const KERNEL: usize = 1;

/// The state of the assembly.
struct Asm {
    settings: Settings,
    layout: &'static Layout,
    /// The labels the current file declares global (`.globl`), moved to [`Asm::global`] at its end.
    globals: Vec<(Token, Place)>,
    errors: Vec<AsmError>,
    warnings: Vec<AsmError>,
    /// The labels of the current file: address, labels data.
    symbols: HashMap<String, (u32, bool)>,
    /// The global labels (`.globl`, `.extern`), seen by every file.
    global: HashMap<String, (u32, bool)>,
    /// The labels of each file assembled, for the second pass.
    unit_symbols: Vec<HashMap<String, (u32, bool)>>,
    /// Index of the current file among the files assembled.
    unit: usize,
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
    forward: Vec<(u32, u32, Token, Place)>,
    pending: Vec<Pending>,
    /// Where the current line comes from.
    place: Place,
    /// Address of the first byte the current data line stored.
    first_write: Option<u32>,
    /// The program's lines after the `.include`s.
    lines: Vec<SrcLine>,
    /// Index of the current line among `lines` (in an expansion: of the body line).
    index: usize,
    /// The macros defined so far.
    macros: Vec<Macro>,
    /// The macro being defined (between `.macro` and `.end_macro`).
    defining: Option<Macro>,
    /// Expansions so far: the `n` of `label_M<n>`.
    counter: usize,
    /// Indices of the lines of the macro calls being expanded (MARS's loop check).
    call_stack: Vec<usize>,
    /// Lines produced by macro expansions so far.
    expanded_lines: usize,
}

impl Asm {
    fn new(settings: Settings) -> Asm {
        let layout = settings.layout();
        Asm {
            settings,
            layout,
            globals: Vec::new(),
            global: HashMap::new(),
            unit_symbols: Vec::new(),
            unit: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            symbols: HashMap::new(),
            eqv: HashMap::new(),
            in_data: false,
            text_space: USER,
            data_space: USER,
            text_addr: [layout.text_base, layout.ktext_base],
            data_addr: [layout.data_base, layout.kdata_base],
            extern_addr: layout.extern_base,
            auto_align: true,
            last_data: None,
            data: BTreeMap::new(),
            data_lines: Vec::new(),
            forward: Vec::new(),
            pending: Vec::new(),
            place: Place::default(),
            first_write: None,
            lines: Vec::new(),
            index: 0,
            macros: Vec::new(),
            defining: None,
            counter: 0,
            call_stack: Vec::new(),
            expanded_lines: 0,
        }
    }

    fn report(&self, col: usize, message: impl Into<String>) -> AsmError {
        AsmError {
            file: self.place.file,
            line: self.place.line,
            col,
            message: message.into(),
            expansion: self.place.calls.iter().map(|c| c.1).collect(),
        }
    }

    fn error(&mut self, col: usize, message: impl Into<String>) {
        let e = self.report(col, message);
        self.errors.push(e);
    }

    fn warning(&mut self, col: usize, message: impl Into<String>) {
        let w = self.report(col, message);
        self.warnings.push(w);
    }

    /// A data line of what the current line stored.
    fn data_line(&self, addr: u32, len: u32, unit: u8) -> DataLine {
        let (file, line) = self.place.statement();
        DataLine { file, line, origin: self.place.origin(), chain: self.place.chain(), addr, len, unit }
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
        if self.defining.is_some() {
            return self.macro_body(&tokens);
        }
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
            return self.directive(&first, &tokens[1..]);
        }
        // A macro call: `name arg arg`, or `name (arg, arg)`.
        let mut call = tokens.clone();
        if call.len() > 2 && call[1].kind == Kind::LParen && call.last().is_some_and(|t| t.kind == Kind::RParen) {
            call.pop();
            call.remove(1);
        }
        if let Some(m) = self.find_macro(&call) {
            return self.expand(m, &call);
        }
        if self.in_data {
            match self.last_data {
                Some(kind) => self.store(kind, &tokens),
                None => self
                    .error(first.col, format!("\"{}\": instructions belong in the text segment (.text)", first.text)),
            }
        } else {
            self.instruction(tokens);
        }
    }

    /// Moves the labels the file declared `.globl` to the global table (MARS's `transferGlobals`).
    fn transfer_globals(&mut self) {
        for (label, place) in std::mem::take(&mut self.globals) {
            self.place = place;
            match self.symbols.get(&label.text).copied() {
                None => self.error(label.col, format!("\"{}\" declared global label but not defined.", label.text)),
                Some(_) if self.global.contains_key(&label.text) => {
                    self.error(label.col, format!("\"{}\" already defined as global in a different file.", label.text))
                }
                Some(v) => {
                    self.symbols.remove(&label.text);
                    self.global.insert(label.text.clone(), v);
                }
            }
        }
    }

    /// Writes label address `v` into the `len` bytes of data at `at`.
    fn store_label(&mut self, at: u32, len: u32, v: u32) {
        for i in 0..len {
            self.data.insert(at + i, (v >> (8 * i)) as u8);
        }
    }

    /// A label's address and kind: the current file's, else the global one (MARS's
    /// `getAddressLocalOrGlobal`).
    fn lookup(&self, name: &str) -> Option<(u32, bool)> {
        self.symbols.get(name).or_else(|| self.global.get(name)).copied()
    }

    fn define(&mut self, label: &Token, addr: u32, data: bool) {
        if self.symbols.contains_key(&label.text) {
            return self.error(label.col, format!("label \"{}\" already defined", label.text));
        }
        self.symbols.insert(label.text.clone(), (addr, data));
    }

    // ---- macros ----

    /// A line between `.macro` and `.end_macro`: only its label (renamed in each expansion),
    /// `.end_macro` and `.eqv` count now, as in MARS (whose tokenizer does `.eqv` in every line
    /// before assembling).
    fn macro_body(&mut self, tokens: &[Token]) {
        let eqv = |t: &Token| t.kind == Kind::Directive && t.text.eq_ignore_ascii_case(".eqv");
        if let Some(p) = [0, 2].into_iter().find(|&p| tokens.get(p).is_some_and(eqv)) {
            self.directive(&tokens[p].clone(), &tokens[p + 1..]);
        }
        if let [label, Token { kind: Kind::Colon, .. }, ..] = tokens
            && label.kind == Kind::Ident
        {
            self.defining.as_mut().unwrap().labels.push(label.text.clone());
            return;
        }
        match tokens.first() {
            Some(d) if d.kind == Kind::Directive && d.text.eq_ignore_ascii_case(".end_macro") => {
                if tokens.len() > 1 {
                    return self.error(d.col, "invalid text after .END_MACRO");
                }
                let mut m = self.defining.take().unwrap();
                m.to = self.index;
                self.macros.push(m);
            }
            Some(d) if d.kind == Kind::Directive && d.text.eq_ignore_ascii_case(".macro") => {
                self.begin_macro(d, &tokens[1..]);
            }
            _ => {}
        }
    }

    /// `.macro name (%a, %b)`: begins a definition (MARS's checks and messages).
    fn begin_macro(&mut self, d: &Token, args: &[Token]) {
        let Some(name) = args.first() else {
            return self.error(d.col, format!("\"{}\" directive requires at least one argument.", d.text));
        };
        if name.kind != Kind::Ident || Table::get().forms(&name.text).is_some() {
            return self.error(name.col, format!("Invalid Macro name \"{}\"", name.text));
        }
        if self.defining.is_some() {
            return self.error(d.col, "Nested macros are not allowed");
        }
        self.defining = Some(Macro {
            name: name.text.clone(),
            params: Vec::new(),
            from: self.index,
            to: self.index,
            labels: Vec::new(),
        });
        for a in &args[1..] {
            if matches!(a.kind, Kind::LParen | Kind::RParen) {
                continue;
            }
            if !is_param(a) {
                return self.error(a.col, format!("Invalid macro argument '{}'", a.text));
            }
            self.defining.as_mut().unwrap().params.push(a.text.clone());
        }
    }

    /// The macro a statement calls: its name and number of arguments match; of several, the one
    /// defined last.
    fn find_macro(&self, call: &[Token]) -> Option<usize> {
        let name = &call.first()?.text;
        self.macros
            .iter()
            .enumerate()
            .filter(|(_, m)| m.name == *name && m.params.len() + 1 == call.len())
            .max_by_key(|(_, m)| m.from)
            .map(|(i, _)| i)
    }

    /// Expands macro `m` called by `call` (its name and arguments): each body line, substituted,
    /// is assembled as if it were written here.
    fn expand(&mut self, m: usize, call: &[Token]) {
        let counter = self.counter;
        self.counter += 1;
        if self.call_stack.contains(&self.index) {
            return self.error(0, "Detected a macro expansion loop (recursive reference). ");
        }
        if self.call_stack.len() >= MAX_NESTING {
            return self.error(call[0].col, format!("macro calls nested deeper than {MAX_NESTING}"));
        }
        let (from, to) = (self.macros[m].from, self.macros[m].to);
        if self.expanded_lines + (to - from) > MAX_EXPANDED_LINES {
            return self
                .error(call[0].col, format!("macro expansions would make more than {MAX_EXPANDED_LINES} lines"));
        }
        self.expanded_lines += to - from;
        let (index, place) = (self.index, self.place.clone());
        self.call_stack.push(index);
        for i in from + 1..to {
            let (file, line) = (self.lines[i].file, self.lines[i].line);
            self.index = i;
            let mut calls = place.calls.clone();
            calls.push((place.file, place.line));
            self.place = Place { file, line, calls, text: None };
            let text = self.substitute(m, &self.lines[i].text.clone(), call, counter);
            self.place.text = Some(text.trim().to_string());
            self.line(&text);
        }
        self.call_stack.pop();
        self.index = index;
        self.place = place;
    }

    /// A body line of macro `m` with the arguments of `call` and the renamed labels, exactly as
    /// MARS's `getSubstitutedLine`: the tokens from the last one, each replacing the first
    /// occurrence of its text in the line.
    fn substitute(&mut self, m: usize, text: &str, call: &[Token], counter: usize) -> String {
        let Ok(tokens) = tokenize(text) else { return text.to_string() };
        let mut s = text.to_string();
        let replace_first = |s: &mut String, from: &str, to: &str| {
            if let Some(p) = s.find(from) {
                s.replace_range(p..p + from.len(), to);
            }
        };
        for t in tokens.iter().rev() {
            if is_param(t) {
                match self.macros[m].params.iter().position(|p| *p == t.text) {
                    Some(k) => {
                        let arg = call[k + 1].text.clone();
                        replace_first(&mut s, &t.text, &arg);
                    }
                    None => self.error(t.col, "Unknown macro parameter"),
                }
            } else if self.macros[m].labels.contains(&t.text) {
                replace_first(&mut s, &t.text, &format!("{}_M{counter}", t.text));
            }
        }
        s
    }

    fn instruction(&mut self, tokens: Vec<Token>) {
        let m = &tokens[0];
        let unknown = |s: &Asm, m: &Token| {
            if s.macros.iter().any(|d| d.name == m.text) {
                format!("forward reference or invalid parameters for macro \"{}\"", m.text)
            } else {
                format!("\"{}\" is not a recognized operator", m.text)
            }
        };
        if m.kind != Kind::Ident {
            return self.error(m.col, unknown(self, m));
        }
        let Some(forms) = Table::get().forms(&m.text) else {
            return self.error(m.col, unknown(self, m));
        };
        let form = match table::choose(&tokens[1..], forms) {
            Ok(f) => f,
            Err((i, msg)) => {
                let col = i.map_or(m.col, |i| tokens[i + 1].col);
                return self.error(col, msg);
            }
        };
        if matches!(form.def, Def::Pseudo { .. }) && !self.settings.pseudo_instructions {
            // As MARS: an error, and the statement is assembled all the same.
            self.error(m.col, "Extended (pseudo) instruction or format not permitted.  See Settings.");
        }
        let words = form.words(&self.settings);
        let addr = self.text_addr[self.text_space];
        let end = addr.wrapping_add(words * 4).wrapping_sub(1);
        let seg = self.layout.segment(addr);
        if !seg.is_some_and(Segment::is_code) || self.layout.segment(end) != seg {
            return self.error(m.col, format!("\"{}\" is not a valid text segment address", addr as i32));
        }
        self.text_addr[self.text_space] = addr.wrapping_add(words * 4);
        self.pending.push(Pending { unit: self.unit, place: self.place.clone(), tokens, form, addr });
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
                        self.data_lines.push(self.data_line(start, n as u32, 1));
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
                // As MARS: straight into the global table, unless it is there.
                if !self.global.contains_key(&label.text) {
                    self.global.insert(label.text.clone(), (self.extern_addr, true));
                    self.extern_addr = self.extern_addr.wrapping_add(size as u32);
                }
            }
            ".globl" | ".global" => {
                if args.is_empty() {
                    return self.error(d.col, format!("\"{}\" directive requires at least one argument.", d.text));
                }
                for a in args {
                    if a.kind != Kind::Ident {
                        return self.error(d.col, format!("\"{}\" directive argument must be label.", d.text));
                    }
                    self.globals.push((a.clone(), self.place.clone()));
                }
            }
            ".eqv" => match args {
                [n, rest @ ..] if n.kind == Kind::Ident && !rest.is_empty() => {
                    // As GNU's .eqv: a name keeps its value (the same one again is fine, as
                    // when a macro's body defines it).
                    let texts = |v: &[Token]| v.iter().map(|t| t.text.clone()).collect::<Vec<_>>();
                    if self.eqv.get(&n.text).is_some_and(|old| texts(old) != texts(rest)) {
                        return self.error(n.col, format!("\"{}\" is already defined", n.text));
                    }
                    self.eqv.insert(n.text.clone(), rest.to_vec());
                }
                _ => self.error(d.col, "Too few operands for .eqv directive."),
            },
            ".set" => self.warning(d.col, "MARS currently ignores the .set directive."),
            ".macro" => self.begin_macro(d, args),
            ".end_macro" => self.error(d.col, ".END_MACRO without .MACRO"),
            // Without a quoted name (with one the line was replaced by the file).
            ".include" => self.error(d.col, format!("\"{}\" directive recognized but not yet implemented.", d.text)),
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
        let seg = self.layout.segment(addr);
        let ok = matches!(seg, Some(Segment::Data | Segment::Stack | Segment::KernelData | Segment::Mmio))
            && (len == 0 || self.layout.segment(addr.wrapping_add(len - 1)) == seg);
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
            self.data_lines.push(self.data_line(begin, len, kind.size() as u8));
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
                    let known = self.lookup(&t.text).map(|s| s.0);
                    let Some(at) = self.write(known.unwrap_or(0) as u64, len, t.col) else { return false };
                    if known.is_none() {
                        self.forward.push((at, len, t.clone(), self.place.clone()));
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
        let lookup = |s: &Asm, name: &str| s.lookup(name).map(|v| v.0);
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
                Kind::FpReg(r) => {
                    values.push(*r as i64);
                    format!("$f{r}")
                }
                Kind::Int(v) => {
                    values.push(*v as i64);
                    v.to_string()
                }
                Kind::Ident => {
                    let target = self
                        .lookup(&t.text)
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
        // As MARS: an error now is reported at the body line, without the calls (they are over).
        self.place = Place { calls: Vec::new(), ..p.place.clone() };
        let ops: Vec<Token> = p.tokens[1..].iter().map(|t| self.resolve(t)).collect::<Option<_>>()?;
        let (file, line) = p.place.statement();
        let mut st = Statement {
            file,
            line,
            origin: p.place.origin(),
            chain: p.place.chain(),
            expanded: p.place.text.clone(),
            addr: p.addr,
            words: Vec::new(),
            basic: Vec::new(),
        };
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
            Def::Pseudo { .. } => {
                let templates = p.form.templates(&self.settings);
                let texts: Vec<&str> =
                    std::iter::once(p.tokens[0].text.as_str()).chain(ops.iter().map(|t| t.text.as_str())).collect();
                let label = p.tokens.last().filter(|t| t.kind == Kind::Ident).map_or("", |t| t.text.as_str());
                let mut addr = p.addr;
                for tpl in templates {
                    let Some(line) = table::substitute(tpl, &texts, label, self.settings.delayed_branching) else {
                        continue;
                    };
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

/// Groups bytes by address into contiguous chunks (each within one segment of `layout`).
fn chunks_of(bytes: impl Iterator<Item = (u32, u8)>, code: bool, layout: &Layout) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    for (a, b) in bytes {
        match out.last_mut() {
            Some(c)
                if c.addr.wrapping_add(c.bytes.len() as u32) == a && layout.segment(a) == layout.segment(c.addr) =>
            {
                c.bytes.push(b)
            }
            _ => out.push(Chunk { addr: a, bytes: vec![b], code }),
        }
    }
    out
}

/// Whether a token is a macro parameter: `%name`, or (SPIM's style) `$name` that is not a
/// register.
fn is_param(t: &Token) -> bool {
    t.kind == Kind::MacroParam || (t.kind == Kind::Ident && t.text.starts_with('$'))
}

/// The lines of `files[file]` with its `.include`s replaced by the files they name (MARS's
/// `Tokenizer.processIncludes`): a line with `.include "name"` anywhere is replaced as a whole;
/// a relative name is taken from the folder of the including file; a file already included
/// (`seen`) is a "Recursive include" error, as is in MARS any second include of a file.
fn include_lines(
    files: &mut Vec<SourceFile>,
    file: usize,
    read: &mut dyn FnMut(&str) -> Option<String>,
    seen: &mut Vec<String>,
    out: &mut Vec<SrcLine>,
    depth: usize,
) -> Result<(), AsmError> {
    let text = files[file].text.clone();
    for (i, line) in text.lines().enumerate() {
        let tokens = tokenize(line).unwrap_or_default();
        let include = tokens.windows(2).find(|w| {
            w[0].kind == Kind::Directive && w[0].text.eq_ignore_ascii_case(".include") && w[1].kind == Kind::Str
        });
        let Some([_, name]) = include else {
            out.push(SrcLine { file, line: i + 1, text: line.to_string() });
            continue;
        };
        let error = |message: String| AsmError { file, line: i + 1, col: name.col, message, expansion: Vec::new() };
        let name = &name.text[1..name.text.len() - 1];
        let path = match std::path::Path::new(&files[file].name).parent() {
            Some(dir) if !std::path::Path::new(name).is_absolute() && !dir.as_os_str().is_empty() => {
                dir.join(name).to_string_lossy().into_owned()
            }
            _ => name.to_string(),
        };
        if seen.contains(&path) {
            return Err(error(format!("Recursive include of file {path}")));
        }
        if depth >= MAX_NESTING {
            return Err(error(format!("include files nested deeper than {MAX_NESTING}")));
        }
        seen.push(path.clone());
        let Some(text) = read(&path) else {
            return Err(error(format!("Error reading include file {path}")));
        };
        files.push(SourceFile { name: path, text });
        include_lines(files, files.len() - 1, read, seen, out, depth + 1)?;
    }
    Ok(())
}

/// Assembles a MARS program without `.include` files, with MARS's default settings (see
/// [`assemble_with`]).
pub fn assemble(source: &str) -> Result<Program, Vec<AsmError>> {
    assemble_with("", source, &mut |_| None).map_err(|e| e.errors)
}

/// Assembles MARS program `source` of file `name` with MARS's default settings; `read` gives
/// the text of a file `.include` names (its path: relative to the folder of the including file,
/// joined to it), None when there is no such file.
pub fn assemble_with(
    name: &str,
    source: &str,
    read: &mut dyn FnMut(&str) -> Option<String>,
) -> Result<Program, AsmErrors> {
    assemble_settings(name, source, read, &Settings::default())
}

/// Like [`assemble_with`], with MARS's `settings`; the program's [`Image`] records them.
pub fn assemble_settings(
    name: &str,
    source: &str,
    read: &mut dyn FnMut(&str) -> Option<String>,
    settings: &Settings,
) -> Result<Program, AsmErrors> {
    let program = SourceFile { name: name.to_string(), text: source.to_string() };
    assemble_files(program, None, Vec::new(), read, settings)
}

/// Assembles a program of several files as MARS does: `exception_handler` (MARS's setting
/// "include this exception handler file") first, then `program`, then `others` (MARS's "assemble
/// all files in directory"). [`Program::files`] has the program first, then the other files and
/// the included ones; labels are local to their file unless declared `.globl`.
pub fn assemble_files(
    program: SourceFile,
    exception_handler: Option<SourceFile>,
    others: Vec<SourceFile>,
    read: &mut dyn FnMut(&str) -> Option<String>,
    settings: &Settings,
) -> Result<Program, AsmErrors> {
    let names = |files: &[SourceFile]| files.iter().map(|f| f.name.clone()).collect();
    // The files in their order of assembly, as indices into `files`.
    let mut files = vec![program];
    let mut order = Vec::new();
    if let Some(h) = exception_handler {
        files.push(h);
        order.push(files.len() - 1);
    }
    order.push(0);
    for f in others {
        files.push(f);
        order.push(files.len() - 1);
    }
    let mut units = Vec::new();
    for &f in &order {
        let mut lines = Vec::new();
        if let Err(e) = include_lines(&mut files, f, read, &mut Vec::new(), &mut lines, 0) {
            return Err(AsmErrors { errors: vec![e], files: names(&files) });
        }
        units.push(lines);
    }
    let mut asm = Asm::new(*settings);
    let mut accumulated = Vec::new();
    for (u, lines) in units.into_iter().enumerate() {
        // What a file starts with (MARS's first pass for each file).
        asm.unit = u;
        asm.lines = lines;
        asm.symbols.clear();
        asm.eqv.clear();
        asm.macros.clear();
        asm.counter = 0;
        asm.globals.clear();
        asm.in_data = false;
        asm.auto_align = true;
        asm.last_data = None;
        for i in 0..asm.lines.len() {
            asm.index = i;
            asm.place = Place { file: asm.lines[i].file, line: asm.lines[i].line, calls: Vec::new(), text: None };
            let text = asm.lines[i].text.clone();
            asm.line(&text);
        }
        if let Some(m) = asm.defining.take() {
            let l = &asm.lines[m.from];
            asm.place = Place { file: l.file, line: l.line, calls: Vec::new(), text: None };
            asm.error(0, "Macro started but not ended (no .end_macro directive)");
        }
        asm.transfer_globals();
        // Data referring to labels not known yet: this file's or global ones; the others may be
        // global labels of a later file.
        for (at, len, t, place) in std::mem::take(&mut asm.forward) {
            match asm.lookup(&t.text) {
                Some((v, _)) => asm.store_label(at, len, v),
                None => accumulated.push((at, len, t, place)),
            }
        }
        asm.unit_symbols.push(std::mem::take(&mut asm.symbols));
    }
    for (at, len, t, place) in accumulated {
        asm.place = Place { calls: Vec::new(), ..place };
        match asm.global.get(&t.text) {
            Some(&(v, _)) => asm.store_label(at, len, v),
            None => asm.error(t.col, format!("Symbol \"{}\" not found in symbol table.", t.text)),
        }
    }
    let pending = std::mem::take(&mut asm.pending);
    let mut statements = Vec::new();
    for p in &pending {
        if asm.symbols.is_empty() || asm.unit != p.unit {
            asm.unit = p.unit;
            asm.symbols = asm.unit_symbols[p.unit].clone();
        }
        statements.extend(asm.build(p));
    }
    if settings.warnings_are_errors && asm.errors.is_empty() && !asm.warnings.is_empty() {
        // As MARS: the warnings stop the assembly.
        asm.errors = std::mem::take(&mut asm.warnings);
    }
    if !asm.errors.is_empty() {
        asm.errors.sort_by_key(|e| (e.file, e.line, e.col));
        return Err(AsmErrors { errors: asm.errors, files: names(&files) });
    }
    statements.sort_by_key(|s| s.addr);
    let code = statements.iter().flat_map(|s| {
        s.words.iter().enumerate().flat_map(move |(k, w)| {
            let a = s.addr + 4 * k as u32;
            w.to_le_bytes().into_iter().enumerate().map(move |(i, b)| (a + i as u32, b))
        })
    });
    let layout = asm.layout;
    let mut chunks = chunks_of(code, true, layout);
    chunks.extend(chunks_of(asm.data.iter().map(|(&a, &b)| (a, b)), false, layout));
    // MARS's `RegisterFile.initializeProgramCounter(startAtMain)`: the global label `main`, if it
    // labels code.
    let main = asm.global.get("main").map(|s| s.0).filter(|&a| layout.in_code(a));
    let entry = match main {
        Some(a) if settings.start_at_main => a,
        _ => layout.text_base,
    };
    let mut symbols: Vec<Symbol> = asm
        .unit_symbols
        .iter()
        .chain(std::iter::once(&asm.global))
        .flat_map(|table| table.iter().map(|(n, &(addr, data))| Symbol { name: n.clone(), addr, data }))
        .collect();
    symbols.sort_by(|a, b| (a.addr, &a.name).cmp(&(b.addr, &b.name)));
    Ok(Program {
        image: Image { chunks, entry, settings: *settings },
        statements,
        data: asm.data_lines,
        symbols,
        warnings: asm.warnings,
        files,
    })
}
