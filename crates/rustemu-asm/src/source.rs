//! Source preprocessing: INCLUDE, MACRO/ENDM (with LOCAL), REPT, IF/IFDEF/ELSE/ENDIF.

use std::collections::HashMap;

use crate::AsmError;
use crate::expr::{Env, ForwardKind, Parser, SegId, SymVal, eval};
use crate::lexer::tokenize;

/// One logical line after preprocessing.
#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    /// Index into the file table.
    pub file: usize,
    /// 1-based line number in that file (for macro expansions: the invocation line).
    pub line: u32,
    /// Nesting depth of macro expansion (0 = written by the user).
    pub macro_depth: u32,
    /// For lines produced by MACRO/REPT expansion: the (file, line) of the body line in each
    /// macro definition, from the outermost invocation to the innermost macro.
    pub chain: Vec<(usize, u32)>,
}

impl Line {
    /// Body line of the innermost macro this line was expanded from.
    pub fn origin(&self) -> Option<(usize, u32)> {
        self.chain.last().copied()
    }

    /// Chain of an expansion of body line `b` invoked from this line.
    fn expand_chain(&self, b: &Line) -> Vec<(usize, u32)> {
        let mut c = self.chain.clone();
        c.push(b.origin().unwrap_or((b.file, b.line)));
        c
    }
}

/// Resolves INCLUDE file names to contents.
pub trait FileLoader {
    fn load(&mut self, name: &str, from_file: &str) -> Option<(String, String)>;
}

struct Macro {
    params: Vec<(String, Option<String>)>,
    locals: Vec<String>,
    body: Vec<Line>,
}

pub struct Preprocessor<'a> {
    loader: &'a mut dyn FileLoader,
    pub files: Vec<String>,
    macros: HashMap<String, Macro>,
    consts: HashMap<String, i64>,
    defined: std::collections::HashSet<String>,
    local_counter: u32,
    pub errors: Vec<AsmError>,
    pub out: Vec<Line>,
}

/// Words that begin/end blocks we must track while collecting macro bodies.
fn first_words(text: &str) -> (String, String) {
    let mut it = text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|s| !s.is_empty());
    let a = it.next().unwrap_or("").to_ascii_uppercase();
    let b = it.next().unwrap_or("").to_ascii_uppercase();
    (a, b)
}

fn strip_comment(text: &str) -> &str {
    let mut q: Option<char> = None;
    for (i, c) in text.char_indices() {
        match q {
            Some(qc) if c == qc => q = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => q = Some(c),
            None if c == ';' => return &text[..i],
            None => {}
        }
    }
    text
}

