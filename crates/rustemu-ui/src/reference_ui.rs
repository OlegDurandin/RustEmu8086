//! Reference window (Help menu, F1): for the 8086 instructions, interrupts, directives, the NASM
//! syntax guide and the quick reference; for MIPS32 instructions (with their forms and what the
//! pseudo-instructions become, from the assembler's table), system calls, directives and registers.

use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Align, FontId, RichText, Ui};

use crate::highlight::{Palette, highlight, highlight_mips};
use crate::reference::{
    self as r, DIRECTIVES, Group, INSTRUCTIONS, INTERRUPTS, Lang, MIPS_DIRECTIVES, MIPS_INSTRUCTIONS, MipsGroup,
    NASM_GUIDE, SYSCALLS, Topic,
};

/// Tabs of the reference window.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tab {
    /// Instructions by group.
    #[default]
    Instructions,
    /// BIOS and DOS interrupts.
    Interrupts,
    /// Directives and operators.
    Directives,
    /// The NASM syntax guide.
    Nasm,
    /// The quick reference (help text).
    Quick,
    /// MIPS: the services of `syscall`.
    Syscalls,
    /// MIPS: the registers and the memory map.
    Registers,
}

/// State of the reference window: the tab, the search and the article shown.
#[derive(Default)]
pub struct RefState {
    /// Bring the reference to the front (the panels layout shows it as a tab).
    pub raise: bool,
    /// The MIPS32 reference is shown (otherwise the 8086's); follows the workspace.
    pub mips: bool,
    /// The tab.
    pub tab: Tab,
    /// The search text.
    pub search: String,
    /// The article shown.
    pub topic: Option<Topic>,
    /// Language of the articles and of the editor tooltips (kept in the settings).
    pub lang: Lang,
    /// Scroll the list and the article to the selected topic on the next frame.
    scroll: bool,
}

impl RefState {
    /// Opens the window at the article `t`.
    pub fn show(&mut self, t: Topic) {
        self.raise = true;
        self.mips = t.is_mips();
        self.tab = tab_of(t);
        self.topic = Some(t);
        self.search.clear();
        self.scroll = true;
    }

    /// Shows the tab `tab`.
    pub fn show_tab(&mut self, tab: Tab) {
        self.raise = true;
        self.tab = tab;
    }
}

/// The tab of an article.
fn tab_of(t: Topic) -> Tab {
    match t {
        Topic::Instr(_) | Topic::MipsInstr(_) => Tab::Instructions,
        Topic::Directive(_) | Topic::MipsDirective(_) => Tab::Directives,
        Topic::Int(..) => Tab::Interrupts,
        Topic::Nasm(_) => Tab::Nasm,
        Topic::Syscall(_) => Tab::Syscalls,
    }
}

