//! Tokenizer for one source line.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Identifier, keyword, register or directive (original spelling kept, compare uppercase).
    Ident(String),
    Num(i64),
    /// Quoted string ('...' or "...").
    Str(Vec<u8>),
    /// Single punctuation character: + - * / ( ) [ ] , : ? $ = < > % ! & | ^ ~
    P(char),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    /// Byte column in the line (0-based).
    pub col: usize,
}

impl Tok {
    pub fn is_ident(&self, s: &str) -> bool {
        matches!(self, Tok::Ident(i) if i.eq_ignore_ascii_case(s))
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || matches!(c, b'_' | b'@' | b'?' | b'.')
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'@' | b'?' | b'$')
}

/// Parses a numeric literal (`123`, `0Ah`, `1010b`, `17o`, `0x1F`, `99d`).
pub fn parse_number(s: &str) -> Option<i64> {
    let l = s.to_ascii_lowercase();
    if let Some(h) = l.strip_prefix("0x") {
        return i64::from_str_radix(h, 16).ok();
    }
    let (body, radix) = match l.as_bytes().last()? {
        b'h' => (&l[..l.len() - 1], 16),
        b'b' if l[..l.len() - 1].bytes().all(|c| c == b'0' || c == b'1') => (&l[..l.len() - 1], 2),
        b'o' | b'q' => (&l[..l.len() - 1], 8),
        b'd' if l[..l.len() - 1].bytes().all(|c| c.is_ascii_digit()) => (&l[..l.len() - 1], 10),
        _ => (&l[..], 10),
    };
    if body.is_empty() {
        return None;
    }
    i64::from_str_radix(body, radix).ok()
}

/// NASM numbers: `0x1F`, `0h1F`, `1Fh`, `$1F`, `0b1010`, `0y1010`, `1010b`, `1010y`, `0o17`, `0q17`,
/// `17o`, `17q`, `0d99`, `99d`.
pub fn parse_number_nasm(s: &str) -> Option<i64> {
    let l = s.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0h", 16), ("0b", 2), ("0y", 2), ("0o", 8), ("0q", 8), ("0d", 10)] {
        if let Some(body) = l.strip_prefix(prefix)
            && !body.is_empty()
            && body.chars().all(|c| c.is_digit(radix))
        {
            return i64::from_str_radix(body, radix).ok();
        }
    }
    if let Some(body) = l.strip_suffix('y').filter(|b| !b.is_empty() && b.bytes().all(|c| c == b'0' || c == b'1')) {
        return i64::from_str_radix(body, 2).ok();
    }
    parse_number(s)
}

/// Splits a line into tokens, stopping at a `;` comment.
pub fn tokenize(line: &str) -> Result<Vec<Token>, (usize, String)> {
    tokenize_with(line, false)
}

/// Like [`tokenize`] for the NASM dialect: `.` may appear in names (local labels `.loop`),
/// backquoted strings with C escapes, `$1F` hexadecimal numbers and NASM number prefixes.
pub fn tokenize_nasm(line: &str) -> Result<Vec<Token>, (usize, String)> {
    tokenize_with(line, true)
}

/// Contents of a NASM backquoted string starting after the opening backquote at `b[i]`;
/// returns the bytes and the index after the closing backquote.
fn backquoted(b: &[u8], mut i: usize, start: usize) -> Result<(Vec<u8>, usize), (usize, String)> {
    let mut s = Vec::new();
    loop {
        let Some(&c) = b.get(i) else { return Err((start, "unterminated string".into())) };
        i += 1;
        match c {
            b'`' => return Ok((s, i)),
            b'\\' => {
                let Some(&e) = b.get(i) else { return Err((start, "unterminated string".into())) };
                i += 1;
                s.push(match e {
                    b'n' => 10,
                    b't' => 9,
                    b'r' => 13,
                    b'0' => 0,
                    b'a' => 7,
                    b'b' => 8,
                    b'e' => 27,
                    b'x' => {
                        let hex: String =
                            b[i..].iter().take(2).take_while(|c| c.is_ascii_hexdigit()).map(|&c| c as char).collect();
                        i += hex.len();
                        u8::from_str_radix(&hex, 16).map_err(|_| (start, "bad \\x escape".to_string()))?
                    }
                    other => other,
                });
            }
            c => s.push(c),
        }
    }
}

