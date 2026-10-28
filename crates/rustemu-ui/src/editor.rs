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
    // Breakpoint column (16 px), bookmark bar, line numbers.
    let gutter_w = digits as f32 * char_w + 34.0;
    let pal = Palette::for_theme(ui.visuals().dark_mode);
    let id = ui.make_persistent_id("source_editor");
    let error_lines = app.error_lines.clone();
    if let Some(line) = app.goto_line.take() {
        let idx = crate::find::line_start_char(&app.text, line);
        app.pending_select = Some((idx, idx));
    }
    let select = app.pending_select.take();
    // Line of the current instruction (strong) or of the statement that called the macro (soft).
    let exec_line = exec_line(app);
    // Text before this frame's edit, to move breakpoints and bookmarks with their lines.
    let old_text = (!app.bp_lines.is_empty() || !app.bookmarks.is_empty()).then(|| app.text.clone());

    egui::ScrollArea::both().id_salt("editor_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (gutter_rect, gutter) =
                ui.allocate_exact_size(vec2(gutter_w, row_h * lines as f32 + 8.0), Sense::click());

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
                if let Some(bp) = app.bp_lines.get(&(0, n)) {
                    painter.circle_filled(
                        pos2(gutter_rect.left() + 8.0, y + row_h / 2.0),
                        (row_h * 0.3).min(5.5),
                        crate::debugger::marker_color(bp),
                    );
                }
                if let Some((l, strong)) = exec_line
                    && l == n
                {
                    let c = app.current_line_color.unwrap_or_else(|| crate::colors::info(ui));
                    let band = Rect::from_min_max(pos2(gutter_rect.right() - 2.0, y), pos2(clip.right(), y + row_h));
                    painter_full(ui).rect_filled(band, 0.0, c.gamma_multiply(if strong { 0.28 } else { 0.12 }));
                    if strong {
                        painter.text(
                            pos2(gutter_rect.left() + 8.0, y + row_h / 2.0),
                            egui::Align2::CENTER_CENTER,
                            "▶",
                            font.clone(),
                            c,
                        );
                    }
                }
                if app.bookmarks.contains(&n) {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(gutter_rect.left() + 17.0, y + 2.0), vec2(4.0, row_h - 4.0)),
                        2.0,
                        Color32::from_rgb(60, 130, 230),
                    );
                }
                if error_lines.contains(&n) {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(gutter_rect.left() + 16.0, y), vec2(gutter_w - 20.0, row_h)),
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

            // Click in the margin: toggle a breakpoint.
            if gutter.clicked()
                && let Some(p) = gutter.interact_pointer_pos()
            {
                let line = ((p.y - top) / row_h).floor().max(0.0) as u32 + 1;
                if line as usize <= lines {
                    app.toggle_breakpoint_at_line(0, line);
                }
            }
            gutter.on_hover_text("click: toggle breakpoint");

            // Keep the current instruction in view when execution stops somewhere else.
            if app.run == crate::app::RunMode::Paused
                && let Some((l, _)) = exec_line
                && app.editor_exec_line != Some(l)
            {
                let y = top + (l - 1) as f32 * row_h;
                let r = Rect::from_min_size(pos2(gutter_rect.left(), y), vec2(10.0, row_h));
                if !clip.contains_rect(r) {
                    ui.scroll_to_rect(r, Some(Align::Center));
                }
            }
            app.editor_exec_line = exec_line.map(|(l, _)| l);

            context_menu(app, ui, &out, top, row_h);
            hover_help(app, ui, &out);

            if let Some(old) = old_text
                && old != app.text
            {
                move_line_marks(app, &old);
            }

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

fn painter_full(ui: &Ui) -> egui::Painter {
    ui.painter().clone()
}

/// Editor line of the current instruction: `(line, true)`, or `(line, false)` for the statement
/// that invoked the macro / library code being executed. None while the program is outdated.
fn exec_line(app: &App) -> Option<(u32, bool)> {
    if !app.loaded() || app.source_files.is_empty() || app.is_stale() {
        return None;
    }
    let v = app.session.current_view(app.show_macro_code)?;
    if v.file == 0 {
        Some((v.line, true))
    } else if v.statement.0 == 0 {
        Some((v.statement.1, false))
    } else {
        None
    }
}

/// Maps 1-based line numbers of `old` to `new` after an edit: lines before and after the changed
/// region move with the text, lines of the region keep their number while it still exists.
pub fn line_mapper(old: &str, new: &str) -> impl Fn(u32) -> Option<u32> {
    let o: Vec<&str> = old.split('\n').collect();
    let n: Vec<&str> = new.split('\n').collect();
    let mut p = 0;
    while p < o.len() && p < n.len() && o[p] == n[p] {
        p += 1;
    }
    let mut s = 0;
    while s < o.len() - p && s < n.len() - p && o[o.len() - 1 - s] == n[n.len() - 1 - s] {
        s += 1;
    }
    let (ol, nl) = (o.len(), n.len());
    move |l| {
        let i = l as usize - 1;
        if i < p {
            Some(l)
        } else if i >= ol - s {
            Some((i + nl + 1 - ol) as u32)
        } else if i < nl - s {
            Some(l)
        } else {
            None
        }
    }
}

