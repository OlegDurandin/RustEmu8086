//! Tokens of a MARS source line, by the rules of MARS's `Tokenizer` and `TokenTypes`.
//!
//! Spaces, tabs and commas separate tokens; `#` starts a comment; `:`, `(`, `)` are tokens of their
//! own. A `+` or `-` belongs to the number after it unless the previous token is an identifier
//! (`label+4` is three tokens, `-100($t2)` starts with the number -100). Character literals
//! (`'a'`, `'\n'`, `'\101'`) become integers. Numbers follow Java's `Integer.decode` (decimal,
//! `0x` hex, a leading `0` octal), plus MARS's eight-digit hex and unsigned decimals.
//!
//! `%hi(label)` and `%lo(label)` (with an optional `+n`) are an extension over MARS, as in the
//! GNU assembler: the upper and the lower half of an address for `lui` / `addiu` / loads.

use crate::cpu::REG_NAMES;

/// What a token is.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// `$t0`: a register by name (its number).
    RegName(u32),
    /// `$8`: a register by number.
    RegNum(u32),
    /// `$f0`: a floating-point register (coprocessor 1, not supported yet).
    FpReg(u32),
    /// An integer (character literals included).
    Int(i32),
    /// A real number (`1.5`, `2e-3`): only in `.float` / `.double`.
    Real(f64),
    /// A name: a label, a mnemonic, an `.eqv` name.
    Ident,
    /// A directive: `.text`, `.word`.
    Directive,
    /// A quoted string, with the quotes and escapes as written.
    Str,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `:`
    Colon,
    /// `+` between a label and a number.
    Plus,
    /// `-` between a label and a number.
    Minus,
    /// `%hi(label+offset)` (`hi`) or `%lo(...)`: resolved to an integer in the second pass.
    Reloc {
        /// The upper half (otherwise the lower).
        hi: bool,
        /// The label.
        label: String,
        /// The offset added to its address.
        offset: i32,
    },
}

/// A token with its text and place in the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What it is.
    pub kind: Kind,
    /// The text as written (for integers from character literals: the value in decimal).
    pub text: String,
    /// 1-based column of its first character.
    pub col: usize,
}

/// The integer kinds of MARS, by value: they decide which form of an instruction matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntClass {
    /// 0..=31 (shift amounts).
    I5,
    /// 0..=65535.
    I16U,
    /// -32768..=32767 (and not the above).
    I16,
    /// Any other 32-bit value.
    I32,
}

impl IntClass {
    /// The class of `v`, checked in MARS's order.
    pub fn of(v: i32) -> IntClass {
        match v {
            0..=31 => IntClass::I5,
            32..=65535 => IntClass::I16U,
            -32768..=-1 => IntClass::I16,
            _ => IntClass::I32,
        }
    }
}

/// A MARS integer: `Integer.decode`, then an eight-digit `0x` value, then an unsigned decimal
/// (both wrapping to 32 bits). None when the text is not an integer.
pub fn parse_int(s: &str) -> Option<i32> {
    if let Some(v) = java_decode(s) {
        return Some(v);
    }
    let lower = s.to_ascii_lowercase();
    if lower.len() == 10
        && let Some(hex) = lower.strip_prefix("0x")
    {
        return u32::from_str_radix(hex, 16).ok().map(|v| v as i32);
    }
    if !lower.starts_with("0x") && !lower.is_empty() && lower.bytes().all(|b| b.is_ascii_digit()) {
        return Some(lower.bytes().fold(0i32, |v, b| v.wrapping_mul(10).wrapping_add((b - b'0') as i32)));
    }
    None
}

/// Java's `Integer.decode`: optional sign, `0x` / `0X` / `#` hex, a leading `0` octal, decimal; in
/// the `int` range.
fn java_decode(s: &str) -> Option<i32> {
    let (neg, rest) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let (radix, digits) = if let Some(h) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16, h)
    } else if let Some(h) = rest.strip_prefix('#') {
        (16, h)
    } else if rest.len() > 1 && rest.starts_with('0') {
        (8, &rest[1..])
    } else {
        (10, rest)
    };
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let v = i64::from_str_radix(digits, radix).ok()?;
    let v = if neg { -v } else { v };
    i32::try_from(v).ok()
}

/// A register by name (`$t0`) or number (`$8`).
fn register(s: &str) -> Option<Kind> {
    let name = s.strip_prefix('$')?;
    if let Some(i) = REG_NAMES.iter().position(|r| *r == name) {
        return Some(Kind::RegName(i as u32));
    }
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
        let n: u32 = name.parse().ok()?;
        return (n < 32 && (name == "0" || !name.starts_with('0'))).then_some(Kind::RegNum(n));
    }
    let f = name.strip_prefix('f')?;
    let n: u32 = f.parse().ok()?;
    (n < 32 && f.bytes().all(|b| b.is_ascii_digit())).then_some(Kind::FpReg(n))
}

