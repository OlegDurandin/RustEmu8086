//! A86 front end. The shared assembler still owns layout, encoding and resource limits.
use std::collections::HashMap;

use crate::expr::{BinOp, Expr, Parser, Reg, UnOp};
use crate::lexer::{Tok, tokenize_a86};
use crate::parse::{DataItem, Operand, PErr, Stmt, StmtKind, parse_tokens_mode};
use crate::source::Line;
use crate::{AsmError, Diagnostics};

struct Expansion {
    lines: Vec<Line>,
    bytes: usize,
    work: usize,
    cancel: Option<crate::CancelFlag>,
    error: Option<String>,
}
impl Expansion {
    fn new(cancel: Option<crate::CancelFlag>) -> Self {
        Self { lines: Vec::new(), bytes: 0, work: 0, cancel, error: None }
    }
    fn tick(&mut self) -> Result<(), String> {
        if self.cancel.as_ref().is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err("Assembly cancelled".into());
        }
        self.work += 1;
        if self.work > crate::source::MAX_EXPANDED_LINES {
            return Err("A86 expansion work budget exceeded".into());
        }
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        Ok(())
    }
    fn push(&mut self, line: Line) {
        use rustemu_target::resources::{Resource, ResourceBudget};
        if self.error.is_some() {
            return;
        }
        if self.lines.len() >= crate::source::MAX_EXPANDED_LINES
            || ResourceBudget::check(Resource::ExpandedBytes, self.bytes, line.text.len()).is_err()
        {
            self.error = Some("A86 expanded source budget exceeded".into());
            return;
        }
        self.bytes += line.text.len();
        self.lines.push(line);
    }
    fn len(&self) -> usize {
        self.lines.len()
    }
    fn finish(self) -> Result<Vec<Line>, String> {
        self.error.map_or(Ok(self.lines), Err)
    }
}

fn code(line: &str) -> &str {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == ';' {
            return &line[..i];
        }
    }
    line
}

fn local(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() > 1 && b[0].is_ascii_alphabetic() && b[1..].iter().all(u8::is_ascii_digit)
}

/// Constant floating arithmetic is evaluated in 80-bit precision, never through host f64.
pub(crate) fn floating_data(p: &mut Parser<'_>, size: u8) -> Result<Option<DataItem>, PErr> {
    use rustc_apfloat::ieee::X87DoubleExtended as F;
    use rustc_apfloat::{Float, Round};
    let mut end = p.pos;
    let mut nesting = 0;
    while let Some(t) = p.toks.get(end) {
        match &t.tok {
            Tok::P(',') if nesting == 0 => break,
            Tok::P(')') if nesting == 0 => break,
            Tok::P('(') => nesting += 1,
            Tok::P(')') => nesting -= 1,
            _ => {}
        }
        end += 1;
    }
    let tokens = &p.toks[p.pos..end];
    if tokens.iter().any(|t| t.tok.is_ident("DUP")) {
        return Ok(None);
    }
    if !tokens.iter().any(|t| matches!(t.tok, Tok::Float(_))) {
        return Ok(None);
    }
    if !matches!(size, 4 | 8 | 10) {
        return Err((p.col(), "a floating-point number needs DD, DQ or DT".into()));
    }
    if tokens.len() > crate::expr::MAX_OPERANDS as usize {
        return Err((p.col(), "floating expression is too complex".into()));
    }
    fn value(t: &[crate::lexer::Token], i: &mut usize, min: u8, depth: u32) -> Result<F, PErr> {
        if depth > crate::expr::MAX_NESTING {
            return Err((0, "floating expression nesting too deep".into()));
        }
        let token = t.get(*i).ok_or((0, "floating operand expected".into()))?;
        *i += 1;
        let mut left = match &token.tok {
            Tok::Num(n) => {
                F::from_str_r(&n.to_string(), Round::NearestTiesToEven)
                    .map_err(|_| (token.col, "invalid floating constant".into()))?
                    .value
            }
            Tok::Float(s) => {
                F::from_str_r(s, Round::NearestTiesToEven)
                    .map_err(|_| (token.col, "invalid floating constant".into()))?
                    .value
            }
            Tok::P('-') => -value(t, i, 3, depth + 1)?,
            Tok::P('+') => value(t, i, 3, depth + 1)?,
            Tok::P('(') => {
                let v = value(t, i, 0, depth + 1)?;
                if !t.get(*i).is_some_and(|t| t.tok == Tok::P(')')) {
                    return Err((token.col, "unclosed floating expression".into()));
                }
                *i += 1;
                v
            }
            _ => return Err((token.col, "A86 floating arithmetic requires numeric constants".into())),
        };
        while let Some(token) = t.get(*i) {
            let Tok::P(op @ ('+' | '-' | '*' | '/')) = token.tok else { break };
            let prec = if matches!(op, '*' | '/') { 2 } else { 1 };
            if prec < min {
                break;
            }
            *i += 1;
            let right = value(t, i, prec + 1, depth + 1)?;
            if op == '/' && right.is_zero() {
                return Err((token.col, "division by zero in floating expression".into()));
            }
            left = match op {
                '+' => left.add_r(right, Round::NearestTiesToEven).value,
                '-' => left.sub_r(right, Round::NearestTiesToEven).value,
                '*' => left.mul_r(right, Round::NearestTiesToEven).value,
                _ => left.div_r(right, Round::NearestTiesToEven).value,
            };
        }
        Ok(left)
    }
    let mut i = 0;
    let value = value(tokens, &mut i, 0, 0)?;
    if i != tokens.len() {
        return Err((tokens[i].col, "unexpected token in floating expression".into()));
    }
    p.pos = end;
    Ok(Some(DataItem::Float(value.to_string())))
}

