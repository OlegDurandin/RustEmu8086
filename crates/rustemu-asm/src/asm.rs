//! Multi-pass assembler driver.

use std::collections::HashMap;

use crate::encode::{Ctx, encode};
use crate::expr::{Env, Expr, ForwardKind, SegId, SymVal, Val, eval};
use crate::parse::{DataItem, DotSeg, Operand, Stmt, StmtKind, parse_line, parse_line_nasm};
use crate::source::{FileLoader, Line, Preprocessor};
use crate::{
    AsmError, Dialect, EmuDirectives, Format, LineInfo, ListingLine, Program, SegmentInfo, Symbol, SymbolKind,
};

const MAX_PASSES: usize = 40;
/// From this pass on instructions may only grow (shrinking is padded with NOPs) to force convergence.
const GROW_ONLY_PASS: usize = 12;

struct Seg {
    name: String,
    stack: bool,
    data: Vec<u8>,
    loc: i64,
    max: i64,
    min: i64,
    written: bool,
    /// NASM `.bss`: takes address space but is not stored in the output file.
    nobits: bool,
}

impl Seg {
    fn new(name: &str, stack: bool) -> Self {
        Seg { name: name.into(), stack, data: Vec::new(), loc: 0, max: 0, min: i64::MAX, written: false, nobits: false }
    }
}

struct StmtInfo {
    stmt: Stmt,
    line: usize,
}

struct Assembler {
    /// Source text of each file (to locate errors in their lines).
    sources: Vec<String>,
    lines: Vec<Line>,
    stmts: Vec<StmtInfo>,
    format: Format,
    flat: bool,
    segs: Vec<Seg>,
    /// Segment names -> physical segment id.
    seg_ids: HashMap<String, SegId>,
    layout: Vec<i64>,
    flat_bases: Vec<i64>,
    fasm: bool,
    forward: HashMap<String, ForwardKind>,
    syms: HashMap<String, SymVal>,
    prev: HashMap<String, SymVal>,
    /// Symbols defined with EQU/labels in this pass (for duplicate detection).
    defined_here: HashMap<String, usize>,
    cur_seg: SegId,
    seg_stack: Vec<SegId>,
    procs: Vec<(String, bool)>,
    sizes: Vec<usize>,
    pass: usize,
    errors: Vec<AsmError>,
    relocs: Vec<(SegId, i64)>,
    emitted: Vec<(usize, SegId, i64, usize)>,
    entry: Option<Val>,
    emu: EmuDirectives,
    sym_kinds: HashMap<String, SymbolKind>,
    data_seg: Option<SegId>,
    code_seg: Option<SegId>,
    /// NASM dialect (flat `-f bin` output, `.data` / `.bss` sections aligned to 4).
    nasm: bool,
    /// Element size of NASM data labels (shown as variables in the debugger).
    data_sizes: HashMap<String, u8>,
    /// ORG of the program (start of the NASM `.text` section).
    org: Option<i64>,
}

impl Env for Assembler {
    fn lookup(&self, name: &str) -> Option<SymVal> {
        self.syms.get(name).or_else(|| self.prev.get(name)).cloned()
    }
    fn is_forward(&self, name: &str) -> Option<ForwardKind> {
        self.forward.get(name).copied()
    }
    fn here(&self) -> (SegId, i64) {
        (self.cur_seg, self.segs[self.cur_seg].loc)
    }
    fn seg_paragraph(&self, seg: SegId) -> i64 {
        if self.flat { 0 } else { self.layout.get(seg).copied().unwrap_or(0) }
    }
    fn section_start(&self) -> (SegId, i64) {
        let s = self.cur_seg;
        let start = if s == 0 && self.flat {
            self.org.unwrap_or_else(|| self.initial_loc())
        } else if self.flat {
            self.flat_bases.get(s).copied().unwrap_or(0)
        } else {
            0
        };
        (s, start)
    }
}

