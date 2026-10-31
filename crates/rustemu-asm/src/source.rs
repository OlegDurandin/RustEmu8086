//! Source preprocessing: INCLUDE, MACRO/ENDM (with LOCAL), REPT, IF/IFDEF/ELSEIF/ELSE/ENDIF.
//! NASM `%` directives are translated to these line by line (see [`Preprocessor::nasm_lines`]).

use std::collections::HashMap;

use crate::AsmError;
use crate::expr::{Env, ForwardKind, Parser, SegId, SymVal, eval};
use crate::lexer::{tokenize, tokenize_nasm};

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
    /// Text of each file (to locate errors in their lines).
    pub sources: Vec<String>,
    macros: HashMap<String, Macro>,
    consts: HashMap<String, i64>,
    /// Text equates (`name EQU a, b, c`), substituted in the lines that follow.
    text_equs: HashMap<String, String>,
    defined: std::collections::HashSet<String>,
    local_counter: u32,
    pub errors: Vec<AsmError>,
    pub out: Vec<Line>,
    /// NASM dialect: `%` directives, `&` is an operator, backquoted strings.
    pub nasm: bool,
    /// Files being processed, the outermost first (to catch an INCLUDE cycle).
    open_files: Vec<String>,
    /// NASM `%define name(params) body` (by upper-case name): expanded where they are used.
    nasm_functions: HashMap<String, (Vec<String>, String)>,
}

/// Deepest INCLUDE nesting (a cycle through differently spelled names stops here).
const MAX_INCLUDE_DEPTH: usize = 32;

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
            None if c == '\'' || c == '"' || c == '`' => q = Some(c),
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
#[cfg(test)]
fn substitute(text: &str, map: &HashMap<String, String>) -> String {
    substitute_with(text, map, false)
}

