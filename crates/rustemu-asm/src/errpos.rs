//! Where in its source line an error is. The assembler reports most errors per statement; the
//! part to underline is found from the message (a quoted name, the operand it is about) and the
//! line itself. Columns and lengths are in characters.

/// Words after a name that make the first word of a line a name (`x DB 1`, `main PROC`).
const NAMED: &[&str] = &[
    "DB", "DW", "DD", "DQ", "DT", "EQU", "=", "SEGMENT", "ENDS", "PROC", "ENDP", "MACRO", "LABEL", "RESB", "RESW",
    "RESD", "RESQ", "TIMES",
];
const PREFIXES: &[&str] = &["REP", "REPE", "REPZ", "REPNE", "REPNZ", "LOCK"];

/// Words of a message that say it is about the operands.
const ABOUT_OPERANDS: &[&str] = &[
    "operand",
    "register",
    "memory",
    "range",
    "size",
    "target",
    "port",
    "count",
    "expression",
    "constant",
    "address",
    "fit",
    "label",
    "symbol",
    "value",
];

/// (column, length) to underline in `line` for error `msg`. `in_macro`: the error is in a line a
/// macro produced, and `line` is the macro call (underlined whole).
pub fn locate(msg: &str, line: &str, in_macro: bool) -> Option<(usize, usize)> {
    let code = strip_comment(line).trim_end();
    let stmt = statement(code)?;
    let whole = stmt.name.as_ref().unwrap_or(&stmt.mnem).start..code.len();
    let range = if in_macro {
        whole
    } else if let Some(r) = quoted(msg).and_then(|t| find_word(code, t)) {
        r
    } else {
        let lower = msg.to_ascii_lowercase();
        let all = stmt.ops.first().zip(stmt.ops.last()).map(|(a, b)| a.start..b.end);
        let pick = if lower.contains("destination") {
            stmt.ops.first().cloned()
        } else if lower.starts_with("value ") || lower.contains("source") {
            stmt.ops.last().cloned()
        } else if ABOUT_OPERANDS.iter().any(|w| lower.contains(w)) {
            all
        } else {
            None
        };
        pick.unwrap_or(whole)
    };
    Some(chars(line, range))
}

/// (column, length) of the token at byte `at` of `line` (a parse error position).
pub fn token_at(line: &str, at: usize) -> Option<(usize, usize)> {
    let rest = line.get(at..)?;
    let first = rest.chars().next()?;
    let len = if is_word(first) {
        rest.find(|c: char| !is_word(c)).unwrap_or(rest.len())
    } else if first == '\'' || first == '"' {
        rest[1..].find(first).map_or(rest.len(), |i| i + 2)
    } else {
        first.len_utf8()
    };
    Some(chars(line, at..at + len))
}

fn chars(line: &str, r: std::ops::Range<usize>) -> (usize, usize) {
    (line[..r.start].chars().count(), line[r].chars().count().max(1))
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '?' | '$' | '.')
}

fn strip_comment(text: &str) -> &str {
    let mut q = None;
    for (i, c) in text.char_indices() {
        match q {
            Some(qc) if c == qc => q = None,
            Some(_) => {}
            None if c == '\'' || c == '"' || c == '`' => q = Some(c),
            None if c == ';' => return &text[..i],
            None => {}
        }
    }
    text
}

/// The first name in quotes in `msg` (`undefined symbol 'X'`), except what was expected but is
/// missing (`expected ')'`).
fn quoted(msg: &str) -> Option<&str> {
    let start = msg.find('\'')?;
    let end = start + 1 + msg[start + 1..].find('\'')?;
    let tok = &msg[start + 1..end];
    (!tok.is_empty() && !msg[..start].trim_end().ends_with("expected")).then_some(tok)
}

/// `tok` in `code` (ignoring case; a name only as a whole word).
fn find_word(code: &str, tok: &str) -> Option<std::ops::Range<usize>> {
    let (hay, needle) = (code.to_ascii_lowercase(), tok.to_ascii_lowercase());
    let word = needle.chars().all(is_word);
    let mut from = 0;
    while let Some(i) = hay[from..].find(&needle) {
        let (s, e) = (from + i, from + i + needle.len());
        let free = |c: Option<char>| !word || c.is_none_or(|c| !is_word(c));
        if free(hay[..s].chars().next_back()) && free(hay[e..].chars().next()) {
            return Some(s..e);
        }
        from = e;
    }
    None
}