/// Assembles `source` (named `name`); INCLUDE files are resolved through `loader`.
pub fn assemble(
    name: &str,
    source: &str,
    loader: &mut dyn FileLoader,
    dialect: Dialect,
) -> Result<Program, (Vec<AsmError>, Vec<String>)> {
    let nasm = dialect == Dialect::Nasm;
    let mut pp = Preprocessor::new(loader);
    pp.nasm = nasm;
    pp.add_file(name, source);
    let files = pp.files.clone();
    let mut errors = std::mem::take(&mut pp.errors);
    let lines = std::mem::take(&mut pp.out);

    let mut stmts = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let parsed = if nasm { parse_line_nasm(&l.text) } else { parse_line(&l.text) };
        match parsed {
            Ok(stmt) => stmts.push(StmtInfo { stmt, line: i }),
            Err((at, msg)) => {
                // The position is in the line as preprocessed; it holds in the source line when
                // that is the same text.
                let source = source_line(&pp.sources, l.file, l.line);
                let span = if l.chain.is_empty() && source == Some(l.text.as_str()) {
                    crate::errpos::token_at(&l.text, at)
                } else {
                    source.and_then(|s| crate::errpos::locate(&msg, s, !l.chain.is_empty()))
                };
                errors.push(AsmError {
                    file: l.file,
                    line: l.line,
                    col: span.map(|s| s.0),
                    len: span.map_or(0, |s| s.1),
                    msg,
                })
            }
        }
    }

    let sources = std::mem::take(&mut pp.sources);
    let mut a = Assembler {
        sources,
        lines,
        stmts,
        format: Format::Com,
        flat: true,
        segs: Vec::new(),
        seg_ids: HashMap::new(),
        layout: Vec::new(),
        flat_bases: Vec::new(),
        fasm: false,
        forward: HashMap::new(),
        syms: HashMap::new(),
        prev: HashMap::new(),
        defined_here: HashMap::new(),
        cur_seg: 0,
        seg_stack: Vec::new(),
        procs: Vec::new(),
        sizes: Vec::new(),
        pass: 0,
        errors: Vec::new(),
        relocs: Vec::new(),
        emitted: Vec::new(),
        entry: None,
        emu: EmuDirectives::default(),
        sym_kinds: HashMap::new(),
        data_seg: None,
        code_seg: None,
        nasm,
        data_sizes: HashMap::new(),
        org: None,
    };
    if nasm {
        rename_local_labels(&mut a.stmts);
    }
    crate::encode::set_strict_size(nasm);
    a.prescan();
    a.sizes = vec![0; a.stmts.len()];
    for pass in 1..=MAX_PASSES {
        a.pass = pass;
        a.run_pass();
        let changed = a.syms != a.prev || a.new_layout() != a.layout || a.new_flat_bases() != a.flat_bases;
        a.layout = a.new_layout();
        a.flat_bases = a.new_flat_bases();
        a.prev = std::mem::take(&mut a.syms);
        if !changed && pass >= 2 {
            a.syms = a.prev.clone();
            break;
        }
        if pass == MAX_PASSES {
            a.syms = a.prev.clone();
            a.errors.push(AsmError { file: 0, line: 0, col: None, len: 0, msg: "assembly did not converge".into() });
        }
    }
    if a.fasm {
        errors.push(AsmError {
            file: 0,
            line: 0,
            col: None,
            len: 0,
            msg: "FASM syntax (#fasm#) is not supported; remove the directive to use the Emu8086/MASM syntax".into(),
        });
    }
    crate::encode::set_strict_size(false);
    errors.append(&mut a.errors);
    if !errors.is_empty() {
        locate_errors(&mut errors, &a.sources);
        return Err((errors, files));
    }
    let sources = std::mem::take(&mut a.sources);
    a.build(files).map_err(|mut e| {
        locate_errors(&mut e, &sources);
        (e, Vec::new())
    })
}

/// Line `line` (1-based) of file `file`.
fn source_line(sources: &[String], file: usize, line: u32) -> Option<&str> {
    sources.get(file)?.lines().nth(line.checked_sub(1)? as usize)
}

/// Finds the part of the line of errors that have no position yet (from the message).
fn locate_errors(errors: &mut [AsmError], sources: &[String]) {
    for e in errors.iter_mut().filter(|e| e.col.is_none()) {
        if let Some((col, len)) =
            source_line(sources, e.file, e.line).and_then(|s| crate::errpos::locate(&e.msg, s, false))
        {
            (e.col, e.len) = (Some(col), len);
        }
    }
}

impl Assembler {
    fn line_of(&self, idx: usize) -> &Line {
        &self.lines[self.stmts[idx].line]
    }

    fn err(&mut self, idx: usize, msg: impl Into<String>) {
        let l = self.line_of(idx);
        let msg = msg.into();
        // In a macro expansion the error is shown at the macro call.
        let span = source_line(&self.sources, l.file, l.line)
            .and_then(|s| crate::errpos::locate(&msg, s, !l.chain.is_empty()));
        let e = AsmError { file: l.file, line: l.line, col: span.map(|s| s.0), len: span.map_or(0, |s| s.1), msg };
        self.errors.push(e);
    }

    fn seg_id(&mut self, name: &str, stack: bool) -> SegId {
        if self.flat {
            return 0;
        }
        self.named_seg(name, stack)
    }

    /// Section for `.code` / `.data` / `.stack`. In flat formats `.data` gets its own section
    /// that is placed after the code (code always starts at the origin); `.stack` is ignored.
    fn dot_seg_id(&mut self, d: &DotSeg) -> SegId {
        let (n, stack) = match d {
            DotSeg::Code => ("CODE", false),
            DotSeg::Data => ("DATA", false),
            DotSeg::Bss => ("BSS", false),
            DotSeg::Stack(_) => ("STACK", true),
        };
        if self.flat {
            return match d {
                DotSeg::Data => self.named_seg(n, false),
                DotSeg::Bss => {
                    let id = self.named_seg(n, false);
                    self.segs[id].nobits = true;
                    id
                }
                _ => 0,
            };
        }
        self.named_seg(n, stack)
    }

