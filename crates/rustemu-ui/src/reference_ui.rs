//! Reference window (Help menu, F1): instructions, interrupts, directives and the quick reference.

use egui::{Align, FontId, RichText, Ui};

use crate::highlight::{Palette, highlight};
use crate::reference::{self as r, DIRECTIVES, Group, INSTRUCTIONS, INTERRUPTS, Topic};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tab {
    #[default]
    Instructions,
    Interrupts,
    Directives,
    Quick,
}

#[derive(Default)]
pub struct RefState {
    pub open: bool,
    pub tab: Tab,
    pub search: String,
    pub topic: Option<Topic>,
    /// Scroll the list and the article to the selected topic on the next frame.
    scroll: bool,
}

impl RefState {
    /// Opens the window at the article `t`.
    pub fn show(&mut self, t: Topic) {
        self.open = true;
        self.tab = match t {
            Topic::Instr(_) => Tab::Instructions,
            Topic::Directive(_) => Tab::Directives,
            Topic::Int(..) => Tab::Interrupts,
        };
        self.topic = Some(t);
        self.search.clear();
        self.scroll = true;
    }

    pub fn show_tab(&mut self, tab: Tab) {
        self.open = true;
        self.tab = tab;
    }
}

const FLAG_TITLES: [&str; 9] =
    ["Overflow", "Direction", "Interrupt enable", "Trap", "Sign", "Zero", "Auxiliary carry", "Parity", "Carry"];

pub fn reference_window(st: &mut RefState, ctx: &egui::Context) {
    let mut open = st.open;
    egui::Window::new("reference")
        .open(&mut open)
        .default_pos(egui::pos2(140.0, 80.0))
        .default_size([880.0, 580.0])
        .show(ctx, |ui| reference_ui(st, ui));
    st.open = open;
}

fn reference_ui(st: &mut RefState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for (t, n) in [
            (Tab::Instructions, "instructions"),
            (Tab::Interrupts, "interrupts"),
            (Tab::Directives, "directives"),
            (Tab::Quick, "quick reference"),
        ] {
            ui.selectable_value(&mut st.tab, t, n);
        }
        ui.separator();
        if st.tab != Tab::Quick {
            ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("search").desired_width(180.0));
        }
    });
    ui.separator();
    if st.tab == Tab::Quick {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(crate::help::HELP).monospace().size(12.5));
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
                    _ => directive_list(st, ui, scroll),
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            let topic = st.topic.filter(|t| {
                matches!(
                    (t, st.tab),
                    (Topic::Instr(_), Tab::Instructions)
                        | (Topic::Directive(_), Tab::Directives)
                        | (Topic::Int(..), Tab::Interrupts)
                )
            });
            let salt = match topic {
                Some(Topic::Int(i, _)) => format!("int{i}"),
                t => format!("{t:?}"),
            };
            egui::ScrollArea::vertical().id_salt(("ref_article", salt)).auto_shrink([false, false]).show(ui, |ui| {
                match topic {
                    Some(Topic::Instr(i)) => instruction_article(ui, i),
                    Some(Topic::Directive(i)) => directive_article(ui, i),
                    Some(Topic::Int(i, f)) => interrupt_article(ui, i, f, scroll),
                    None => intro(ui, st.tab),
                }
            });
        });
    });
}

