//! Index of macros and procedures defined in an include file (for editor help and navigation).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Macro,
    Proc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Name in upper case.
    pub name: String,
    pub kind: EntryKind,
    /// Parameters of a macro as written (`col, row`).
    pub params: String,
    /// Comment block written right above the definition (without `;`).
    pub doc: String,
    /// 1-based line of the definition.
    pub line: u32,
    /// 1-based line of the matching ENDM / ENDP.
    pub end_line: u32,
}

fn words(line: &str) -> Vec<String> {
    let code = line.split(';').next().unwrap_or("");
    code.split_whitespace().map(|w| w.to_ascii_uppercase()).collect()
}

/// Scans `text` for `NAME MACRO` and `NAME PROC` definitions. Procedures defined inside a macro
/// (Emu8086's `DEFINE_xxx` pattern) inherit the macro's documentation.
pub fn index(text: &str) -> Vec<Entry> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Entry> = Vec::new();
    let mut open_macro: Option<usize> = None;
    let mut open_proc: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        let w = words(l);
        let n = i as u32 + 1;
        match (w.first().map(String::as_str), w.get(1).map(String::as_str)) {
            (Some(name), Some("MACRO")) => {
                // Documentation: the consecutive comment lines just above.
                let mut doc = Vec::new();
                let mut j = i;
                while j > 0 && lines[j - 1].trim_start().starts_with(';') {
                    j -= 1;
                    doc.push(lines[j].trim_start().trim_start_matches(';').trim_end());
                }
                doc.reverse();
                let params = l.split(';').next().unwrap_or("").split_whitespace().skip(2).collect::<Vec<_>>().join(" ");
                out.push(Entry {
                    name: name.to_string(),
                    kind: EntryKind::Macro,
                    params,
                    doc: doc.iter().map(|d| d.strip_prefix(' ').unwrap_or(d)).collect::<Vec<_>>().join("\n"),
                    line: n,
                    end_line: n,
                });
                open_macro = Some(out.len() - 1);
            }
            (Some(name), Some("PROC")) => {
                let doc = open_macro.map(|m| out[m].doc.clone()).unwrap_or_default();
                out.push(Entry {
                    name: name.to_string(),
                    kind: EntryKind::Proc,
                    params: String::new(),
                    doc,
                    line: n,
                    end_line: n,
                });
                open_proc = Some(out.len() - 1);
            }
            (Some(_), Some("ENDP")) => {
                if let Some(p) = open_proc.take() {
                    out[p].end_line = n;
                }
            }
            (Some("ENDM"), _) => {
                if let Some(m) = open_macro.take() {
                    out[m].end_line = n;
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_builtin_library() {
        let idx = index(crate::EMU8086_INC);
        let putc = idx.iter().find(|e| e.name == "PUTC").unwrap();
        assert_eq!(putc.kind, EntryKind::Macro);
        assert_eq!(putc.params, "char");
        assert!(putc.doc.starts_with("PUTC char"));
        let scan = idx.iter().find(|e| e.name == "SCAN_NUM").unwrap();
        assert_eq!(scan.kind, EntryKind::Proc);
        assert!(scan.doc.contains("CX"));
        assert!(scan.end_line > scan.line);
        for name in ["PRINT", "PRINTN", "GOTOXY", "CURSOROFF", "PRINT_NUM_UNS", "PTHIS", "GET_STRING", "CLEAR_SCREEN"] {
            assert!(idx.iter().any(|e| e.name == name), "{name}");
        }
    }
}
