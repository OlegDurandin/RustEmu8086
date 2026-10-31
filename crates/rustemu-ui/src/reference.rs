//! Built-in reference: 8086/80186 instructions, assembler directives, BIOS and DOS interrupts and
//! a guide to the NASM syntax, in English and Russian ([`Lang`]).

mod directives;
mod instructions;
mod interrupts;
mod ru;
mod ru_directives;
mod ru_instructions;
mod ru_interrupts;

pub use directives::*;
pub use instructions::*;
pub use interrupts::*;
pub use ru::{FunctionText, GuideSection, Lang, flag_titles, nasm_section, syntax_in};

/// Finds an instruction entry by name or alias (case-insensitive).
pub fn find_instruction(name: &str) -> Option<usize> {
    let u = name.to_ascii_uppercase();
    INSTRUCTIONS.iter().position(|i| i.name == u || i.aliases.contains(&u.as_str()))
}

/// Finds a directive entry; `#...#` directives match by their prefix (`#cs=0500#` -> `#CS=`).
pub fn find_directive(name: &str) -> Option<usize> {
    let u = name.to_ascii_uppercase();
    DIRECTIVES.iter().position(|d| {
        std::iter::once(d.name)
            .chain(d.aliases.iter().copied())
            .any(|n| if n.starts_with('#') { u.starts_with(n.trim_end_matches('#')) } else { n == u })
    })
}

pub fn find_interrupt(number: u8) -> Option<usize> {
    INTERRUPTS.iter().position(|i| i.number == number)
}

/// Readable effect of a flag code.
pub fn flag_effect(c: char, lang: Lang) -> &'static str {
    match c {
        'r' => lang.pick("changed", "меняется"),
        'u' => lang.pick("undefined", "не определён"),
        '0' => lang.pick("cleared (0)", "сбрасывается (0)"),
        '1' => lang.pick("set (1)", "устанавливается (1)"),
        _ => lang.pick("unchanged", "не меняется"),
    }
}

/// One line like `O=changed S=changed ... (D I T unchanged)`.
pub fn flags_summary(flags: &str, lang: Lang) -> String {
    let changed: Vec<String> = flags
        .chars()
        .zip(FLAG_NAMES)
        .filter(|(c, _)| *c != '-')
        .map(|(c, n)| match c {
            'r' => n.to_string(),
            'u' => format!("{n}?"),
            v => format!("{n}={v}"),
        })
        .collect();
    match (changed.is_empty(), lang) {
        (true, Lang::En) => "flags: not affected".into(),
        (true, Lang::Ru) => "флаги не меняются".into(),
        (false, Lang::En) => format!("flags: {}", changed.join(" ")),
        (false, Lang::Ru) => format!("флаги: {}", changed.join(" ")),
    }
}

/// An article of the reference.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Topic {
    Instr(usize),
    Directive(usize),
    /// A section of [`NASM_GUIDE`].
    Nasm(usize),
    /// Interrupt index and the function code, if known.
    Int(usize, Option<u16>),
}

impl Topic {
    /// Title and one line of details for the editor tooltip.
    pub fn tooltip(self, lang: Lang) -> (String, String) {
        match self {
            Topic::Instr(i) => {
                let e = &INSTRUCTIONS[i];
                (format!("{}  {}", e.name, e.summary_in(lang)), flags_summary(e.flags, lang))
            }
            Topic::Directive(i) => (DIRECTIVES[i].name.to_string(), DIRECTIVES[i].summary_in(lang).to_string()),
            Topic::Nasm(i) => {
                (lang.pick("NASM syntax", "Синтаксис NASM").to_string(), nasm_section(i, lang).title.to_string())
            }
            Topic::Int(i, f) => {
                let int = &INTERRUPTS[i];
                let head = format!("INT {:02X}h  {}", int.number, int.title_in(lang));
                match f.and_then(|c| int.functions.iter().find(|x| x.code == c)) {
                    Some(func) => (
                        head,
                        format!(
                            "{} = {}: {}",
                            int.selector,
                            hex(func.code, int.selector),
                            func.text_in(int.number, lang).title
                        ),
                    ),
                    None if int.functions.is_empty() => (head, int.description_in(lang).to_string()),
                    None => (head, format!("{} {}", lang.pick("function number in", "номер функции в"), int.selector)),
                }
            }
        }
    }
}

