//! Syntax highlighting for 8086 assembly.

use egui::text::{LayoutJob, TextFormat};
use std::collections::HashMap;

use egui::{Color32, FontId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Plain,
    Comment,
    Str,
    Number,
    Register,
    Mnemonic,
    Directive,
    Label,
    Emu,
}

#[derive(Clone, PartialEq)]
pub struct Palette {
    plain: Color32,
    comment: Color32,
    string: Color32,
    number: Color32,
    register: Color32,
    mnemonic: Color32,
    directive: Color32,
    label: Color32,
    emu: Color32,
}

impl Palette {
    pub fn for_theme(dark: bool) -> Self {
        if dark {
            Palette {
                plain: Color32::from_rgb(0xD4, 0xD4, 0xD4),
                comment: Color32::from_rgb(0x6A, 0x99, 0x55),
                string: Color32::from_rgb(0xCE, 0x91, 0x78),
                number: Color32::from_rgb(0xB5, 0xCE, 0xA8),
                register: Color32::from_rgb(0x9C, 0xDC, 0xFE),
                mnemonic: Color32::from_rgb(0x56, 0x9C, 0xD6),
                directive: Color32::from_rgb(0xC5, 0x86, 0xC0),
                label: Color32::from_rgb(0xDC, 0xDC, 0xAA),
                emu: Color32::from_rgb(0xD1, 0x69, 0x69),
            }
        } else {
            Palette {
                plain: Color32::from_rgb(0x20, 0x20, 0x20),
                comment: Color32::from_rgb(0x00, 0x80, 0x00),
                string: Color32::from_rgb(0xA3, 0x15, 0x15),
                number: Color32::from_rgb(0x09, 0x86, 0x58),
                register: Color32::from_rgb(0x00, 0x10, 0x80),
                mnemonic: Color32::from_rgb(0x00, 0x00, 0xFF),
                directive: Color32::from_rgb(0xAF, 0x00, 0xDB),
                label: Color32::from_rgb(0x79, 0x5E, 0x26),
                emu: Color32::from_rgb(0xC0, 0x40, 0x00),
            }
        }
    }

    fn color(&self, k: Kind) -> Color32 {
        match k {
            Kind::Plain => self.plain,
            Kind::Comment => self.comment,
            Kind::Str => self.string,
            Kind::Number => self.number,
            Kind::Register => self.register,
            Kind::Mnemonic => self.mnemonic,
            Kind::Directive => self.directive,
            Kind::Label => self.label,
            Kind::Emu => self.emu,
        }
    }
}

const DIRECTIVES: &[&str] = &[
    "ORG", "DB", "DW", "DD", "DQ", "DT", "EQU", "SEGMENT", "ENDS", "PROC", "ENDP", "END", "MACRO", "ENDM", "LOCAL",
    "INCLUDE", "ASSUME", "NAME", "TITLE", "LABEL", "PTR", "BYTE", "WORD", "DWORD", "NEAR", "FAR", "SHORT", "OFFSET",
    "SEG", "DUP", "EVEN", "ALIGN", ".MODEL", ".STACK", ".DATA", ".CODE", ".DATA?", "SMALL", "TINY", "IF", "IFDEF",
    "IFNDEF", "ELSE", "ENDIF", "REPT", "TYPE", "LENGTH", "SIZE", "MOD", "SHL", "SHR", "AND", "OR", "XOR", "NOT",
    "STACK", "PUBLIC", "LOW", "HIGH", "B.", "W.", // NASM
    "BITS", "USE16", "SECTION", "TIMES", "RESB", "RESW", "RESD", "RESQ", "REST", "ALIGNB", "CPU", "GLOBAL", "EXTERN",
    "QWORD", "TWORD", "STRICT", ".TEXT", ".BSS", ".RODATA", "STRUC", "ENDSTRUC", "ISTRUC", "IEND",
];

/// NASM preprocessor directives.
const NASM_PP: &[&str] = &[
    "%DEFINE",
    "%XDEFINE",
    "%UNDEF",
    "%ASSIGN",
    "%MACRO",
    "%ENDMACRO",
    "%REP",
    "%ENDREP",
    "%IF",
    "%ELIF",
    "%ELSE",
    "%ENDIF",
    "%IFDEF",
    "%IFNDEF",
    "%ELIFDEF",
    "%ELIFNDEF",
    "%INCLUDE",
    "%ERROR",
    "%WARNING",
];

fn is_register(u: &str) -> bool {
    matches!(
        u,
        "AX" | "BX"
            | "CX"
            | "DX"
            | "SI"
            | "DI"
            | "BP"
            | "SP"
            | "AL"
            | "AH"
            | "BL"
            | "BH"
            | "CL"
            | "CH"
            | "DL"
            | "DH"
            | "CS"
            | "DS"
            | "ES"
            | "SS"
    )
}

/// Classifies a word: instruction, directive, register or other.
fn word_kind(word: &str, first: bool, next_is_colon: bool) -> Kind {
    let u = word.to_ascii_uppercase();
    if is_register(&u) {
        return Kind::Register;
    }
    if DIRECTIVES.contains(&u.as_str()) {
        return Kind::Directive;
    }
    if rustemu_asm::encode::is_mnemonic(&u) {
        return Kind::Mnemonic;
    }
    if first && next_is_colon {
        return Kind::Label;
    }
    Kind::Plain
}

