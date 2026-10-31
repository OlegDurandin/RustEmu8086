//! Reference window (Help menu, F1): instructions, interrupts, directives, the NASM syntax guide
//! and the quick reference.

use egui::{Align, FontId, RichText, Ui};

use crate::highlight::{Palette, highlight};
use crate::reference::{self as r, DIRECTIVES, Group, INSTRUCTIONS, INTERRUPTS, Lang, NASM_GUIDE, Topic};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tab {
    #[default]
    Instructions,
    Interrupts,
    Directives,
    Nasm,
    Quick,
}

#[derive(Default)]
pub struct RefState {
    pub open: bool,
    /// Bring the reference to the front (the panels layout shows it as a tab).
    pub raise: bool,
    pub tab: Tab,
    pub search: String,
    pub topic: Option<Topic>,
    /// Language of the articles and of the editor tooltips (kept in the settings).
    pub lang: Lang,
    /// Scroll the list and the article to the selected topic on the next frame.
    scroll: bool,
}

impl RefState {
    /// Opens the window at the article `t`.
    pub fn show(&mut self, t: Topic) {
        self.open = true;
        self.raise = true;
        self.tab = match t {
            Topic::Instr(_) => Tab::Instructions,
            Topic::Directive(_) => Tab::Directives,
            Topic::Int(..) => Tab::Interrupts,
            Topic::Nasm(_) => Tab::Nasm,
        };
        self.topic = Some(t);
        self.search.clear();
        self.scroll = true;
    }

    pub fn show_tab(&mut self, tab: Tab) {
        self.open = true;
        self.raise = true;
        self.tab = tab;
    }
}

pub fn reference_window(st: &mut RefState, ctx: &egui::Context) {
    let mut open = st.open;
    egui::Window::new("reference")
        .open(&mut open)
        .default_pos(egui::pos2(140.0, 80.0))
        .default_size([880.0, 580.0])
        .show(ctx, |ui| reference_ui(st, ui));
    st.open = open;
}

pub(crate) fn reference_ui(st: &mut RefState, ui: &mut Ui) {
    let l = st.lang;
    ui.horizontal(|ui| {
        for (t, n) in [
            (Tab::Instructions, l.pick("instructions", "инструкции")),
            (Tab::Interrupts, l.pick("interrupts", "прерывания")),
            (Tab::Directives, l.pick("directives", "директивы")),
            (Tab::Nasm, l.pick("NASM syntax", "синтаксис NASM")),
            (Tab::Quick, l.pick("quick reference", "шпаргалка")),
        ] {
            ui.selectable_value(&mut st.tab, t, n);
        }
        ui.separator();
        if st.tab != Tab::Quick {
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
    let scroll = std::mem::take(&mut st.scroll);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(260.0);
            egui::ScrollArea::vertical()
                .id_salt("ref_list")
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| match st.tab {
                    Tab::Instructions => instruction_list(st, ui, scroll),
                    Tab::Interrupts => interrupt_list(st, ui, scroll),
                    Tab::Nasm => nasm_list(st, ui, scroll),
                    _ => directive_list(st, ui, scroll),
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            let topic = st.topic.filter(|t| {
                matches!(
                    (t, st.tab),
                    (Topic::Instr(_), Tab::Instructions)
                        | (Topic::Directive(_), Tab::Directives | Tab::Nasm)
                        | (Topic::Int(..), Tab::Interrupts)
                        | (Topic::Nasm(_), Tab::Nasm)
                )
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
                Some(item) => crate::windows::bullet(ui, item),
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
        code_block(ui, s.example, l);
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
    code_block(ui, e.example_in(l), l);
}

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

fn code_block(ui: &mut Ui, code: &str, l: Lang) {
    let pal = Palette::for_theme(ui.visuals().dark_mode);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add(egui::Label::new(highlight(code, FontId::monospace(12.5), &pal)));
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
    code_block(ui, d.example_in(l), l);
}

fn interrupt_article(ui: &mut Ui, i: usize, func: Option<u16>, scroll: bool, l: Lang) {
    let int = &INTERRUPTS[i];
    ui.heading(RichText::new(format!("INT {:02X}h", int.number)).monospace());
    ui.label(RichText::new(int.title_in(l)).strong());
    ui.label(int.description_in(l));
    if int.functions.is_empty() {
        section(ui, l.pick("Example", "Пример"));
        code_block(ui, &format!("int {:02X}h", int.number), l);
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
            code_block(ui, t.example, l);
        });
        if sel && scroll {
            ui.scroll_to_rect(resp.response.rect.expand(8.0), Some(Align::TOP));
        }
    }
}