/// [`substitute`]; in NASM `&` is an operator and backquotes delimit strings.
fn substitute_with(text: &str, map: &HashMap<String, String>, nasm: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut q: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(qc) = q {
            // Inside strings only `&name&` style substitution is done.
            if c == '&' && !nasm {
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
        if c == '\'' || c == '"' || (nasm && c == '`') {
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
        if c == '&' && !nasm {
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
            sources: Vec::new(),
            macros: HashMap::new(),
            consts: HashMap::new(),
            text_equs: HashMap::new(),
            defined: Default::default(),
            local_counter: 0,
            errors: Vec::new(),
            out: Vec::new(),
            nasm: false,
            open_files: Vec::new(),
            nasm_functions: HashMap::new(),
        }
    }

    fn error(&mut self, l: &Line, msg: impl Into<String>) {
        self.errors.push(AsmError { file: l.file, line: l.line, col: None, len: 0, msg: msg.into() });
    }

    pub fn add_file(&mut self, name: &str, text: &str) {
        let idx = self.files.len();
        self.files.push(name.to_string());
        self.sources.push(text.to_string());
        let lines = self.file_lines(idx, text, 0);
        self.open_files.push(name.to_string());
        self.process(lines, 0);
        self.open_files.pop();
    }

    /// The lines of a source file (NASM directives translated).
    fn file_lines(&mut self, file: usize, text: &str, depth: u32) -> Vec<Line> {
        let numbered: Vec<(u32, String)> = if self.nasm {
            self.nasm_lines(file, text)
        } else {
            text.lines().enumerate().map(|(i, t)| (i as u32 + 1, t.to_string())).collect()
        };
        numbered
            .into_iter()
            .map(|(line, text)| Line { text, file, line, macro_depth: depth, chain: Vec::new() })
            .collect()
    }

    /// Translates the NASM preprocessor (`%define` with and without parameters, `%assign`, `%macro`
    /// with `%1` and `%%label`, `%rep`, `%if`/`%elif`/`%else`/`%endif`, `%ifdef`, `%include`) and
    /// `struc` / `istruc` to the MASM directives and NASM lines the rest handles. Lines keep their
    /// numbers (one NASM line may give several).
    fn nasm_lines(&mut self, file: usize, text: &str) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        // Inside `%macro`: (parameter count, local labels used).
        let mut in_macro: Option<(usize, Vec<String>)> = None;
        // Inside `struc name`: (name, offset of the next field as an expression).
        let mut in_struc: Option<(String, String)> = None;
        // Inside `istruc name`: (name, the equate holding its start address).
        let mut in_istruc: Option<(String, String)> = None;
        for (i, raw) in text.lines().enumerate() {
            let n = i as u32 + 1;
            let err = |s: &mut Self, msg: String| s.errors.push(AsmError { file, line: n, col: None, len: 0, msg });
            // Function-like %define is expanded everywhere but in the lines that define one.
            let expanded;
            let raw = if self.nasm_functions.is_empty() || defines_something(raw) {
                raw
            } else {
                match expand_functions(raw, &self.nasm_functions) {
                    Ok(t) => {
                        expanded = t;
                        expanded.as_str()
                    }
                    Err(msg) => {
                        err(self, msg);
                        raw
                    }
                }
            };
            let code = strip_comment(raw).trim();
            if let Some(directive) = code.strip_prefix('%').filter(|d| d.starts_with(|c: char| c.is_ascii_alphabetic()))
            {
                let word: String = directive.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                let rest = directive[word.len()..].trim();
                let word = word.to_ascii_lowercase();
                let first_word = rest.split_whitespace().next().unwrap_or("");
                let after_first = rest[first_word.len()..].trim();
                let line = match word.as_str() {
                    "define" | "xdefine" | "idefine" | "ixdefine" => match function_define(rest) {
                        Some(Ok((name, params, body))) => {
                            self.nasm_functions.insert(name.to_ascii_uppercase(), (params, body));
                            None
                        }
                        Some(Err(msg)) => {
                            err(self, msg);
                            None
                        }
                        None => Some(format!("{first_word} EQU <{after_first}>")),
                    },
                    "undef" => {
                        let name: String =
                            first_word.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                        self.nasm_functions.remove(&name.to_ascii_uppercase());
                        Some(format!("%UNDEF {first_word}"))
                    }
                    "assign" | "iassign" => Some(format!("{first_word} = {after_first}")),
                    "macro" | "imacro" => {
                        // `%macro name 2`, `%macro name 1-3`, `%macro name 1+` (no greedy last parameter).
                        let spec = after_first.split_whitespace().next().unwrap_or("0");
                        let max = spec
                            .split('-')
                            .next_back()
                            .and_then(|s| s.trim_end_matches(['+', '*']).parse::<usize>().ok())
                            .or_else(|| spec.trim_end_matches(['+', '*']).parse().ok())
                            .unwrap_or(0)
                            .min(9);
                        in_macro = Some((max, Vec::new()));
                        let params: Vec<String> = (1..=max).map(|k| format!("__P{k}")).collect();
                        Some(format!("{first_word} MACRO {}", params.join(", ")))
                    }
                    "endmacro" | "endm" => {
                        if let Some((_, locals)) = in_macro.take()
                            && !locals.is_empty()
                        {
                            out.push((n, format!("LOCAL {}", locals.join(", "))));
                        }
                        Some("ENDM".into())
                    }
                    "rep" => Some(format!("REPT {rest}")),
                    "endrep" => Some("ENDM".into()),
                    "if" => Some(format!("IF {rest}")),
                    "elif" => Some(format!("ELSEIF {rest}")),
                    "else" => Some("ELSE".into()),
                    "endif" => Some("ENDIF".into()),
                    "ifdef" => Some(format!("IFDEF {rest}")),
                    "ifndef" => Some(format!("IFNDEF {rest}")),
                    "elifdef" => Some(format!("ELSEIFDEF {rest}")),
                    "elifndef" => Some(format!("ELSEIFNDEF {rest}")),
                    "include" => Some(format!("INCLUDE {rest}")),
                    "error" | "fatal" => Some(format!("%ERROR {rest}")),
                    "warning" | "line" | "pragma" | "use" | "push" | "pop" => None,
                    _ => {
                        err(self, format!("NASM directive %{word} is not supported"));
                        None
                    }
                };
                if let Some(l) = line {
                    out.push((n, l));
                }
                continue;
            }
            let words: Vec<&str> = code.split_whitespace().collect();
            let word = |k: usize| words.get(k).map(|w| w.to_ascii_lowercase()).unwrap_or_default();
            // struc name ... endstruc: the fields become equates of their offsets.
            if word(0) == "struc" {
                match words.get(1) {
                    Some(name) => in_struc = Some((name.to_string(), "0".into())),
                    None => err(self, "struc needs a name".into()),
                }
                continue;
            }
            if let Some((name, offset)) = &mut in_struc {
                if word(0) == "endstruc" {
                    out.push((n, format!("{name}_size equ {offset}")));
                    out.push((n, format!("{name} equ 0")));
                    in_struc = None;
                    continue;
                }
                if words.is_empty() {
                    continue;
                }
                // [label[:]] [resb|resw|resd|resq|rest|alignb n]
                let is_directive = |w: &str| {
                    matches!(w.to_ascii_lowercase().as_str(), "resb" | "resw" | "resd" | "resq" | "rest" | "alignb")
                };
                let (label, rest) = match code.split_once(':') {
                    Some((l, r)) if !l.contains(char::is_whitespace) => (Some(l.trim()), r.trim()),
                    _ if !is_directive(words[0]) => (Some(words[0]), code[words[0].len()..].trim()),
                    _ => (None, code),
                };
                if let Some(label) = label {
                    let full = if label.starts_with('.') { format!("{name}{label}") } else { label.to_string() };
                    out.push((n, format!("{full} equ {offset}")));
                }
                let directive = rest.split_whitespace().next().unwrap_or("");
                let count = rest[directive.len()..].trim();
                let unit = match directive.to_ascii_lowercase().as_str() {
                    "" => None,
                    "resb" => Some(1),
                    "resw" => Some(2),
                    "resd" => Some(4),
                    "resq" => Some(8),
                    "rest" => Some(10),
                    "alignb" => {
                        *offset = format!("((({offset})+({count})-1)/({count})*({count}))");
                        None
                    }
                    _ => {
                        err(self, format!("only resb/resw/resd/resq/rest/alignb can be inside struc, not {directive}"));
                        None
                    }
                };
                if let Some(unit) = unit {
                    *offset = format!("{offset}+{unit}*({count})");
                }
                continue;
            }
            // [label[:]] istruc name ... at field, data ... iend: the data padded to the offsets.
            let istruc_at = if word(0) == "istruc" {
                Some(0)
            } else if word(1) == "istruc" {
                Some(1)
            } else {
                None
            };
            if let Some(k) = istruc_at {
                let Some(name) = words.get(k + 1) else {
                    err(self, "istruc needs the name of a struc".into());
                    continue;
                };
                if k == 1 {
                    out.push((n, format!("{}:", words[0].trim_end_matches(':'))));
                }
                self.local_counter += 1;
                let start = format!("@ISTRUC_{}", self.local_counter);
                out.push((n, format!("{start} equ $")));
                in_istruc = Some((name.to_string(), start));
                continue;
            }
            if let Some((name, start)) = &in_istruc {
                if word(0) == "iend" {
                    out.push((n, format!("times ({name}_size)-($-{start}) db 0")));
                    in_istruc = None;
                    continue;
                }
                if word(0) == "at" {
                    let args = code[2..].trim();
                    let (field, data) = args.split_once(',').unwrap_or((args, ""));
                    out.push((n, format!("times ({})-($-{start}) db 0", field.trim())));
                    if !data.trim().is_empty() {
                        out.push((n, data.trim().to_string()));
                    }
                    continue;
                }
            }
            let mut line = raw.to_string();
            if let Some((count, locals)) = &mut in_macro {
                line = nasm_macro_body(&line, *count, locals);
            }
            out.push((n, line));
        }
        if let Some((name, _)) = in_struc {
            self.errors.push(AsmError {
                file,
                line: 0,
                col: None,
                len: 0,
                msg: format!("struc {name} has no endstruc"),
            });
        }
        if let Some((name, _)) = in_istruc {
            self.errors.push(AsmError { file, line: 0, col: None, len: 0, msg: format!("istruc {name} has no iend") });
        }
        out
    }

    fn eval_const(&self, text: &str) -> Option<i64> {
        let toks = if self.nasm { tokenize_nasm(text).ok()? } else { tokenize(text).ok()? };
        let mut p = Parser::new(&toks);
        p.nasm = self.nasm;
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
        // Conditional assembly stack: (active, any_branch_taken, enclosing_active). An inner
        // level is active only if the enclosing one is, so the top decides alone.
        let mut cond: Vec<(bool, bool, bool)> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let l = lines[i].clone();
            i += 1;
            let mut l = l;
            let (fw1, fw2) = first_words(&l.text);
            // Names tested by IFDEF and friends must not be replaced by their text.
            let names_line = matches!(fw1.as_str(), "IFDEF" | "IFNDEF" | "ELSEIFDEF" | "ELSEIFNDEF" | "%UNDEF");
            if !self.text_equs.is_empty() && fw2 != "EQU" && !names_line {
                l.text = substitute_with(&l.text, &self.text_equs, self.nasm);
            }
            let code = strip_comment(&l.text).trim().to_string();
            let (w1, w2) = first_words(&code);
            let active = cond.last().is_none_or(|c| c.0);

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
                    cond.push((v, v, active));
                    continue;
                }
                "ELSEIF" | "ELSEIFDEF" | "ELSEIFNDEF" => {
                    let Some(&(_, taken, parent)) = cond.last() else {
                        self.error(&l, "ELSEIF without IF");
                        continue;
                    };
                    let v = parent && !taken && {
                        let rest = code[w1.len()..].trim();
                        match w1.as_str() {
                            "ELSEIFDEF" => self.is_defined(rest),
                            "ELSEIFNDEF" => !self.is_defined(rest),
                            _ => {
                                self.eval_const(rest).unwrap_or_else(|| {
                                    self.error(&l, "ELSEIF requires a constant expression");
                                    0
                                }) != 0
                            }
                        }
                    };
                    *cond.last_mut().unwrap() = (v, taken || v, parent);
                    continue;
                }
                "ELSE" => {
                    match cond.last_mut() {
                        Some(c) => {
                            c.0 = c.2 && !c.1;
                            c.1 = true;
                        }
                        None => self.error(&l, "ELSE without IF"),
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
                "%UNDEF" if self.nasm => {
                    let name = w2.clone();
                    self.text_equs.remove(&name);
                    self.defined.remove(&name);
                    continue;
                }
                "%ERROR" if self.nasm => {
                    let msg = code[6..].trim().trim_matches(['\'', '"']).to_string();
                    self.error(&l, if msg.is_empty() { "%error".to_string() } else { msg });
                    continue;
                }
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
                        Some((resolved, _)) if self.open_files.contains(&resolved) => {
                            self.error(&l, format!("'{name}' includes itself"));
                        }
                        Some(_) if self.open_files.len() > MAX_INCLUDE_DEPTH => {
                            self.error(&l, "INCLUDE nesting too deep");
                        }
                        Some((resolved, text)) => {
                            let idx = self.files.len();
                            self.files.push(resolved.clone());
                            self.sources.push(text.clone());
                            let inc = self.file_lines(idx, &text, depth);
                            self.open_files.push(resolved);
                            self.process(inc, depth);
                            self.open_files.pop();
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

            // Everything after END is ignored (Emu8086 allows free text there).
            if w1 == "END" && depth == 0 && !self.nasm {
                self.out.push(l);
                return;
            }

            // Text equate: `name EQU 0Dh, 0Ah, '$'` or `name EQU <text>` is substituted textually.
            if w2 == "EQU" {
                let rest = code.split_whitespace().skip(2).collect::<Vec<_>>().join(" ");
                let angle = rest.starts_with('<') && rest.ends_with('>');
                if angle || split_args(&rest).len() > 1 {
                    let name = code.split_whitespace().next().unwrap().to_ascii_uppercase();
                    let text = if angle { rest[1..rest.len() - 1].to_string() } else { rest };
                    self.text_equs.insert(name.clone(), text);
                    self.defined.insert(name);
                    continue;
                }
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
                let nasm = self.nasm;
                let expanded: Vec<Line> = m
                    .body
                    .iter()
                    .map(|b| Line {
                        text: substitute_with(&b.text, &map, nasm),
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

/// A line that defines or undefines a NASM macro (function-like ones are not expanded in it).
fn defines_something(line: &str) -> bool {
    let code = strip_comment(line).trim_start().to_ascii_lowercase();
    ["%define", "%xdefine", "%idefine", "%ixdefine", "%undef", "%macro", "%imacro"]
        .iter()
        .any(|d| code.strip_prefix(d).is_some_and(|r| r.starts_with(char::is_whitespace)))
}

/// `name(a, b) body` of a function-like `%define`: `None` for an ordinary one.
#[allow(clippy::type_complexity)]
fn function_define(rest: &str) -> Option<Result<(String, Vec<String>, String), String>> {
    let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '?' | '@')).collect();
    let after = rest[name.len()..].strip_prefix('(')?;
    let Some((params, body)) = after.split_once(')') else {
        return Some(Err(format!("%define {name}: ')' is missing")));
    };
    let params: Vec<String> =
        params.split(',').map(|p| p.trim().to_ascii_uppercase()).filter(|p| !p.is_empty()).collect();
    Some(Ok((name, params, body.trim().to_string())))
}

/// Expands the uses `name(args)` of function-like `%define`s in `line` (outside strings and the
/// comment), again and again for the uses their bodies bring.
fn expand_functions(line: &str, defs: &HashMap<String, (Vec<String>, String)>) -> Result<String, String> {
    let mut text = line.to_string();
    for _ in 0..64 {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut changed = false;
        let mut quote = None;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = quote {
                out.push(c);
                if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            if c == ';' {
                out.extend(&chars[i..]);
                break;
            }
            if matches!(c, '\'' | '"' | '`') {
                quote = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let mut j = i;
                while j < chars.len() && (chars[j].is_alphanumeric() || matches!(chars[j], '_' | '.' | '?' | '@')) {
                    j += 1;
                }
                let name: String = chars[i..j].iter().collect();
                let mut k = j;
                while k < chars.len() && chars[k] == ' ' {
                    k += 1;
                }
                if let Some((params, body)) = defs.get(&name.to_ascii_uppercase())
                    && chars.get(k) == Some(&'(')
                {
                    // The arguments: split at the commas outside brackets and strings.
                    let (mut depth, mut q, mut args, mut cur, mut end) = (0, None, Vec::new(), String::new(), None);
                    for (m, &d) in chars.iter().enumerate().skip(k + 1) {
                        match (q, d) {
                            (Some(qc), _) if d == qc => q = None,
                            (Some(_), _) => {}
                            (None, '\'' | '"' | '`') => q = Some(d),
                            (None, '(' | '[') => depth += 1,
                            (None, ']') => depth -= 1,
                            (None, ')') if depth == 0 => {
                                end = Some(m);
                                break;
                            }
                            (None, ')') => depth -= 1,
                            (None, ',') if depth == 0 => {
                                args.push(std::mem::take(&mut cur));
                                continue;
                            }
                            _ => {}
                        }
                        cur.push(d);
                    }
                    let Some(end) = end else { return Err(format!("{name}(: ')' is missing")) };
                    if !cur.trim().is_empty() || !args.is_empty() {
                        args.push(cur);
                    }
                    if args.len() != params.len() {
                        return Err(format!("{name} takes {} argument(s), not {}", params.len(), args.len()));
                    }
                    let map: HashMap<String, String> =
                        params.iter().cloned().zip(args.iter().map(|a| a.trim().to_string())).collect();
                    out.push_str(&substitute_with(body, &map, true));
                    changed = true;
                    i = end + 1;
                    continue;
                }
                out.push_str(&name);
                i = j;
                continue;
            }
            out.push(c);
            i += 1;
        }
        if !changed {
            return Ok(out);
        }
        text = out;
    }
    Err("a %define expands into itself".into())
}

/// A line of a NASM macro body: `%1`..`%9` / `%{1}` become the parameters `__P1`..`__P9`,
/// `%%name` becomes the local label `__L_name` (collected into `locals`).
fn nasm_macro_body(line: &str, count: usize, locals: &mut Vec<String>) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            ';' => {
                out.extend(&chars[i..]);
                break;
            }
            '\'' | '"' | '`' => {
                quote = Some(c);
                out.push(c);
                i += 1;
            }
            '%' if chars.get(i + 1) == Some(&'%')
                && chars.get(i + 2).is_some_and(|c| c.is_alphanumeric() || *c == '_') =>
            {
                let mut j = i + 2;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name = format!("__L_{}", chars[i + 2..j].iter().collect::<String>()).to_ascii_uppercase();
                if !locals.contains(&name) {
                    locals.push(name.clone());
                }
                out.push_str(&name);
                i = j;
            }
            '%' if chars.get(i + 1).is_some_and(|c| c.is_ascii_digit() || *c == '{') => {
                let braced = chars[i + 1] == '{';
                let start = if braced { i + 2 } else { i + 1 };
                let mut j = start;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let k: usize = chars[start..j].iter().collect::<String>().parse().unwrap_or(0);
                if braced && chars.get(j) == Some(&'}') {
                    j += 1;
                }
                if (1..=count).contains(&k) {
                    out.push_str(&format!("__P{k}"));
                } else {
                    out.extend(&chars[i..j]);
                }
                i = j;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
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