pub(crate) fn reference_ui(st: &mut RefState, ui: &mut Ui) {
    let l = st.lang;
    ui.horizontal(|ui| {
        // The processor; its own tabs.
        let mips = st.mips;
        ui.selectable_value(&mut st.mips, false, "8086").on_hover_text("Intel 8086 (Emu8086)");
        ui.selectable_value(&mut st.mips, true, "MIPS32").on_hover_text("MIPS32 (MARS)");
        if st.mips != mips {
            st.tab = Tab::Instructions;
            st.search.clear();
        }
        ui.separator();
        let tabs: &[(Tab, &str)] = if st.mips {
            &[
                (Tab::Instructions, l.pick("instructions", "инструкции")),
                (Tab::Syscalls, l.pick("system calls", "системные вызовы")),
                (Tab::Directives, l.pick("directives", "директивы")),
                (Tab::Registers, l.pick("registers & memory", "регистры и память")),
            ]
        } else {
            &[
                (Tab::Instructions, l.pick("instructions", "инструкции")),
                (Tab::Interrupts, l.pick("interrupts", "прерывания")),
                (Tab::Directives, l.pick("directives", "директивы")),
                (Tab::Nasm, l.pick("NASM syntax", "синтаксис NASM")),
                (Tab::Quick, l.pick("quick reference", "шпаргалка")),
            ]
        };
        if !tabs.iter().any(|(t, _)| *t == st.tab) {
            st.tab = Tab::Instructions;
        }
        for &(t, n) in tabs {
            ui.selectable_value(&mut st.tab, t, n);
        }
        ui.separator();
        if !matches!(st.tab, Tab::Quick | Tab::Registers) {
            ui.add(
                egui::TextEdit::singleline(&mut st.search).hint_text(l.pick("search", "поиск")).desired_width(160.0),
            );
        }
        // The language of the articles (and of the tooltips in the editor).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(&mut st.lang, Lang::Ru, "RU").on_hover_text("Справочник на русском");
            ui.selectable_value(&mut st.lang, Lang::En, "EN").on_hover_text("Reference in English");
        });
    });
    ui.separator();
    if st.tab == Tab::Quick {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(l.pick(crate::help::HELP, crate::help::HELP_RU)).monospace().size(12.5));
        });
        return;
    }
    if st.tab == Tab::Registers {
        egui::ScrollArea::vertical().id_salt("ref_registers").auto_shrink([false, false]).show(ui, |ui| {
            registers_article(ui, l);
        });
        return;
    }
    let scroll = std::mem::take(&mut st.scroll);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(260.0);
            egui::ScrollArea::vertical()
                .id_salt("ref_list")
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| match st.tab {
                    Tab::Instructions if st.mips => mips_instruction_list(st, ui, scroll),
                    Tab::Directives if st.mips => mips_directive_list(st, ui, scroll),
                    Tab::Syscalls => syscall_list(st, ui, scroll),
                    Tab::Instructions => instruction_list(st, ui, scroll),
                    Tab::Interrupts => interrupt_list(st, ui, scroll),
                    Tab::Nasm => nasm_list(st, ui, scroll),
                    _ => directive_list(st, ui, scroll),
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            // (The NASM tab lists NASM directives too.)
            let topic = st.topic.filter(|&t| {
                t.is_mips() == st.mips && (tab_of(t) == st.tab || (st.tab == Tab::Nasm && tab_of(t) == Tab::Directives))
            });
            let salt = match topic {
                Some(Topic::Int(i, _)) => format!("int{i}"),
                t => format!("{t:?}"),
            };
            egui::ScrollArea::vertical().id_salt(("ref_article", salt)).auto_shrink([false, false]).show(ui, |ui| {
                match topic {
                    Some(Topic::Instr(i)) => instruction_article(ui, i, l),
                    Some(Topic::Directive(i)) => directive_article(ui, i, l),
                    Some(Topic::Int(i, f)) => interrupt_article(ui, i, f, scroll, l),
                    Some(Topic::Nasm(i)) => nasm_article(ui, i, l),
                    Some(Topic::MipsInstr(i)) => mips_instruction_article(ui, i, l),
                    Some(Topic::MipsDirective(i)) => mips_directive_article(ui, i, l),
                    Some(Topic::Syscall(i)) => syscall_article(ui, i, l),
                    None if st.mips => mips_intro(ui, st.tab, l),
                    // The guide opens at its overview.
                    None if st.tab == Tab::Nasm => nasm_article(ui, 0, l),
                    None => intro(ui, st.tab, l),
                }
            });
        });
    });
}

fn matches(search: &str, texts: &[&str]) -> bool {
    let s = search.trim().to_lowercase();
    s.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(&s))
}

fn list_item(st: &mut RefState, ui: &mut Ui, topic: Topic, text: RichText, scroll: bool) {
    let sel = st.topic == Some(topic);
    let resp = ui.selectable_label(sel, text);
    if resp.clicked() {
        st.topic = Some(topic);
    }
    if sel && scroll {
        resp.scroll_to_me(Some(Align::Center));
    }
}

fn instruction_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    let l = st.lang;
    for g in Group::ALL {
        let items: Vec<usize> = (0..INSTRUCTIONS.len())
            .filter(|&i| INSTRUCTIONS[i].group == g)
            .filter(|&i| {
                let e = &INSTRUCTIONS[i];
                let mut texts = vec![e.name, e.summary_in(l)];
                texts.extend(e.aliases);
                matches(&st.search, &texts)
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        ui.label(RichText::new(g.title_in(l)).strong());
        for i in items {
            let e = &INSTRUCTIONS[i];
            let mut name = e.name.to_string();
            for a in e.aliases {
                name.push_str(" / ");
                name.push_str(a);
            }
            list_item(st, ui, Topic::Instr(i), RichText::new(name).monospace(), scroll);
        }
        ui.add_space(4.0);
    }
}

fn interrupt_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    let l = st.lang;
    let searching = !st.search.trim().is_empty();
    for (i, int) in INTERRUPTS.iter().enumerate() {
        let funcs: Vec<&r::Function> = int
            .functions
            .iter()
            .filter(|f| {
                !searching || matches(&st.search, &[f.text_in(int.number, l).title, &r::hex(f.code, int.selector)])
            })
            .collect();
        let head_match = matches(&st.search, &[int.title_in(l), &format!("{:02X}h", int.number)]);
        if !head_match && funcs.is_empty() {
            continue;
        }
        list_item(
            st,
            ui,
            Topic::Int(i, None),
            RichText::new(format!("INT {:02X}h  {}", int.number, int.title_in(l))),
            scroll,
        );
        let expanded = searching || matches!(st.topic, Some(Topic::Int(j, _)) if j == i);
        if expanded {
            ui.indent(("int_funcs", i), |ui| {
                for f in funcs {
                    let text =
                        format!("{}={} {}", int.selector, r::hex(f.code, int.selector), f.text_in(int.number, l).title);
                    list_item(st, ui, Topic::Int(i, Some(f.code)), RichText::new(text), scroll);
                }
            });
        }
    }
}