pub(crate) fn parse_line(text: &str) -> Result<Stmt, PErr> {
    if text.trim_start().starts_with('#') {
        if code(text).trim().eq_ignore_ascii_case("#a86#") {
            return crate::parse::parse_line(text);
        }
        return Err((0, "unsupported A86 macro directive".into()));
    }
    let mut toks = tokenize_a86(text)?;
    for t in &mut toks {
        if let Tok::Ident(name) = &mut t.tok {
            if name.eq_ignore_ascii_case("this") {
                t.tok = Tok::P('$');
            } else if name.eq_ignore_ascii_case("end") {
                *name = "__A86_END".into();
            } else if name.eq_ignore_ascii_case("equ") {
                t.tok = Tok::P('=');
            }
        }
    }
    // A386 is a different processor target, even though it also runs in real mode.
    for t in &toks {
        if let Tok::Ident(s) = &t.tok
            && ["eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp", "a32", "o32", "a4", "o4", "fs", "gs"]
                .iter()
                .any(|r| s.eq_ignore_ascii_case(r))
        {
            return Err((t.col, format!("A386 operand '{s}' requires a 386 target; the A86 target is 8086/80186")));
        }
    }
    let prefix_at = if toks.get(1).is_some_and(|t| t.tok == Tok::P(':')) { 2 } else { 0 };
    let segment = toks.get(prefix_at).and_then(|t| match &t.tok {
        Tok::Ident(s) => match crate::expr::parse_reg(s) {
            Some(Reg::Seg(n)) => Some(n),
            _ => None,
        },
        _ => None,
    });
    if segment.is_some() && toks.get(prefix_at + 1).is_some_and(|t| matches!(t.tok, Tok::Ident(_))) {
        toks.remove(prefix_at);
    }
    let mut stmt = parse_tokens_mode(&toks, true)?;
    if let StmtKind::Instr { prefixes, mnem, ops } = &mut stmt.kind {
        if let Some(seg) = segment {
            prefixes.insert(0, ["ES", "CS", "SS", "DS"][seg as usize].into());
        }
        // A86 uses 0..7 as concise FPU stack register names (ST(0)..ST(7)).
        if mnem.starts_with('F') {
            for op in ops.iter_mut() {
                if let Operand::Expr(Expr::Num(n @ 0..=7)) = op {
                    *op = Operand::Expr(Expr::Reg(Reg::St(*n as u8)));
                }
            }
            if matches!(mnem.as_str(), "FADD" | "FSUB" | "FSUBR" | "FMUL" | "FDIV" | "FDIVR") {
                if ops.is_empty() {
                    mnem.push('P');
                } else if ops.len() == 1 && matches!(ops[0], Operand::Expr(Expr::Reg(Reg::St(_)))) {
                    ops.insert(0, Operand::Expr(Expr::Reg(Reg::St(0))));
                }
            }
        }
    }
    Ok(stmt)
}

