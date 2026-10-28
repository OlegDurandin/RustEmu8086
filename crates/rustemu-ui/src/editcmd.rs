//! Editor commands of the Edit and Bookmarks menus: line operations, clipboard, bookmarks.

use crate::app::App;
use crate::find::line_start_char;

fn char_to_byte(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
}

/// 0-based indices of the first and last line touched by the selection.
fn selected_lines(app: &App) -> (usize, usize) {
    let (a, b) = app.selection;
    let first = app.text.chars().take(a).filter(|&c| c == '\n').count();
    let mut last = app.text.chars().take(b).filter(|&c| c == '\n').count();
    // A selection ending at the start of a line does not include that line.
    if b > a && app.text.chars().nth(b.saturating_sub(1)) == Some('\n') {
        last = last.saturating_sub(1).max(first);
    }
    (first, last)
}

/// Replaces every selected line with `f(line)` and selects the changed lines.
pub fn map_lines(app: &mut App, f: impl Fn(&str) -> String) {
    let (first, last) = selected_lines(app);
    let mut lines: Vec<String> = app.text.split('\n').map(str::to_string).collect();
    let last = last.min(lines.len().saturating_sub(1));
    for l in &mut lines[first..=last] {
        *l = f(l);
    }
    app.text = lines.join("\n");
    let start = line_start_char(&app.text, first as u32 + 1);
    let end = line_start_char(&app.text, last as u32 + 2).saturating_sub(1);
    app.pending_select = Some((start, end.max(start)));
}

pub fn indent(app: &mut App) {
    map_lines(app, |l| if l.is_empty() { String::new() } else { format!("\t{l}") });
}

pub fn outdent(app: &mut App) {
    map_lines(app, |l| {
        if let Some(rest) = l.strip_prefix('\t') {
            rest.to_string()
        } else {
            let n = l.len() - l.trim_start_matches(' ').len();
            l[n.min(8)..].to_string()
        }
    });
}

/// Ctrl+Q in Emu8086: adds `;` in front of the selected lines.
pub fn comment(app: &mut App) {
    map_lines(app, |l| if l.trim().is_empty() { l.to_string() } else { format!(";{l}") });
}

/// Ctrl+W in Emu8086: removes one leading `;` from the selected lines.
pub fn uncomment(app: &mut App) {
    map_lines(app, |l| {
        let n = l.len() - l.trim_start().len();
        if l[n..].starts_with(';') { format!("{}{}", &l[..n], &l[n + 1..]) } else { l.to_string() }
    });
}

fn map_selection(app: &mut App, f: impl Fn(&str) -> String) {
    let (a, b) = app.selection;
    if a == b {
        return;
    }
    let (ba, bb) = (char_to_byte(&app.text, a), char_to_byte(&app.text, b));
    let new = f(&app.text[ba..bb]);
    app.text.replace_range(ba..bb, &new);
    app.pending_select = Some((a, a + new.chars().count()));
}

pub fn lowercase(app: &mut App) {
    map_selection(app, str::to_lowercase);
}

pub fn uppercase(app: &mut App) {
    map_selection(app, str::to_uppercase);
}

pub fn select_all(app: &mut App) {
    app.pending_select = Some((0, app.text.chars().count()));
}

pub fn selected_text(app: &App) -> String {
    let (a, b) = app.selection;
    let (ba, bb) = (char_to_byte(&app.text, a), char_to_byte(&app.text, b));
    app.text[ba..bb].to_string()
}

pub fn copy(app: &App, ctx: &egui::Context) {
    let t = selected_text(app);
    if !t.is_empty() {
        ctx.copy_text(t);
    }
}

pub fn cut(app: &mut App, ctx: &egui::Context) {
    copy(app, ctx);
    let (a, _) = app.selection;
    map_selection(app, |_| String::new());
    app.pending_select = Some((a, a));
}

/// Pastes the system clipboard (egui cannot read the clipboard on demand; see `platform`).
pub fn paste(app: &mut App) {
    let Some(text) = crate::platform::read_clipboard() else { return };
    let (a, b) = app.selection;
    let (ba, bb) = (char_to_byte(&app.text, a), char_to_byte(&app.text, b));
    app.text.replace_range(ba..bb, &text);
    let end = a + text.chars().count();
    app.pending_select = Some((end, end));
}

// ---- bookmarks ---------------------------------------------------------------------------

pub fn toggle_bookmark(app: &mut App) {
    let l = app.cursor_line;
    if !app.bookmarks.remove(&l) {
        app.bookmarks.insert(l);
    }
}

pub fn next_bookmark(app: &mut App) {
    let cur = app.cursor_line;
    let next = app.bookmarks.range(cur + 1..).next().or_else(|| app.bookmarks.iter().next());
    if let Some(&l) = next {
        app.goto_line = Some(l);
    }
}

pub fn prev_bookmark(app: &mut App) {
    let cur = app.cursor_line;
    let prev = app.bookmarks.range(..cur).next_back().or_else(|| app.bookmarks.iter().next_back());
    if let Some(&l) = prev {
        app.goto_line = Some(l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Settings};

    fn app(text: &str, sel: (usize, usize)) -> App {
        let mut a = App::with_settings(Settings { last_file: None, ..Settings::default() });
        a.text = text.into();
        a.selection = sel;
        a
    }

    #[test]
    fn line_operations() {
        let mut a = app("mov ax, 1\n  nop\nret", (0, 12));
        comment(&mut a);
        assert_eq!(a.text, ";mov ax, 1\n;  nop\nret");
        a.selection = (0, 14);
        uncomment(&mut a);
        assert_eq!(a.text, "mov ax, 1\n  nop\nret");
        a.selection = (0, 12);
        indent(&mut a);
        assert_eq!(a.text, "\tmov ax, 1\n\t  nop\nret");
        a.selection = (0, 14);
        outdent(&mut a);
        assert_eq!(a.text, "mov ax, 1\n  nop\nret");
        a.selection = (0, 3);
        uppercase(&mut a);
        assert_eq!(a.text, "MOV ax, 1\n  nop\nret");
    }

    #[test]
    fn bookmarks_cycle() {
        let mut a = app("a\nb\nc\nd", (0, 0));
        a.cursor_line = 2;
        toggle_bookmark(&mut a);
        a.cursor_line = 4;
        toggle_bookmark(&mut a);
        a.cursor_line = 1;
        next_bookmark(&mut a);
        assert_eq!(a.goto_line, Some(2));
        a.cursor_line = 4;
        next_bookmark(&mut a);
        assert_eq!(a.goto_line, Some(2));
        prev_bookmark(&mut a);
        assert_eq!(a.goto_line, Some(2));
    }
}
