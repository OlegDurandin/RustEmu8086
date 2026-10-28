//! Syntax highlighting for 8086 assembly.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

#[derive(Clone, Copy, PartialEq, Eq)]
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
    "QWORD", "TWORD", "STRICT", ".TEXT", ".BSS", ".RODATA",
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
    let mut push = |s: &str, k: Kind| {
        if !s.is_empty() {
            job.append(s, 0.0, TextFormat { font_id: font.clone(), color: pal.color(k), ..Default::default() });
        }
    };
    for line in text.split_inclusive('\n') {
        let b = line.as_bytes();
        if line.trim_start().starts_with('#') {
            push(line, Kind::Emu);
            continue;
        }
        let mut i = 0;
        let mut first_word = true;
        while i < b.len() {
            let c = b[i];
            let start = i;
            if c == b';' {
                push(&line[i..], Kind::Comment);
                break;
            }
            if c == b'%' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) {
                i += 1;
                while i < b.len() && b[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let word = &line[start..i];
                let k =
                    if NASM_PP.contains(&word.to_ascii_uppercase().as_str()) { Kind::Directive } else { Kind::Plain };
                first_word = false;
                push(word, k);
                continue;
            }
            if c == b'\'' || c == b'"' || c == b'`' {
                i += 1;
                while i < b.len() && b[i] != c && b[i] != b'\n' {
                    i += 1;
                }
                i = (i + 1).min(b.len());
                push(&line[start..i], Kind::Str);
                continue;
            }
            if c.is_ascii_digit() {
                while i < b.len() && b[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                push(&line[start..i], Kind::Number);
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
                push(word, k);
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
            push(&line[start..i], Kind::Plain);
        }
    }
    job
}