/// Function code as written in the reference: `09h` for AH, `0003h` for AX.
pub fn hex(code: u16, selector: &str) -> String {
    if selector == "AX" { format!("{code:04X}h") } else { format!("{code:02X}h") }
}

/// Code part of a source line (without the comment), and whether char column `col` is inside
/// a string or a comment.
fn code_part(line: &str, col: usize) -> (&str, bool) {
    let mut quote = None;
    for (n, (b, c)) in line.char_indices().enumerate() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), _) if q == c => quote = None,
            (None, ';') => return (&line[..b], col >= n),
            _ => {}
        }
        if n == col && quote.is_some() {
            return (line, true);
        }
    }
    (line, false)
}

/// Upper-case statement tokens after an optional `label:`.
fn statement(code: &str) -> Vec<String> {
    let code = match code.split_once(':') {
        Some((l, rest)) if !l.trim().is_empty() && !l.trim().contains(char::is_whitespace) && !l.contains('[') => rest,
        _ => code,
    };
    code.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).map(str::to_ascii_uppercase).collect()
}

/// Reference article for the word at char index `ci` of `text`: an instruction, a directive,
/// or the interrupt of `INT nn` with the function taken from the preceding `MOV AH, ..`.
pub fn topic_at(text: &str, ci: usize) -> Option<Topic> {
    let chars: Vec<char> = text.chars().collect();
    let is_id = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '?' | '$' | '.');
    if ci >= chars.len() || !is_id(chars[ci]) {
        return None;
    }
    let mut a = ci;
    while a > 0 && is_id(chars[a - 1]) {
        a -= 1;
    }
    let mut b = ci;
    while b < chars.len() && is_id(chars[b]) {
        b += 1;
    }
    let line_start = chars[..a].iter().rposition(|&c| c == '\n').map_or(0, |p| p + 1);
    let line_end = chars[b..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| b + p);
    let line: String = chars[line_start..line_end].iter().collect();
    let line_no = chars[..a].iter().filter(|&&c| c == '\n').count();
    let (code, ignored) = code_part(&line, a - line_start);
    let word = chars[a..b].iter().collect::<String>().to_ascii_uppercase();
    // NASM preprocessor directives: %define, %macro, ...
    if a > 0 && chars[a - 1] == '%' {
        return find_directive(&format!("%{word}")).map(Topic::Directive);
    }
    // Emu8086 directives in #...#.
    if a > 0 && chars[a - 1] == '#' {
        let next = chars.get(b).copied().unwrap_or(' ');
        return find_directive(&format!("#{word}{}", if next == '=' { "=" } else { "#" })).map(Topic::Directive);
    }
    if ignored {
        return None;
    }
    let tokens = statement(code);
    let pos = tokens.iter().position(|t| *t == word)?;
    if tokens[0] == "INT" && tokens.len() > 1 {
        let n = rustemu_asm::evaluate(&tokens[1]).ok()?;
        let i = find_interrupt(u8::try_from(n).ok()?)?;
        let lines: Vec<&str> = text.lines().take(line_no).collect();
        return Some(Topic::Int(i, preceding_function(&lines, INTERRUPTS[i].selector)));
    }
    // Instructions only in the mnemonic position (AND, SHL, ... are also expression operators).
    let prefixes = ["REP", "REPE", "REPZ", "REPNE", "REPNZ", "LOCK"];
    let mnemonic_pos = pos == 0 || (pos == 1 && prefixes.contains(&tokens[0].as_str()));
    if mnemonic_pos && let Some(i) = find_instruction(&word) {
        return Some(Topic::Instr(i));
    }
    find_directive(&word).map(Topic::Directive)
}

/// Like [`topic_at`] for the text cursor, which may also stand right after the word.
pub fn topic_at_cursor(text: &str, ci: usize) -> Option<Topic> {
    topic_at(text, ci).or_else(|| ci.checked_sub(1).and_then(|c| topic_at(text, c)))
}

