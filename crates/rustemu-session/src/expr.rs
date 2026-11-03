//! Debugger expressions: watches, breakpoint conditions and "stop on condition".
//!
//! Registers, flags and memory come from the [`Target`]: its register table
//! ([`Target::registers`]) gives the names and widths, [`Target::data_address`] turns `[off]` /
//! `seg:[off]` into a physical address.
//!
//! Operands: numbers in assembler notation (`10`, `0Ah`, `0x0A`, `1010b`, `12o`, `'c'`), registers
//! (8086: `AX`, `AL`, `CS`, `IP`, `FLAGS`), flags (`CF`, `ZF`, `SF`, `OF`, `PF`, `AF`, `IF`, `DF`, `TF`),
//! program symbols (a variable gives its value from memory, a label its offset, `OFFSET name` the
//! offset of a variable), memory `[expr]`, `seg:[expr]`, `BYTE [..]` / `WORD [..]` / `b.[..]` / `w.[..]`,
//! indexed variables `table[si]`.
//! Operators (C precedence): `! ~ -` (unary), `* / %`, `+ -`, `<< >>`, `< <= > >=`, `== !=` (`=` too),
//! `&`, `^`, `|`, `&&`, `||`, and the assembler words `MOD SHL SHR AND OR XOR NOT`.

use rustemu_target::{RegKind, Target};

/// A compiled expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    node: Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(i64),
    /// Register `index` of the target's table, `bits` wide (1 for a flag).
    Reg {
        index: usize,
        bits: u8,
    },
    /// Memory at segment:offset; `seg` = None means the target's default (8086: DS).
    Mem {
        size: u8,
        seg: Option<Box<Node>>,
        off: Box<Node>,
    },
    /// A variable at a fixed physical address.
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
    Variable {
        /// Segment.
        seg: u16,
        /// Offset.
        off: u16,
        /// Element size in bytes.
        size: u8,
    },
    /// A label or constant: its value.
    Value(i64),
    /// A NASM data label: its value is the address; `[name]` reads `size` bytes there.
    Address {
        /// Segment.
        seg: u16,
        /// Offset.
        off: u16,
        /// Element size in bytes.
        size: u8,
    },
}

impl Expr {
    /// Parses `text`, resolving names with `lookup` and the registers of `target`.
    pub fn parse(text: &str, lookup: &dyn Fn(&str) -> Option<SymbolRef>, target: &dyn Target) -> Result<Expr, String> {
        let tokens = lex(text)?;
        if tokens.is_empty() {
            return Err("empty expression".into());
        }
        let mut p = Parser { t: &tokens, pos: 0, lookup, target, depth: 0, operands: 0 };
        let node = p.binary(0)?;
        if p.pos < tokens.len() {
            return Err(format!("unexpected '{}'", tokens[p.pos].text()));
        }
        Ok(Expr { node })
    }

    /// The value of the expression in the current state of `t`.
    pub fn eval(&self, t: &dyn Target) -> i64 {
        eval(&self.node, t)
    }

    /// Natural width of the value in bits (8 for byte registers and variables, 16 for words, ...).
    pub fn bits(&self) -> u32 {
        match &self.node {
            Node::Reg { bits, .. } => (*bits).max(8) as u32,
            Node::Mem { size, .. } | Node::Var { size, .. } | Node::Indexed { size, .. } => *size as u32 * 8,
            _ => 16,
        }
    }

    /// Physical address of the memory the expression reads directly (for `[..]` and variables).
    pub fn address(&self, t: &dyn Target) -> Option<u32> {
        match &self.node {
            Node::Mem { seg, off, .. } => Some(mem_addr(t, seg.as_deref(), off)),
            Node::Var { addr, .. } => Some(*addr),
            Node::Indexed { seg, off, index, .. } => Some(indexed_addr(t, *seg, *off, index)),
            _ => None,
        }
    }
}

fn mem_addr(t: &dyn Target, seg: Option<&Node>, off: &Node) -> u32 {
    t.data_address(seg.map(|s| eval(s, t) as u64), eval(off, t) as u64)
}

/// `var[index]`: the variable's offset plus the index, in the variable's segment.
fn indexed_addr(t: &dyn Target, seg: u16, off: u16, index: &Node) -> u32 {
    t.data_address(Some(seg as u64), off.wrapping_add(eval(index, t) as u16) as u64)
}

fn read(t: &dyn Target, addr: u32, size: u8) -> i64 {
    let mut v: i64 = 0;
    for i in (0..size as u32).rev() {
        v = (v << 8) | t.read8(addr.wrapping_add(i)) as i64;
    }
    v
}

