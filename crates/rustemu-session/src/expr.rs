//! Debugger expressions: watches, breakpoint conditions and "stop on condition".
//!
//! Operands: numbers in assembler notation (`10`, `0Ah`, `0x0A`, `1010b`, `12o`, `'c'`), registers
//! (`AX`, `AL`, `CS`, `IP`, `FLAGS`), flags (`CF`, `ZF`, `SF`, `OF`, `PF`, `AF`, `IF`, `DF`, `TF`),
//! program symbols (a variable gives its value from memory, a label its offset, `OFFSET name` the
//! offset of a variable), memory `[expr]`, `seg:[expr]`, `BYTE [..]` / `WORD [..]` / `b.[..]` / `w.[..]`,
//! indexed variables `table[si]`.
//! Operators (C precedence): `! ~ -` (unary), `* / %`, `+ -`, `<< >>`, `< <= > >=`, `== !=` (`=` too),
//! `&`, `^`, `|`, `&&`, `||`, and the assembler words `MOD SHL SHR AND OR XOR NOT`.

use rustemu_core::cpu::{self, flags};
use rustemu_core::{Machine, linear};

/// A compiled expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    node: Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(i64),
    Reg16(usize),
    Reg8(u8),
    Sreg(usize),
    Ip,
    Flags,
    Flag(u16),
    /// Memory at segment:offset; `seg` = None means DS.
    Mem {
        size: u8,
        seg: Option<Box<Node>>,
        off: Box<Node>,
    },
    /// A variable at a fixed linear address (for COM/EXE images).
    Var {
        addr: u32,
        size: u8,
    },
    /// `var[index]`: the variable's segment, its offset + index.
    Indexed {
        seg: u16,
        off: u16,
        size: u8,
        index: Box<Node>,
    },
    /// A NASM data label (evaluates to its offset).
    Addr {
        seg: u16,
        off: u16,
        size: u8,
    },
    Unary(char, Box<Node>),
    Binary(&'static str, Box<Node>, Box<Node>),
}

/// What a name refers to (resolved by the session from the program's symbols).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolRef {
    /// A variable: segment and offset where it lives, element size in bytes.
    Variable { seg: u16, off: u16, size: u8 },
    /// A label or constant: its value.
    Value(i64),
    /// A NASM data label: its value is the address; `[name]` reads `size` bytes there.
    Address { seg: u16, off: u16, size: u8 },
}

impl Expr {
    /// Parses `text`, resolving names with `lookup`.
    pub fn parse(text: &str, lookup: &dyn Fn(&str) -> Option<SymbolRef>) -> Result<Expr, String> {
        let tokens = lex(text)?;
        if tokens.is_empty() {
            return Err("empty expression".into());
        }
        let mut p = Parser { t: &tokens, pos: 0, lookup };
        let node = p.binary(0)?;
        if p.pos < tokens.len() {
            return Err(format!("unexpected '{}'", tokens[p.pos].text()));
        }
        Ok(Expr { node })
    }

    pub fn eval(&self, m: &Machine) -> i64 {
        eval(&self.node, m)
    }

    /// Natural width of the value in bits (8 for byte registers and variables, 16 for words, ...).
    pub fn bits(&self) -> u32 {
        match &self.node {
            Node::Reg8(_) | Node::Flag(_) => 8,
            Node::Reg16(_) | Node::Sreg(_) | Node::Ip | Node::Flags => 16,
            Node::Mem { size, .. } | Node::Var { size, .. } | Node::Indexed { size, .. } => *size as u32 * 8,
            _ => 16,
        }
    }

    /// Linear address of the memory the expression reads directly (for `[..]` and variables).
    pub fn address(&self, m: &Machine) -> Option<u32> {
        match &self.node {
            Node::Mem { seg, off, .. } => Some(mem_addr(m, seg.as_deref(), off)),
            Node::Var { addr, .. } => Some(*addr),
            Node::Indexed { seg, off, index, .. } => Some(linear(*seg, off.wrapping_add(eval(index, m) as u16))),
            _ => None,
        }
    }
}

fn mem_addr(m: &Machine, seg: Option<&Node>, off: &Node) -> u32 {
    let s = seg.map_or(m.cpu.sregs[cpu::DS], |s| eval(s, m) as u16);
    linear(s, eval(off, m) as u16)
}

fn read(m: &Machine, addr: u32, size: u8) -> i64 {
    let mut v: i64 = 0;
    for i in (0..size as u32).rev() {
        v = (v << 8) | m.sys.mem.read8((addr + i) & 0xFFFFF) as i64;
    }
    v
}

