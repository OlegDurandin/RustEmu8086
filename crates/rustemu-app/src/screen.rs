//! Emulator screen: text and graphics rendering, keyboard and mouse input.

use egui::{Color32, ColorImage, FontId, Key, Rect, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2};
use rustemu_core::font::{char_to_cp437, cp437_to_char};
use rustemu_core::keyboard::{keys, scan_code_for};
use rustemu_core::video::{self, Video};

use crate::app::App;

#[derive(Default)]
pub struct ScreenState {
    pub focus_request: bool,
    /// The emulator screen has keyboard focus (keys go to the program).
    pub focused: bool,
    texture: Option<TextureHandle>,
    pub font_size: f32,
}

impl ScreenState {
    pub fn new() -> Self {
        ScreenState { font_size: 13.0, ..Default::default() }
    }
}

/// The 16 CGA/EGA text colours.
pub const CGA: [Color32; 16] = [
    Color32::from_rgb(0x00, 0x00, 0x00),
    Color32::from_rgb(0x00, 0x00, 0xAA),
    Color32::from_rgb(0x00, 0xAA, 0x00),
    Color32::from_rgb(0x00, 0xAA, 0xAA),
    Color32::from_rgb(0xAA, 0x00, 0x00),
    Color32::from_rgb(0xAA, 0x00, 0xAA),
    Color32::from_rgb(0xAA, 0x55, 0x00),
    Color32::from_rgb(0xAA, 0xAA, 0xAA),
    Color32::from_rgb(0x55, 0x55, 0x55),
    Color32::from_rgb(0x55, 0x55, 0xFF),
    Color32::from_rgb(0x55, 0xFF, 0x55),
    Color32::from_rgb(0x55, 0xFF, 0xFF),
    Color32::from_rgb(0xFF, 0x55, 0x55),
    Color32::from_rgb(0xFF, 0x55, 0xFF),
    Color32::from_rgb(0xFF, 0xFF, 0x55),
    Color32::from_rgb(0xFF, 0xFF, 0xFF),
];

pub fn screen_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.windows.screen;
    let title = match app.session.machine.sys.mem.read8(video::BDA_MODE) {
        0x13 => "emulator screen (320x200, 256 colours)".to_string(),
        _ => {
            let m = &app.session.machine.sys.mem;
            format!("emulator screen ({}x{} chars)", Video::cols(m), Video::rows(m))
        }
    };
    egui::Window::new(title)
        .id(egui::Id::unique("screen_window"))
        .open(&mut open)
        .default_pos(pos2(700.0, 64.0))
        .resizable(false)
        .show(ctx, |ui| screen_ui(app, ui));
    app.windows.screen = open;
}

fn screen_ui(app: &mut App, ui: &mut Ui) {
    if app.screen.font_size == 0.0 {
        app.screen.font_size = 13.0;
    }
    let mem = &app.session.machine.sys.mem;
    let mode = Video::mode(mem);
    let font = FontId::monospace(app.screen.font_size);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).ceil();
    let ch = (app.screen.font_size * 1.25).ceil();
    let size = if Video::is_text(mem) {
        vec2(Video::cols(mem) as f32 * cw, Video::rows(mem) as f32 * ch)
    } else {
        vec2(640.0, 400.0)
    };
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let id = resp.id;
    if resp.clicked() || app.screen.focus_request {
        resp.request_focus();
        app.screen.focus_request = false;
    }
    let focused = resp.has_focus();
    app.screen.focused = focused;
    if focused {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
            )
        });
        keyboard(app, ui);
    }
    mouse(app, ui, rect, &resp);

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::BLACK);
    let mem = &app.session.machine.sys.mem;
    if Video::is_text(mem) {
        draw_text(app, ui, rect, cw, ch, &font);
    } else if mode == 0x13 {
        let mut img = ColorImage::new([320, 200], vec![Color32::BLACK; 320 * 200]);
        let pal = &app.session.machine.sys.dev.palette;
        let vram = mem.slice(video::GFX_BASE, 64000);
        for (i, &p) in vram.iter().enumerate() {
            let c = pal[p as usize];
            img.pixels[i] = Color32::from_rgb(c[0] << 2 | c[0] >> 4, c[1] << 2 | c[1] >> 4, c[2] << 2 | c[2] >> 4);
        }
        let tex = match &mut app.screen.texture {
            Some(t) => {
                t.set(img, TextureOptions::NEAREST);
                t.clone()
            }
            None => {
                let t = ui.ctx().load_texture("vga", img, TextureOptions::NEAREST);
                app.screen.texture = Some(t.clone());
                t
            }
        };
        painter.image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("video mode {mode:02X}h is not supported"),
            font.clone(),
            Color32::GRAY,
        );
    }
    let stroke_color =
        if focused { ui.visuals().selection.bg_fill } else { ui.visuals().widgets.noninteractive.bg_stroke.color };
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, stroke_color), egui::StrokeKind::Inside);

    ui.horizontal(|ui| {
        let waiting = app.session.machine.is_waiting_input();
        if waiting {
            ui.colored_label(crate::colors::warn(ui), "⌨ waiting for input");
        } else if focused {
            ui.weak("keyboard → emulator");
        } else {
            ui.weak("click the screen to type");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("copy").on_hover_text("Copy the text on the screen (⌘C)").clicked() {
                ui.ctx().copy_text(screen_text(&app.session.machine.sys.mem));
            }
            if ui
                .small_button("paste")
                .on_hover_text("Type the clipboard text into the program (⌘V); new lines become Enter")
                .clicked()
            {
                paste_from_clipboard(app);
                app.screen.focus_request = true;
            }
            if ui.small_button("clear").on_hover_text("Clear the screen").clicked() {
                let m = &mut app.session.machine.sys.mem;
                let mode = Video::mode(m);
                Video::set_mode(m, mode);
            }
            ui.add(egui::Slider::new(&mut app.screen.font_size, 10.0..=22.0).show_value(false).text("zoom"));
            let n = app.session.machine.sys.keys.len();
            if n > 0 {
                ui.weak(format!("{n} key(s) buffered"));
            }
        });
    });
}