    fn named_seg(&mut self, name: &str, stack: bool) -> SegId {
        if let Some(&id) = self.seg_ids.get(name) {
            if stack {
                self.segs[id].stack = true;
            }
            return id;
        }
        let id = self.segs.len();
        self.segs.push(Seg::new(name, stack));
        self.seg_ids.insert(name.to_string(), id);
        id
    }

    /// Determines the output format, segments and forward symbol kinds.
    fn prescan(&mut self) {
        let mut has_segments = false;
        let mut org_100h = false;
        let mut fasm = false;
        let mut explicit = None;
        for s in &self.stmts {
            match &s.stmt.kind {
                StmtKind::Segment { .. } | StmtKind::DotSeg(_) | StmtKind::Model => has_segments = true,
                StmtKind::Org(Expr::Num(0x100)) => org_100h = true,
                StmtKind::Emu(d) => {
                    let u = d.to_ascii_uppercase();
                    explicit = match u.as_str() {
                        "MAKE_COM" => Some(Format::Com),
                        "MAKE_EXE" => Some(Format::Exe),
                        "MAKE_BIN" => Some(Format::Bin),
                        "MAKE_BOOT" => Some(Format::Boot),
                        _ => explicit,
                    };
                    if u == "FASM" {
                        fasm = true;
                    }
                }
                _ => {}
            }
        }
        self.fasm = fasm;
        // Emu8086: COM when ORG 100h is found, EXE when segments are used, COM otherwise.
        self.format = explicit.unwrap_or(if org_100h || !has_segments { Format::Com } else { Format::Exe });
        if self.nasm {
            // NASM -f bin: ORG 100h is a COM program, ORG 7C00h a boot sector, anything else BIN.
            let org = self.stmts.iter().find_map(|s| match &s.stmt.kind {
                StmtKind::Org(Expr::Num(n)) => Some(*n),
                _ => None,
            });
            self.format = match org {
                Some(0x100) => Format::Com,
                Some(0x7C00) => Format::Boot,
                _ => Format::Bin,
            };
        }
        self.flat = self.format != Format::Exe;
        self.segs.clear();
        if self.flat {
            self.segs.push(Seg::new("", false));
        }
        if self.nasm {
            // Sections in NASM's output order: .text, .data, .bss.
            let has = |d: DotSeg| self.stmts.iter().any(|s| s.stmt.kind == StmtKind::DotSeg(d.clone()));
            let (data, bss) = (has(DotSeg::Data), has(DotSeg::Bss));
            if data {
                self.dot_seg_id(&DotSeg::Data);
            }
            if bss {
                self.dot_seg_id(&DotSeg::Bss);
            }
        }

        for i in 0..self.stmts.len() {
            let st = self.stmts[i].stmt.clone();
            if let Some(l) = &st.label {
                let size = match &st.kind {
                    StmtKind::Data { size, .. } if !st.colon => Some(*size),
                    _ => None,
                };
                self.forward.insert(l.clone(), ForwardKind::Label { size, far: false });
                self.sym_kinds.insert(l.clone(), if size.is_some() { SymbolKind::Variable } else { SymbolKind::Label });
                // NASM data labels are addresses for the assembler but variables for the debugger.
                let data = match &st.kind {
                    StmtKind::Data { size, .. } => Some(*size),
                    StmtKind::Times(_, inner) => match inner.as_ref() {
                        StmtKind::Data { size, .. } => Some(*size),
                        _ => None,
                    },
                    _ => None,
                };
                if self.nasm
                    && let Some(size) = data
                {
                    self.sym_kinds.insert(l.clone(), SymbolKind::Variable);
                    self.data_sizes.insert(l.clone(), size);
                }
            }
            match &st.kind {
                StmtKind::Segment { name, stack } => {
                    let id = self.seg_id(name, *stack);
                    if self.code_seg.is_none() && name.contains("CODE") {
                        self.code_seg = Some(id);
                    }
                    self.forward.insert(name.clone(), ForwardKind::Segment);
                    self.sym_kinds.insert(name.clone(), SymbolKind::Segment);
                }
                StmtKind::DotSeg(d) => {
                    let id = self.dot_seg_id(d);
                    match d {
                        DotSeg::Code => self.code_seg = Some(id),
                        DotSeg::Data => self.data_seg = Some(id),
                        _ => {}
                    }
                }
                StmtKind::Proc { name, far } => {
                    self.forward.insert(name.clone(), ForwardKind::Label { size: None, far: *far });
                    self.sym_kinds.insert(name.clone(), SymbolKind::Proc);
                }
                StmtKind::Label { name, size, far } => {
                    self.forward.insert(name.clone(), ForwardKind::Label { size: *size, far: *far });
                    self.sym_kinds
                        .insert(name.clone(), if size.is_some() { SymbolKind::Variable } else { SymbolKind::Label });
                }
                StmtKind::Equ(n, _) | StmtKind::Assign(n, _) => {
                    self.forward.insert(n.clone(), ForwardKind::Const);
                    self.sym_kinds.insert(n.clone(), SymbolKind::Constant);
                }
                _ => {}
            }
        }
        if !self.flat && self.segs.is_empty() {
            self.segs.push(Seg::new("CODE", false));
        }
        if self.data_seg.is_none()
            && let Some(&id) = self.seg_ids.get("DATA")
        {
            self.data_seg = Some(id);
        }
        for n in ["@DATA", "@CODE"] {
            self.forward.insert(n.into(), ForwardKind::Segment);
        }
        self.layout = vec![0; self.segs.len()];
    }