/// Splits macro arguments at top-level commas, honouring quotes, (), and <> grouping.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let mut angle = 0;
    let mut q: Option<char> = None;
    for c in s.chars() {
        if let Some(qc) = q {
            cur.push(c);
            if c == qc {
                q = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                q = Some(c);
                cur.push(c);
            }
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            '<' if depth == 0 => {
                if angle > 0 {
                    cur.push(c);
                }
                angle += 1;
            }
            '>' if angle > 0 => {
                angle -= 1;
                if angle > 0 {
                    cur.push(c);
                }
            }
            ',' if depth == 0 && angle == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Replaces whole identifiers (outside strings) using `map`; `&` glues tokens and is removed.
fn substitute(text: &str, map: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut q: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(qc) = q {
            // Inside strings only `&name&` style substitution is done.
            if c == '&' {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let id: String = chars[i + 1..j].iter().collect();
                if let Some(v) = map.get(&id.to_ascii_uppercase()) {
                    out.push_str(v);
                    i = if j < chars.len() && chars[j] == '&' { j + 1 } else { j };
                    continue;
                }
            }
            out.push(c);
            if c == qc {
                q = None;
            }
            i += 1;
            continue;
        }
        if c == ';' {
            break;
        }
        if c == '\'' || c == '"' {
            q = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '@' || c == '?' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || matches!(chars[j], '_' | '@' | '?' | '$')) {
                j += 1;
            }
            let id: String = chars[i..j].iter().collect();
            match map.get(&id.to_ascii_uppercase()) {
                Some(v) => out.push_str(v),
                None => out.push_str(&id),
            }
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            // Numbers like 0Ah must not be treated as identifiers.
            while i < chars.len() && chars[i].is_alphanumeric() {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if c == '&' {
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

struct ConstEnv<'a>(&'a HashMap<String, i64>);

impl Env for ConstEnv<'_> {
    fn lookup(&self, name: &str) -> Option<SymVal> {
        self.0.get(name).map(|v| SymVal::Const(*v))
    }
    fn is_forward(&self, _: &str) -> Option<ForwardKind> {
        None
    }
    fn here(&self) -> (SegId, i64) {
        (0, 0)
    }
    fn seg_paragraph(&self, _: SegId) -> i64 {
        0
    }
}

impl<'a> Preprocessor<'a> {
    pub fn new(loader: &'a mut dyn FileLoader) -> Self {
        Preprocessor {
            loader,
            files: Vec::new(),
            macros: HashMap::new(),
            consts: HashMap::new(),
            defined: Default::default(),
            local_counter: 0,
            errors: Vec::new(),
            out: Vec::new(),
        }
    }

    fn error(&mut self, l: &Line, msg: impl Into<String>) {
        self.errors.push(AsmError { file: l.file, line: l.line, col: None, msg: msg.into() });
    }

    pub fn add_file(&mut self, name: &str, text: &str) {
        let idx = self.files.len();
        self.files.push(name.to_string());
        let lines: Vec<Line> = text
            .lines()
            .enumerate()
            .map(|(i, t)| Line {
                text: t.to_string(),
                file: idx,
                line: i as u32 + 1,
                macro_depth: 0,
                chain: Vec::new(),
            })
            .collect();
        self.process(lines, 0);
    }

    fn eval_const(&self, text: &str) -> Option<i64> {
        let toks = tokenize(text).ok()?;
        let mut p = Parser::new(&toks);
        let e = p.expr().ok()?;
        eval(&e, &ConstEnv(&self.consts)).ok().map(|v| v.num)
    }

    /// Processes lines, expanding macros recursively, and appends the result to `self.out`.
    fn process(&mut self, lines: Vec<Line>, depth: u32) {
        if depth > 64 {
            if let Some(l) = lines.first() {
                self.error(l, "macro nesting too deep");
            }
            return;
        }
        // Conditional assembly stack: (active, any_branch_taken)
        let mut cond: Vec<(bool, bool)> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let l = lines[i].clone();
            i += 1;
            let code = strip_comment(&l.text).trim().to_string();
            let (w1, w2) = first_words(&code);
            let active = cond.iter().all(|c| c.0);

            // Conditionals are evaluated even in inactive regions to keep nesting right.
            match w1.as_str() {
                "IF" | "IFE" | "IFDEF" | "IFNDEF" => {
                    let rest = code[w1.len()..].trim();
                    let v = if !active {
                        false
                    } else {
                        match w1.as_str() {
                            "IFDEF" => self.is_defined(rest),
                            "IFNDEF" => !self.is_defined(rest),
                            _ => {
                                let n = self.eval_const(rest).unwrap_or_else(|| {
                                    self.error(&l, "IF requires a constant expression");
                                    0
                                });
                                (n != 0) == (w1 == "IF")
                            }
                        }
                    };
                    cond.push((v, v));
                    continue;
                }
                "ELSE" => {
                    match cond.last_mut() {
                        Some(c) => {
                            c.0 = !c.1;
                            c.1 = true;
                        }
                        None => self.error(&l, "ELSE without IF"),
                    }
                    // Parent conditions still apply.
                    let n = cond.len();
                    if n > 1 && !cond[..n - 1].iter().all(|c| c.0) {
                        cond[n - 1].0 = false;
                    }
                    continue;
                }
                "ENDIF" => {
                    if cond.pop().is_none() {
                        self.error(&l, "ENDIF without IF");
                    }
                    continue;
                }
                _ => {}
            }
            if !active {
                continue;
            }

            // MACRO definition: `name MACRO params`
            if w2 == "MACRO" {
                let name = code.split_whitespace().next().unwrap().to_ascii_uppercase();
                let rest = code.split_whitespace().skip(2).collect::<Vec<_>>().join(" ");
                let params = split_args(&rest)
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .map(|p| {
                        // `param:=default` or `param:REQ`
                        let mut parts = p.splitn(2, ":=");
                        let n = parts.next().unwrap().split(':').next().unwrap().trim().to_ascii_uppercase();
                        (n, parts.next().map(|d| d.trim().trim_matches(['<', '>']).to_string()))
                    })
                    .collect();
                let (body, next) = self.collect_block(&lines, i, &l);
                i = next;
                let mut locals = Vec::new();
                let mut real_body = Vec::new();
                for b in body {
                    let c = strip_comment(&b.text).trim().to_string();
                    let (bw1, _) = first_words(&c);
                    if bw1 == "LOCAL" {
                        locals
                            .extend(c[5..].split(',').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()));
                    } else {
                        real_body.push(b);
                    }
                }
                self.macros.insert(name, Macro { params, locals, body: real_body });
                continue;
            }

            match w1.as_str() {
                // MASM 6 simplified startup/exit code.
                ".STARTUP" => {
                    for t in ["@STARTUP:", "mov ax, @data", "mov ds, ax"] {
                        self.out.push(Line { text: t.into(), ..l.clone() });
                    }
                    continue;
                }
                ".EXIT" => {
                    let code_arg = code[5..].trim();
                    let first = if code_arg.is_empty() {
                        "mov ax, 4C00h".to_string()
                    } else {
                        format!("mov ax, 4C00h + (({code_arg}) and 0FFh)")
                    };
                    for t in [first.as_str(), "int 21h"] {
                        self.out.push(Line { text: t.into(), ..l.clone() });
                    }
                    continue;
                }
                "INCLUDE" => {
                    let name = code[7..].trim().trim_matches(['\'', '"', '<', '>']).trim().to_string();
                    let from = self.files.get(l.file).cloned().unwrap_or_default();
                    match self.loader.load(&name, &from) {
                        Some((resolved, text)) => {
                            let idx = self.files.len();
                            self.files.push(resolved);
                            let inc: Vec<Line> = text
                                .lines()
                                .enumerate()
                                .map(|(n, t)| Line {
                                    text: t.to_string(),
                                    file: idx,
                                    line: n as u32 + 1,
                                    macro_depth: depth,
                                    chain: Vec::new(),
                                })
                                .collect();
                            self.process(inc, depth);
                        }
                        None => self.error(&l, format!("cannot open include file '{name}'")),
                    }
                    continue;
                }
                "REPT" => {
                    let n = self.eval_const(code[4..].trim()).unwrap_or_else(|| {
                        self.error(&l, "REPT requires a constant count");
                        0
                    });
                    let (body, next) = self.collect_block(&lines, i, &l);
                    i = next;
                    for _ in 0..n.clamp(0, 65536) {
                        let copy = body
                            .iter()
                            .map(|b| Line {
                                file: l.file,
                                line: l.line,
                                macro_depth: depth + 1,
                                chain: l.expand_chain(b),
                                ..b.clone()
                            })
                            .collect();
                        self.process(copy, depth + 1);
                    }
                    continue;
                }
                "PURGE" => {
                    for m in code[5..].split(',') {
                        self.macros.remove(&m.trim().to_ascii_uppercase());
                    }
                    continue;
                }
                _ => {}
            }

            // Track constants/labels for IF/IFDEF.
            let words: Vec<&str> = code.split_whitespace().collect();
            if words.len() >= 3 && (w2 == "EQU" || words[1] == "=") {
                let name = words[0].to_ascii_uppercase();
                let rest = words[2..].join(" ");
                if let Some(v) = self.eval_const(&rest) {
                    self.consts.insert(name.clone(), v);
                }
                self.defined.insert(name);
            } else if !w1.is_empty() {
                let lbl = w1.trim_end_matches(':');
                if w1.ends_with(':') || w2 == "DB" || w2 == "DW" || w2 == "DD" || w2 == "PROC" || w2 == "LABEL" {
                    self.defined.insert(lbl.to_string());
                }
            }

            // Macro invocation (optionally after a label).
            let (label_part, call_part) = split_label(&code);
            let (cw, _) = first_words(call_part);
            if self.macros.contains_key(&cw) {
                let args_text = call_part.trim_start()[cw.len()..].trim();
                let args = split_args(args_text);
                if let Some(lbl) = label_part {
                    self.out.push(Line { text: format!("{lbl}:"), ..l.clone() });
                }
                let m = &self.macros[&cw];
                let mut map = HashMap::new();
                for (k, (p, def)) in m.params.iter().enumerate() {
                    let v = args.get(k).cloned().filter(|a| !a.is_empty()).or_else(|| def.clone()).unwrap_or_default();
                    map.insert(p.clone(), v);
                }
                for loc in &m.locals {
                    self.local_counter += 1;
                    // `name??NNNN`: unique, but the debugger can still show the original name.
                    map.insert(loc.clone(), format!("{loc}??{:04X}", self.local_counter));
                }
                let expanded: Vec<Line> = m
                    .body
                    .iter()
                    .map(|b| Line {
                        text: substitute(&b.text, &map),
                        file: l.file,
                        line: l.line,
                        macro_depth: depth + 1,
                        chain: l.expand_chain(b),
                    })
                    .collect();
                // EXITM support: cut the expansion at the first EXITM.
                let cut = expanded
                    .iter()
                    .position(|e| first_words(strip_comment(&e.text)).0 == "EXITM")
                    .unwrap_or(expanded.len());
                self.process(expanded[..cut].to_vec(), depth + 1);
                continue;
            }

            self.out.push(l);
        }
        if !cond.is_empty()
            && let Some(l) = lines.last()
        {
            let l = l.clone();
            self.error(&l, "IF without ENDIF");
        }
    }

    fn is_defined(&self, name: &str) -> bool {
        let n = name.trim().to_ascii_uppercase();
        self.defined.contains(&n) || self.macros.contains_key(&n)
    }

    /// Collects lines up to the matching ENDM; returns (body, index after ENDM).
    fn collect_block(&mut self, lines: &[Line], mut i: usize, start: &Line) -> (Vec<Line>, usize) {
        let mut depth = 1;
        let mut body = Vec::new();
        while i < lines.len() {
            let code = strip_comment(&lines[i].text).trim().to_string();
            let (w1, w2) = first_words(&code);
            if w2 == "MACRO" || matches!(w1.as_str(), "REPT" | "IRP" | "IRPC") {
                depth += 1;
            }
            if w1 == "ENDM" || w2 == "ENDM" {
                depth -= 1;
                if depth == 0 {
                    return (body, i + 1);
                }
            }
            body.push(lines[i].clone());
            i += 1;
        }
        self.error(start, "missing ENDM");
        (body, i)
    }
}

/// Splits `label: rest` (label is not a segment register prefix).
fn split_label(code: &str) -> (Option<&str>, &str) {
    if let Some(pos) = code.find(':') {
        let head = &code[..pos];
        let ok = !head.is_empty()
            && head.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '@' | '?' | '$' | '.'))
            && crate::expr::parse_reg(head).is_none();
        if ok {
            return (Some(head), &code[pos + 1..]);
        }
    }
    (None, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoFiles;
    impl FileLoader for NoFiles {
        fn load(&mut self, _: &str, _: &str) -> Option<(String, String)> {
            None
        }
    }

    fn pp(src: &str) -> Vec<String> {
        let mut l = NoFiles;
        let mut p = Preprocessor::new(&mut l);
        p.add_file("t.asm", src);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        p.out.into_iter().map(|l| l.text.trim().to_string()).collect()
    }

    #[test]
    fn macro_expansion() {
        let out = pp("putc macro ch\n local skip\n mov al, ch\n jmp skip\nskip:\nendm\nputc 'A'\nputc <'B'>\n");
        assert_eq!(
            out,
            vec!["mov al, 'A'", "jmp SKIP??0001", "SKIP??0001:", "mov al, 'B'", "jmp SKIP??0002", "SKIP??0002:"]
        );
    }

    #[test]
    fn conditionals_and_rept() {
        let out = pp("X equ 1\nif X\nnop\nelse\nhlt\nendif\nifdef Y\nint 3\nendif\nrept 2\ncld\nendm");
        assert_eq!(out, vec!["X equ 1", "nop", "cld", "cld"]);
    }

    #[test]
    fn substitution_rules() {
        let mut m = HashMap::new();
        m.insert("P".to_string(), "AX".to_string());
        assert_eq!(substitute("mov p, 0Ah ; p", &m), "mov AX, 0Ah ");
        assert_eq!(substitute("db 'p&p&'", &m), "db 'pAX'");
        assert_eq!(substitute("x&p", &m), "xAX");
    }
}