/// Value of the function selector (AH or AX) set by the lines before an `INT`, looking back
/// until a label, a jump or another call.
fn preceding_function(lines: &[&str], selector: &str) -> Option<u16> {
    if selector.is_empty() {
        return None;
    }
    for line in lines.iter().rev().take(16) {
        let (code, _) = code_part(line, usize::MAX);
        if code.trim().ends_with(':') {
            return None;
        }
        let t = statement(code);
        let Some(op) = t.first() else { continue };
        let dest = t.get(1).map(String::as_str);
        let src = t.get(2).map(String::as_str);
        let value = || rustemu_asm::evaluate(&t[2..].join(" ")).ok().map(|v| v as u16);
        match (op.as_str(), dest) {
            ("MOV", Some("AX")) if t.len() > 2 => return value().map(|v| if selector == "AX" { v } else { v >> 8 }),
            ("MOV", Some("AH")) if t.len() > 2 && selector == "AH" => return value(),
            ("XOR" | "SUB", Some("AX")) if src == Some("AX") => return Some(0),
            ("XOR" | "SUB", Some("AH")) if src == Some("AH") && selector == "AH" => return Some(0),
            ("INT" | "CALL" | "RET" | "RETF" | "IRET" | "LOOP", _) => return None,
            (j, _) if j.starts_with('J') => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_in_source() {
        let src = "start: mov ah, 9 ; mov here\n lea dx, msg\n int 21h\n rep movsb\n mov ax, 1 shl 2\n\
                   msg db 'add$'\n#make_com#\n mov ax, 3\n int 33h\n jmp x\n int 10h";
        let at = |w: &str| topic_at(src, src.find(w).unwrap());
        let name = |t: Option<Topic>| match t {
            Some(Topic::Instr(i)) => INSTRUCTIONS[i].name.to_string(),
            Some(Topic::Directive(i)) => DIRECTIVES[i].name.to_string(),
            Some(Topic::Int(i, f)) => format!("INT {:X} {f:?}", INTERRUPTS[i].number),
            Some(Topic::Nasm(i)) => NASM_GUIDE[i].title.to_string(),
            None => "-".into(),
        };
        assert_eq!(name(at("mov ah")), "MOV");
        assert_eq!(name(at("mov here")), "-");
        assert_eq!(name(at("21h")), "INT 21 Some(9)");
        assert_eq!(name(at("int 21h")), "INT 21 Some(9)");
        assert_eq!(name(at("movsb")), "MOVSB");
        assert_eq!(name(at("rep")), "REP");
        assert_eq!(name(at("shl")), "-");
        assert_eq!(name(at("db")), "DB");
        assert_eq!(name(at("add$")), "-");
        assert_eq!(name(at("make_com")), "#MAKE_COM#");
        assert_eq!(name(at("33h")), "INT 33 Some(3)");
        assert_eq!(name(at("10h")), "INT 10 None");
        // The text cursor right after the word.
        assert_eq!(name(topic_at_cursor(src, src.find("lea").unwrap() + 3)), "LEA");
        assert_eq!(name(topic_at(src, src.find("lea").unwrap() + 3)), "-");
    }

    #[test]
    fn data_is_consistent() {
        for i in INSTRUCTIONS {
            assert_eq!(i.flags.len(), 9, "{}", i.name);
            assert!(i.flags.chars().all(|c| "r-u01".contains(c)), "{}", i.name);
            assert!(!i.summary.is_empty(), "{}", i.name);
        }
        // Every mnemonic the assembler knows has a reference entry.
        for m in [
            "MOV", "ADD", "JNZ", "JCXZ", "LOOPNE", "REPZ", "XLAT", "MOVSW", "SAL", "RETF", "IRET", "PUSHA", "AAM",
            "CMC", "SCASB", "LODSW", "STOSB", "IN", "OUT", "INT", "HLT", "LEAVE",
        ] {
            assert!(find_instruction(m).is_some(), "{m}");
        }
        assert_eq!(find_directive("#cs=0500"), find_directive("#LOAD_SEGMENT="));
        assert!(find_directive("db").is_some());
        assert!(find_interrupt(0x21).is_some());
        assert_eq!(flags_summary(LOGIC, Lang::En), "flags: O=0 S Z A? P C=0");
        assert_eq!(flags_summary(LOGIC, Lang::Ru), "флаги: O=0 S Z A? P C=0");
    }

    #[test]
    fn covers_all_assembler_mnemonics() {
        // Mnemonics accepted by the assembler (excluding prefixes handled by REP).
        let list = [
            "AAA", "AAS", "CBW", "CWD", "CLC", "CLD", "CLI", "CMC", "DAA", "DAS", "HLT", "INTO", "IRET", "LAHF",
            "SAHF", "NOP", "PUSHF", "POPF", "WAIT", "STC", "STD", "STI", "MOVSB", "MOVSW", "CMPSB", "CMPSW", "STOSB",
            "STOSW", "LODSB", "LODSW", "SCASB", "SCASW", "ADC", "SBB", "AND", "OR", "XOR", "CMP", "SUB", "ROL", "ROR",
            "RCL", "RCR", "SHL", "SHR", "SAR", "NOT", "NEG", "MUL", "IMUL", "DIV", "IDIV", "TEST", "XCHG", "INC",
            "DEC", "PUSH", "POP", "LEA", "LDS", "LES", "JMP", "CALL", "RET", "LOOP", "LOOPE", "LOOPZ", "AAD", "ESC",
            "LOCK", "JA", "JAE", "JB", "JBE", "JC", "JE", "JG", "JGE", "JL", "JLE", "JNA", "JNAE", "JNB", "JNBE",
            "JNC", "JNE", "JNG", "JNGE", "JNL", "JNLE", "JNO", "JNP", "JNS", "JO", "JP", "JPE", "JPO", "JS", "JZ",
            "POPA", "PUSHA", "LEAVE",
        ];
        for m in list {
            assert!(rustemu_asm::encode::is_mnemonic(m), "{m}");
            assert!(find_instruction(m).is_some(), "{m}");
        }
        // And every reference name is something the assembler accepts.
        let prefixes = ["REP", "REPE", "REPZ", "REPNE", "REPNZ", "LOCK"];
        for i in INSTRUCTIONS {
            for n in std::iter::once(&i.name).chain(i.aliases) {
                assert!(prefixes.contains(n) || rustemu_asm::encode::is_mnemonic(n), "{n}");
            }
        }
    }

    /// The examples of the NASM guide are recognized as NASM.
    #[test]
    fn nasm_guide_examples_are_nasm() {
        for s in NASM_GUIDE.iter().filter(|s| !s.example.is_empty()) {
            let d = rustemu_asm::detect_dialect("example.asm", s.example);
            assert_eq!(d, rustemu_asm::Dialect::Nasm, "{}", s.title);
        }
    }

    /// Every example assembles once the names it only refers to are defined (as a label, a
    /// byte or a word variable — whichever fits).
    #[test]
    fn examples_assemble() {
        let mut all: Vec<(String, &str)> = Vec::new();
        all.extend(INSTRUCTIONS.iter().map(|i| (i.name.to_string(), i.example)));
        all.extend(DIRECTIVES.iter().map(|d| (d.name.to_string(), d.example)));
        all.extend(NASM_GUIDE.iter().filter(|s| !s.example.is_empty()).map(|s| (s.title.to_string(), s.example)));
        for int in INTERRUPTS {
            all.extend(int.functions.iter().map(|f| (format!("INT {:X}h/{:X}h", int.number, f.code), f.example)));
        }
        let undefined = |src: &str| -> Result<Vec<String>, String> {
            match rustemu_asm::assemble_str(src) {
                Ok(_) => Ok(Vec::new()),
                Err(e) => {
                    let mut names = Vec::new();
                    for er in e.errors {
                        match er.msg.strip_prefix("undefined symbol '").and_then(|s| s.strip_suffix('\'')) {
                            Some(n) if !names.contains(&n.to_string()) => names.push(n.to_string()),
                            Some(_) => {}
                            None => return Err(er.to_string()),
                        }
                    }
                    Ok(names)
                }
            }
        };
        for (name, example) in all {
            // An include of a user file cannot be assembled here.
            if example.contains("%include") {
                continue;
            }
            // Examples with segments or their own ORG are complete programs.
            let own_org = example.lines().any(|l| l.trim_start().to_ascii_lowercase().starts_with("org "));
            let src = if example.contains("\nends") || example.contains(".model") || own_org {
                format!("{example}\n")
            } else {
                format!("org 100h\n{example}\n")
            };
            let missing = undefined(&src).unwrap_or_else(|e| panic!("{name}: {e}\n{src}"));
            let combos = 3usize.pow(missing.len() as u32);
            let ok = (0..combos).any(|mut c| {
                let mut s = src.clone();
                for m in &missing {
                    s.push_str(&[format!("{m}:\n"), format!("{m} db 0\n"), format!("{m} dw 0\n")][c % 3]);
                    c /= 3;
                }
                undefined(&s) == Ok(Vec::new())
            });
            assert!(ok, "{name}: example does not assemble with {missing:?} defined\n{src}");
        }
    }

    /// Code of an example without comments and trailing spaces, line by line.
    fn code_only(example: &str) -> Vec<String> {
        example.lines().map(|l| code_part(l, usize::MAX).0.trim_end().to_string()).collect()
    }

    #[test]
    fn russian_text_is_complete() {
        let ru = Lang::Ru;
        for e in INSTRUCTIONS {
            let t = e.ru().unwrap_or_else(|| panic!("no Russian text for {}", e.name));
            assert!(!t.summary.is_empty() && (e.description.is_empty() || !t.description.is_empty()), "{}", e.name);
            assert_eq!(
                code_only(e.example_in(ru)),
                code_only(e.example),
                "{}: the Russian example has other code",
                e.name
            );
        }
        for d in DIRECTIVES {
            let t = d.ru().unwrap_or_else(|| panic!("no Russian text for {}", d.name));
            assert!(!t.summary.is_empty() && !t.description.is_empty(), "{}", d.name);
            assert_eq!(
                code_only(d.example_in(ru)),
                code_only(d.example),
                "{}: the Russian example has other code",
                d.name
            );
        }
        assert_eq!(ru_directives::NASM_GUIDE.len(), NASM_GUIDE.len());
        for (i, en) in NASM_GUIDE.iter().enumerate() {
            let s = nasm_section(i, ru);
            assert!(!s.title.is_empty() && !s.text.is_empty(), "{}", en.title);
            assert_eq!(code_only(s.example), code_only(en.example), "{}: the Russian example has other code", en.title);
            assert_eq!(s.table.len(), en.table.len(), "{}", en.title);
            for (a, b) in s.table.iter().zip(en.table) {
                assert_eq!(a.len(), b.len(), "{}", en.title);
            }
        }
        for int in INTERRUPTS {
            assert!(int.ru().is_some(), "no Russian text for INT {:02X}h", int.number);
            for f in int.functions {
                let found = ru_interrupts::FUNCTIONS.iter().any(|r| r.number == int.number && r.code == f.code);
                assert!(found, "no Russian text for INT {:02X}h function {:X}h", int.number, f.code);
                let t = f.text_in(int.number, ru);
                assert_eq!(code_only(t.example), code_only(f.example), "INT {:02X}h/{:X}h", int.number, f.code);
            }
        }
        // No translation for an article that does not exist.
        for t in ru_instructions::INSTRUCTIONS {
            assert!(INSTRUCTIONS.iter().any(|e| e.name == t.key), "{}", t.key);
        }
        for t in ru_directives::DIRECTIVES {
            assert!(DIRECTIVES.iter().any(|d| d.name == t.key), "{}", t.key);
        }
        for f in ru_interrupts::FUNCTIONS {
            let int = INTERRUPTS.iter().find(|i| i.number == f.number).unwrap();
            assert!(int.functions.iter().any(|x| x.code == f.code), "INT {:02X}h/{:X}h", f.number, f.code);
        }
        assert_eq!(Topic::Instr(0).tooltip(ru).0, format!("MOV  {}", INSTRUCTIONS[0].summary_in(ru)));
    }
}