fn matches(search: &str, texts: &[&str]) -> bool {
    let s = search.trim().to_ascii_lowercase();
    s.is_empty() || texts.iter().any(|t| t.to_ascii_lowercase().contains(&s))
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
    for g in Group::ALL {
        let items: Vec<usize> = (0..INSTRUCTIONS.len())
            .filter(|&i| INSTRUCTIONS[i].group == g)
            .filter(|&i| {
                let e = &INSTRUCTIONS[i];
                let mut texts = vec![e.name, e.summary];
                texts.extend(e.aliases);
                matches(&st.search, &texts)
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        ui.label(RichText::new(g.title()).strong());
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
    let searching = !st.search.trim().is_empty();
    for (i, int) in INTERRUPTS.iter().enumerate() {
        let funcs: Vec<&r::Function> = int
            .functions
            .iter()
            .filter(|f| !searching || matches(&st.search, &[f.title, &r::hex(f.code, int.selector)]))
            .collect();
        let head_match = matches(&st.search, &[int.title, &format!("{:02X}h", int.number)]);
        if !head_match && funcs.is_empty() {
            continue;
        }
        list_item(
            st,
            ui,
            Topic::Int(i, None),
            RichText::new(format!("INT {:02X}h  {}", int.number, int.title)),
            scroll,
        );
        let expanded = searching || matches!(st.topic, Some(Topic::Int(j, _)) if j == i);
        if expanded {
            ui.indent(("int_funcs", i), |ui| {
                for f in funcs {
                    let text = format!("{}={} {}", int.selector, r::hex(f.code, int.selector), f.title);
                    list_item(st, ui, Topic::Int(i, Some(f.code)), RichText::new(text), scroll);
                }
            });
        }
    }
}

fn directive_list(st: &mut RefState, ui: &mut Ui, scroll: bool) {
    for (i, d) in DIRECTIVES.iter().enumerate() {
        let mut texts = vec![d.name, d.summary];
        texts.extend(d.aliases);
        if matches(&st.search, &texts) {
            list_item(st, ui, Topic::Directive(i), RichText::new(d.name).monospace(), scroll);
        }
    }
}

fn intro(ui: &mut Ui, tab: Tab) {
    ui.label(match tab {
        Tab::Instructions => {
            "8086 instructions (and the 80186 additions accepted by Emu8086): syntax, flags, examples."
        }
        Tab::Interrupts => "BIOS and DOS services available to programs in the emulator.",
        _ => "Assembler directives and operators.",
    });
    ui.add_space(6.0);
    ui.weak("In the editor: hover over an instruction, a directive or INT nn to see a summary; press F1 or ⌘-click to open its article here.");
}

fn also(ui: &mut Ui, aliases: &[&str]) {
    if !aliases.is_empty() {
        ui.weak(format!("also: {}", aliases.join(", ")));
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong());
}

fn instruction_article(ui: &mut Ui, i: usize) {
    let e = &INSTRUCTIONS[i];
    ui.horizontal(|ui| {
        ui.heading(RichText::new(e.name).monospace());
        ui.weak(e.group.title());
    });
    also(ui, e.aliases);
    ui.label(RichText::new(e.summary).strong());
    section(ui, "Syntax");
    if e.syntax.is_empty() {
        ui.monospace(e.name);
    }
    for s in e.syntax {
        ui.monospace(if s.is_empty() { e.name.to_string() } else { format!("{} {s}", e.name) });
    }
    if !e.description.is_empty() {
        ui.add_space(6.0);
        ui.label(e.description);
    }
    section(ui, "Flags");
    flags_table(ui, e.flags);
    section(ui, "Example");
    code_block(ui, e.example);
}

pub fn flags_table(ui: &mut Ui, flags: &str) {
    let changed = ui.visuals().hyperlink_color;
    let undefined = crate::colors::warn(ui);
    egui::Grid::new("flags_table").spacing([10.0, 2.0]).show(ui, |ui| {
        for (n, t) in r::FLAG_NAMES.iter().zip(FLAG_TITLES) {
            ui.label(RichText::new(*n).monospace().strong()).on_hover_text(t);
        }
        ui.end_row();
        for ((c, n), t) in flags.chars().zip(r::FLAG_NAMES).zip(FLAG_TITLES) {
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
            ui.label(text).on_hover_text(format!("{n} ({t}): {}", r::flag_effect(c)));
        }
        ui.end_row();
    });
    ui.weak("r = set by the result   0 / 1 = cleared / set   ? = undefined   - = not changed");
}

fn code_block(ui: &mut Ui, code: &str) {
    let pal = Palette::for_theme(ui.visuals().dark_mode);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add(egui::Label::new(highlight(code, FontId::monospace(12.5), &pal)));
            ui.add_space(16.0);
            if ui.small_button("copy").on_hover_text("copy the example to the clipboard").clicked() {
                ui.ctx().copy_text(code.to_string());
            }
        });
    });
}

fn directive_article(ui: &mut Ui, i: usize) {
    let d = &DIRECTIVES[i];
    ui.heading(RichText::new(d.name).monospace());
    also(ui, d.aliases);
    ui.label(RichText::new(d.summary).strong());
    ui.add_space(6.0);
    ui.label(d.description);
    section(ui, "Example");
    code_block(ui, d.example);
}

fn interrupt_article(ui: &mut Ui, i: usize, func: Option<u16>, scroll: bool) {
    let int = &INTERRUPTS[i];
    ui.heading(RichText::new(format!("INT {:02X}h", int.number)).monospace());
    ui.label(RichText::new(int.title).strong());
    ui.label(int.description);
    if int.functions.is_empty() {
        section(ui, "Example");
        code_block(ui, &format!("int {:02X}h", int.number));
        return;
    }
    ui.weak(format!("The function number is passed in {}.", int.selector));
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
            ui.label(
                RichText::new(format!("{} = {}   {}", int.selector, r::hex(f.code, int.selector), f.title)).strong(),
            );
            egui::Grid::new(("func", f.code)).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                for (k, v) in [("input", f.input), ("output", f.output)] {
                    if !v.is_empty() {
                        ui.weak(k);
                        ui.label(v);
                        ui.end_row();
                    }
                }
            });
            code_block(ui, f.example);
        });
        if sel && scroll {
            ui.scroll_to_rect(resp.response.rect.expand(8.0), Some(Align::TOP));
        }
    }
}