fn eval(n: &Node, t: &dyn Target) -> i64 {
    match n {
        Node::Num(v) => *v,
        Node::Reg { index, .. } => t.register(*index) as i64,
        Node::Mem { size, seg, off } => read(t, mem_addr(t, seg.as_deref(), off), *size),
        Node::Var { addr, size } => read(t, *addr, *size),
        Node::Indexed { seg, off, size, index } => read(t, indexed_addr(t, *seg, *off, index), *size),
        Node::Addr { off, .. } => *off as i64,
        Node::Unary(op, a) => {
            let a = eval(a, t);
            match op {
                '-' => a.wrapping_neg(),
                '~' => !a,
                _ => (a == 0) as i64,
            }
        }
        Node::Binary(op, a, b) => {
            let a = eval(a, t);
            if *op == "&&" {
                return (a != 0 && eval(b, t) != 0) as i64;
            }
            if *op == "||" {
                return (a != 0 || eval(b, t) != 0) as i64;
            }
            let b = eval(b, t);
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
        } else if c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '?' | '.' | '$') {
            // `$` starts MIPS register names ($t0, $8).
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '@' | '?' | '.' | '$'))
            {
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
    target: &'a dyn Target,
    /// Nesting now (parentheses, brackets, unary operators).
    depth: u32,
    /// Operands parsed.
    operands: u32,
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

    /// An operand: limited in nesting and number, as the assembler's (a deep expression would
    /// overflow the stack when it is parsed, evaluated or dropped).
    fn unary(&mut self) -> Result<Node, String> {
        use rustemu_asm::expr::{MAX_NESTING, MAX_OPERANDS};
        self.operands += 1;
        if self.operands > MAX_OPERANDS {
            return Err(format!("the expression is too long (more than {MAX_OPERANDS} operands)"));
        }
        if self.depth >= MAX_NESTING {
            return Err(format!("the expression is nested too deeply (more than {MAX_NESTING} levels)"));
        }
        self.depth += 1;
        let r = self.unary_inner();
        self.depth -= 1;
        r
    }

    fn unary_inner(&mut self) -> Result<Node, String> {
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
                && let Some(r) = self.segment_register(n)
                && self.t.get(self.pos + 1) == Some(&Tok::Op(":"))
            {
                seg = Some(Box::new(r));
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
                    && let Some(r) = self.segment_register(n)
                    && self.t.get(self.pos + 1) == Some(&Tok::Op(":"))
                {
                    seg = Some(Box::new(r));
                    self.pos += 2;
                }
                let off = self.binary(0)?;
                self.expect("]")?;
                // [nasm_label] reads the label's data size.
                if let (None, Node::Addr { seg: s, off: o, size }) = (&seg, &off) {
                    return Ok(Node::Var { addr: self.target.data_address(Some(*s as u64), *o as u64), size: *size });
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

    /// A segment register named `n` (it can prefix a memory reference).
    fn segment_register(&self, n: &str) -> Option<Node> {
        let index = self.target.register_index(n)?;
        let r = self.target.registers()[index];
        (r.kind == RegKind::Segment).then_some(Node::Reg { index, bits: r.bits })
    }

    fn name(&mut self, n: &str) -> Result<Node, String> {
        if let Some(index) = self.target.register_index(n) {
            return Ok(Node::Reg { index, bits: self.target.registers()[index].bits });
        }
        match (self.lookup)(n) {
            Some(SymbolRef::Variable { seg, off, size }) => {
                if self.eat("[") {
                    let index = self.binary(0)?;
                    self.expect("]")?;
                    Ok(Node::Indexed { seg, off, size, index: Box::new(index) })
                } else {
                    Ok(Node::Var { addr: self.target.data_address(Some(seg as u64), off as u64), size })
                }
            }
            Some(SymbolRef::Value(v)) => Ok(Node::Num(v)),
            Some(SymbolRef::Address { seg, off, size }) => Ok(Node::Addr { seg, off, size }),
            None => Err(format!("unknown name '{n}'")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustemu_core::cpu::{self, flags};
    use rustemu_core::{Machine, linear};

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
        let m = machine();
        Expr::parse(text, &lookup, &m).map(|e| e.eval(&m))
    }

    /// A deep or long watch expression is an error, not a stack overflow (TODO item 7).
    #[test]
    fn deep_expressions_are_errors() {
        let deep = format!("{}1{}", "(".repeat(20_000), ")".repeat(20_000));
        assert!(ev(&deep).unwrap_err().contains("nested too deeply"));
        assert!(ev(&"-".repeat(20_000)).unwrap_err().contains("nested too deeply"));
        assert!(ev(&format!("1{}", "+1".repeat(200_000))).unwrap_err().contains("too long"));
        assert_eq!(ev(&format!("{}ax{}", "(".repeat(20), ")".repeat(20))), Ok(0x1234));
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