/// Moves breakpoints and bookmarks of the edited file with their lines.
fn move_line_marks(app: &mut App, old: &str) {
    let map = line_mapper(old, &app.text);
    let bps = std::mem::take(&mut app.bp_lines);
    app.bp_lines = bps
        .into_iter()
        .filter_map(|((f, l), bp)| if f == 0 { map(l).map(|l| ((0, l), bp)) } else { Some(((f, l), bp)) })
        .collect();
    app.bookmarks = app.bookmarks.iter().filter_map(|&l| map(l)).collect();
}

/// Right-click menu of the editor: breakpoints, run to cursor, watches, reference.
fn context_menu(app: &mut App, ui: &Ui, out: &egui::text_edit::TextEditOutput, top: f32, row_h: f32) {
    let id = ui.make_persistent_id("editor_ctx");
    if out.response.secondary_clicked()
        && let Some(p) = out.response.interact_pointer_pos()
    {
        let line = ((p.y - top) / row_h).floor().max(0.0) as u32 + 1;
        let ci = out.galley.cursor_from_pos(p - out.galley_pos).index.0;
        let word = word_at(&app.text, ci).map(|(w, _)| w).unwrap_or_default();
        ui.data_mut(|d| d.insert_temp(id, (line, word)));
    }
    out.response.response.clone().context_menu(|ui| {
        let (line, word) = ui.data(|d| d.get_temp::<(u32, String)>(id)).unwrap_or((app.cursor_line, String::new()));
        if ui.button(format!("Run to line {line}          F4")).clicked() {
            app.run_to_line(0, line);
            ui.close();
        }
        let has_bp = app.bp_lines.contains_key(&(0, line));
        if ui.button(if has_bp { "Remove breakpoint      ⌘F8" } else { "Add breakpoint         ⌘F8" }).clicked() {
            app.toggle_breakpoint_at_line(0, line);
            ui.close();
        }
        if has_bp && ui.button("Breakpoint condition…").clicked() {
            app.windows.breakpoints = true;
            ui.close();
        }
        if !word.is_empty() && !word.starts_with(|c: char| c.is_ascii_digit()) {
            ui.separator();
            if ui.button(format!("Add watch: {word}")).clicked() {
                crate::debugger::add_watch(app, &word);
                ui.close();
            }
        }
    });
}

/// Value of a register or program variable under the mouse while a program is loaded.
fn value_hover(app: &App, word: &str) -> Option<String> {
    if !app.loaded()
        || app.is_stale()
        || app.session.program.is_none()
        || word.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    let e = app.session.compile(word).ok()?;
    let v = e.eval(&app.session.machine);
    let bits = e.bits();
    let hex = format!("{:0w$X}h", v as u64 & ((1u64 << bits) - 1), w = (bits / 4) as usize);
    let at = e.address(&app.session.machine).map(|a| format!("   at {a:05X}h")).unwrap_or_default();
    let name = word.to_ascii_uppercase();
    // NASM: a data label is an address; its value is [name].
    if let Some(rustemu_session::expr::SymbolRef::Address { size, .. }) = app.session.lookup_symbol(word) {
        let m = app.session.compile(&format!("[{word}]")).ok()?.eval(&app.session.machine);
        let digits = (size.min(4) as usize) * 2;
        return Some(format!("{name} = {v:04X}h (address)   [{name}] = {m:0digits$X}h  ({m})"));
    }
    Some(format!("{name} = {hex}  ({v}){at}"))
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

/// Tooltip with a summary of an instruction, directive or interrupt; Cmd-click opens the reference.
fn reference_hover(app: &mut App, ci: usize, out: &egui::text_edit::TextEditOutput, cmd_click: bool) {
    let Some(topic) = crate::reference::topic_at(&app.text, ci) else { return };
    let (title, details) = topic.tooltip();
    out.response.response.clone().on_hover_ui_at_pointer(|ui| {
        ui.set_max_width(460.0);
        ui.label(RichText::new(title).strong().monospace());
        ui.label(details);
        ui.weak("F1 or ⌘-click: open the reference");
    });
    if cmd_click {
        app.reference.show(topic);
    }
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
    let found = match own {
        Some(e) => Some((e, false)),
        None if uses_lib => app.library.find(&upper).map(|e| (e, true)),
        None => None,
    };
    let Some((entry, from_lib)) = found else {
        if let Some(text) = value_hover(app, &word) {
            out.response.response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(RichText::new(text).monospace());
            });
            return;
        }
        reference_hover(app, ci, out, cmd_click);
        return;
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
