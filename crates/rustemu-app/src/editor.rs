//! Source editor with syntax highlighting, line numbers and error markers.

use egui::text::{CCursor, CCursorRange};
use egui::{Align, Color32, FontId, Rect, RichText, Sense, Stroke, TextEdit, Ui, pos2, vec2};

use crate::app::App;
use crate::highlight::{Palette, highlight};

pub fn editor(app: &mut App, ui: &mut Ui) {
    let font = FontId::monospace(app.settings.font_size);
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let lines = app.text.split('\n').count().max(1);
    let digits = lines.to_string().len().max(3);
    let gutter_w = digits as f32 * char_w + 18.0;
    let pal = Palette::for_theme(ui.visuals().dark_mode);
    let id = ui.make_persistent_id("source_editor");
    let error_lines = app.error_lines.clone();
    if let Some(line) = app.goto_line.take() {
        let idx = crate::find::line_start_char(&app.text, line);
        app.pending_select = Some((idx, idx));
    }
    let select = app.pending_select.take();

    egui::ScrollArea::both().id_salt("editor_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (gutter_rect, _) = ui.allocate_exact_size(vec2(gutter_w, row_h * lines as f32 + 8.0), Sense::hover());

            let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                let job = highlight(buf.as_str(), font.clone(), &pal);
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let out = TextEdit::multiline(&mut app.text)
                .id(id)
                .font(font.clone())
                .code_editor()
                .lock_focus(true)
                .hint_text("assembly source code")
                .desired_width(f32::INFINITY)
                .desired_rows(40)
                .frame(egui::Frame::NONE)
                .layouter(&mut layouter)
                .show(ui);

            // Line numbers aligned with the text rows.
            let painter = ui.painter_at(Rect::from_min_max(
                pos2(gutter_rect.left(), ui.clip_rect().top()),
                pos2(gutter_rect.right(), ui.clip_rect().bottom()),
            ));
            let top = out.galley_pos.y;
            let num_color = ui.visuals().weak_text_color();
            let clip = ui.clip_rect();
            let first = (((clip.top() - top) / row_h).floor().max(0.0)) as usize;
            let last = (((clip.bottom() - top) / row_h).ceil().max(0.0) as usize + 1).min(lines);
            for i in first..last {
                let y = top + i as f32 * row_h;
                let n = i as u32 + 1;
                if app.bookmarks.contains(&n) {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(gutter_rect.left() + 2.0, y + 2.0), vec2(6.0, row_h - 4.0)),
                        2.0,
                        Color32::from_rgb(60, 130, 230),
                    );
                }
                if error_lines.contains(&n) {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(gutter_rect.left(), y), vec2(gutter_w - 4.0, row_h)),
                        2.0,
                        Color32::from_rgba_unmultiplied(220, 50, 50, 90),
                    );
                }
                painter.text(
                    pos2(gutter_rect.right() - 10.0, y),
                    egui::Align2::RIGHT_TOP,
                    n.to_string(),
                    font.clone(),
                    if error_lines.contains(&n) { Color32::from_rgb(240, 80, 80) } else { num_color },
                );
            }
            painter.line_segment(
                [pos2(gutter_rect.right() - 4.0, clip.top()), pos2(gutter_rect.right() - 4.0, clip.bottom())],
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            );

            hover_help(app, ui, &out);

            // Undo / redo requested from the Edit menu.
            if let Some(undo) = app.pending_undo.take() {
                let mut state = out.state.clone();
                let mut undoer = state.undoer();
                let range = state.cursor.char_range().unwrap_or_default();
                let current = (range, app.text.clone());
                let restored = if undo { undoer.undo(&current) } else { undoer.redo(&current) }.cloned();
                if let Some((r, text)) = restored {
                    app.text = text;
                    state.cursor.set_char_range(Some(r));
                }
                state.set_undoer(undoer);
                state.store(ui.ctx(), id);
            }

            if let Some(r) = &out.cursor_range {
                let idx = r.primary.index.0;
                app.cursor_line = app.text.chars().take(idx).filter(|&c| c == '\n').count() as u32 + 1;
                let (a, b) = (r.primary.index.0, r.secondary.index.0);
                app.selection = (a.min(b), a.max(b));
            }

            if let Some((start, end)) = select {
                let mut state = out.state.clone();
                state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(start), CCursor::new(end))));
                state.store(ui.ctx(), id);
                out.response.request_focus();
                app.selection = (start, end);
                let line = app.text.chars().take(start).filter(|&c| c == '\n').count() as u32 + 1;
                let y = top + (line.saturating_sub(1)) as f32 * row_h;
                ui.scroll_to_rect(
                    Rect::from_min_size(pos2(gutter_rect.left(), y), vec2(10.0, row_h)),
                    Some(Align::Center),
                );
            }
        });
    });
}

/// Identifier under char index `ci`.
fn word_at(text: &str, ci: usize) -> Option<(String, usize)> {
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
    let line = chars[..a].iter().filter(|&&c| c == '\n').count() + 1;
    Some((chars[a..b].iter().collect(), line))
}

/// Tooltip with the documentation of macros/procedures; Cmd-click opens the library viewer.
fn hover_help(app: &mut App, ui: &Ui, out: &egui::text_edit::TextEditOutput) {
    let Some(pos) = out.response.hover_pos() else { return };
    let ci = out.galley.cursor_from_pos(pos - out.galley_pos).index.0;
    let Some((word, line)) = word_at(&app.text, ci) else { return };
    let upper = word.to_ascii_uppercase();
    let line_text = app.text.lines().nth(line - 1).unwrap_or("").to_ascii_lowercase();
    let cmd_click = out.response.clicked() && ui.input(|i| i.modifiers.command);
    if line_text.contains("include") && line_text.contains("emu8086") {
        out.response.response.clone().on_hover_ui_at_pointer(|ui| {
            ui.label("Library of macros (PRINT, PUTC, GOTOXY, ...) and procedures (SCAN_NUM, PRINT_NUM, ...).");
            ui.weak("⌘-click to open it");
        });
        if cmd_click {
            app.library.open = true;
        }
        return;
    }
    // Macros defined in this file take precedence over the library.
    let own = rustemu_asm::library::index(&app.text).into_iter().find(|e| e.name == upper);
    let uses_lib = app.text.to_ascii_lowercase().contains("emu8086.inc");
    let (entry, from_lib) = match own {
        Some(e) => (e, false),
        None if uses_lib => match app.library.find(&upper) {
            Some(e) => (e, true),
            None => return,
        },
        None => return,
    };
    // Do not show help for the definition line itself.
    if !from_lib && entry.line as usize == line {
        return;
    }
    out.response.response.clone().on_hover_ui_at_pointer(|ui| {
        ui.label(RichText::new(crate::library_ui::title(&entry)).strong().monospace());
        if !entry.doc.is_empty() {
            ui.label(&entry.doc);
        }
        if from_lib {
            ui.weak("from emu8086.inc — ⌘-click to see the code");
        } else {
            ui.weak(format!("defined at line {}", entry.line));
        }
    });
    if cmd_click {
        if from_lib {
            app.library.show_entry(&upper);
        } else {
            app.goto_line = Some(entry.line);
        }
    }
}