    fn new_layout(&self) -> Vec<i64> {
        let mut out = Vec::with_capacity(self.segs.len());
        let mut para = 0;
        for s in &self.segs {
            out.push(para);
            para += (s.max + 15) / 16;
        }
        out
    }

    fn initial_loc(&self) -> i64 {
        match self.format {
            Format::Com => 0x100,
            Format::Boot => 0x7C00,
            _ => 0,
        }
    }

    /// Start offsets of the sections of a flat program: each one follows the previous.
    fn new_flat_bases(&self) -> Vec<i64> {
        let mut out = Vec::with_capacity(self.segs.len());
        let mut at = self.initial_loc();
        for (i, s) in self.segs.iter().enumerate() {
            if self.nasm && i > 0 {
                // NASM -f bin aligns .data and .bss to 4 bytes.
                at = (at + 3) & !3;
            }
            out.push(at);
            at = s.max.max(at);
        }
        out
    }

    fn run_pass(&mut self) {
        let start = self.initial_loc();
        for (i, s) in self.segs.iter_mut().enumerate() {
            let base = self.flat_bases.get(i).copied().unwrap_or(start);
            s.data.clear();
            s.loc = base;
            s.max = base;
            s.min = i64::MAX;
            s.written = false;
        }
        if !self.flat {
            for s in &mut self.segs {
                s.loc = 0;
                s.max = 0;
            }
        }
        self.errors.clear();
        self.relocs.clear();
        self.emitted.clear();
        self.defined_here.clear();
        self.syms.clear();
        self.cur_seg = if self.flat { 0 } else { self.code_seg.unwrap_or(0) };
        self.seg_stack.clear();
        self.procs.clear();
        self.entry = None;
        self.org = None;
        self.emu = EmuDirectives::default();
        if let Some(d) = self.data_seg {
            self.syms.insert("@DATA".into(), SymVal::Segment(d));
        }
        if let Some(c) = self.code_seg {
            self.syms.insert("@CODE".into(), SymVal::Segment(c));
        }
        if !self.flat {
            for (name, &id) in &self.seg_ids {
                self.syms.insert(name.clone(), SymVal::Segment(id));
            }
        }
        for idx in 0..self.stmts.len() {
            self.statement(idx);
        }
        if !self.seg_stack.is_empty() && !self.flat {
            let n = self.stmts.len().saturating_sub(1);
            if !self.stmts.is_empty() {
                self.err(n, "segment not closed with ENDS");
            }
        }
    }

    fn define(&mut self, idx: usize, name: &str, v: SymVal, redefinable: bool) {
        if !redefinable && let Some(&prev_idx) = self.defined_here.get(name) {
            if prev_idx != idx {
                let l = self.line_of(prev_idx).line;
                self.err(idx, format!("symbol '{name}' already defined (line {l})"));
            }
            return;
        }
        if !redefinable {
            self.defined_here.insert(name.to_string(), idx);
        }
        self.syms.insert(name.to_string(), v);
    }

    fn loc(&self) -> i64 {
        self.segs[self.cur_seg].loc
    }

    fn emit(&mut self, bytes: &[u8]) {
        let s = &mut self.segs[self.cur_seg];
        let at = s.loc;
        if at < 0 || at + bytes.len() as i64 > 0x10000 {
            s.loc += bytes.len() as i64;
            s.max = s.max.max(s.loc);
            return;
        }
        let end = at as usize + bytes.len();
        if s.data.len() < end {
            s.data.resize(end, 0);
        }
        s.data[at as usize..end].copy_from_slice(bytes);
        if !bytes.is_empty() {
            s.min = s.min.min(at);
            s.written = true;
        }
        s.loc += bytes.len() as i64;
        s.max = s.max.max(s.loc);
    }

    fn eval_plain(&mut self, idx: usize, e: &Expr) -> Option<i64> {
        match eval(e, self) {
            Ok(v) if v.unknown => {
                if self.pass > 1 {
                    self.err(idx, "expression uses a symbol that is never defined");
                }
                None
            }
            Ok(v) if v.regs.is_empty() && v.seg_base.is_none() => Some(v.num),
            Ok(_) => {
                self.err(idx, "constant expression expected");
                None
            }
            Err(m) => {
                self.err(idx, m);
                None
            }
        }
    }