/// MARS's `isValidIdentifier`: a letter, `_`, `.` or `$`, then letters, digits, `_`, `.`, `$`.
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '.' | '$'))
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '$'))
}

/// The directives MARS knows.
pub const DIRECTIVES: [&str; 21] = [
    ".align",
    ".ascii",
    ".asciiz",
    ".byte",
    ".data",
    ".double",
    ".end_macro",
    ".eqv",
    ".extern",
    ".float",
    ".globl",
    ".half",
    ".include",
    ".kdata",
    ".ktext",
    ".macro",
    ".set",
    ".space",
    ".text",
    ".word",
    ".global",
];

/// An error in a line: 1-based column and message.
pub type LexError = (usize, String);

/// Classifies the text of a token.
fn classify(text: &str, col: usize) -> Result<Token, LexError> {
    let kind = if let Some(r) = register(text) {
        r
    } else if let Some(v) = parse_int(text) {
        Kind::Int(v)
    } else if text.starts_with('.') && DIRECTIVES.iter().any(|d| d.eq_ignore_ascii_case(text)) {
        Kind::Directive
    } else if is_identifier(text) {
        Kind::Ident
    } else if let Some(v) = parse_real(text) {
        Kind::Real(v)
    } else {
        return Err((col, format!("Invalid language element: {text}")));
    };
    Ok(Token { kind, text: text.to_string(), col })
}

/// Java's `Double.parseDouble` for the usual forms (`1.5`, `-2e3`, `.5`).
fn parse_real(s: &str) -> Option<f64> {
    let t = s.strip_prefix(['+', '-']).unwrap_or(s);
    let ok = !t.is_empty()
        && t.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        && t.bytes().next().is_some_and(|b| b.is_ascii_digit() || b == b'.');
    if ok { s.parse().ok() } else { None }
}

/// The value of a character literal (`'a'`, `'\n'`, `'\101'`), as MARS's tokenizer converts it.
fn char_literal(body: &str) -> Option<i32> {
    let mut chars = body.chars();
    match (chars.next()?, chars.as_str()) {
        ('\\', rest) => {
            let escaped = match rest {
                "'" => 39,
                "\"" => 34,
                "\\" => 92,
                "n" => 10,
                "t" => 9,
                "b" => 8,
                "r" => 13,
                "f" => 12,
                "0" => 0,
                _ if rest.len() == 3 && rest.bytes().all(|b| (b'0'..=b'7').contains(&b)) => {
                    let v = i32::from_str_radix(rest, 8).ok()?;
                    return (0..=255).contains(&v).then_some(v);
                }
                _ => return None,
            };
            Some(escaped)
        }
        (c, "") => Some(c as i32),
        _ => None,
    }
}

/// The tokens of `line` (the comment dropped).
pub fn tokenize(line: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut cur = String::new();
    let mut start = 0;
    let flush = |cur: &mut String, start: usize, out: &mut Vec<Token>| -> Result<(), LexError> {
        if !cur.is_empty() {
            out.push(classify(cur, start + 1)?);
            cur.clear();
        }
        Ok(())
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '#' => break,
            ' ' | '\t' | ',' | '\r' => flush(&mut cur, start, &mut out)?,
            '+' | '-' => {
                // E notation: 1.2e-5.
                if !cur.is_empty() && cur.ends_with(['e', 'E']) && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())
                {
                    cur.push(c);
                } else {
                    flush(&mut cur, start, &mut out)?;
                    let unary = out.last().is_none_or(|t| t.kind != Kind::Ident)
                        && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit());
                    if unary {
                        start = i;
                        cur.push(c);
                    } else {
                        let kind = if c == '+' { Kind::Plus } else { Kind::Minus };
                        out.push(Token { kind, text: c.to_string(), col: i + 1 });
                    }
                }
            }
            ':' | '(' | ')' => {
                flush(&mut cur, start, &mut out)?;
                let kind = match c {
                    ':' => Kind::Colon,
                    '(' => Kind::LParen,
                    _ => Kind::RParen,
                };
                out.push(Token { kind, text: c.to_string(), col: i + 1 });
            }
            '"' => {
                flush(&mut cur, start, &mut out)?;
                let begin = i;
                i += 1;
                while i < chars.len() && !(chars[i] == '"' && chars[i - 1] != '\\') {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err((begin + 1, "unterminated string".into()));
                }
                let text: String = chars[begin..=i].iter().collect();
                out.push(Token { kind: Kind::Str, text, col: begin + 1 });
            }
            '\'' => {
                flush(&mut cur, start, &mut out)?;
                let begin = i;
                // The closing quote: after one character, or after a backslash and what it escapes
                // (up to three octal digits).
                let mut close = i + if chars.get(i + 1) == Some(&'\\') { 3 } else { 2 };
                while close < chars.len() && chars[close] != '\'' && close < i + 5 {
                    close += 1;
                }
                if chars.get(close) != Some(&'\'') {
                    return Err((begin + 1, "Invalid language element: '".into()));
                }
                let body: String = chars[i + 1..close].iter().collect();
                let Some(v) = char_literal(&body) else {
                    return Err((begin + 1, format!("Invalid language element: '{body}'")));
                };
                out.push(Token { kind: Kind::Int(v), text: v.to_string(), col: begin + 1 });
                i = close;
            }
            '%' if cur.is_empty() => {
                // %hi(label) / %lo(label+n)
                let rest: String = chars[i..].iter().collect();
                let (tok, used) = reloc(&rest, i + 1)?;
                out.push(tok);
                i += used - 1;
            }
            _ => {
                if cur.is_empty() {
                    start = i;
                }
                cur.push(c);
            }
        }
        i += 1;
    }
    flush(&mut cur, start, &mut out)?;
    Ok(out)
}

