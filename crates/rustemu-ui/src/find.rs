//! Editor commands: find / replace, go to line, toggle comment.

use egui::Ui;

use crate::app::App;

#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub focus: bool,
    pub query: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub message: String,
    pub goto_open: bool,
    pub goto_text: String,
}

/// Char index of the first character of 1-based `line`.
pub fn line_start_char(text: &str, line: u32) -> usize {
    text.split('\n').take(line.saturating_sub(1) as usize).map(|l| l.chars().count() + 1).sum()
}

fn char_to_byte(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
}

fn byte_to_char(text: &str, bi: usize) -> usize {
    text[..bi].chars().count()
}

fn fold(s: &str, case: bool) -> String {
    if case { s.to_string() } else { s.to_ascii_lowercase() }
}

/// Selects the next (or previous) occurrence of the query, wrapping around.
pub fn find_next(app: &mut App, backwards: bool) -> bool {
    let q = fold(&app.find.query, app.find.case_sensitive);
    if q.is_empty() {
        return false;
    }
    let hay = fold(&app.text, app.find.case_sensitive);
    let (sel_a, sel_b) = app.selection;
    let found = if backwards {
        let before = char_to_byte(&app.text, sel_a);
        hay[..before].rfind(&q).or_else(|| hay.rfind(&q))
    } else {
        let after = char_to_byte(&app.text, sel_b);
        hay[after..].find(&q).map(|i| i + after).or_else(|| hay.find(&q))
    };
    match found {
        Some(b) => {
            let start = byte_to_char(&app.text, b);
            let end = start + app.find.query.chars().count();
            app.pending_select = Some((start, end));
            app.selection = (start, end);
            app.find.message.clear();
            true
        }
        None => {
            app.find.message = "not found".into();
            false
        }
    }
}

fn selection_matches(app: &App) -> bool {
    let (a, b) = app.selection;
    let (ba, bb) = (char_to_byte(&app.text, a), char_to_byte(&app.text, b));
    fold(&app.text[ba..bb], app.find.case_sensitive) == fold(&app.find.query, app.find.case_sensitive)
        && !app.find.query.is_empty()
}

pub fn replace_one(app: &mut App) {
    if selection_matches(app) {
        let (a, b) = app.selection;
        let (ba, bb) = (char_to_byte(&app.text, a), char_to_byte(&app.text, b));
        let rep = app.find.replace.clone();
        app.text.replace_range(ba..bb, &rep);
        let end = a + rep.chars().count();
        app.selection = (end, end);
    }
    find_next(app, false);
}

pub fn replace_all(app: &mut App) {
    let q = fold(&app.find.query, app.find.case_sensitive);
    if q.is_empty() {
        return;
    }
    let hay = fold(&app.text, app.find.case_sensitive);
    let mut out = String::with_capacity(app.text.len());
    let mut last = 0;
    let mut n = 0;
    for (i, _) in hay.match_indices(&q) {
        out.push_str(&app.text[last..i]);
        out.push_str(&app.find.replace);
        last = i + q.len();
        n += 1;
    }
    out.push_str(&app.text[last..]);
    app.text = out;
    app.find.message = format!("{n} replaced");
}

/// Adds or removes a leading `;` on every line touched by the selection.
pub fn toggle_comment(app: &mut App) {
    let (a, b) = app.selection;
    let first = app.text.chars().take(a).filter(|&c| c == '\n').count();
    let mut last = app.text.chars().take(b).filter(|&c| c == '\n').count();
    // A selection ending at the start of a line does not include that line.
    if b > a && app.text.chars().nth(b.saturating_sub(1)) == Some('\n') {
        last = last.saturating_sub(1).max(first);
    }
    let mut lines: Vec<String> = app.text.split('\n').map(str::to_string).collect();
    let range = first..=last.min(lines.len().saturating_sub(1));
    let all_commented = lines[range.clone()].iter().all(|l| l.trim().is_empty() || l.trim_start().starts_with(';'));
    for l in &mut lines[range.clone()] {
        if all_commented {
            if let Some(pos) = l.find(';') {
                let after = if l[pos + 1..].starts_with(' ') { pos + 2 } else { pos + 1 };
                l.replace_range(pos..after, "");
            }
        } else if !l.trim().is_empty() {
            l.insert_str(0, "; ");
        }
    }
    app.text = lines.join("\n");
    let start = line_start_char(&app.text, *range.start() as u32 + 1);
    let end = line_start_char(&app.text, *range.end() as u32 + 2).saturating_sub(1);
    app.pending_select = Some((start, end.max(start)));
}

pub fn find_bar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut app.find.query).hint_text("find").desired_width(200.0));
        if app.find.focus {
            r.request_focus();
            app.find.focus = false;
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let back = ui.input(|i| i.modifiers.shift);
            find_next(app, back);
            app.find.focus = true;
        }
        if ui.button("▲").on_hover_text("previous (⇧⌘G)").clicked() {
            find_next(app, true);
        }
        if ui.button("▼").on_hover_text("next (⌘G)").clicked() {
            find_next(app, false);
        }
        ui.checkbox(&mut app.find.case_sensitive, "Aa").on_hover_text("case sensitive");
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut app.find.replace).hint_text("replace with").desired_width(160.0));
        if ui.button("replace").clicked() {
            replace_one(app);
        }
        if ui.button("all").on_hover_text("replace all").clicked() {
            replace_all(app);
        }
        if !app.find.message.is_empty() {
            ui.colored_label(crate::colors::warn(ui), &app.find.message);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✖").on_hover_text("close (Esc)").clicked() {
                app.find.open = false;
            }
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.find.open = false;
    }
}

pub fn goto_dialog(app: &mut App, ctx: &egui::Context) {
    if !app.find.goto_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Go to line").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        let r =
            ui.add(egui::TextEdit::singleline(&mut app.find.goto_text).hint_text("line number").desired_width(120.0));
        r.request_focus();
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Ok(n) = app.find.goto_text.trim().parse::<u32>() {
                app.goto_line = Some(n.max(1));
            }
            app.find.goto_open = false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.find.goto_open = false;
        }
    });
    if !open {
        app.find.goto_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Settings};

    fn app(text: &str) -> App {
        let mut a = App::with_settings(Settings { last_file: None, ..Settings::default() });
        a.text = text.into();
        a
    }

    #[test]
    fn find_and_replace() {
        let mut a = app("mov ax, 1\nMOV bx, 2\nmov cx, 3\n");
        a.find.query = "mov".into();
        assert!(find_next(&mut a, false));
        assert_eq!(a.selection, (0, 3));
        assert!(find_next(&mut a, false));
        assert_eq!(a.selection, (10, 13));
        a.find.replace = "add".into();
        replace_all(&mut a);
        assert_eq!(a.text, "add ax, 1\nadd bx, 2\nadd cx, 3\n");
    }

    #[test]
    fn comments_toggle() {
        let mut a = app("nop\n  hlt\n\nret");
        a.selection = (0, 8);
        toggle_comment(&mut a);
        assert_eq!(a.text, "; nop\n;   hlt\n\nret");
        a.selection = (0, 9);
        toggle_comment(&mut a);
        assert_eq!(a.text, "nop\n  hlt\n\nret");
    }
}