    fn statement(&mut self, idx: usize) {
        let st = self.stmts[idx].stmt.clone();
        let start_loc = self.loc();
        let start_seg = self.cur_seg;

        if let Some(name) = &st.label {
            let (size, count) = match &st.kind {
                StmtKind::Data { size, items } if !st.colon => (Some(*size), self.count_items(items)),
                _ => (None, 1),
            };
            let v = SymVal::Label { seg: self.cur_seg, off: self.loc(), size, far: false, count };
            self.define(idx, name, v, false);
        }

        match &st.kind {
            StmtKind::Empty | StmtKind::Ignored | StmtKind::Model => {}
            StmtKind::Emu(d) => self.emu_directive(d),
            StmtKind::Instr { prefixes, mnem, ops } => self.instruction(idx, prefixes, mnem, ops),
            StmtKind::Data { size, items } => self.data(idx, *size, items),
            StmtKind::Times(count, inner) => {
                if let Some(n) = self.eval_plain(idx, count) {
                    if !(0..=0x10000).contains(&n) {
                        self.err(idx, format!("TIMES count {n} is out of range"));
                    }
                    for _ in 0..n.clamp(0, 0x10000) {
                        match inner.as_ref() {
                            StmtKind::Instr { prefixes, mnem, ops } => self.instruction(idx, prefixes, mnem, ops),
                            StmtKind::Data { size, items } => self.data(idx, *size, items),
                            _ => {
                                self.err(idx, "TIMES can repeat an instruction or data only");
                                break;
                            }
                        }
                    }
                }
            }
            StmtKind::Equ(name, e) => match eval(e, self) {
                Ok(v) => {
                    let sv = equ_value(&v);
                    self.define(idx, name, sv, false);
                }
                Err(m) => self.err(idx, m),
            },
            StmtKind::Assign(name, e) => match eval(e, self) {
                Ok(v) => {
                    let sv = equ_value(&v);
                    self.define(idx, name, sv, true);
                }
                Err(m) => self.err(idx, m),
            },
            StmtKind::Org(e) => {
                if let Some(v) = self.eval_plain(idx, e) {
                    if !(0..=0xFFFF).contains(&v) {
                        self.err(idx, "ORG value out of range");
                    } else {
                        self.org.get_or_insert(v);
                        self.segs[self.cur_seg].loc = v;
                        let s = &mut self.segs[self.cur_seg];
                        s.max = s.max.max(v);
                    }
                }
            }
            StmtKind::Even => {
                if self.loc() % 2 != 0 {
                    self.emit(&[0x90]);
                }
            }
            StmtKind::Align(e) => {
                if let Some(n) = self.eval_plain(idx, e)
                    && n > 0
                {
                    let pad = (n - self.loc().rem_euclid(n)) % n;
                    self.emit(&vec![0x90; pad as usize]);
                }
            }
            StmtKind::Segment { name, stack } => {
                if !self.flat {
                    self.seg_stack.push(self.cur_seg);
                    self.cur_seg = self.seg_id(name, *stack);
                }
            }
            StmtKind::Ends => {
                if !self.flat {
                    match self.seg_stack.pop() {
                        Some(s) => self.cur_seg = s,
                        None => self.err(idx, "ENDS without SEGMENT"),
                    }
                }
            }
            StmtKind::DotSeg(d) => {
                let id = self.dot_seg_id(d);
                if self.flat {
                    if !matches!(d, DotSeg::Stack(_)) {
                        self.cur_seg = id;
                    }
                } else {
                    if let DotSeg::Stack(sz) = d {
                        let n = match sz {
                            Some(e) => self.eval_plain(idx, e).unwrap_or(0x400),
                            None => 0x400,
                        };
                        let save = self.cur_seg;
                        self.cur_seg = id;
                        self.emit(&vec![0; n.clamp(0, 0xFFFF) as usize]);
                        self.cur_seg = save;
                    } else {
                        self.cur_seg = id;
                    }
                }
            }
            StmtKind::Proc { name, far } => {
                let v = SymVal::Label { seg: self.cur_seg, off: self.loc(), size: None, far: *far, count: 1 };
                self.define(idx, name, v, false);
                self.procs.push((name.clone(), *far));
            }
            StmtKind::Endp => {
                if self.procs.pop().is_none() {
                    self.err(idx, "ENDP without PROC");
                }
            }
            StmtKind::Label { name, size, far } => {
                let v = SymVal::Label { seg: self.cur_seg, off: self.loc(), size: *size, far: *far, count: 1 };
                self.define(idx, name, v, false);
            }
            StmtKind::End(e) => {
                if let Some(e) = e {
                    match eval(e, self) {
                        Ok(v) => self.entry = Some(v),
                        Err(m) => self.err(idx, m),
                    }
                }
            }
        }

        let len = (self.loc() - start_loc).max(0) as usize;
        if self.cur_seg == start_seg && len > 0 && !matches!(st.kind, StmtKind::Org(_)) {
            self.emitted.push((idx, start_seg, start_loc, len));
        }
    }