/// Resolve repeated local labels after macro expansion. Replacements retain original line
/// numbers and file identities; diagnostic columns are recovered from the original sources.
pub(crate) fn prepare(lines: &mut Vec<Line>, errors: &mut Vec<AsmError>, cancel: Option<crate::CancelFlag>) {
    let mut expanded = Expansion::new(cancel);
    let mut structure: Option<(String, Expr)> = None;
    let mut active_segment = false;
    let segments: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            let w: Vec<_> = code(&l.text).split_whitespace().collect();
            (w.len() > 1 && w[1].eq_ignore_ascii_case("segment")).then(|| w[0].to_ascii_uppercase())
        })
        .collect();
    for (i, l) in lines.iter().enumerate() {
        if let Err(msg) = expanded.tick() {
            errors.push_diagnostic(AsmError { file: l.file, line: l.line, col: None, len: 0, msg });
            return;
        }
        let text = code(&l.text).trim();
        let words: Vec<&str> = text.split_whitespace().collect();
        if text.eq_ignore_ascii_case("enter") {
            for s in ["push bp", "mov bp,sp"] {
                expanded.push(Line { text: s.into(), ..l.clone() });
            }
            continue;
        }
        if words.get(1).is_some_and(|s| s.eq_ignore_ascii_case("segment")) {
            if active_segment {
                expanded.push(Line { text: "ends".into(), ..l.clone() });
            }
            active_segment = true;
        }
        if structure.is_none() && words.iter().any(|w| w.eq_ignore_ascii_case("ends")) {
            active_segment = false;
        }
        if words.first().is_some_and(|s| s.eq_ignore_ascii_case("struc")) {
            if structure.is_some() {
                errors.push_diagnostic(AsmError {
                    file: l.file,
                    line: l.line,
                    col: None,
                    len: 0,
                    msg: "nested A86 STRUC".into(),
                });
            }
            let base = text[5..].trim().trim_matches(['[', ']']);
            structure = Some((if base.is_empty() { "0".into() } else { format!("offset {base}") }, Expr::Num(0)));
            continue;
        }
        if let Some((base, offset)) = &mut structure {
            if words.iter().any(|w| w.eq_ignore_ascii_case("ends")) {
                if words.len() > 1 {
                    expanded.push(Line { text: format!("{} equ {}", words[0], expression(offset)), ..l.clone() });
                }
                structure = None;
                continue;
            }
            match parse_line(text) {
                Ok(Stmt { label: Some(name), kind: StmtKind::Data { size, items }, .. }) => {
                    let ty = match size {
                        1 => "byte",
                        2 => "word",
                        4 => "dword",
                        8 => "qword",
                        _ => "tbyte",
                    };
                    expanded.push(Line {
                        text: format!("{name} equ {ty} ptr [({base})+({})]", expression(offset)),
                        ..l.clone()
                    });
                    let next = format!("__A86_STRUCT_OFFSET_{i}");
                    expanded.push(Line {
                        text: format!(
                            "{next} equ ({})+(({})*{size})",
                            expression(offset),
                            expression(&item_count(&items))
                        ),
                        ..l.clone()
                    });
                    *offset = Expr::Sym(next);
                }
                Ok(Stmt { kind: StmtKind::Empty, .. }) => {}
                _ => errors.push_diagnostic(AsmError {
                    file: l.file,
                    line: l.line,
                    col: None,
                    len: 0,
                    msg: "A86 STRUC expects named data fields or ENDS".into(),
                }),
            }
            continue;
        }
        // A86's repeated PUSH/POP, repeated string instructions and chained MOV shorthand.
        if let Ok(Stmt { label, kind: StmtKind::Instr { mnem, ops, .. }, .. }) = parse_line(text) {
            let start = if label.is_some() { text.find(':').map(|p| p + 1).unwrap_or(0) } else { 0 };
            let instruction = text[start..].trim();
            let rest = instruction.split_once(char::is_whitespace).map(|(_, s)| s.trim()).unwrap_or("");
            if mnem == "ENTER" && ops.len() <= 1 {
                if let Some(label) = label {
                    expanded.push(Line { text: format!("{label}:"), ..l.clone() });
                }
                for s in ["push bp", "mov bp,sp"] {
                    expanded.push(Line { text: s.into(), ..l.clone() });
                }
                if !rest.is_empty() {
                    expanded.push(Line { text: format!("sub sp,{rest}"), ..l.clone() });
                }
                continue;
            }
            if mnem == "MOV"
                && ops.len() == 2
                && let [Operand::Expr(Expr::Reg(Reg::Seg(dest))), Operand::Expr(source)] = ops.as_slice()
            {
                let parts: Vec<_> = crate::source::split_args(rest);
                if parts.len() == 2 {
                    let sequence = match source {
                        Expr::Reg(Reg::Seg(_)) => Some(vec![format!("push {}", parts[1]), format!("pop {}", parts[0])]),
                        Expr::Num(_) => Some(vec![
                            "push ax".into(),
                            format!("mov ax,{}", parts[1]),
                            format!("mov {},ax", parts[0]),
                            "pop ax".into(),
                        ]),
                        Expr::Sym(s) if segments.contains(s) => Some(vec![
                            "push ax".into(),
                            format!("mov ax,{}", parts[1]),
                            format!("mov {},ax", parts[0]),
                            "pop ax".into(),
                        ]),
                        _ => None,
                    };
                    if let Some(sequence) = sequence {
                        if *dest == 1 {
                            errors.push_diagnostic(AsmError {
                                file: l.file,
                                line: l.line,
                                col: None,
                                len: 0,
                                msg: "A86 MOV cannot load CS".into(),
                            });
                            continue;
                        }
                        if let Some(label) = label {
                            expanded.push(Line { text: format!("{label}:"), ..l.clone() });
                        }
                        for s in sequence {
                            expanded.push(Line { text: s, ..l.clone() });
                        }
                        continue;
                    }
                }
            }
            if mnem == "TEST" && ops.len() == 1 {
                if let Some(label) = label {
                    expanded.push(Line { text: format!("{label}:"), ..l.clone() });
                }
                expanded.push(Line { text: format!("cmp {rest},0"), ..l.clone() });
                continue;
            }
            if matches!(mnem.as_str(), "PUSH" | "POP" | "INC" | "DEC") && ops.len() > 1 {
                if let Some(label) = label {
                    expanded.push(Line { text: format!("{label}:"), ..l.clone() });
                }
                let mut operands = crate::source::split_args(rest);
                let count = if matches!(mnem.as_str(), "INC" | "DEC") {
                    if let Some(Operand::Expr(Expr::Num(n))) = ops.last() {
                        if !(0..=65536).contains(n) {
                            errors.push_diagnostic(AsmError {
                                file: l.file,
                                line: l.line,
                                col: None,
                                len: 0,
                                msg: "A86 instruction repeat count out of range".into(),
                            });
                            continue;
                        }
                        operands.pop();
                        *n as usize
                    } else {
                        1
                    }
                } else {
                    1
                };
                for _ in 0..count {
                    expanded.tick().unwrap_or_else(|e| expanded.error = Some(e));
                    for operand in &operands {
                        expanded.push(Line { text: format!("{mnem} {operand}"), ..l.clone() });
                    }
                }
                continue;
            }
            if mnem == "MOV" && ops.len() > 2 {
                let operands: Vec<_> = crate::source::split_args(rest);
                if let Some(label) = label {
                    expanded.push(Line { text: format!("{label}:"), ..l.clone() });
                }
                if operands.len() != 3 {
                    errors.push_diagnostic(AsmError {
                        file: l.file,
                        line: l.line,
                        col: None,
                        len: 0,
                        msg: "A86 MOV accepts two or three operands".into(),
                    });
                } else {
                    expanded.push(Line { text: format!("mov {},{}", operands[1], operands[2]), ..l.clone() });
                    expanded.push(Line { text: format!("mov {},{}", operands[0], operands[1]), ..l.clone() });
                }
                continue;
            }
            if matches!(mnem.as_str(), "STOSB" | "STOSW" | "LODSB" | "LODSW" | "MOVSB" | "MOVSW")
                && ops.len() == 1
                && let Operand::Expr(Expr::Num(n)) = ops[0]
                && (0..=65536).contains(&n)
            {
                if let Some(label) = label {
                    expanded.push(Line { text: format!("{label}:"), ..l.clone() });
                }
                for _ in 0..n {
                    expanded.push(Line { text: mnem.clone(), ..l.clone() });
                }
                continue;
            }
        }
        // Immediate floating constants are emitted inline behind a jump, exactly as A86
        // documents FLD constant; CS makes the pool independent of the data segment.
        if words.first().is_some_and(|w| w.eq_ignore_ascii_case("fld"))
            && let Ok(t) = tokenize_a86(text)
            && t.iter().any(|t| matches!(t.tok, Tok::Float(_)))
        {
            let value = text[3..].trim();
            for s in [
                format!("cs fld tbyte ptr [__A86_FLOAT_{i}]"),
                format!("jmp __A86_FLOAT_END_{i}"),
                format!("__A86_FLOAT_{i} dt {value}"),
                format!("__A86_FLOAT_END_{i}:"),
            ] {
                expanded.push(Line { text: s, ..l.clone() });
            }
            continue;
        }
        expanded.push(l.clone());
    }
    if active_segment && let Some(l) = lines.last() {
        expanded.push(Line { text: "ends".into(), ..l.clone() });
    }
    if structure.is_some()
        && let Some(l) = lines.last()
    {
        errors.push_diagnostic(AsmError {
            file: l.file,
            line: l.line,
            col: None,
            len: 0,
            msg: "A86 STRUC without ENDS".into(),
        });
    }
    match expanded.finish() {
        Ok(v) => *lines = v,
        Err(msg) => {
            errors.push_diagnostic(AsmError { file: 0, line: 0, col: None, len: 0, msg });
            return;
        }
    }
    type Scope = (String, usize, u32, Vec<(usize, u32)>);
    let mut defs: HashMap<Scope, Vec<usize>> = HashMap::new();
    let mut all_defs: HashMap<String, Vec<usize>> = HashMap::new();
    let scope = |l: &Line| if l.macro_depth > 0 { l.line } else { 0 };
    let file_scope = |l: &Line| if l.macro_depth > 0 { l.file } else { usize::MAX };
    let chain_scope = |l: &Line| l.chain[..l.chain.len().saturating_sub(1)].to_vec();
    for (i, l) in lines.iter().enumerate() {
        let Ok(t) = tokenize_a86(&l.text) else { continue };
        if let Some(Tok::Ident(n)) = t.first().map(|t| &t.tok)
            && local(n)
            && t.get(1).is_some_and(|t| {
                matches!(t.tok, Tok::P(':')) || ["db", "dw", "dd", "dq", "dt", "equ"].iter().any(|d| t.tok.is_ident(d))
            })
        {
            let name = n.to_ascii_uppercase();
            defs.entry((name.clone(), file_scope(l), scope(l), chain_scope(l))).or_default().push(i);
            all_defs.entry(name).or_default().push(i);
        }
    }
    let mut explicit_end = false;
    for (i, l) in lines.iter_mut().enumerate() {
        let Ok(t) = tokenize_a86(&l.text) else { continue };
        explicit_end |= t.first().is_some_and(|t| t.tok.is_ident("end"))
            && t.get(1).is_some_and(|t| {
                matches!(t.tok, Tok::P(':')) || ["db", "dw", "dd", "equ"].iter().any(|d| t.tok.is_ident(d))
            });
        let mut replacements = Vec::new();
        for (k, token) in t.iter().enumerate() {
            let Tok::Ident(n) = &token.tok else { continue };
            let name = n.to_ascii_uppercase();
            let positions = defs
                .get(&(name.clone(), file_scope(l), scope(l), chain_scope(l)))
                .or_else(|| all_defs.get(&name))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if positions.is_empty() {
                continue;
            }
            let forward = k > 0 && t[k - 1].tok == Tok::P('>');
            let position = if forward {
                positions.get(positions.partition_point(|&p| p <= i))
            } else {
                positions
                    .partition_point(|&p| p <= i)
                    .checked_sub(1)
                    .and_then(|p| positions.get(p))
                    .or_else(|| positions.first())
            };
            if let Some(&position) = position {
                let start = if forward { t[k - 1].col } else { token.col };
                replacements.push((
                    start,
                    token.col + n.len(),
                    format!("{}??A86_{}", n.to_ascii_uppercase(), position),
                ));
            } else {
                errors.push_diagnostic(AsmError {
                    file: l.file,
                    line: l.line,
                    col: Some(token.col),
                    len: n.len(),
                    msg: format!("no forward definition of A86 local label '{n}'"),
                });
            }
        }
        for (start, end, value) in replacements.into_iter().rev() {
            l.text.replace_range(start..end, &value);
        }
    }
    if !explicit_end && let Some(l) = lines.last().cloned() {
        lines.push(Line { text: "__A86_END:".into(), ..l });
    }
}