fn directive_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    for (i, d) in DIRECTIVES.iter().enumerate() {
        let mut texts = vec![d.name, d.summary_in(st.lang)];
        texts.extend(d.aliases);
        if matches(&st.search, &texts) {
            list_item(st, ui, Topic::Directive(i), RichText::new(d.name).monospace(), scroll);
        }
    }
}

/// The sections of the NASM guide and the NASM directives.
fn nasm_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    let l = st.lang;
    for i in 0..NASM_GUIDE.len() {
        let s = r::nasm_section(i, l);
        if matches(&st.search, &[s.title, s.text, s.example]) {
            list_item(st, ui, Topic::Nasm(i), RichText::new(s.title), scroll);
        }
    }
    ui.add_space(6.0);
    ui.label(RichText::new(l.pick("NASM directives", "Директивы NASM")).strong());
    for (i, d) in DIRECTIVES.iter().enumerate() {
        let mut texts = vec![d.name, d.summary_in(l)];
        texts.extend(d.aliases);
        if d.is_nasm() && matches(&st.search, &texts) {
            list_item(st, ui, Topic::Directive(i), RichText::new(d.name).monospace(), scroll);
        }
    }
}

fn nasm_article(ui: &mut Ui, i: usize, l: Lang) {
    let s = r::nasm_section(i, l);
    ui.weak(l.pick("NASM syntax", "Синтаксис NASM"));
    ui.heading(s.title);
    for paragraph in s.text.split("\n\n") {
        ui.add_space(6.0);
        for line in paragraph.lines() {
            match line.strip_prefix("- ") {
                Some(item) => crate::utility::bullet(ui, item),
                None => {
                    ui.label(line);
                }
            }
        }
    }
    if let Some((head, rows)) = s.table.split_first() {
        ui.add_space(8.0);
        egui::Grid::new(("nasm_table", i)).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
            for h in *head {
                ui.label(RichText::new(*h).strong());
            }
            ui.end_row();
            for row in rows {
                for (c, cell) in row.iter().enumerate() {
                    // The first column is code.
                    if c == 0 {
                        ui.monospace(*cell)
                    } else {
                        ui.label(*cell)
                    };
                }
                ui.end_row();
            }
        });
    }
    if !s.example.is_empty() {
        section(ui, l.pick("Example", "Пример"));
        code_block(ui, s.example, l, false);
    }
}

fn intro(ui: &mut Ui, tab: Tab, l: Lang) {
    ui.label(match tab {
        Tab::Instructions => l.pick(
            "8086 instructions (and the 80186 additions accepted by Emu8086): syntax, flags, examples.",
            "Инструкции 8086 (и дополнения 80186, которые понимает Emu8086): синтаксис, флаги, примеры.",
        ),
        Tab::Interrupts => l.pick(
            "BIOS and DOS services available to programs in the emulator.",
            "Функции BIOS и DOS, доступные программам в эмуляторе.",
        ),
        _ => l.pick("Assembler directives and operators.", "Директивы и операторы ассемблера."),
    });
    ui.add_space(6.0);
    ui.weak(l.pick(
        "In the editor: hover over an instruction, a directive or INT nn to see a summary; press F1 or ⌘-click to open its article here.",
        "В редакторе: наведите мышь на инструкцию, директиву или INT nn — появится краткое описание; F1 или ⌘-клик открывает статью здесь.",
    ));
}

fn also(ui: &mut Ui, aliases: &[&str], l: Lang) {
    if !aliases.is_empty() {
        ui.weak(format!("{} {}", l.pick("also:", "также:"), aliases.join(", ")));
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong());
}