fn eval(n: &Node, m: &Machine) -> i64 {
    let c = &m.cpu;
    match n {
        Node::Num(v) => *v,
        Node::Reg16(r) => c.regs[*r] as i64,
        Node::Reg8(r) => c.reg8(*r) as i64,
        Node::Sreg(r) => c.sregs[*r] as i64,
        Node::Ip => c.ip as i64,
        Node::Flags => c.flags as i64,
        Node::Flag(f) => (c.flags & f != 0) as i64,
        Node::Mem { size, seg, off } => read(m, mem_addr(m, seg.as_deref(), off), *size),
        Node::Var { addr, size } => read(m, *addr, *size),
        Node::Indexed { seg, off, size, index } => {
            read(m, linear(*seg, off.wrapping_add(eval(index, m) as u16)), *size)
        }
        Node::Addr { off, .. } => *off as i64,
        Node::Unary(op, a) => {
            let a = eval(a, m);
            match op {
                '-' => a.wrapping_neg(),
                '~' => !a,
                _ => (a == 0) as i64,
            }
        }
        Node::Binary(op, a, b) => {
            let a = eval(a, m);
            if *op == "&&" {
                return (a != 0 && eval(b, m) != 0) as i64;
            }
            if *op == "||" {
                return (a != 0 || eval(b, m) != 0) as i64;
            }
            let b = eval(b, m);
            match *op {
                "*" => a.wrapping_mul(b),
                "/" => a.checked_div(b).unwrap_or(0),
                "%" => a.checked_rem(b).unwrap_or(0),
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "<<" => a.wrapping_shl(b as u32),
                ">>" => a.wrapping_shr(b as u32),
                "<" => (a < b) as i64,
                "<=" => (a <= b) as i64,
                ">" => (a > b) as i64,
                ">=" => (a >= b) as i64,
                "==" => (a == b) as i64,
                "!=" => (a != b) as i64,
                "&" => a & b,
                "^" => a ^ b,
                _ => a | b,
            }
        }
    }
}

// ---- lexer --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(i64),
    Name(String),
    Op(&'static str),
}

impl Tok {
    fn text(&self) -> String {
        match self {
            Tok::Num(v) => v.to_string(),
            Tok::Name(s) => s.clone(),
            Tok::Op(o) => o.to_string(),
        }
    }
}

const OPS: [&str; 26] = [
    "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "=", "&", "^", "|", "~", "!",
    "(", ")", "[", "]", ":",
];

fn number(word: &str) -> Option<i64> {
    let w = word.to_ascii_lowercase().replace('_', "");
    let (digits, radix) = if let Some(h) = w.strip_prefix("0x") {
        (h, 16)
    } else if let Some(h) = w.strip_suffix('h') {
        (h, 16)
    } else if let Some(b) = w.strip_suffix('b').filter(|b| b.chars().all(|c| c == '0' || c == '1')) {
        (b, 2)
    } else if let Some(o) = w.strip_suffix('o').or_else(|| w.strip_suffix('q')) {
        (o, 8)
    } else if let Some(d) = w.strip_suffix('d') {
        (d, 10)
    } else {
        (w.as_str(), 10)
    };
    i64::from_str_radix(digits, radix).ok()
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' || c == '"' {
            // Character constant: 'a' or 'ab' (word, first character in the low byte like DW 'ab').
            let end = chars[i + 1..].iter().position(|&x| x == c).ok_or("unterminated character constant")? + i + 1;
            let v = chars[i + 1..end].iter().rev().fold(0i64, |v, &ch| (v << 8) | (ch as i64 & 0xFF));
            out.push(Tok::Num(v));
            i = end + 1;
        } else if c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '?' | '.') {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '@' | '?' | '.')) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if c.is_ascii_digit() {
                out.push(Tok::Num(number(&word).ok_or_else(|| format!("bad number '{word}'"))?));
            } else {
                out.push(Tok::Name(word.to_ascii_uppercase()));
            }
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let op = OPS.iter().find(|o| rest.starts_with(**o)).ok_or_else(|| format!("unexpected '{c}'"))?;
            out.push(Tok::Op(op));
            i += op.chars().count();
        }
    }
    Ok(out)
}

// ---- parser -------------------------------------------------------------------------------

struct Parser<'a> {
    t: &'a [Tok],
    pos: usize,
    lookup: &'a dyn Fn(&str) -> Option<SymbolRef>,
}

/// Binary operators from the lowest precedence level up.
const LEVELS: [&[&str]; 10] = [
    &["||"],
    &["&&"],
    &["|", "OR"],
    &["^", "XOR"],
    &["&", "AND"],
    &["==", "!=", "="],
    &["<", "<=", ">", ">="],
    &["<<", ">>", "SHL", "SHR"],
    &["+", "-"],
    &["*", "/", "%", "MOD"],
];

