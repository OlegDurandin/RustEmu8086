//! The language of the reference and its Russian text. The English text is in the entries
//! themselves; the Russian one is looked up by name (instructions, directives), by number
//! (interrupts and functions) or by position (NASM guide). A missing translation falls back to
//! English (the tests check that there is none).

use serde::{Deserialize, Serialize};

use super::{Directive, Function, Group, Instr, Interrupt, NASM_GUIDE};

/// Language of the reference window and of the editor tooltips.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    En,
    Ru,
}

impl Lang {
    /// `en` or `ru`.
    pub fn pick(self, en: &'static str, ru: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Ru => ru,
        }
    }
}

/// Russian text of an instruction or a directive.
pub(super) struct Text {
    pub key: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    /// Empty: the English example (no comments to translate).
    pub example: &'static str,
}

/// Russian text of a section of the NASM guide.
pub(super) struct Section {
    pub title: &'static str,
    pub text: &'static str,
    pub table: &'static [&'static [&'static str]],
    pub example: &'static str,
}

pub(super) struct Int {
    pub number: u8,
    pub title: &'static str,
    pub description: &'static str,
}

pub(super) struct Func {
    pub number: u8,
    pub code: u16,
    pub title: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub example: &'static str,
}

fn text(table: &'static [Text], key: &str) -> Option<&'static Text> {
    table.iter().find(|t| t.key == key)
}

/// `ru` unless the language is English or the translation is missing.
fn choose(lang: Lang, en: &'static str, ru: Option<&'static str>) -> &'static str {
    match (lang, ru) {
        (Lang::Ru, Some(r)) if !r.is_empty() || en.is_empty() => r,
        _ => en,
    }
}

impl Instr {
    pub(super) fn ru(&self) -> Option<&'static Text> {
        text(super::ru_instructions::INSTRUCTIONS, self.name)
    }
    pub fn summary_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.summary, self.ru().map(|t| t.summary))
    }
    pub fn description_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.description, self.ru().map(|t| t.description))
    }
    pub fn example_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.example, self.ru().map(|t| t.example))
    }
}

impl Directive {
    pub(super) fn ru(&self) -> Option<&'static Text> {
        text(super::ru_directives::DIRECTIVES, self.name)
    }
    pub fn summary_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.summary, self.ru().map(|t| t.summary))
    }
    pub fn description_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.description, self.ru().map(|t| t.description))
    }
    pub fn example_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.example, self.ru().map(|t| t.example))
    }
    /// A NASM directive (listed in the NASM tab).
    pub fn is_nasm(&self) -> bool {
        self.summary.starts_with("NASM:")
    }
}

/// A section of the NASM guide in `lang`.
pub struct GuideSection {
    pub title: &'static str,
    pub text: &'static str,
    pub table: &'static [&'static [&'static str]],
    pub example: &'static str,
}

pub fn nasm_section(i: usize, lang: Lang) -> GuideSection {
    let en = &NASM_GUIDE[i];
    let ru = (lang == Lang::Ru).then(|| super::ru_directives::NASM_GUIDE.get(i)).flatten();
    match ru {
        Some(r) => GuideSection {
            title: r.title,
            text: r.text,
            table: if r.table.is_empty() { en.table } else { r.table },
            example: if r.example.is_empty() { en.example } else { r.example },
        },
        None => GuideSection { title: en.title, text: en.text, table: en.table, example: en.example },
    }
}

impl Interrupt {
    pub(super) fn ru(&self) -> Option<&'static Int> {
        super::ru_interrupts::INTERRUPTS.iter().find(|i| i.number == self.number)
    }
    pub fn title_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.title, self.ru().map(|t| t.title))
    }
    pub fn description_in(&self, lang: Lang) -> &'static str {
        choose(lang, self.description, self.ru().map(|t| t.description))
    }
}

/// A function of interrupt `number` in `lang`.
pub struct FunctionText {
    pub title: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub example: &'static str,
}

impl Function {
    pub fn text_in(&self, number: u8, lang: Lang) -> FunctionText {
        let ru = super::ru_interrupts::FUNCTIONS.iter().find(|f| f.number == number && f.code == self.code);
        FunctionText {
            title: choose(lang, self.title, ru.map(|f| f.title)),
            input: choose(lang, self.input, ru.map(|f| f.input)),
            output: choose(lang, self.output, ru.map(|f| f.output)),
            example: choose(lang, self.example, ru.map(|f| f.example)),
        }
    }
}

impl Group {
    pub fn title_in(self, lang: Lang) -> &'static str {
        if lang == Lang::En {
            return self.title();
        }
        match self {
            Group::Transfer => "Пересылка данных",
            Group::Arithmetic => "Арифметика",
            Group::Bcd => "Двоично-десятичная коррекция",
            Group::Logic => "Логика",
            Group::Shift => "Сдвиги",
            Group::String => "Строки",
            Group::Jump => "Переходы, вызовы, циклы",
            Group::Flags => "Управление флагами",
            Group::Control => "Управление процессором",
            Group::I186 => "Дополнения 80186",
        }
    }
}

/// Long names of the flags O D I T S Z A P C.
pub fn flag_titles(lang: Lang) -> [&'static str; 9] {
    match lang {
        Lang::En => {
            ["Overflow", "Direction", "Interrupt enable", "Trap", "Sign", "Zero", "Auxiliary carry", "Parity", "Carry"]
        }
        Lang::Ru => [
            "Переполнение",
            "Направление",
            "Разрешение прерываний",
            "Трассировка",
            "Знак",
            "Ноль",
            "Вспомогательный перенос",
            "Чётность",
            "Перенос",
        ],
    }
}

/// The words in the operand forms of an instruction (`sreg (not CS)`) in `lang`.
pub fn syntax_in(form: &'static str, lang: Lang) -> std::borrow::Cow<'static, str> {
    const WORDS: &[(&str, &str)] = &[
        ("(more than 1: repeated 1-bit shifts)", "(больше 1: несколько сдвигов на 1)"),
        ("(not CS)", "(не CS)"),
        ("(segment override allowed)", "(можно заменить сегмент)"),
        ("(sizes from the operands)", "(размер - по операндам)"),
        ("(short / near / far)", "(короткий / ближний / дальний)"),
        ("(near, indirect)", "(ближний, косвенный)"),
        ("(far, indirect)", "(дальний, косвенный)"),
        ("(far)", "(дальний)"),
        ("LOCK instruction", "LOCK инструкция"),
    ];
    match WORDS.iter().find(|(en, _)| form.contains(en)) {
        Some((en, ru)) if lang == Lang::Ru => form.replace(en, ru).into(),
        _ => form.into(),
    }
}