fn instruction_article(ui: &mut Ui, i: usize, l: Lang) {
    let e = &INSTRUCTIONS[i];
    ui.horizontal(|ui| {
        ui.heading(RichText::new(e.name).monospace());
        ui.weak(e.group.title_in(l));
    });
    also(ui, e.aliases, l);
    ui.label(RichText::new(e.summary_in(l)).strong());
    section(ui, l.pick("Syntax", "Синтаксис"));
    if e.syntax.is_empty() {
        ui.monospace(e.name);
    }
    for s in e.syntax {
        ui.monospace(if s.is_empty() { e.name.to_string() } else { format!("{} {}", e.name, r::syntax_in(s, l)) });
    }
    let description = e.description_in(l);
    if !description.is_empty() {
        ui.add_space(6.0);
        ui.label(description);
    }
    section(ui, l.pick("Flags", "Флаги"));
    flags_table(ui, e.flags, l);
    section(ui, l.pick("Example", "Пример"));
    code_block(ui, e.example_in(l), l, false);
}

/// The flags table of an instruction (O D I T S Z A P C with their effect).
pub fn flags_table(ui: &mut Ui, flags: &str, l: Lang) {
    let changed = ui.visuals().hyperlink_color;
    let undefined = crate::colors::warn(ui);
    let titles = r::flag_titles(l);
    egui::Grid::new("flags_table").spacing([10.0, 2.0]).show(ui, |ui| {
        for (n, t) in r::FLAG_NAMES.iter().zip(titles) {
            ui.label(RichText::new(*n).monospace().strong()).on_hover_text(t);
        }
        ui.end_row();
        for ((c, n), t) in flags.chars().zip(r::FLAG_NAMES).zip(titles) {
            let text = RichText::new(match c {
                '-' => "-".to_string(),
                'u' => "?".to_string(),
                c => c.to_string(),
            })
            .monospace();
            let text = match c {
                'r' | '0' | '1' => text.color(changed).strong(),
                'u' => text.color(undefined),
                _ => text.weak(),
            };
            ui.label(text).on_hover_text(format!("{n} ({t}): {}", r::flag_effect(c, l)));
        }
        ui.end_row();
    });
    ui.weak(l.pick(
        "r = set by the result   0 / 1 = cleared / set   ? = undefined   - = not changed",
        "r = по результату   0 / 1 = сбрасывается / устанавливается   ? = не определён   - = не меняется",
    ));
}

fn code_block(ui: &mut Ui, code: &str, l: Lang, mips: bool) {
    let pal = Palette::for_theme(ui.visuals().dark_mode);
    let job = if mips {
        highlight_mips(code, FontId::monospace(12.5), &pal)
    } else {
        highlight(code, FontId::monospace(12.5), &pal)
    };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add(egui::Label::new(job));
            ui.add_space(16.0);
            if ui
                .small_button(l.pick("copy", "копировать"))
                .on_hover_text(l.pick("copy the example to the clipboard", "скопировать пример в буфер обмена"))
                .clicked()
            {
                ui.ctx().copy_text(code.to_string());
            }
        });
    });
}

fn directive_article(ui: &mut Ui, i: usize, l: Lang) {
    let d = &DIRECTIVES[i];
    ui.heading(RichText::new(d.name).monospace());
    also(ui, d.aliases, l);
    ui.label(RichText::new(d.summary_in(l)).strong());
    ui.add_space(6.0);
    ui.label(d.description_in(l));
    section(ui, l.pick("Example", "Пример"));
    code_block(ui, d.example_in(l), l, false);
}

fn interrupt_article(ui: &mut Ui, i: usize, func: Option<u16>, scroll: bool, l: Lang) {
    let int = &INTERRUPTS[i];
    ui.heading(RichText::new(format!("INT {:02X}h", int.number)).monospace());
    ui.label(RichText::new(int.title_in(l)).strong());
    ui.label(int.description_in(l));
    if int.functions.is_empty() {
        section(ui, l.pick("Example", "Пример"));
        code_block(ui, &format!("int {:02X}h", int.number), l, false);
        return;
    }
    ui.weak(format!("{} {}.", l.pick("The function number is passed in", "Номер функции передаётся в"), int.selector));
    let band = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(38, 46, 60)
    } else {
        egui::Color32::from_rgb(232, 240, 252)
    };
    for f in int.functions {
        ui.add_space(6.0);
        let sel = func == Some(f.code);
        let frame = egui::Frame::group(ui.style()).fill(if sel { band } else { egui::Color32::TRANSPARENT });
        let resp = frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            let t = f.text_in(int.number, l);
            ui.label(
                RichText::new(format!("{} = {}   {}", int.selector, r::hex(f.code, int.selector), t.title)).strong(),
            );
            egui::Grid::new(("func", f.code)).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                for (k, v) in [(l.pick("input", "вход"), t.input), (l.pick("output", "выход"), t.output)] {
                    if !v.is_empty() {
                        ui.weak(k);
                        ui.label(v);
                        ui.end_row();
                    }
                }
            });
            code_block(ui, t.example, l, false);
        });
        if sel && scroll {
            ui.scroll_to_rect(resp.response.rect.expand(8.0), Some(Align::TOP));
        }
    }
}