/// Builds a coloured layout for `text`.
pub fn highlight(text: &str, font: FontId, pal: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    for line in text.split_inclusive('\n') {
        append_line(&mut job, line, &line_spans(line), &font, pal);
    }
    job
}

/// Highlighting of the editor, kept between frames: unchanged text reuses the last layout, an
/// edit highlights only the lines that were not there before.
#[derive(Default)]
pub struct Highlighter {
    last: Option<(LayoutJob, FontId, Palette)>,
    lines: HashMap<String, Spans>,
}

impl Highlighter {
    pub fn job(&mut self, text: &str, font: &FontId, pal: &Palette) -> LayoutJob {
        if let Some((job, f, p)) = &self.last
            && job.text == text
            && f == font
            && p == pal
        {
            return job.clone();
        }
        // Spans of the lines that are gone are dropped.
        let mut old = std::mem::take(&mut self.lines);
        let mut job = LayoutJob::default();
        for line in text.split_inclusive('\n') {
            if !self.lines.contains_key(line) {
                let (k, v) = old.remove_entry(line).unwrap_or_else(|| (line.to_owned(), line_spans(line)));
                self.lines.insert(k, v);
            }
            append_line(&mut job, line, &self.lines[line], font, pal);
        }
        self.last = Some((job.clone(), font.clone(), pal.clone()));
        job
    }
}

/// Coloured parts of a line, back to back: (end byte, kind).
type Spans = Vec<(usize, Kind)>;

fn append_line(job: &mut LayoutJob, line: &str, spans: &Spans, font: &FontId, pal: &Palette) {
    let mut start = 0;
    for &(end, k) in spans {
        job.append(
            &line[start..end],
            0.0,
            TextFormat { font_id: font.clone(), color: pal.color(k), ..Default::default() },
        );
        start = end;
    }
}

fn line_spans(line: &str) -> Spans {
    let mut spans: Spans = Vec::new();
    // The part of the line from the end of the previous one up to `end` is of kind `k`.
    let mut push = |end: usize, k: Kind| match spans.last_mut() {
        Some(last) if last.1 == k => last.0 = end,
        _ => spans.push((end, k)),
    };
    let b = line.as_bytes();
    if line.trim_start().starts_with('#') {
        push(b.len(), Kind::Emu);
        return spans;
    }
    let mut i = 0;
    let mut first_word = true;
    while i < b.len() {
        let c = b[i];
        let start = i;
        if c == b';' {
            push(b.len(), Kind::Comment);
            break;
        }
        if c == b'%' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) {
            i += 1;
            while i < b.len() && b[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let word = &line[start..i];
            let k = if NASM_PP.contains(&word.to_ascii_uppercase().as_str()) { Kind::Directive } else { Kind::Plain };
            first_word = false;
            push(i, k);
            continue;
        }
        if c == b'\'' || c == b'"' || c == b'`' {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                i += 1;
            }
            i = (i + 1).min(b.len());
            push(i, Kind::Str);
            continue;
        }
        if c.is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_alphanumeric() {
                i += 1;
            }
            push(i, Kind::Number);
            continue;
        }
        if c.is_ascii_alphabetic() || matches!(c, b'_' | b'@' | b'.' | b'?') {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'@' | b'?' | b'$')) {
                i += 1;
            }
            if i < b.len() && b[i] == b'.' && i - start == 1 {
                i += 1;
            }
            let word = &line[start..i];
            let next_colon = b.get(i) == Some(&b':');
            let k = word_kind(word, first_word, next_colon);
            first_word = false;
            push(i, k);
            continue;
        }
        // Run of other characters.
        while i < b.len()
            && !(b[i].is_ascii_alphanumeric()
                || matches!(b[i], b'_' | b'@' | b'.' | b'?' | b';' | b'\'' | b'"' | b'`' | b'%'))
        {
            i += 1;
        }
        if i == start {
            i += 1;
        }
        push(i, Kind::Plain);
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(job: &LayoutJob) -> Vec<(String, Color32)> {
        // Neighbouring sections of one colour are merged, so compare the colour of each byte.
        let mut out = Vec::new();
        for s in &job.sections {
            for ch in job.text[s.byte_range.start.0..s.byte_range.end.0].chars() {
                out.push((ch.to_string(), s.format.color));
            }
        }
        out
    }

    #[test]
    fn cached_highlighting_matches_and_follows_edits() {
        let font = FontId::monospace(14.0);
        let pal = Palette::for_theme(false);
        let mut h = Highlighter::default();
        let mut text =
            String::from("#make_com#\norg 100h\nstart: mov ax, 5 ; five\n  db 'hi', 0Dh\n%define X 1\n\nret");
        for edit in ["", "\nnop", "\n\nret"] {
            text.push_str(edit);
            let job = h.job(&text, &font, &pal);
            assert_eq!(job.text, text);
            assert_eq!(colors(&job), colors(&highlight(&text, font.clone(), &pal)));
        }
        // Lines that are gone are not kept.
        text = "nop".into();
        h.job(&text, &font, &pal);
        assert_eq!(h.lines.len(), 1);
        // Another theme is not served from the cache.
        let dark = Palette::for_theme(true);
        assert_eq!(colors(&h.job(&text, &font, &dark)), colors(&highlight(&text, font.clone(), &dark)));
    }
}