fn tokenize_with(line: &str, nasm: bool) -> Result<Vec<Token>, (usize, String)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b';' {
            break;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c == b'\'' || c == b'"' {
            let q = c;
            i += 1;
            let mut s = Vec::new();
            loop {
                if i >= b.len() {
                    return Err((start, "unterminated string".into()));
                }
                if b[i] == q {
                    // Doubled quote = literal quote.
                    if i + 1 < b.len() && b[i + 1] == q {
                        s.push(q);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                s.push(b[i]);
                i += 1;
            }
            out.push(Token { tok: Tok::Str(s), col: start });
            continue;
        }
        if nasm && c == b'`' {
            let (s, next) = backquoted(b, i + 1, start)?;
            i = next;
            out.push(Token { tok: Tok::Str(s), col: start });
            continue;
        }
        if nasm && c == b'$' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            // $1F: hexadecimal number.
            i += 1;
            while i < b.len() && (b[i].is_ascii_hexdigit() || b[i] == b'_') {
                i += 1;
            }
            let text = line[start + 1..i].replace('_', "");
            let n = i64::from_str_radix(&text, 16).map_err(|_| (start, format!("invalid number '${text}'")))?;
            out.push(Token { tok: Tok::Num(n), col: start });
            continue;
        }
        if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let text = line[start..i].replace('_', "");
            let parsed = if nasm { parse_number_nasm(&text) } else { parse_number(&text) };
            let n = parsed.ok_or((start, format!("invalid number '{text}'")))?;
            out.push(Token { tok: Tok::Num(n), col: start });
            continue;
        }
        if c == b'?' && !b.get(i + 1).is_some_and(|&n| is_ident_char(n)) {
            out.push(Token { tok: Tok::P('?'), col: start });
            i += 1;
            continue;
        }
        if is_ident_start(c) {
            i += 1;
            while i < b.len() && (is_ident_char(b[i]) || (nasm && b[i] == b'.')) {
                i += 1;
            }
            let mut text = line[start..i].to_string();
            // Emu8086 size shorthand: b.[bx] / w.[bx] / b.var
            if !nasm
                && (text.eq_ignore_ascii_case("b") || text.eq_ignore_ascii_case("w"))
                && i < b.len()
                && b[i] == b'.'
            {
                text.push('.');
                i += 1;
            }
            out.push(Token { tok: Tok::Ident(text), col: start });
            continue;
        }
        if c == b'$' {
            // `$` alone is the location counter; `$name` is not supported.
            out.push(Token { tok: Tok::P('$'), col: start });
            i += 1;
            continue;
        }
        if b"+-*/()[],:?=<>%!&#|^~".contains(&c) {
            out.push(Token { tok: Tok::P(c as char), col: start });
            i += 1;
            continue;
        }
        return Err((start, format!("unexpected character '{}'", c as char)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("10"), Some(10));
        assert_eq!(parse_number("0Ah"), Some(10));
        assert_eq!(parse_number("0FFFFh"), Some(0xFFFF));
        assert_eq!(parse_number("1010b"), Some(10));
        assert_eq!(parse_number("17o"), Some(15));
        assert_eq!(parse_number("0x1f"), Some(31));
        assert_eq!(parse_number("1Bh"), Some(0x1B));
        assert_eq!(parse_number("10d"), Some(10));
        assert_eq!(parse_number("0Dh"), Some(13));
        assert_eq!(parse_number("12x"), None);
        assert_eq!(tokenize("0000_0011b").unwrap()[0].tok, Tok::Num(3));
    }

    #[test]
    fn tokens() {
        let t = tokenize("mov b.[bx+2], 'A' ; comment").unwrap();
        let k: Vec<_> = t.into_iter().map(|t| t.tok).collect();
        assert_eq!(
            k,
            vec![
                Tok::Ident("mov".into()),
                Tok::Ident("b.".into()),
                Tok::P('['),
                Tok::Ident("bx".into()),
                Tok::P('+'),
                Tok::Num(2),
                Tok::P(']'),
                Tok::P(','),
                Tok::Str(b"A".to_vec()),
            ]
        );
        let t = tokenize("msg db \"it's\", 0Dh,0Ah,'$'").unwrap();
        assert_eq!(t[2].tok, Tok::Str(b"it's".to_vec()));
        assert_eq!(t.len(), 9);
    }
}