// ---- MIPS32 -------------------------------------------------------------------------------------

fn mips_instruction_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    let l = st.lang;
    for g in MipsGroup::ALL {
        let items: Vec<usize> = (0..MIPS_INSTRUCTIONS.len())
            .filter(|&i| MIPS_INSTRUCTIONS[i].group == g)
            .filter(|&i| {
                let e = &MIPS_INSTRUCTIONS[i];
                let mut texts = vec![e.name, e.summary_in(l)];
                texts.extend(e.aliases);
                matches(&st.search, &texts)
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        ui.label(RichText::new(g.title_in(l)).strong());
        for i in items {
            let e = &MIPS_INSTRUCTIONS[i];
            let name = std::iter::once(e.name).chain(e.aliases.iter().copied()).collect::<Vec<_>>().join(" / ");
            list_item(st, ui, Topic::MipsInstr(i), RichText::new(name).monospace(), scroll);
        }
        ui.add_space(4.0);
    }
}

fn mips_directive_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    for (i, d) in MIPS_DIRECTIVES.iter().enumerate() {
        if matches(&st.search, &[d.name, d.summary_in(st.lang)]) {
            list_item(st, ui, Topic::MipsDirective(i), RichText::new(d.name).monospace(), scroll);
        }
    }
}

fn syscall_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    let l = st.lang;
    for (i, s) in SYSCALLS.iter().enumerate() {
        let t = s.text_in(l);
        if !matches(&st.search, &[t.name, s.name, &s.code.to_string()]) {
            continue;
        }
        let text = RichText::new(format!("{:>2}  {}", s.code, t.name)).monospace();
        // The services the emulator does not perform yet are grey.
        let text = if s.supported { text } else { text.weak() };
        list_item(st, ui, Topic::Syscall(i), text, scroll);
    }
}

fn mips_intro(ui: &mut Ui, tab: Tab, l: Lang) {
    ui.label(match tab {
        Tab::Instructions => l.pick(
            "MIPS32 instructions as MARS assembles them: the machine instructions and the pseudo-instructions the assembler turns into them. Each article lists the forms and what they become.",
            "Инструкции MIPS32 так, как их собирает MARS: машинные инструкции и псевдоинструкции, которые ассемблер в них превращает. В каждой статье — формы и во что они собираются.",
        ),
        Tab::Syscalls => l.pick(
            "The services of syscall: put the number in $v0, the arguments in $a0..$a3, then execute syscall. Grey ones are not supported yet.",
            "Сервисы syscall: номер — в $v0, аргументы — в $a0..$a3, затем syscall. Серые пока не поддерживаются.",
        ),
        _ => l.pick("Directives of the MARS assembler.", "Директивы ассемблера MARS."),
    });
    ui.add_space(6.0);
    ui.weak(l.pick(
        "In the editor: hover over an instruction, a directive or a syscall to see a summary; press F1 or ⌘-click to open its article here.",
        "В редакторе: наведите мышь на инструкцию, директиву или syscall — появится краткое описание; F1 или ⌘-клик открывает статью здесь.",
    ));
}

/// What the assembler makes of an example form: its machine instructions (None when it does not
/// assemble alone). Cached: the forms are drawn every frame.
fn expansion(example: &str) -> Option<Vec<String>> {
    thread_local! {
        static CACHE: RefCell<HashMap<String, Option<Vec<String>>>> = RefCell::new(HashMap::new());
    }
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(example.to_string())
            .or_insert_with(|| {
                // The forms branch to `label`.
                let p = rustemu_mips::asm::assemble(&format!(".text\n{example}\nlabel:\n")).ok()?;
                let st = p.statements.first()?;
                Some(
                    st.words
                        .iter()
                        .enumerate()
                        .map(|(k, &w)| {
                            rustemu_mips::disasm::disassemble(
                                w,
                                st.addr + 4 * k as u32,
                                rustemu_mips::disasm::Style::Named,
                            )
                        })
                        .collect(),
                )
            })
            .clone()
    })
}