/// `%hi(label)` / `%lo(label+n)` at the start of `s`: the token and the characters it takes.
fn reloc(s: &str, col: usize) -> Result<(Token, usize), LexError> {
    let bad = || (col, format!("Invalid language element: {}", s.split([' ', ',', '\t']).next().unwrap_or(s)));
    let (hi, rest) = if let Some(r) = s.strip_prefix("%hi(") {
        (true, r)
    } else if let Some(r) = s.strip_prefix("%lo(") {
        (false, r)
    } else {
        return Err(bad());
    };
    let close = rest.find(')').ok_or_else(bad)?;
    let inner: String = rest[..close].chars().filter(|c| !c.is_whitespace()).collect();
    let (label, offset) = match inner.find(['+', '-']) {
        Some(p) => (&inner[..p], parse_int(&inner[p..]).ok_or_else(bad)?),
        None => (&inner[..], 0),
    };
    if !is_identifier(label) {
        return Err(bad());
    }
    let used = 4 + close + 1;
    let text: String = s.chars().take(used).collect();
    Ok((Token { kind: Kind::Reloc { hi, label: label.to_string(), offset }, text, col }, used))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<Kind> {
        tokenize(line).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn integers_like_java() {
        assert_eq!(parse_int("100"), Some(100));
        assert_eq!(parse_int("-100"), Some(-100));
        assert_eq!(parse_int("0x10"), Some(16));
        assert_eq!(parse_int("010"), Some(8), "a leading 0 is octal (Integer.decode)");
        assert_eq!(parse_int("0xffffffff"), Some(-1));
        assert_eq!(parse_int("4294967295"), Some(-1));
        assert_eq!(parse_int("0x1ffffffff"), None);
        assert_eq!(parse_int("12a"), None);
        assert_eq!(IntClass::of(10), IntClass::I5);
        assert_eq!(IntClass::of(100), IntClass::I16U);
        assert_eq!(IntClass::of(-100), IntClass::I16);
        assert_eq!(IntClass::of(100_000), IntClass::I32);
    }

    #[test]
    fn mars_tokens() {
        use Kind::*;
        assert_eq!(kinds("lw $t1, -100($t2) # load"), [Ident, RegName(9), Int(-100), LParen, RegName(10), RParen]);
        assert_eq!(kinds("lw $8,label+4"), [Ident, RegNum(8), Ident, Plus, Int(4)]);
        assert_eq!(kinds("main: li $v0,'\\n'"), [Ident, Colon, Ident, RegName(2), Int(10)]);
        assert_eq!(kinds("x: .asciiz \"a \\\"b\\\" # c\""), [Ident, Colon, Directive, Str]);
        assert_eq!(kinds(".word 1:5, 'A'"), [Directive, Int(1), Colon, Int(5), Int(65)]);
        assert_eq!(kinds(".double 1.5e-3"), [Directive, Real(1.5e-3)]);
        assert_eq!(kinds("addi $t0 $t0 - 1"), [Ident, RegName(8), RegName(8), Minus, Int(1)]);
        assert_eq!(kinds("lui $t0, %hi(msg+4)")[2], Reloc { hi: true, label: "msg".into(), offset: 4 });
        assert!(tokenize("li $t0, 12x").unwrap_err().1.contains("Invalid language element: 12x"));
        assert!(tokenize("li $t0, \"abc").is_err());
    }
}