/// Byte ranges of the parts of a statement.
struct Statement {
    /// `x` of `x DB 1`.
    name: Option<std::ops::Range<usize>>,
    /// The instruction (with its prefix) or directive.
    mnem: std::ops::Range<usize>,
    ops: Vec<std::ops::Range<usize>>,
}

fn statement(code: &str) -> Option<Statement> {
    let skip_ws = |i: usize| code[i..].find(|c: char| !c.is_whitespace()).map(|j| i + j);
    let word_end = |i: usize| code[i..].find(char::is_whitespace).map_or(code.len(), |j| i + j);
    let mut i = skip_ws(0)?;
    // A label: `name:` (not `es:[bx]`, which only follows a mnemonic).
    let first_end = word_end(i);
    if code[i..first_end].ends_with(':') && code[i..first_end - 1].chars().all(is_word) {
        i = skip_ws(first_end)?;
    }
    let mut name = None;
    let mut mnem = i..word_end(i);
    if let Some(j) = skip_ws(mnem.end) {
        let next = &code[j..word_end(j)];
        if NAMED.iter().any(|d| next.eq_ignore_ascii_case(d)) {
            name = Some(mnem.clone());
            mnem = j..word_end(j);
        } else if PREFIXES.iter().any(|p| code[mnem.clone()].eq_ignore_ascii_case(p)) {
            mnem = mnem.start..word_end(j);
        }
    }
    let mut ops = Vec::new();
    if let Some(start) = skip_ws(mnem.end) {
        let (mut depth, mut q, mut from) = (0i32, None, start);
        for (k, c) in code[start..].char_indices().map(|(k, c)| (start + k, c)) {
            match (q, c) {
                (Some(qc), _) if c == qc => q = None,
                (Some(_), _) => {}
                (None, '\'' | '"' | '`') => q = Some(c),
                (None, '[' | '(' | '<') => depth += 1,
                (None, ']' | ')' | '>') => depth -= 1,
                (None, ',') if depth <= 0 => {
                    ops.push(trimmed(code, from..k));
                    from = k + 1;
                }
                _ => {}
            }
        }
        ops.push(trimmed(code, from..code.len()));
        ops.retain(|r| !r.is_empty());
    }
    Some(Statement { name, mnem, ops })
}

fn trimmed(code: &str, r: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let s = &code[r.clone()];
    let start = r.start + (s.len() - s.trim_start().len());
    start..start + s.trim().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text `locate` underlines.
    fn under(msg: &str, line: &str) -> String {
        let (col, len) = locate(msg, line, false).unwrap();
        line.chars().skip(col).take(len).collect()
    }

    #[test]
    fn finds_the_part_the_message_is_about() {
        assert_eq!(under("undefined symbol 'NOWHERE'", "    jmp nowhere ; go"), "nowhere");
        assert_eq!(under("unknown instruction 'FOO'", "start: foo dx"), "foo");
        assert_eq!(under("MOV expects 2 operand(s), got 3", "    mov ax, bx, cx"), "ax, bx, cx");
        assert_eq!(under("value 300 out of byte range", "\tmov al, 300"), "300");
        assert_eq!(under("destination cannot be a constant", "mov 5, ax"), "5");
        assert_eq!(under("value 1000 does not fit in a byte", "x db 1, 1000 ; data"), "1000");
        assert_eq!(under("invalid MOV operands", "rep movsb"), "rep movsb");
        assert_eq!(under("expected ')'", "x dw 3 dup (1"), "x dw 3 dup (1");
        assert_eq!(under("symbol 'X' already defined (line 2)", "x: mov ax, 'x'"), "x");
        assert_eq!(under("invalid operands for ADD", "add [bx], [si]"), "[bx], [si]");
        // Non-ASCII text before the error: columns count characters.
        assert_eq!(locate("undefined symbol 'B'", "mov al, 'é' + b", false), Some((14, 1)));
        assert_eq!(locate("anything", "  call_macro 1, 2", true), Some((2, 15)));
        assert_eq!(locate("anything", "   ; only a comment", false), None);
    }

    #[test]
    fn parse_positions_cover_the_token() {
        assert_eq!(token_at("mov ax, 12q4", 8), Some((8, 4)));
        assert_eq!(token_at("db 'abc", 3), Some((3, 4)));
        assert_eq!(token_at("mov ax, #", 8), Some((8, 1)));
    }
}