fn mips_instruction_article(ui: &mut Ui, i: usize, l: Lang) {
    let e = &MIPS_INSTRUCTIONS[i];
    ui.horizontal(|ui| {
        ui.heading(RichText::new(e.name).monospace());
        ui.weak(e.group.title_in(l));
    });
    also(ui, e.aliases, l);
    ui.label(RichText::new(e.summary_in(l)).strong());
    ui.add_space(6.0);
    ui.label(e.description_in(l));
    section(ui, l.pick("Forms", "Формы"));
    ui.weak(l.pick(
        "As MARS's table writes them; a pseudo-instruction shows the machine instructions it becomes.",
        "Как их записывает таблица MARS; у псевдоинструкции показано, в какие машинные инструкции она собирается.",
    ));
    let table = rustemu_mips::asm::Table::get();
    egui::Grid::new(("mips_forms", i)).striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
        for name in std::iter::once(e.name).chain(e.aliases.iter().copied()) {
            for f in table.forms(name).unwrap_or_default() {
                ui.monospace(&f.example);
                match &f.def {
                    rustemu_mips::asm::Def::Basic(_) => {
                        ui.weak(l.pick("machine instruction", "машинная инструкция"));
                    }
                    rustemu_mips::asm::Def::Pseudo { .. } => {
                        let text =
                            expansion(&f.example).map_or_else(|| "?".to_string(), |v| format!("→ {}", v.join("; ")));
                        ui.label(RichText::new(text).monospace().color(ui.visuals().hyperlink_color));
                    }
                }
                ui.end_row();
            }
        }
    });
    section(ui, l.pick("Example", "Пример"));
    code_block(ui, e.example_in(l), l, true);
}

fn mips_directive_article(ui: &mut Ui, i: usize, l: Lang) {
    let d = &MIPS_DIRECTIVES[i];
    ui.heading(RichText::new(d.name).monospace());
    ui.label(RichText::new(d.summary_in(l)).strong());
    ui.add_space(6.0);
    ui.label(d.description_in(l));
    section(ui, l.pick("Example", "Пример"));
    code_block(ui, d.example_in(l), l, true);
}

fn syscall_article(ui: &mut Ui, i: usize, l: Lang) {
    let s = &SYSCALLS[i];
    let t = s.text_in(l);
    ui.heading(RichText::new(format!("syscall {}", s.code)).monospace());
    ui.label(RichText::new(t.name).strong());
    if !s.supported {
        ui.colored_label(
            crate::colors::warn(ui),
            l.pick(
                "Not supported yet: the program stops with an error.",
                "Пока не поддерживается: программа останавливается с ошибкой.",
            ),
        );
    }
    ui.add_space(6.0);
    egui::Grid::new(("syscall", s.code)).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
        ui.weak("$v0");
        ui.monospace(s.code.to_string());
        ui.end_row();
        for (k, v) in [(l.pick("arguments", "аргументы"), t.input), (l.pick("result", "результат"), t.output)]
        {
            if !v.is_empty() {
                ui.weak(k);
                ui.label(v);
                ui.end_row();
            }
        }
    });
    if !t.note.is_empty() {
        ui.add_space(6.0);
        ui.label(t.note);
    }
    if !t.example.is_empty() {
        section(ui, l.pick("Example", "Пример"));
        code_block(ui, t.example, l, true);
    }
}