fn draw_text(app: &App, ui: &Ui, rect: Rect, cw: f32, ch: f32, font: &FontId) {
    let mem = &app.session.machine.sys.mem;
    let painter = ui.painter_at(rect);
    let page = Video::page(mem);
    let (cols, rows) = (Video::cols(mem), Video::rows(mem));
    // BIOS blink bit: with blinking disabled, bit 7 selects bright backgrounds.
    let blink_enabled = mem.read8(video::BDA + 0x65) & 0x20 != 0;
    let time = ui.input(|i| i.time);
    let blink_on = (time * 2.0) as i64 % 2 == 0;
    for r in 0..rows {
        for c in 0..cols {
            let (chr, attr) = Video::read_cell(mem, page, c, r);
            let bg_idx = if blink_enabled { (attr >> 4) & 7 } else { attr >> 4 };
            let fg = CGA[(attr & 15) as usize];
            let bg = CGA[bg_idx as usize];
            let cell = Rect::from_min_size(pos2(rect.left() + c as f32 * cw, rect.top() + r as f32 * ch), vec2(cw, ch));
            if bg_idx != 0 {
                painter.rect_filled(cell, 0.0, bg);
            }
            let hidden = blink_enabled && attr & 0x80 != 0 && !blink_on;
            if chr != 0 && chr != b' ' && chr != 0xFF && !hidden {
                let glyph = cp437_to_char(chr);
                // Block characters are drawn as rectangles so they tile without gaps.
                let block = match chr {
                    0xDB => Some(cell),
                    0xDC => Some(Rect::from_min_max(pos2(cell.left(), cell.center().y), cell.max)),
                    0xDF => Some(Rect::from_min_max(cell.min, pos2(cell.right(), cell.center().y))),
                    0xDD => Some(Rect::from_min_max(cell.min, pos2(cell.center().x, cell.bottom()))),
                    0xDE => Some(Rect::from_min_max(pos2(cell.center().x, cell.top()), cell.max)),
                    _ => None,
                };
                match block {
                    Some(r) => {
                        painter.rect_filled(r, 0.0, fg);
                    }
                    None => {
                        painter.text(cell.center(), egui::Align2::CENTER_CENTER, glyph, font.clone(), fg);
                    }
                }
            }
        }
    }
    // Cursor (hidden when CH bit 5 is set).
    let shape = mem.read16(video::BDA_CURSOR_SHAPE);
    if shape & 0x2000 == 0 && blink_on {
        let (cc, cr) = Video::cursor(mem, page);
        if cc < cols && cr < rows {
            let (_, attr) = Video::read_cell(mem, page, cc, cr);
            let x = rect.left() + cc as f32 * cw;
            let y = rect.top() + cr as f32 * ch;
            painter.rect_filled(
                Rect::from_min_max(pos2(x, y + ch - 3.0), pos2(x + cw, y + ch - 1.0)),
                0.0,
                CGA[(attr & 15).max(7) as usize],
            );
        }
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
}

/// Text of the screen in text modes, one line per row without trailing spaces.
pub fn screen_text(mem: &rustemu_core::Memory) -> String {
    if !Video::is_text(mem) {
        return String::new();
    }
    let page = Video::page(mem);
    let mut lines: Vec<String> = (0..Video::rows(mem))
        .map(|r| {
            let line: String = (0..Video::cols(mem))
                .map(|c| match Video::read_cell(mem, page, c, r).0 {
                    0 => ' ',
                    b => cp437_to_char(b),
                })
                .collect();
            line.trim_end().to_string()
        })
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Queues `text` as keystrokes: a new line is Enter, a tab is Tab.
pub fn paste_keys(m: &mut rustemu_core::Machine, text: &str) {
    for c in text.replace("\r\n", "\n").chars() {
        match c {
            '\n' | '\r' => m.push_key(0x1C, 13),
            '\t' => m.push_key(0x0F, 9),
            c if (c as u32) < 32 => {}
            c => m.push_char(char_to_cp437(c)),
        }
    }
}

/// Paste from the menu or the button (egui only delivers the clipboard with ⌘V events).
fn paste_from_clipboard(app: &mut App) {
    if let Ok(out) = std::process::Command::new("pbpaste").output() {
        paste_keys(&mut app.session.machine, &String::from_utf8_lossy(&out.stdout));
    }
}

fn keyboard(app: &mut App, ui: &Ui) {
    let events = ui.input(|i| i.events.clone());
    let m = &mut app.session.machine;
    for ev in events {
        match ev {
            egui::Event::Paste(t) => paste_keys(m, &t),
            egui::Event::Copy => ui.ctx().copy_text(screen_text(&m.sys.mem)),
            egui::Event::Text(t) => {
                for c in t.chars() {
                    m.push_char(char_to_cp437(c));
                }
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                let ext = |scan: u8| (scan, 0u8);
                let k = match key {
                    Key::Enter => Some((0x1C, 13)),
                    Key::Backspace => Some((0x0E, 8)),
                    Key::Escape => Some((0x01, 27)),
                    Key::Tab => Some((0x0F, 9)),
                    Key::ArrowUp => Some(ext(keys::UP)),
                    Key::ArrowDown => Some(ext(keys::DOWN)),
                    Key::ArrowLeft => Some(ext(keys::LEFT)),
                    Key::ArrowRight => Some(ext(keys::RIGHT)),
                    Key::Home => Some(ext(keys::HOME)),
                    Key::End => Some(ext(keys::END)),
                    Key::PageUp => Some(ext(keys::PGUP)),
                    Key::PageDown => Some(ext(keys::PGDN)),
                    Key::Insert => Some(ext(keys::INS)),
                    Key::Delete => Some((keys::DEL, 0)),
                    Key::F1
                    | Key::F2
                    | Key::F3
                    | Key::F4
                    | Key::F5
                    | Key::F6
                    | Key::F7
                    | Key::F8
                    | Key::F9
                    | Key::F10 => {
                        let n =
                            [Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10]
                                .iter()
                                .position(|k| *k == key)
                                .unwrap() as u8;
                        Some(ext(keys::F1 + n))
                    }
                    _ if modifiers.ctrl => {
                        // Ctrl+letter -> ASCII 1..26
                        let name = key.name();
                        let c = name.chars().next().filter(|_| name.len() == 1).map(|c| c.to_ascii_lowercase());
                        c.filter(|c| c.is_ascii_lowercase()).map(|c| (scan_code_for(c as u8), c as u8 - b'a' + 1))
                    }
                    _ => None,
                };
                if let Some((scan, ascii)) = k {
                    m.push_key(scan, ascii);
                }
            }
            _ => {}
        }
    }
}

fn mouse(app: &mut App, ui: &Ui, rect: Rect, resp: &egui::Response) {
    if let Some(p) = resp.hover_pos() {
        let rel = p - rect.min;
        let (w, h) = (rect.width().max(1.0), rect.height().max(1.0));
        // INT 33h reports coordinates on a 640x200 virtual screen in text mode and mode 13h.
        let (mx, my) = (640.0, 200.0);
        let x = (rel.x / w * mx).clamp(0.0, mx - 1.0) as u16;
        let y = (rel.y / h * my).clamp(0.0, my - 1.0) as u16;
        let buttons = ui.input(|i| {
            (i.pointer.primary_down() as u16)
                | ((i.pointer.secondary_down() as u16) << 1)
                | ((i.pointer.middle_down() as u16) << 2)
        });
        app.session.machine.mouse = (x, y, buttons);
    }
}
