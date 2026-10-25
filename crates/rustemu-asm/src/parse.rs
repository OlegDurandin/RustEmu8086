//! Statement parser: one source line -> [`Stmt`].

use crate::expr::{Expr, Parser, describe, parse_reg};
use crate::lexer::{Tok, Token, tokenize};

#[derive(Debug, Clone, PartialEq)]
pub enum DataItem {
    Expr(Expr),
    /// String in DB (each char a byte).
    Str(Vec<u8>),
    /// `?` - uninitialized (emitted as zero).
    Uninit,
    Dup(Expr, Vec<DataItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Expr(Expr),
    /// `seg:off` immediate far address, e.g. `JMP 0F000h:0100h`.
    Far(Expr, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Empty,
    Instr {
        prefixes: Vec<String>,
        mnem: String,
        ops: Vec<Operand>,
    },
    /// DB/DW/DD/DQ/DT; `size` is the element size in bytes.
    Data {
        size: u8,
        items: Vec<DataItem>,
    },
    /// `name EQU expr` (not redefinable)
    Equ(String, Expr),
    /// `name = expr` (redefinable)
    Assign(String, Expr),
    Org(Expr),
    Segment {
        name: String,
        stack: bool,
    },
    Ends,
    Proc {
        name: String,
        far: bool,
    },
    Endp,
    /// `name LABEL BYTE|WORD|DWORD|NEAR|FAR`
    Label {
        name: String,
        size: Option<u8>,
        far: bool,
    },
    End(Option<Expr>),
    Even,
    Align(Expr),
    /// `.model`, `.data`, `.code`, `.stack [n]`
    Model,
    DotSeg(DotSeg),
    /// Emu8086 `#directive#` (contents without the hashes).
    Emu(String),
    /// Directives that are accepted and ignored (ASSUME, NAME, TITLE, PUBLIC, ...).
    Ignored,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DotSeg {
    Code,
    Data,
    Stack(Option<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    /// Label defined at this statement (`name:`), uppercased.
    pub label: Option<String>,
    pub kind: StmtKind,
    /// The label was written with a colon (`msg: db ...`): it is a plain address label, not a
    /// typed variable (Emu8086 semantics: `mov dx, msg` loads the offset).
    pub colon: bool,
}

pub type PErr = (usize, String);

const DATA_DIRS: [(&str, u8); 5] = [("DB", 1), ("DW", 2), ("DD", 4), ("DQ", 8), ("DT", 10)];

/// Keywords that, as the second word of a line, mean the first word is a name being defined.
fn is_naming_directive(u: &str) -> bool {
    DATA_DIRS.iter().any(|(d, _)| *d == u)
        || matches!(u, "EQU" | "SEGMENT" | "ENDS" | "PROC" | "ENDP" | "LABEL" | "MACRO" | "GROUP" | "STRUC")
}

pub const PREFIXES: [&str; 6] = ["REP", "REPE", "REPZ", "REPNE", "REPNZ", "LOCK"];

pub fn parse_line(line: &str) -> Result<Stmt, PErr> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        let body = trimmed.trim_start_matches('#');
        let body = body.split('#').next().unwrap_or("").trim().to_string();
        return Ok(Stmt { label: None, kind: StmtKind::Emu(body), colon: false });
    }
    let toks = tokenize(line)?;
    parse_tokens(&toks)
}

pub fn parse_tokens(toks: &[Token]) -> Result<Stmt, PErr> {
    let mut p = Parser::new(toks);
    let mut label = None;
    let mut colon = false;

    // `name:` label
    if let (Some(Tok::Ident(id)), Some(Tok::P(':'))) = (p.peek(), p.peek_at(1))
        && parse_reg(id).is_none()
    {
        label = Some(id.to_ascii_uppercase());
        colon = true;
        p.pos += 2;
    }
    if p.at_end() {
        return Ok(Stmt { colon: false, label, kind: StmtKind::Empty });
    }

    // `name DIRECTIVE ...`
    if let (Some(Tok::Ident(name)), Some(Tok::Ident(dir))) = (p.peek(), p.peek_at(1)) {
        let du = dir.to_ascii_uppercase();
        if is_naming_directive(&du) && label.is_none() {
            let name = name.to_ascii_uppercase();
            p.pos += 2;
            let kind = named_directive(&mut p, name.clone(), &du)?;
            // Data definitions carry their name as the statement label.
            let label = match &kind {
                StmtKind::Data { .. } => Some(name),
                _ => None,
            };
            return finish(&p, Stmt { label, kind, colon: false });
        }
    }
    if let (Some(Tok::Ident(name)), Some(Tok::P('='))) = (p.peek(), p.peek_at(1)) {
        let name = name.to_ascii_uppercase();
        p.pos += 2;
        let e = p.expr()?;
        return finish(&p, Stmt { colon: false, label, kind: StmtKind::Assign(name, e) });
    }

    let Some(Tok::Ident(word)) = p.advance() else {
        return Err((toks[p.pos - 1].col, format!("unexpected {}", describe(&toks[p.pos - 1].tok))));
    };
    let u = word.to_ascii_uppercase();

    if let Some(&(_, size)) = DATA_DIRS.iter().find(|(d, _)| *d == u) {
        let items = data_items(&mut p, size)?;
        return finish(&p, Stmt { colon, label, kind: StmtKind::Data { size, items } });
    }

    let kind = match u.as_str() {
        "ORG" => StmtKind::Org(p.expr()?),
        "END" => StmtKind::End(if p.at_end() { None } else { Some(p.expr()?) }),
        "EVEN" => StmtKind::Even,
        "ALIGN" => StmtKind::Align(p.expr()?),
        "ENDS" => {
            p.pos = toks.len();
            StmtKind::Ends
        }
        "ENDP" => StmtKind::Endp,
        ".MODEL" | ".8086" | ".186" | ".286" | "ASSUME" | "NAME" | "TITLE" | "PUBLIC" | "EXTRN" | "PAGE" | "SUBTTL"
        | ".LIST" | ".NOLIST" | "JUMPS" | "LOCALS" | "IDEAL" => {
            p.pos = toks.len();
            if u == ".MODEL" { StmtKind::Model } else { StmtKind::Ignored }
        }
        ".CODE" => {
            p.pos = toks.len();
            StmtKind::DotSeg(DotSeg::Code)
        }
        ".DATA" | ".DATA?" | ".CONST" | ".FARDATA" => {
            p.pos = toks.len();
            StmtKind::DotSeg(DotSeg::Data)
        }
        ".STACK" => StmtKind::DotSeg(DotSeg::Stack(if p.at_end() { None } else { Some(p.expr()?) })),
        _ => {
            let mut prefixes = Vec::new();
            let mut mnem = u;
            while PREFIXES.contains(&mnem.as_str()) {
                match p.peek() {
                    Some(Tok::Ident(next)) => {
                        prefixes.push(mnem);
                        mnem = next.to_ascii_uppercase();
                        p.pos += 1;
                    }
                    _ => break,
                }
            }
            let ops = operands(&mut p)?;
            StmtKind::Instr { prefixes, mnem, ops }
        }
    };
    finish(&p, Stmt { label, kind, colon: false })
}

fn finish(p: &Parser, s: Stmt) -> Result<Stmt, PErr> {
    if !p.at_end() {
        return Err((p.col(), format!("unexpected {}", describe(p.peek().unwrap()))));
    }
    Ok(s)
}

fn named_directive(p: &mut Parser, name: String, dir: &str) -> Result<StmtKind, PErr> {
    if let Some(&(_, size)) = DATA_DIRS.iter().find(|(d, _)| *d == dir) {
        return Ok(StmtKind::Data { size, items: data_items(p, size)? });
    }
    Ok(match dir {
        "EQU" => StmtKind::Equ(name, p.expr()?),
        "SEGMENT" => {
            let mut stack = name == "STACK";
            while let Some(t) = p.advance() {
                if t.is_ident("STACK") {
                    stack = true;
                }
                if let Tok::Str(s) = &t
                    && s.eq_ignore_ascii_case(b"STACK")
                {
                    stack = true;
                }
            }
            StmtKind::Segment { name, stack }
        }
        "ENDS" => StmtKind::Ends,
        "PROC" => {
            let far = p.eat_kw("FAR");
            if !far {
                p.eat_kw("NEAR");
            }
            // Ignore calling conventions / USES lists.
            p.pos = p.toks.len();
            StmtKind::Proc { name, far }
        }
        "ENDP" => StmtKind::Endp,
        "LABEL" => {
            let t = p.advance().ok_or((p.col(), "type expected after LABEL".to_string()))?;
            let (size, far) = match t {
                Tok::Ident(s) => match s.to_ascii_uppercase().as_str() {
                    "BYTE" => (Some(1), false),
                    "WORD" => (Some(2), false),
                    "DWORD" => (Some(4), false),
                    "NEAR" => (None, false),
                    "FAR" => (None, true),
                    _ => return Err((p.col(), format!("unknown LABEL type '{s}'"))),
                },
                _ => return Err((p.col(), "type expected after LABEL".into())),
            };
            StmtKind::Label { name, size, far }
        }
        _ => {
            p.pos = p.toks.len();
            StmtKind::Ignored
        }
    })
}

fn data_items(p: &mut Parser, size: u8) -> Result<Vec<DataItem>, PErr> {
    let mut items = Vec::new();
    if p.at_end() {
        return Err((p.col(), "data expected".into()));
    }
    loop {
        items.push(data_item(p, size)?);
        // A trailing comma at the end of the line is tolerated.
        if !p.eat_p(',') || p.at_end() {
            break;
        }
    }
    Ok(items)
}

fn data_item(p: &mut Parser, size: u8) -> Result<DataItem, PErr> {
    if p.peek() == Some(&Tok::P('?')) && matches!(p.peek_at(1), None | Some(Tok::P(',')) | Some(Tok::P(')'))) {
        p.pos += 1;
        return Ok(DataItem::Uninit);
    }
    if let Some(Tok::Str(s)) = p.peek()
        && matches!(p.peek_at(1), None | Some(Tok::P(',')) | Some(Tok::P(')')))
        && (size == 1 || s.len() > 2)
    {
        let s = s.clone();
        p.pos += 1;
        return Ok(DataItem::Str(s));
    }
    let e = p.expr()?;
    if p.eat_kw("DUP") {
        if !p.eat_p('(') {
            return Err((p.col(), "expected '(' after DUP".into()));
        }
        let mut inner = Vec::new();
        loop {
            inner.push(data_item(p, size)?);
            if !p.eat_p(',') {
                break;
            }
        }
        if !p.eat_p(')') {
            return Err((p.col(), "expected ')'".into()));
        }
        return Ok(DataItem::Dup(e, inner));
    }
    Ok(DataItem::Expr(e))
}

fn operands(p: &mut Parser) -> Result<Vec<Operand>, PErr> {
    let mut ops = Vec::new();
    if p.at_end() {
        return Ok(ops);
    }
    loop {
        let e = p.expr()?;
        // `seg:off` far immediate (segment registers are handled by the expression parser).
        // A colon at the very end (`jne label:`) is tolerated.
        if p.eat_p(':') && !p.at_end() {
            let off = p.expr()?;
            ops.push(Operand::Far(e, off));
        } else {
            ops.push(Operand::Expr(e));
        }
        if !p.eat_p(',') {
            break;
        }
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Reg;

    #[test]
    fn labels_and_instr() {
        let s = parse_line("start: mov ax, [bx+si+2] ; c").unwrap();
        assert_eq!(s.label.as_deref(), Some("START"));
        match s.kind {
            StmtKind::Instr { mnem, ops, .. } => {
                assert_eq!(mnem, "MOV");
                assert_eq!(ops.len(), 2);
                assert_eq!(ops[0], Operand::Expr(Expr::Reg(Reg::R16(0))));
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn data_and_dup() {
        let s = parse_line("msg db 'Hi', 0Dh, 0Ah, '$'").unwrap();
        assert_eq!(s.label.as_deref(), Some("MSG"));
        let StmtKind::Data { size: 1, items } = s.kind else { panic!() };
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], DataItem::Str(b"Hi".to_vec()));
        let s = parse_line("arr dw 10 dup(?), 5").unwrap();
        let StmtKind::Data { size: 2, items } = s.kind else { panic!() };
        assert!(matches!(items[0], DataItem::Dup(_, _)));
    }

    #[test]
    fn prefixes_and_directives() {
        let s = parse_line("rep movsb").unwrap();
        assert_eq!(s.kind, StmtKind::Instr { prefixes: vec!["REP".into()], mnem: "MOVSB".into(), ops: vec![] });
        assert_eq!(parse_line("#make_COM#").unwrap().kind, StmtKind::Emu("make_COM".into()));
        assert!(matches!(parse_line("code segment").unwrap().kind, StmtKind::Segment { .. }));
        assert!(matches!(parse_line("ends").unwrap().kind, StmtKind::Ends));
        assert!(matches!(parse_line("main proc far").unwrap().kind, StmtKind::Proc { far: true, .. }));
        assert!(matches!(parse_line("x = 5").unwrap().kind, StmtKind::Assign(..)));
        assert!(matches!(parse_line("jmp 0F000h:0100h").unwrap().kind, StmtKind::Instr { .. }));
    }
}