    fn instruction(&mut self, idx: usize, prefixes: &[String], mnem: &str, ops: &[Operand]) {
        let start_loc = self.loc();
        let ctx = Ctx {
            loc: start_loc,
            seg: self.cur_seg,
            flat: self.flat,
            in_far_proc: self.procs.last().is_some_and(|p| p.1),
        };
        match encode(prefixes, mnem, ops, self, &ctx) {
            Ok(mut enc) => {
                if self.pass >= GROW_ONLY_PASS && enc.bytes.len() < self.sizes[idx] {
                    enc.bytes.resize(self.sizes[idx], 0x90);
                }
                self.sizes[idx] = enc.bytes.len();
                for r in enc.relocs {
                    self.relocs.push((self.cur_seg, start_loc + r as i64));
                }
                self.emit(&enc.bytes);
            }
            Err(m) => {
                self.err(idx, m);
                // Keep layout stable: reserve the previous size.
                let n = self.sizes[idx];
                self.emit(&vec![0x90; n]);
            }
        }
    }

    fn data(&mut self, idx: usize, size: u8, items: &[DataItem]) {
        let mut out = Vec::new();
        for it in items {
            self.data_item(idx, size, it, &mut out);
        }
        self.emit(&out);
    }

    fn count_items(&self, items: &[DataItem]) -> i64 {
        items
            .iter()
            .map(|it| match it {
                DataItem::Str(s) => s.len() as i64,
                DataItem::Dup(c, inner) => {
                    let n = eval(c, self).map(|v| v.num).unwrap_or(1);
                    n * self.count_items(inner)
                }
                _ => 1,
            })
            .sum()
    }

    fn data_item(&mut self, idx: usize, size: u8, it: &DataItem, out: &mut Vec<u8>) {
        match it {
            DataItem::Uninit => out.extend(std::iter::repeat_n(0, size as usize)),
            DataItem::Str(s) => {
                if size == 1 {
                    out.extend_from_slice(s);
                } else if self.nasm {
                    // NASM: the characters in order, padded with zeros to a whole element.
                    out.extend_from_slice(s);
                    let n = size as usize;
                    out.extend(std::iter::repeat_n(0, (n - s.len() % n) % n));
                } else {
                    self.err(idx, "string too long for this data size");
                }
            }
            DataItem::Dup(count, inner) => {
                let Some(n) = self.eval_plain(idx, count) else { return };
                if !(0..=0x10000).contains(&n) {
                    self.err(idx, "DUP count out of range");
                    return;
                }
                let mut one = Vec::new();
                for i in inner {
                    self.data_item(idx, size, i, &mut one);
                }
                for _ in 0..n {
                    out.extend_from_slice(&one);
                }
            }
            DataItem::Expr(e) => {
                let v = match eval(e, self) {
                    Ok(v) => v,
                    Err(m) => {
                        self.err(idx, m);
                        out.extend(std::iter::repeat_n(0, size as usize));
                        return;
                    }
                };
                if !v.regs.is_empty() {
                    self.err(idx, "registers are not allowed in data");
                }
                let here = self.loc() + out.len() as i64;
                match size {
                    1 => {
                        if v.seg_base.is_some() || (!v.unknown && !(-128..=255).contains(&v.num)) {
                            self.err(idx, format!("value {} does not fit in a byte", v.num));
                        }
                        out.push(v.num as u8);
                    }
                    2 => {
                        if !v.unknown && !(-32768..=65535).contains(&v.num) {
                            self.err(idx, format!("value {} does not fit in a word", v.num));
                        }
                        if v.seg_base.is_some() {
                            self.relocs.push((self.cur_seg, here));
                        }
                        out.extend_from_slice(&(v.num as u16).to_le_bytes());
                    }
                    4 if v.label_seg.is_some() => {
                        // Far pointer: offset then segment.
                        out.extend_from_slice(&(v.num as u16).to_le_bytes());
                        self.relocs.push((self.cur_seg, here + 2));
                        out.extend_from_slice(&(v.label_para as u16).to_le_bytes());
                    }
                    _ => {
                        let bytes = (v.num as i128).to_le_bytes();
                        out.extend_from_slice(&bytes[..size as usize]);
                    }
                }
            }
        }
    }

    fn emu_directive(&mut self, d: &str) {
        let (key, val) = match d.split_once('=') {
            Some((k, v)) => (k.trim().to_ascii_uppercase(), Some(v.trim().to_string())),
            None => (d.trim().to_ascii_uppercase(), None),
        };
        let num = val.as_deref().and_then(|v| {
            let t = v.trim_end_matches(['h', 'H']);
            u16::from_str_radix(t, 16).ok()
        });
        match (key.as_str(), num) {
            ("LOAD_SEGMENT", Some(n)) => self.emu.load_segment = Some(n),
            ("LOAD_OFFSET", Some(n)) => self.emu.load_offset = Some(n),
            ("START", _) => {
                if let Some(v) = val {
                    self.emu.start.push(v);
                }
            }
            (r, Some(n))
                if matches!(
                    r,
                    "AX" | "BX"
                        | "CX"
                        | "DX"
                        | "SI"
                        | "DI"
                        | "BP"
                        | "SP"
                        | "CS"
                        | "DS"
                        | "ES"
                        | "SS"
                        | "IP"
                        | "AL"
                        | "AH"
                        | "BL"
                        | "BH"
                        | "CL"
                        | "CH"
                        | "DL"
                        | "DH"
                ) =>
            {
                self.emu.regs.push((r.to_string(), n));
            }
            ("MEM", _) => {
                if let Some(v) = val {
                    self.emu.mem.extend(parse_mem_directive(&v));
                }
            }
            _ => {}
        }
    }