/// The registers (number, name, use), PC / HI / LO and the memory map of MARS.
fn registers_article(ui: &mut Ui, l: Lang) {
    ui.heading(l.pick("Registers", "Регистры"));
    ui.label(l.pick(
        "32 general purpose registers of 32 bits. Write them by name ($t0) or number ($8); the use of each is a convention of the MIPS calling standard, except $zero, which is always 0.",
        "32 регистра общего назначения по 32 бита. Их пишут по имени ($t0) или номеру ($8); назначение каждого — соглашение о вызовах MIPS, кроме $zero, который всегда равен 0.",
    ));
    ui.add_space(6.0);
    egui::Grid::new("mips_regs_ref").striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
        for h in [l.pick("number", "номер"), l.pick("name", "имя"), l.pick("use", "назначение")] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for (n, name) in rustemu_mips::cpu::REG_NAMES.iter().enumerate() {
            ui.monospace(format!("${n}"));
            ui.monospace(format!("${name}"));
            ui.label(r::register_role(n, l));
            ui.end_row();
        }
        for (name, en, ru) in [
            (
                "PC",
                "program counter: the address of the next instruction",
                "счётчик команд: адрес следующей инструкции",
            ),
            ("HI", "mult: the high word; div: the remainder", "mult: старшее слово; div: остаток"),
            ("LO", "mult: the low word; div: the quotient", "mult: младшее слово; div: частное"),
        ] {
            ui.label("");
            ui.monospace(name);
            ui.label(l.pick(en, ru));
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(l.pick(
        "Procedures: arguments in $a0..$a3, results in $v0 (and $v1); jal puts the return address in $ra, jr $ra returns. A procedure keeps $s0..$s7, $sp, $fp and $ra as they were (saving them on the stack when it uses them); $t0..$t9 may change.",
        "Процедуры: аргументы — в $a0..$a3, результат — в $v0 (и $v1); jal кладёт адрес возврата в $ra, jr $ra возвращает. Процедура сохраняет $s0..$s7, $sp, $fp и $ra (кладёт их в стек, если использует); $t0..$t9 могут измениться.",
    ));
    ui.add_space(4.0);
    ui.label(l.pick(
        "Coprocessor 1 (floating point): 32 registers $f0..$f31 of 32 bits, each holding a float; a double takes an even register and the next one ($f2 = $f2:$f3, the low word in the even one). By convention results go in $f0 (and $f2), arguments in $f12 and $f14, $f20..$f30 are saved by a procedure, the others are temporary. 8 condition flags (0..7) hold the results of c.eq, c.lt, c.le for bc1t / bc1f and movt / movf. The FPU group of the registers window shows them all.",
        "Сопроцессор 1 (плавающая точка): 32 регистра $f0..$f31 по 32 бита, в каждом — число float; double занимает чётный регистр и следующий ($f2 = $f2:$f3, младшее слово — в чётном). По соглашению результат — в $f0 (и $f2), аргументы — в $f12 и $f14, $f20..$f30 процедура сохраняет, остальные временные. 8 флагов условий (0..7) хранят результаты c.eq, c.lt, c.le для bc1t / bc1f и movt / movf. Всё это показывает группа FPU в окне регистров.",
    ));
    section(ui, l.pick("Memory (as MARS lays it out)", "Память (как её размечает MARS)"));
    egui::Grid::new("mips_memory_ref").striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
        for (addr, en, ru) in [
            (
                "0x00400000",
                ".text: the program's instructions; execution starts here",
                ".text: инструкции программы; выполнение начинается здесь",
            ),
            ("0x10000000", "global area (.extern)", "глобальная область (.extern)"),
            ("0x10008000", "$gp points here", "сюда указывает $gp"),
            ("0x10010000", ".data: the program's data", ".data: данные программы"),
            (
                "0x10040000",
                "heap: the blocks of sbrk (syscall 9), growing up",
                "куча: блоки sbrk (syscall 9), растёт вверх",
            ),
            ("0x7FFFEFFC", "$sp at the start: the stack grows down", "$sp в начале: стек растёт вниз"),
            ("0x80000000", ".ktext: kernel code", ".ktext: код ядра"),
            (
                "0x80000180",
                "the exception handler, when there is code there",
                "обработчик исключений, если там есть код",
            ),
            ("0x90000000", ".kdata: kernel data", ".kdata: данные ядра"),
            (
                "0xFFFF0000",
                "memory-mapped I/O: the keyboard and display (below)",
                "ввод-вывод через память: клавиатура и дисплей (ниже)",
            ),
        ] {
            ui.monospace(addr);
            ui.label(l.pick(en, ru));
            ui.end_row();
        }
    });
    ui.weak(l.pick(
        "Words are little-endian. Reading or writing outside these segments, or a word at an address that is not a multiple of 4, raises an address error exception.",
        "Слова хранятся младшим байтом вперёд. Чтение или запись вне этих сегментов, а также слово по адресу, не кратному 4, вызывают исключение address error.",
    ));
    devices_article(ui, l);
}