fn canonical(op: &str) -> &'static str {
    match op {
        "OR" => "|",
        "XOR" => "^",
        "AND" => "&",
        "=" | "==" => "==",
        "SHL" => "<<",
        "SHR" => ">>",
        "MOD" => "%",
        _ => OPS.iter().copied().find(|o| *o == op).unwrap_or("|"),
    }
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.pos)
    }

    fn peek_op(&self) -> Option<&str> {
        match self.peek()? {
            Tok::Op(o) => Some(o),
            Tok::Name(n) => Some(n.as_str()),
            Tok::Num(_) => None,
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        if self.peek_op() == Some(op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), String> {
        if self.eat(op) { Ok(()) } else { Err(format!("'{op}' expected")) }
    }

    fn binary(&mut self, level: usize) -> Result<Node, String> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        while let Some(op) = self.peek_op().filter(|o| LEVELS[level].contains(o)) {
            let op = canonical(op);
            self.pos += 1;
            let right = self.binary(level + 1)?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Node, String> {
        for (tok, op) in [("-", '-'), ("~", '~'), ("NOT", '~'), ("!", '!')] {
            if self.eat(tok) {
                return Ok(Node::Unary(op, Box::new(self.unary()?)));
            }
        }
        if self.eat("+") {
            return self.unary();
        }
        let node = self.primary()?;
        // seg:[off] / seg:off
        if self.peek_op() == Some(":") {
            self.pos += 1;
            let size = self.size_prefix().unwrap_or(1);
            return if self.eat("[") {
                let off = self.binary(0)?;
                self.expect("]")?;
                Ok(Node::Mem { size, seg: Some(Box::new(node)), off: Box::new(off) })
            } else {
                Err("'[' expected after ':'".into())
            };
        }
        Ok(node)
    }

    /// `BYTE [PTR]`, `WORD [PTR]`, `DWORD [PTR]`, `B.`, `W.`
    fn size_prefix(&mut self) -> Option<u8> {
        let size = match self.peek_op()? {
            "BYTE" | "B." => 1,
            "WORD" | "W." => 2,
            "DWORD" => 4,
            _ => return None,
        };
        self.pos += 1;
        self.eat("PTR");
        Some(size)
    }

    fn primary(&mut self) -> Result<Node, String> {
        if let Some(size) = self.size_prefix() {
            let mut seg = None;
            if let Some(Tok::Name(n)) = self.peek()
                && let Some(r) = sreg(n)
                && self.t.get(self.pos + 1) == Some(&Tok::Op(":"))
            {
                seg = Some(Box::new(Node::Sreg(r)));
                self.pos += 2;
            }
            self.expect("[")?;
            let off = self.binary(0)?;
            self.expect("]")?;
            return Ok(Node::Mem { size, seg, off: Box::new(off) });
        }
        let tok = self.peek().cloned().ok_or("unexpected end of expression")?;
        self.pos += 1;
        match tok {
            Tok::Num(v) => Ok(Node::Num(v)),
            Tok::Op("(") => {
                let n = self.binary(0)?;
                self.expect(")")?;
                Ok(n)
            }
            Tok::Op("[") => {
                // NASM style segment override inside the brackets: [es:di].
                let mut seg = None;
                if let Some(Tok::Name(n)) = self.peek()
                    && let Some(r) = sreg(n)
                    && self.t.get(self.pos + 1) == Some(&Tok::Op(":"))
                {
                    seg = Some(Box::new(Node::Sreg(r)));
                    self.pos += 2;
                }
                let off = self.binary(0)?;
                self.expect("]")?;
                // [nasm_label] reads the label's data size.
                if let (None, Node::Addr { seg: s, off: o, size }) = (&seg, &off) {
                    return Ok(Node::Var { addr: linear(*s, *o), size: *size });
                }
                Ok(Node::Mem { size: 1, seg, off: Box::new(off) })
            }
            Tok::Op(o) => Err(format!("unexpected '{o}'")),
            Tok::Name(n) if n == "OFFSET" => match self.peek().cloned() {
                Some(Tok::Name(v)) => {
                    self.pos += 1;
                    match (self.lookup)(&v) {
                        Some(SymbolRef::Variable { off, .. } | SymbolRef::Address { off, .. }) => {
                            Ok(Node::Num(off as i64))
                        }
                        Some(SymbolRef::Value(x)) => Ok(Node::Num(x)),
                        None => Err(format!("unknown name '{v}'")),
                    }
                }
                _ => Err("name expected after OFFSET".into()),
            },
            Tok::Name(n) => self.name(&n),
        }
    }

    fn name(&mut self, n: &str) -> Result<Node, String> {
        const R16: [&str; 8] = ["AX", "CX", "DX", "BX", "SP", "BP", "SI", "DI"];
        const R8: [&str; 8] = ["AL", "CL", "DL", "BL", "AH", "CH", "DH", "BH"];
        if let Some(i) = R16.iter().position(|r| *r == n) {
            return Ok(Node::Reg16(i));
        }
        if let Some(i) = R8.iter().position(|r| *r == n) {
            return Ok(Node::Reg8(i as u8));
        }
        if let Some(i) = sreg(n) {
            return Ok(Node::Sreg(i));
        }
        let flag = match n {
            "IP" => return Ok(Node::Ip),
            "FLAGS" | "FL" => return Ok(Node::Flags),
            "CF" => flags::CF,
            "PF" => flags::PF,
            "AF" => flags::AF,
            "ZF" => flags::ZF,
            "SF" => flags::SF,
            "TF" => flags::TF,
            "IF" => flags::IF,
            "DF" => flags::DF,
            "OF" => flags::OF,
            _ => 0,
        };
        if flag != 0 {
            return Ok(Node::Flag(flag));
        }
        match (self.lookup)(n) {
            Some(SymbolRef::Variable { seg, off, size }) => {
                if self.eat("[") {
                    let index = self.binary(0)?;
                    self.expect("]")?;
                    Ok(Node::Indexed { seg, off, size, index: Box::new(index) })
                } else {
                    Ok(Node::Var { addr: linear(seg, off), size })
                }
            }
            Some(SymbolRef::Value(v)) => Ok(Node::Num(v)),
            Some(SymbolRef::Address { seg, off, size }) => Ok(Node::Addr { seg, off, size }),
            None => Err(format!("unknown name '{n}'")),
        }
    }
}

fn sreg(n: &str) -> Option<usize> {
    ["ES", "CS", "SS", "DS"].iter().position(|r| *r == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> Machine {
        let mut m = Machine::new();
        m.cpu.regs[cpu::AX] = 0x1234;
        m.cpu.regs[cpu::BX] = 0x0010;
        m.cpu.regs[cpu::SI] = 2;
        m.cpu.sregs[cpu::DS] = 0x0100;
        m.cpu.sregs[cpu::ES] = 0x0200;
        m.cpu.flags |= flags::ZF;
        m.sys.mem.write16(linear(0x0100, 0x10), 0xBEEF);
        m.sys.mem.write8(linear(0x0200, 0x10), 0x77);
        m.sys.mem.load(linear(0x0100, 0x40), &[1, 2, 3, 4, 5]);
        m
    }

    fn ev(text: &str) -> Result<i64, String> {
        let lookup = |n: &str| match n {
            "TABLE" => Some(SymbolRef::Variable { seg: 0x0100, off: 0x40, size: 1 }),
            "COUNT" => Some(SymbolRef::Value(7)),
            _ => None,
        };
        Expr::parse(text, &lookup).map(|e| e.eval(&machine()))
    }

    #[test]
    fn evaluates() {
        assert_eq!(ev("ax"), Ok(0x1234));
        assert_eq!(ev("ah + al"), Ok(0x46));
        assert_eq!(ev("0Ah * 2 + 1010b"), Ok(30));
        assert_eq!(ev("[bx]"), Ok(0xEF));
        assert_eq!(ev("word [bx]"), Ok(0xBEEF));
        assert_eq!(ev("w.[bx+0]"), Ok(0xBEEF));
        assert_eq!(ev("es:[bx]"), Ok(0x77));
        assert_eq!(ev("byte ptr es:[bx]"), Ok(0x77));
        assert_eq!(ev("table"), Ok(1));
        assert_eq!(ev("table[si]"), Ok(3));
        assert_eq!(ev("offset table"), Ok(0x40));
        assert_eq!(ev("count shl 1"), Ok(14));
        assert_eq!(ev("ax == 1234h && zf"), Ok(1));
        assert_eq!(ev("ax = 5 || cf"), Ok(0));
        assert_eq!(ev("'A'"), Ok(65));
        assert_eq!(ev("-1 & 0FFh"), Ok(255));
        assert_eq!(ev("(1 + 2) * 3 % 4"), Ok(1));
        assert!(ev("foo").unwrap_err().contains("unknown name"));
        assert!(ev("1 +").is_err());
        assert!(ev("(1").is_err());
    }
}