    fn build(mut self, files: Vec<String>) -> Result<Program, Vec<AsmError>> {
        let mut errors = Vec::new();
        let layout = if self.flat { vec![0; self.segs.len()] } else { self.layout.clone() };
        let flat_min = self.segs.iter().filter(|s| s.written && !s.nobits).map(|s| s.min).min();
        let origin: i64 = match self.format {
            Format::Com => 0x100,
            _ if self.flat => flat_min.unwrap_or(self.initial_loc()),
            _ => 0,
        };
        if self.format == Format::Com && flat_min.is_some_and(|m| m < 0x100) {
            errors.push(AsmError {
                file: 0,
                line: 0,
                col: None,
                len: 0,
                msg: "COM program has code below offset 100h (missing ORG 100h?)".into(),
            });
        }
        // Image assembly.
        let image_off = |seg: SegId, off: i64| -> i64 { if self.flat { off - origin } else { layout[seg] * 16 + off } };
        let mut image = Vec::new();
        if self.flat {
            let end = self.segs.iter().filter(|s| !s.nobits).map(|s| s.max).max().unwrap_or(origin).max(origin);
            image.resize((end - origin) as usize, 0);
            for s in self.segs.iter().filter(|s| !s.nobits) {
                for (o, &b) in s.data.iter().enumerate().skip(origin as usize) {
                    if (o as i64) < s.max && b != 0 {
                        image[o - origin as usize] = b;
                    }
                }
            }
        } else {
            for (i, s) in self.segs.iter().enumerate() {
                let base = (layout[i] * 16) as usize;
                image.resize(base, 0);
                let mut d = s.data.clone();
                d.resize(s.max as usize, 0);
                image.extend_from_slice(&d);
            }
        }
        // Flat formats keep their segment fixups too: the emulator adds the load segment
        // (segment references in a .com/.bin file on disk assume a load segment of 0).
        if self.format == Format::Boot && image.len() > 512 {
            errors.push(AsmError { file: 0, line: 0, col: None, len: 0, msg: "boot sector exceeds 512 bytes".into() });
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        let relocs: Vec<u32> = self.relocs.iter().map(|&(s, o)| image_off(s, o) as u32).collect();

        let (entry, stack) = match self.format {
            Format::Exe => {
                let entry = match (&self.entry, self.syms.get("@STARTUP")) {
                    (Some(v), _) if v.label_seg.is_some() => (layout[v.label_seg.unwrap()] as u16, v.num as u16),
                    (_, Some(SymVal::Label { seg, off, .. })) => (layout[*seg] as u16, *off as u16),
                    _ => (self.code_seg.map(|c| layout[c]).unwrap_or(0) as u16, 0),
                };
                let stack = match self.segs.iter().position(|s| s.stack) {
                    Some(i) => (layout[i] as u16, self.segs[i].max as u16),
                    None => (image.len().div_ceil(16) as u16, 0xFFFE),
                };
                (entry, stack)
            }
            Format::Com => ((0, 0x100), (0, 0xFFFE)),
            _ => {
                let ip = match &self.entry {
                    Some(v) if v.label_seg.is_some() => v.num as u16,
                    _ => origin as u16,
                };
                ((0, ip), (0, 0xFFFE))
            }
        };

        let mut lines = Vec::new();
        let mut listing = Vec::new();
        let emitted: HashMap<usize, (SegId, i64, usize)> =
            self.emitted.iter().map(|&(i, s, o, l)| (i, (s, o, l))).collect();
        for (i, st) in self.stmts.iter().enumerate() {
            let l = &self.lines[st.line];
            let e = emitted.get(&i).copied();
            if let Some((s, o, len)) = e {
                let io = image_off(s, o);
                if io >= 0 {
                    lines.push(LineInfo {
                        file: l.file,
                        line: l.line,
                        image_offset: io as u32,
                        len: len as u32,
                        is_code: matches!(st.stmt.kind, StmtKind::Instr { .. }),
                        origin: l.origin(),
                        chain: l.chain.clone(),
                    });
                }
            }
            let bytes = e
                .map(|(_, _, len)| {
                    let io = image_off(e.unwrap().0, e.unwrap().1).max(0) as usize;
                    image.get(io..(io + len).min(image.len())).unwrap_or(&[]).to_vec()
                })
                .unwrap_or_default();
            listing.push(ListingLine {
                file: l.file,
                line: l.line,
                segment: e.map(|x| layout[x.0] as u16),
                offset: e.map(|x| x.1 as u16),
                bytes,
                text: l.text.clone(),
                macro_depth: l.macro_depth,
                origin: l.origin(),
            });
        }

        let mut symbols: Vec<Symbol> = self
            .syms
            .iter()
            .filter(|(n, _)| !n.starts_with('@'))
            .map(|(n, v)| {
                let kind = self.sym_kinds.get(n).copied().unwrap_or(SymbolKind::Constant);
                let (segment, value, size) = match v {
                    SymVal::Const(c) => (None, *c, None),
                    SymVal::Label { seg, off, size, .. } => {
                        (Some(layout[*seg] as u16), *off, size.or_else(|| self.data_sizes.get(n).copied()))
                    }
                    SymVal::Segment(s) => (None, layout[*s], None),
                };
                Symbol { name: n.clone(), kind, segment, value, size }
            })
            .collect();
        symbols.sort_by(|a, b| a.name.cmp(&b.name));

        let segments = if self.flat {
            vec![SegmentInfo { name: String::new(), paragraph: 0, size: image.len() as u32 }]
        } else {
            self.segs
                .iter()
                .enumerate()
                .map(|(i, s)| SegmentInfo { name: s.name.clone(), paragraph: layout[i] as u16, size: s.max as u32 })
                .collect()
        };
        self.emu.start.dedup();
        Ok(Program {
            format: self.format,
            image,
            origin: origin as u16,
            relocs,
            entry,
            stack,
            lines,
            listing,
            symbols,
            segments,
            files,
            emu: self.emu,
            dialect: if self.nasm { Dialect::Nasm } else { Dialect::Masm },
        })
    }
}

/// NASM local labels: `.next` belongs to the last label without a dot, so it becomes `START.NEXT`
/// both where it is defined and where it is used.
fn rename_local_labels(stmts: &mut [StmtInfo]) {
    fn expr(e: &mut Expr, base: &str) {
        match e {
            Expr::Sym(n) if n.starts_with('.') && !n.starts_with("..") => *n = format!("{base}{n}"),
            Expr::Mem(x) | Expr::Un(_, x) | Expr::Ptr(_, x) | Expr::Dist(_, x) | Expr::SegOv(_, x) => expr(x, base),
            Expr::Bin(_, a, b) => {
                expr(a, base);
                expr(b, base);
            }
            _ => {}
        }
    }
    fn item(it: &mut DataItem, base: &str) {
        match it {
            DataItem::Expr(e) => expr(e, base),
            DataItem::Dup(n, inner) => {
                expr(n, base);
                inner.iter_mut().for_each(|i| item(i, base));
            }
            _ => {}
        }
    }
    fn kind(k: &mut StmtKind, base: &str) {
        match k {
            StmtKind::Instr { ops, .. } => {
                for op in ops {
                    match op {
                        Operand::Expr(e) => expr(e, base),
                        Operand::Far(a, b) => {
                            expr(a, base);
                            expr(b, base);
                        }
                    }
                }
            }
            StmtKind::Data { items, .. } => items.iter_mut().for_each(|i| item(i, base)),
            StmtKind::Equ(_, e) | StmtKind::Assign(_, e) | StmtKind::Org(e) | StmtKind::Align(e) => expr(e, base),
            StmtKind::Times(n, inner) => {
                expr(n, base);
                kind(inner, base);
            }
            _ => {}
        }
    }
    let mut base = String::new();
    for s in stmts {
        if let Some(l) = &mut s.stmt.label {
            if l.starts_with('.') && !l.starts_with("..") {
                *l = format!("{base}{l}");
            } else {
                base = l.clone();
            }
        }
        kind(&mut s.stmt.kind, &base);
    }
}

fn equ_value(v: &Val) -> SymVal {
    match (v.label_seg, v.seg_base) {
        (Some(seg), _) if seg != usize::MAX => {
            SymVal::Label { seg, off: v.num, size: v.size, far: v.far_label, count: 1 }
        }
        (_, Some(s)) if s != usize::MAX => SymVal::Segment(s),
        _ => SymVal::Const(v.num),
    }
}

/// `#MEM=nnnnn,bytes-ssss:oooo,bytes#` (all hex; spaces inside byte strings are ignored).
fn parse_mem_directive(v: &str) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in v.split('-') {
        let Some((addr, bytes)) = entry.split_once(',') else { continue };
        let hex = |t: &str| u32::from_str_radix(t.trim().trim_end_matches(['h', 'H']), 16).ok();
        let addr = match addr.split_once(':') {
            Some((s, o)) => match (hex(s), hex(o)) {
                (Some(s), Some(o)) => ((s << 4) + o) & 0xFFFFF,
                _ => continue,
            },
            None => match hex(addr) {
                Some(a) => a & 0xFFFFF,
                None => continue,
            },
        };
        let digits: String = bytes.chars().filter(|c| !c.is_whitespace()).collect();
        let data: Vec<u8> =
            (0..digits.len() / 2).filter_map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).ok()).collect();
        out.push((addr, data));
    }
    out
}