/// MARS's memory-mapped devices: the Keyboard and Display MMIO Simulator and the Bitmap Display.
fn devices_article(ui: &mut Ui, l: Lang) {
    section(ui, l.pick("Memory-mapped devices", "Устройства в памяти"));
    ui.label(l.pick(
        "The Keyboard and Display MMIO Simulator (Tools menu of the emulator window) is four words of memory:",
        "Keyboard and Display MMIO Simulator (меню tools окна эмулятора) — это четыре слова памяти:",
    ));
    egui::Grid::new("mips_mmio_ref").striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
        for (addr, en, ru) in [
            (
                "0xFFFF0000",
                "receiver control: bit 0 — a key is ready, bit 1 — interrupt enable",
                "управление приёмником: бит 0 — есть клавиша, бит 1 — разрешение прерывания",
            ),
            (
                "0xFFFF0004",
                "receiver data: the key's character; reading it (lw, lb at this address) clears the ready bit",
                "данные приёмника: символ клавиши; чтение (lw, lb по этому адресу) сбрасывает бит готовности",
            ),
            (
                "0xFFFF0008",
                "transmitter control: bit 0 — the display is ready, bit 1 — interrupt enable",
                "управление передатчиком: бит 0 — дисплей готов, бит 1 — разрешение прерывания",
            ),
            (
                "0xFFFF000C",
                "transmitter data: a character stored here (sw, sb) while the display is ready is shown after the delay",
                "данные передатчика: символ, записанный сюда (sw, sb), пока дисплей готов, показывается после задержки",
            ),
        ] {
            ui.monospace(addr);
            ui.label(l.pick(en, ru));
            ui.end_row();
        }
    });
    ui.label(l.pick(
        "A program polls the ready bit before reading a key or storing a character. The display is busy for the delay set in the tool (5 instructions by default). Character 12 (form feed) clears the display; 7 (bell) makes it a terminal and puts the cursor at column = bits 20..31 and row = bits 8..19 of the stored word.",
        "Программа опрашивает бит готовности, прежде чем читать клавишу или записывать символ. Дисплей занят на время задержки, заданной в окне инструмента (по умолчанию 5 инструкций). Символ 12 (form feed) очищает дисплей; 7 (bell) превращает его в терминал и ставит курсор в столбец = биты 20..31 и строку = биты 8..19 записанного слова.",
    ));
    ui.label(l.pick(
        "Interrupts: with bit 1 of a control register and bit 0 of Status set (and not inside a handler), a key, or the display becoming ready, interrupts the program before its next instruction: Cause gets code 0 with bit 8 (keyboard) or 9 (display), EPC the instruction's address, and execution goes to the handler at 0x80000180; eret returns.",
        "Прерывания: если установлены бит 1 регистра управления и бит 0 Status (и программа не в обработчике), клавиша или готовность дисплея прерывают программу перед следующей инструкцией: в Cause — код 0 и бит 8 (клавиатура) или 9 (дисплей), в EPC — адрес инструкции, выполнение переходит в обработчик по адресу 0x80000180; eret возвращает.",
    ));
    ui.code(
        "        li   $t0, 0xffff0000\nwait:   lw   $t1, 8($t0)        # transmitter control\n        andi $t1, $t1, 1\n        beqz $t1, wait          # not ready yet\n        sw   $a0, 12($t0)       # display the character in $a0",
    );
    ui.add_space(4.0);
    ui.label(l.pick(
        "The Bitmap Display (Tools menu) shows words of memory as pixels, row by row from its base address (0x10010000 by default): 0x00RRGGBB. Its window sets the size of a unit (one word) and of the display in pixels: 512 x 256 with 1 x 1 units is 512 x 256 words.",
        "Bitmap Display (меню tools) показывает слова памяти как пиксели, строка за строкой от базового адреса (по умолчанию 0x10010000): 0x00RRGGBB. В его окне задаются размер единицы (одно слово) и дисплея в пикселях: 512 x 256 при единице 1 x 1 — это 512 x 256 слов.",
    ));
    ui.add_space(4.0);
    ui.label(l.pick(
        "The Digital Lab Sim (Tools menu): bytes 0xFFFF0010 and 0xFFFF0011 light the segments of the right and the left display (bit 0 = a … bit 6 = g, bit 7 = the point). A program scans the hexadecimal keyboard by storing a row (1, 2, 4, 8) to 0xFFFF0012 and reading 0xFFFF0014: the row bit and the column bit << 4 of the key pressed in that row, 0 if none (key 2: 0x41). Bits 4..7 of the row byte enable the keyboard interrupt (Cause bit 11). A non-zero byte at 0xFFFF0013 starts the counter: an interrupt (Cause bit 10) every 30 instructions of .text and accesses to these bytes. These interrupts only need the program to be outside the handler.",
        "Digital Lab Sim (меню tools): байты 0xFFFF0010 и 0xFFFF0011 зажигают сегменты правого и левого индикатора (бит 0 = a … бит 6 = g, бит 7 = точка). Программа опрашивает шестнадцатеричную клавиатуру, записывая строку (1, 2, 4, 8) в 0xFFFF0012 и читая 0xFFFF0014: бит строки и бит столбца << 4 нажатой в этой строке клавиши, 0 — если её нет (клавиша 2: 0x41). Биты 4..7 байта строки разрешают прерывание клавиатуры (бит 11 Cause). Ненулевой байт в 0xFFFF0013 запускает счётчик: прерывание (бит 10 Cause) каждые 30 инструкций из .text и обращений к этим байтам. Для этих прерываний программе достаточно быть вне обработчика.",
    ));
}
