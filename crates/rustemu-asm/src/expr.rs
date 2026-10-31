//! Operand expressions: parsing and evaluation.

use crate::lexer::{Tok, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reg {
    /// 8-bit register, encoding 0..7 (AL CL DL BL AH CH DH BH).
    R8(u8),
    /// 16-bit register, encoding 0..7 (AX CX DX BX SP BP SI DI).
    R16(u8),
    /// Segment register, encoding 0..3 (ES CS SS DS).
    Seg(u8),
}

pub fn parse_reg(name: &str) -> Option<Reg> {
    const R8: [&str; 8] = ["AL", "CL", "DL", "BL", "AH", "CH", "DH", "BH"];
    const R16: [&str; 8] = ["AX", "CX", "DX", "BX", "SP", "BP", "SI", "DI"];
    const SR: [&str; 4] = ["ES", "CS", "SS", "DS"];
    let u = name.to_ascii_uppercase();
    if let Some(i) = R8.iter().position(|r| *r == u) {
        return Some(Reg::R8(i as u8));
    }
    if let Some(i) = R16.iter().position(|r| *r == u) {
        return Some(Reg::R16(i as u8));
    }
    SR.iter().position(|r| *r == u).map(|i| Reg::Seg(i as u8))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dist {
    Short,
    Near,
    Far,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Offset,
    Seg,
    Type,
    Length,
    Size,
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(i64),
    Str(Vec<u8>),
    Sym(String),
    Here,
    /// NASM `$$`: start of the current section.
    SectionStart,
    Reg(Reg),
    Mem(Box<Expr>),
    Un(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// `BYTE PTR x` (size 1/2/4), `b.x`, `w.x`.
    Ptr(u8, Box<Expr>),
    /// `SHORT x`, `NEAR PTR x`, `FAR PTR x`.
    Dist(Dist, Box<Expr>),
    /// `ES:x`
    SegOv(u8, Box<Expr>),
}

/// A comparison (-1 / 0) as NASM's 1 / 0.
fn one_or_zero(e: Expr) -> Expr {
    Expr::Bin(BinOp::And, Box::new(e), Box::new(Expr::Num(1)))
}

pub struct Parser<'a> {
    pub toks: &'a [Token],
    pub pos: usize,
    /// NASM syntax: `byte [x]` without PTR, `$$`, C operators, little-endian character constants.
    pub nasm: bool,
}

type PResult<T> = Result<T, (usize, String)>;

impl<'a> Parser<'a> {
    pub fn new(toks: &'a [Token]) -> Self {
        Parser { toks, pos: 0, nasm: false }
    }

    pub fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    pub fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.tok)
    }

    pub fn col(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.col).or_else(|| self.toks.last().map(|t| t.col + 1)).unwrap_or(0)
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    pub fn advance(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|t| t.tok.clone());
        self.pos += 1;
        t
    }

    pub fn eat_p(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::P(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Consumes `cc` written as two adjacent characters (`<<`, `>>`).
    fn eat_pair(&mut self, c: char) -> bool {
        let adjacent = match (self.toks.get(self.pos), self.toks.get(self.pos + 1)) {
            (Some(a), Some(b)) => a.tok == Tok::P(c) && b.tok == Tok::P(c) && b.col == a.col + 1,
            _ => false,
        };
        if adjacent {
            self.pos += 2;
        }
        adjacent
    }

    /// Whether the token after the current one can start an operand (otherwise an operator
    /// keyword such as SIZE or LOW is just a symbol name).
    fn operand_follows(&self) -> bool {
        !matches!(self.peek_at(1), None | Some(Tok::P(',' | ')' | ']' | '+' | '-' | '*' | '/' | ':' | '=')))
    }

    pub fn eat_kw(&mut self, kw: &str) -> bool {
        if self.peek().is_some_and(|t| t.is_ident(kw)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err((self.col(), msg.into()))
    }

    pub fn expr(&mut self) -> PResult<Expr> {
        if self.nasm {
            return self.nasm_cond();
        }
        self.or_expr()
    }

    /// NASM: `c ? a : b`, the lowest precedence.
    fn nasm_cond(&mut self) -> PResult<Expr> {
        let c = self.nasm_logic(0)?;
        if !self.eat_p('?') {
            return Ok(c);
        }
        let a = self.nasm_cond()?;
        if !self.eat_p(':') {
            return self.err("':' of ?: expected");
        }
        let b = self.nasm_cond()?;
        // a where the mask (c != 0: all ones) is set, b elsewhere.
        let mask = Expr::Bin(BinOp::Ne, Box::new(c), Box::new(Expr::Num(0)));
        Ok(Expr::Bin(
            BinOp::Or,
            Box::new(Expr::Bin(BinOp::And, Box::new(a), Box::new(mask.clone()))),
            Box::new(Expr::Bin(BinOp::And, Box::new(b), Box::new(Expr::Un(UnOp::Not, Box::new(mask))))),
        ))
    }

    /// NASM's logical `||`, `^^`, `&&` (in this order of precedence): 1 or 0.
    fn nasm_logic(&mut self, level: usize) -> PResult<Expr> {
        if level == 3 {
            return self.nasm_cmp();
        }
        let (c, op) = [('|', BinOp::Or), ('^', BinOp::Xor), ('&', BinOp::And)][level];
        let truth = |e: Expr| Expr::Bin(BinOp::Ne, Box::new(e), Box::new(Expr::Num(0)));
        let mut l = self.nasm_logic(level + 1)?;
        while self.eat_two(c, c) {
            let r = self.nasm_logic(level + 1)?;
            l = one_or_zero(Expr::Bin(op, Box::new(truth(l)), Box::new(truth(r))));
        }
        Ok(l)
    }

    /// NASM comparisons: below the bitwise operators, 1 when true (MASM: -1).
    fn nasm_cmp(&mut self) -> PResult<Expr> {
        let mut l = self.nasm_bits(0)?;
        loop {
            let op = if self.eat_two('=', '=') {
                BinOp::Eq
            } else if self.eat_two('!', '=') || self.eat_two('<', '>') {
                BinOp::Ne
            } else if self.eat_two('<', '=') {
                BinOp::Le
            } else if self.eat_two('>', '=') {
                BinOp::Ge
            } else if self.eat_p('<') {
                BinOp::Lt
            } else if self.eat_p('>') {
                BinOp::Gt
            } else if self.eat_p('=') {
                BinOp::Eq
            } else {
                return Ok(l);
            };
            let r = self.nasm_bits(0)?;
            l = one_or_zero(Expr::Bin(op, Box::new(l), Box::new(r)));
        }
    }

    /// NASM's bitwise `|`, `^`, `&` (in this order of precedence), above the shifts.
    fn nasm_bits(&mut self, level: usize) -> PResult<Expr> {
        if level == 3 {
            return self.shift_expr();
        }
        let (c, op) = [('|', BinOp::Or), ('^', BinOp::Xor), ('&', BinOp::And)][level];
        let mut l = self.nasm_bits(level + 1)?;
        loop {
            // A single operator character: `||` is the logical one.
            let single = matches!(self.toks.get(self.pos), Some(t) if t.tok == Tok::P(c))
                && !matches!((self.toks.get(self.pos), self.toks.get(self.pos + 1)),
                    (Some(x), Some(y)) if y.tok == Tok::P(c) && y.col == x.col + 1);
            if !single {
                return Ok(l);
            }
            self.pos += 1;
            let r = self.nasm_bits(level + 1)?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    /// Consumes two adjacent punctuation characters `ab` (`==`, `!=`, `<=`, ...).
    fn eat_two(&mut self, a: char, b: char) -> bool {
        let adjacent = match (self.toks.get(self.pos), self.toks.get(self.pos + 1)) {
            (Some(x), Some(y)) => x.tok == Tok::P(a) && y.tok == Tok::P(b) && y.col == x.col + 1,
            _ => false,
        };
        if adjacent {
            self.pos += 2;
        }
        adjacent
    }

    fn or_expr(&mut self) -> PResult<Expr> {
        let mut l = self.xor_expr()?;
        while self.eat_two('|', '|') || self.eat_kw("OR") || self.eat_p('|') {
            let r = self.xor_expr()?;
            l = Expr::Bin(BinOp::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn xor_expr(&mut self) -> PResult<Expr> {
        let mut l = self.and_expr()?;
        while self.eat_kw("XOR") || self.eat_p('^') {
            let r = self.and_expr()?;
            l = Expr::Bin(BinOp::Xor, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and_expr(&mut self) -> PResult<Expr> {
        let mut l = self.not_expr()?;
        while self.eat_two('&', '&') || self.eat_kw("AND") || self.eat_p('&') {
            let r = self.not_expr()?;
            l = Expr::Bin(BinOp::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn not_expr(&mut self) -> PResult<Expr> {
        if self.eat_kw("NOT") {
            let e = self.not_expr()?;
            return Ok(Expr::Un(UnOp::Not, Box::new(e)));
        }
        self.cmp_expr()
    }

    fn cmp_expr(&mut self) -> PResult<Expr> {
        let l = self.shift_expr()?;
        if self.nasm {
            // NASM: == != <> <= >= < > =
            let op = if self.eat_two('=', '=') {
                Some(BinOp::Eq)
            } else if self.eat_two('!', '=') || self.eat_two('<', '>') {
                Some(BinOp::Ne)
            } else if self.eat_two('<', '=') {
                Some(BinOp::Le)
            } else if self.eat_two('>', '=') {
                Some(BinOp::Ge)
            } else if self.eat_p('<') {
                Some(BinOp::Lt)
            } else if self.eat_p('>') {
                Some(BinOp::Gt)
            } else if self.eat_p('=') {
                Some(BinOp::Eq)
            } else {
                None
            };
            if let Some(op) = op {
                let r = self.shift_expr()?;
                return Ok(Expr::Bin(op, Box::new(l), Box::new(r)));
            }
        }
        let op = match self.peek() {
            Some(t) if t.is_ident("EQ") => BinOp::Eq,
            Some(t) if t.is_ident("NE") => BinOp::Ne,
            Some(t) if t.is_ident("LT") => BinOp::Lt,
            Some(t) if t.is_ident("LE") => BinOp::Le,
            Some(t) if t.is_ident("GT") => BinOp::Gt,
            Some(t) if t.is_ident("GE") => BinOp::Ge,
            _ => return Ok(l),
        };
        self.pos += 1;
        let r = self.shift_expr()?;
        Ok(Expr::Bin(op, Box::new(l), Box::new(r)))
    }

    fn shift_expr(&mut self) -> PResult<Expr> {
        let mut l = self.add_expr()?;
        loop {
            let op = if self.eat_kw("SHL") || self.eat_pair('<') {
                BinOp::Shl
            } else if self.eat_kw("SHR") || self.eat_pair('>') {
                BinOp::Shr
            } else {
                return Ok(l);
            };
            let r = self.add_expr()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn add_expr(&mut self) -> PResult<Expr> {
        let mut l = self.mul_expr()?;
        loop {
            let op = if self.eat_p('+') {
                BinOp::Add
            } else if self.eat_p('-') {
                BinOp::Sub
            } else {
                return Ok(l);
            };
            let r = self.mul_expr()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn mul_expr(&mut self) -> PResult<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat_p('*') {
                BinOp::Mul
            } else if self.eat_p('/') {
                BinOp::Div
            } else if self.eat_p('%') || self.eat_kw("MOD") {
                BinOp::Mod
            } else {
                return Ok(l);
            };
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn unary(&mut self) -> PResult<Expr> {
        if self.eat_p('~') {
            let e = self.unary()?;
            return Ok(Expr::Un(UnOp::Not, Box::new(e)));
        }
        if self.nasm && self.eat_p('!') {
            // Logical not: 1 when the operand is zero.
            let e = self.unary()?;
            return Ok(Expr::Bin(
                BinOp::And,
                Box::new(Expr::Bin(BinOp::Eq, Box::new(e), Box::new(Expr::Num(0)))),
                Box::new(Expr::Num(1)),
            ));
        }
        if self.eat_p('-') {
            let e = self.unary()?;
            return Ok(Expr::Un(UnOp::Neg, Box::new(e)));
        }
        if self.eat_p('+') {
            return self.unary();
        }
        const UNOPS: [(&str, UnOp); 7] = [
            ("OFFSET", UnOp::Offset),
            ("SEG", UnOp::Seg),
            ("TYPE", UnOp::Type),
            ("LENGTH", UnOp::Length),
            ("SIZE", UnOp::Size),
            ("LOW", UnOp::Low),
            ("HIGH", UnOp::High),
        ];
        for (kw, op) in UNOPS {
            if self.peek().is_some_and(|t| t.is_ident(kw)) && self.operand_follows() && self.eat_kw(kw) {
                let e = self.unary()?;
                return Ok(Expr::Un(op, Box::new(e)));
            }
        }
        // Size / distance qualifiers.
        if let Some(Tok::Ident(id)) = self.peek() {
            let u = id.to_ascii_uppercase();
            let size = match u.as_str() {
                "BYTE" => Some(1),
                "WORD" => Some(2),
                "DWORD" => Some(4),
                _ => None,
            };
            if let Some(sz) = size
                && self.peek_at(1).is_some_and(|t| t.is_ident("PTR"))
            {
                self.pos += 2;
                let e = self.add_expr()?;
                return Ok(Expr::Ptr(sz, Box::new(e)));
            }
            // NASM: `byte [x]`, `word 5`, `dword [bx]` (and `qword` / `tword`).
            let nasm_size = match u.as_str() {
                "QWORD" => Some(8),
                "TWORD" => Some(10),
                _ => size,
            };
            if self.nasm
                && let Some(sz) = nasm_size
                && self.peek_at(1).is_some()
                && !matches!(self.peek_at(1), Some(Tok::P(',' | ')' | ']')))
            {
                self.pos += 1;
                self.eat_kw("STRICT");
                let e = self.unary()?;
                return Ok(Expr::Ptr(sz, Box::new(e)));
            }
            if u == "B." || u == "W." {
                self.pos += 1;
                let e = self.add_expr()?;
                return Ok(Expr::Ptr(if u == "B." { 1 } else { 2 }, Box::new(e)));
            }
            if u == "SHORT" {
                self.pos += 1;
                let e = self.add_expr()?;
                return Ok(Expr::Dist(Dist::Short, Box::new(e)));
            }
            if u == "NEAR" || u == "FAR" {
                self.pos += 1;
                self.eat_kw("PTR");
                let e = self.add_expr()?;
                let d = if u == "NEAR" { Dist::Near } else { Dist::Far };
                return Ok(Expr::Dist(d, Box::new(e)));
            }
        }
        self.postfix()
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        // var[bx], [bx][si], 5[bx]
        while self.peek() == Some(&Tok::P('[')) {
            let m = self.primary()?;
            e = Expr::Bin(BinOp::Add, Box::new(e), Box::new(m));
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        match self.advance() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            // NASM character constants are little-endian: 'ab' = 6261h.
            Some(Tok::Str(s)) if self.nasm && !s.is_empty() && s.len() <= 8 => {
                Ok(Expr::Num(s.iter().rev().fold(0i64, |a, &c| (a << 8) | c as i64)))
            }
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::P('$')) => {
                let col = self.toks[self.pos - 1].col;
                if self.toks.get(self.pos).is_some_and(|t| t.tok == Tok::P('$') && t.col == col + 1) {
                    self.pos += 1;
                    return Ok(Expr::SectionStart);
                }
                Ok(Expr::Here)
            }
            Some(Tok::P('?')) => Ok(Expr::Num(0)),
            Some(Tok::P('(')) => {
                let e = self.expr()?;
                if !self.eat_p(')') {
                    return self.err("expected ')'");
                }
                Ok(e)
            }
            Some(Tok::P('[')) => {
                let e = self.expr()?;
                if !self.eat_p(']') {
                    return self.err("expected ']'");
                }
                Ok(Expr::Mem(Box::new(e)))
            }
            Some(Tok::Ident(id)) => {
                if let Some(r) = parse_reg(&id) {
                    if let Reg::Seg(s) = r
                        && self.eat_p(':')
                    {
                        let e = self.add_expr()?;
                        return Ok(Expr::SegOv(s, Box::new(e)));
                    }
                    return Ok(Expr::Reg(r));
                }
                Ok(Expr::Sym(id.to_ascii_uppercase()))
            }
            Some(t) => {
                self.pos -= 1;
                self.err(format!("unexpected {}", describe(&t)))
            }
            None => self.err("expression expected"),
        }
    }
}

pub fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("'{s}'"),
        Tok::Num(n) => format!("number {n}"),
        Tok::Str(_) => "string".into(),
        Tok::P(c) => format!("'{c}'"),
    }
}

// ---- evaluation ------------------------------------------------------------

pub type SegId = usize;

/// What a symbol resolves to during evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum SymVal {
    Const(i64),
    /// Code label or variable inside segment `seg`.
    Label {
        seg: SegId,
        off: i64,
        /// Data size for variables (1/2/4); None for code labels.
        size: Option<u8>,
        far: bool,
        /// Element count (for LENGTH).
        count: i64,
    },
    /// Segment name: its value is the segment's paragraph address.
    Segment(SegId),
}

pub trait Env {
    fn lookup(&self, name: &str) -> Option<SymVal>;
    /// Whether the symbol is known to exist (it may have no value yet in early passes).
    fn is_forward(&self, name: &str) -> Option<ForwardKind>;
    /// Current location counter.
    fn here(&self) -> (SegId, i64);
    /// Paragraph number of a segment relative to the program start (flat formats return 0).
    fn seg_paragraph(&self, seg: SegId) -> i64;
    /// Start of the current section (NASM `$$`).
    fn section_start(&self) -> (SegId, i64) {
        (self.here().0, 0)
    }
}

/// Information about symbols that are declared but not yet resolved in the current pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ForwardKind {
    Label { size: Option<u8>, far: bool },
    Const,
    Segment,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Val {
    pub num: i64,
    /// Base/index registers (16-bit encodings).
    pub regs: Vec<u8>,
    /// The expression denotes a memory operand.
    pub mem: bool,
    /// Operand size from a variable type or PTR.
    pub size: Option<u8>,
    pub seg_ov: Option<u8>,
    pub dist: Option<Dist>,
    /// The value is an address inside this segment (label offset).
    pub label_seg: Option<SegId>,
    /// The value is the paragraph of a segment (needs a relocation in EXE output).
    pub seg_base: Option<SegId>,
    /// Paragraph of `label_seg` (for far jumps/calls and DD pointers).
    pub label_para: i64,
    /// The referenced label is declared FAR.
    pub far_label: bool,
    /// Contains a symbol without a value yet (early pass).
    pub unknown: bool,
}

impl Val {
    fn num(n: i64) -> Self {
        Val { num: n, ..Default::default() }
    }

    /// No registers, memory, segment or relocatable parts.
    pub fn is_plain(&self) -> bool {
        self.regs.is_empty() && !self.mem && self.label_seg.is_none() && self.seg_base.is_none()
    }
}

pub fn eval(e: &Expr, env: &dyn Env) -> Result<Val, String> {
    Ok(match e {
        Expr::Num(n) => Val::num(*n),
        Expr::Str(s) => {
            if s.len() > 2 {
                return Err("string constant too long".into());
            }
            Val::num(s.iter().fold(0i64, |a, &c| (a << 8) | c as i64))
        }
        Expr::Here => {
            let (seg, off) = env.here();
            Val { num: off, label_seg: Some(seg), label_para: env.seg_paragraph(seg), ..Default::default() }
        }
        Expr::SectionStart => {
            let (seg, off) = env.section_start();
            Val { num: off, label_seg: Some(seg), label_para: env.seg_paragraph(seg), ..Default::default() }
        }
        Expr::Sym(name) => match env.lookup(name) {
            Some(SymVal::Const(n)) => Val::num(n),
            Some(SymVal::Label { seg, off, size, far, .. }) => Val {
                num: off,
                mem: size.is_some(),
                size,
                label_seg: Some(seg),
                label_para: env.seg_paragraph(seg),
                far_label: far,
                ..Default::default()
            },
            Some(SymVal::Segment(s)) => Val { num: env.seg_paragraph(s), seg_base: Some(s), ..Default::default() },
            None => match env.is_forward(name) {
                Some(ForwardKind::Label { size, far }) => Val {
                    mem: size.is_some(),
                    size,
                    far_label: far,
                    unknown: true,
                    label_seg: Some(usize::MAX),
                    ..Default::default()
                },
                Some(ForwardKind::Segment) => Val { seg_base: Some(usize::MAX), unknown: true, ..Default::default() },
                Some(ForwardKind::Const) => Val { unknown: true, ..Default::default() },
                None => return Err(format!("undefined symbol '{name}'")),
            },
        },
        Expr::Reg(Reg::R16(r)) if matches!(r, 3 | 5 | 6 | 7) => Val { regs: vec![*r], ..Default::default() },
        Expr::Reg(_) => return Err("invalid register in address expression".into()),
        Expr::Mem(inner) => {
            let mut v = eval(inner, env)?;
            v.mem = true;
            v
        }
        Expr::Ptr(sz, inner) => {
            let mut v = eval(inner, env)?;
            v.size = Some(*sz);
            if *sz == 4 {
                v.dist.get_or_insert(Dist::Far);
            }
            v
        }
        Expr::Dist(d, inner) => {
            let mut v = eval(inner, env)?;
            v.dist = Some(*d);
            v
        }
        Expr::SegOv(s, inner) => {
            let mut v = eval(inner, env)?;
            v.seg_ov = Some(*s);
            v.mem = true;
            v
        }
        Expr::Un(op, inner) => {
            let v = eval(inner, env)?;
            match op {
                UnOp::Offset => Val { num: v.num, unknown: v.unknown, ..Default::default() },
                UnOp::Seg => {
                    let seg = v.label_seg.ok_or("SEG requires a label or variable")?;
                    if seg == usize::MAX {
                        Val { seg_base: Some(seg), unknown: true, ..Default::default() }
                    } else {
                        Val { num: env.seg_paragraph(seg), seg_base: Some(seg), ..Default::default() }
                    }
                }
                UnOp::Type => {
                    let t = match v.size {
                        Some(s) => s as i64,
                        None if v.far_label => 0xFFFE,
                        None if v.label_seg.is_some() => 0xFFFF,
                        None => 0,
                    };
                    Val::num(t)
                }
                UnOp::Length | UnOp::Size => {
                    let count = match &**inner {
                        Expr::Sym(n) => match env.lookup(n) {
                            Some(SymVal::Label { count, .. }) => count,
                            _ => 1,
                        },
                        _ => 1,
                    };
                    let n = if *op == UnOp::Size { count * v.size.unwrap_or(1) as i64 } else { count };
                    Val { num: n, unknown: v.unknown, ..Default::default() }
                }
                _ => {
                    if !v.is_plain() && !v.unknown {
                        return Err("operator requires a constant".into());
                    }
                    let n = match op {
                        UnOp::Neg => -v.num,
                        UnOp::Not => !v.num,
                        UnOp::Low => v.num & 0xFF,
                        _ => (v.num >> 8) & 0xFF,
                    };
                    Val { num: n, unknown: v.unknown, ..Default::default() }
                }
            }
        }
        Expr::Bin(op, a, b) => {
            let x = eval(a, env)?;
            let y = eval(b, env)?;
            bin(*op, x, y)?
        }
    })
}

fn bin(op: BinOp, x: Val, y: Val) -> Result<Val, String> {
    let unknown = x.unknown || y.unknown;
    match op {
        BinOp::Add => {
            if x.label_seg.is_some() && y.label_seg.is_some() {
                return Err("cannot add two addresses".into());
            }
            let mut regs = x.regs;
            regs.extend(y.regs);
            Ok(Val {
                num: x.num.wrapping_add(y.num),
                regs,
                mem: x.mem || y.mem,
                size: x.size.or(y.size),
                seg_ov: x.seg_ov.or(y.seg_ov),
                dist: x.dist.or(y.dist),
                label_seg: x.label_seg.or(y.label_seg),
                seg_base: x.seg_base.or(y.seg_base),
                label_para: x.label_para + y.label_para,
                far_label: x.far_label || y.far_label,
                unknown,
            })
        }
        BinOp::Sub => {
            if !y.regs.is_empty() {
                return Err("cannot subtract a register".into());
            }
            let label_seg = match (x.label_seg, y.label_seg) {
                (Some(a), Some(b)) if a == b || unknown => None,
                (Some(_), Some(_)) => return Err("cannot subtract labels from different segments".into()),
                (None, Some(_)) => return Err("cannot subtract an address from a constant".into()),
                (l, None) => l,
            };
            let both_labels = x.label_seg.is_some() && y.label_seg.is_some();
            Ok(Val {
                num: x.num.wrapping_sub(y.num),
                regs: x.regs,
                mem: if both_labels { false } else { x.mem },
                size: if both_labels { None } else { x.size },
                seg_ov: x.seg_ov,
                dist: x.dist,
                label_seg,
                seg_base: x.seg_base,
                label_para: if label_seg.is_some() { x.label_para } else { 0 },
                far_label: x.far_label,
                unknown,
            })
        }
        _ => {
            if !unknown && (!x.is_plain() || !y.is_plain()) {
                return Err("operator requires constant operands".into());
            }
            let (a, b) = (x.num, y.num);
            let t = |c: bool| if c { -1 } else { 0 };
            let n = match op {
                BinOp::Mul => a.wrapping_mul(b),
                BinOp::Div | BinOp::Mod if b == 0 => {
                    if unknown {
                        0
                    } else {
                        return Err("division by zero".into());
                    }
                }
                BinOp::Div => a / b,
                BinOp::Mod => a % b,
                BinOp::Shl => a.wrapping_shl(b as u32),
                BinOp::Shr => a.wrapping_shr(b as u32),
                BinOp::And => a & b,
                BinOp::Or => a | b,
                BinOp::Xor => a ^ b,
                BinOp::Eq => t(a == b),
                BinOp::Ne => t(a != b),
                BinOp::Lt => t(a < b),
                BinOp::Le => t(a <= b),
                BinOp::Gt => t(a > b),
                BinOp::Ge => t(a >= b),
                BinOp::Add | BinOp::Sub => unreachable!(),
            };
            Ok(Val { num: n, unknown, ..Default::default() })
        }
    }
}