fn item_count(items: &[DataItem]) -> Expr {
    items.iter().fold(Expr::Num(0), |sum, item| {
        let n = match item {
            DataItem::Dup(n, items) => Expr::Bin(BinOp::Mul, Box::new(n.clone()), Box::new(item_count(items))),
            DataItem::Str(s) => Expr::Num(s.len() as i64),
            _ => Expr::Num(1),
        };
        Expr::Bin(BinOp::Add, Box::new(sum), Box::new(n))
    })
}

fn expression(e: &Expr) -> String {
    match e {
        Expr::Num(n) => n.to_string(),
        Expr::Sym(s) => s.clone(),
        Expr::Here => "$".into(),
        Expr::Bin(op, a, b) => format!(
            "({} {} {})",
            expression(a),
            match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "mod",
                BinOp::Shl => "shl",
                BinOp::Shr => "shr",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Xor => "xor",
                BinOp::Eq => "eq",
                BinOp::Ne => "ne",
                BinOp::Lt => "lt",
                BinOp::Le => "le",
                BinOp::Gt => "gt",
                BinOp::Ge => "ge",
            },
            expression(b)
        ),
        Expr::Un(op, a) => format!(
            "({} {})",
            match op {
                UnOp::Neg => "-",
                UnOp::Not => "not",
                UnOp::Offset => "offset",
                UnOp::Seg => "seg",
                UnOp::Type => "type",
                UnOp::Length => "length",
                UnOp::Size => "size",
                UnOp::Low => "low",
                UnOp::High => "high",
            },
            expression(a)
        ),
        _ => "__A86_UNSUPPORTED_STRUCTURE_EXPRESSION".into(),
    }
}

/// Expand A86 positional parameters and bounded operand/counter loops. The preprocessor
/// subsequently applies its normal macro recursion, cancellation and source budgets.
pub(crate) fn expand_macro(
    body: &[Line],
    args: &[String],
    depth: u32,
    call: &Line,
    cancel: Option<crate::CancelFlag>,
) -> Result<Vec<Line>, String> {
    fn substitution(s: &str, args: &[String], vars: &HashMap<char, usize>) -> Result<String, String> {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '#' {
                result.push(c);
                continue;
            }
            let Some(c) = chars.next() else { return Err("trailing '#' in A86 macro".into()) };
            if c == '#' {
                result.push('#');
                continue;
            }
            let (mode, key) = if matches!(c.to_ascii_uppercase(), 'N' | 'S' | 'V') {
                (c.to_ascii_uppercase(), chars.next().ok_or("missing A86 macro parameter")?)
            } else {
                (' ', c)
            };
            let n = if let Some(d) = key.to_digit(10) {
                d as usize
            } else if key.eq_ignore_ascii_case(&'L') {
                args.len()
            } else {
                *vars.get(&key.to_ascii_uppercase()).ok_or_else(|| format!("undefined A86 macro parameter #{key}"))?
            };
            let arg = n.checked_sub(1).and_then(|n| args.get(n)).map(String::as_str).unwrap_or("");
            match mode {
                'N' => result.push_str(&n.to_string()),
                'S' => result.push_str(&arg.len().to_string()),
                _ => result.push_str(arg),
            }
        }
        Ok(result)
    }
    fn expand(
        body: &[Line],
        args: &[String],
        vars: &mut HashMap<char, usize>,
        level: usize,
        depth: u32,
        call: &Line,
        out: &mut Expansion,
    ) -> Result<(), String> {
        if level > 32 {
            return Err("A86 macro loop nesting too deep".into());
        }
        let mut i = 0;
        while i < body.len() {
            out.tick()?;
            let l = &body[i];
            i += 1;
            let c = code(&l.text).trim();
            let u = c.to_ascii_uppercase();
            if u.starts_with("#R") || u.starts_with("#Q") {
                let bytes = u.as_bytes();
                if bytes.len() < 5 || !matches!(bytes[2], b'W' | b'X' | b'Y' | b'Z') {
                    return Err(format!("unsupported A86 macro loop '{c}'"));
                }
                let variable = bytes[2] as char;
                fn bound(s: &str, last: usize) -> Result<(usize, usize), String> {
                    let c = *s.as_bytes().first().ok_or("missing A86 loop bound")?;
                    if c == b'B' {
                        let (n, used) = bound(&s[1..], last)?;
                        return Ok((n.saturating_sub(1), used + 1));
                    }
                    if c == b'L' {
                        return Ok((last, 1));
                    }
                    if c == b'(' {
                        let end = s.find(')').ok_or("unclosed A86 loop count")?;
                        return Ok((s[1..end].parse().map_err(|_| "invalid A86 loop count")?, end + 1));
                    }
                    Ok(((c as char).to_digit(10).ok_or("invalid A86 macro loop bound")? as usize, 1))
                }
                let (start, used) = bound(&u[3..], args.len())?;
                let (end, used_end) = bound(&u[3 + used..], args.len())?;
                if 3 + used + used_end != u.len() {
                    return Err("unsupported trailing text in A86 macro loop".into());
                }
                if end > 65536 {
                    return Err("A86 macro loop count exceeds resource budget".into());
                }
                let mut nested = 0;
                let begin = i;
                while i < body.len() {
                    let u = code(&body[i].text).trim().to_ascii_uppercase();
                    if u.starts_with("#R") || u.starts_with("#Q") {
                        nested += 1;
                    }
                    if u == "#ER" {
                        if nested == 0 {
                            break;
                        }
                        nested -= 1;
                    }
                    i += 1;
                }
                let end_body = i;
                i += usize::from(i < body.len()); // #EM implicitly closes the final loop.
                let previous = vars.get(&variable).copied();
                let values: Vec<usize> =
                    if bytes[1] == b'Q' { (end..=start).rev().collect() } else { (start..=end).collect() };
                for n in values {
                    out.tick()?;
                    vars.insert(variable, n);
                    expand(&body[begin..end_body], args, vars, level + 1, depth, call, out)?;
                }
                if let Some(n) = previous {
                    vars.insert(variable, n);
                } else {
                    vars.remove(&variable);
                }
                continue;
            }
            let text = substitution(code(&l.text), args, vars)?;
            let upper = text.trim_start().to_ascii_uppercase();
            let text = if upper.starts_with("#IF") || upper.starts_with("#ELSE") || upper.starts_with("#ENDIF") {
                text.replacen('#', "", 1)
            } else {
                text
            };
            if out.len() >= crate::source::MAX_EXPANDED_LINES {
                return Err("expanded source line budget exceeded".into());
            }
            out.push(Line {
                text,
                file: call.file,
                line: call.line,
                macro_depth: depth,
                chain: {
                    let mut c = call.chain.clone();
                    c.push(l.origin().unwrap_or((l.file, l.line)));
                    c
                },
            });
        }
        Ok(())
    }
    let mut out = Expansion::new(cancel);
    expand(body, args, &mut HashMap::new(), 0, depth, call, &mut out)?;
    out.finish()
}
