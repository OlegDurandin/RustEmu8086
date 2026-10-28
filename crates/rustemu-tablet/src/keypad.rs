//! On-screen keyboard of the tablet layout: a US layout plus the PC keys DOS programs use (Esc,
//! Tab, Ctrl, arrows, F1-F10, Ins / Del / Home / End / PgUp / PgDn). Keys go to the BIOS keyboard
//! buffer of the emulated machine, like the keys typed on the emulator screen.

use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use rustemu_core::font::char_to_cp437;
use rustemu_core::keyboard::keys;

use rustemu_ui::app::{App, RunMode};

use crate::theme::pal;
use crate::widgets::{GAP as SHEET_GAP, split_x, split_y};

#[derive(Default)]
pub struct KeypadState {
    pub open: bool,
    /// One-shot modifiers: they apply to the next key.
    shift: bool,
    ctrl: bool,
    /// The first row shows F1-F10, the bottom row Home / End / PgUp / PgDn.
    fn_keys: bool,
    /// The program waited for input in the last frame (the keyboard opens when a wait starts).
    was_waiting: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Key {
    /// A character key: normal and with Shift.
    Ch(char, char),
    Esc,
    Tab,
    Enter,
    Back,
    Space,
    Shift,
    Ctrl,
    Fn,
    Hide,
    F(u8),
    Ext(u8, &'static str),
}

/// A row of keys with their widths (a normal key is 1).
type Row = Vec<(Key, f32)>;

/// Width of every row in key units.
const UNITS: f32 = 15.0;

fn chars(s: &str, shifted: &str) -> Vec<(Key, f32)> {
    s.chars().zip(shifted.chars()).map(|(c, sc)| (Key::Ch(c, sc), 1.0)).collect()
}

fn rows(fn_keys: bool) -> Vec<Row> {
    let mut r1 = vec![(Key::Esc, 1.0)];
    if fn_keys {
        r1.extend((0..10).map(|n| (Key::F(n), 1.0)));
        r1.extend([(Key::Ext(keys::INS, "Ins"), 1.0), (Key::Ext(keys::DEL, "Del"), 1.0)]);
    } else {
        r1.extend(chars("1234567890-=", "!@#$%^&*()_+"));
    }
    r1.push((Key::Back, 2.0));
    let mut r2 = vec![(Key::Tab, 1.5)];
    r2.extend(chars("qwertyuiop[]\\", "QWERTYUIOP{}|"));
    r2.last_mut().unwrap().1 = 1.5;
    let mut r3 = vec![(Key::Ctrl, 1.75)];
    r3.extend(chars("asdfghjkl;'", "ASDFGHJKL:\""));
    r3.push((Key::Enter, 2.25));
    let mut r4 = vec![(Key::Shift, 2.25)];
    r4.extend(chars("zxcvbnm,./`", "ZXCVBNM<>?~"));
    r4.extend([(Key::Ext(keys::UP, "Up"), 1.0), (Key::Shift, 0.75)]);
    let mut r5 = vec![(Key::Fn, 1.5)];
    if fn_keys {
        r5.extend([
            (Key::Ext(keys::HOME, "Home"), 1.25),
            (Key::Ext(keys::END, "End"), 1.25),
            (Key::Ext(keys::PGUP, "PgUp"), 1.25),
            (Key::Ext(keys::PGDN, "PgDn"), 1.25),
            (Key::Space, 4.5),
        ]);
    } else {
        r5.push((Key::Space, 9.5));
    }
    r5.extend([
        (Key::Ext(keys::LEFT, "Left"), 1.0),
        (Key::Ext(keys::DOWN, "Down"), 1.0),
        (Key::Ext(keys::RIGHT, "Right"), 1.0),
        (Key::Hide, 1.0),
    ]);
    vec![r1, r2, r3, r4, r5]
}

/// Height of the keyboard for a given width.
pub fn height(width: f32) -> f32 {
    let key_h = (width / UNITS * 0.85).clamp(34.0, 52.0);
    5.0 * (key_h + GAP) + GAP
}

const GAP: f32 = 6.0;

/// Opens the keyboard when the program starts waiting for input.
pub fn follow_input(app: &App, k: &mut KeypadState) {
    let waiting = app.session.machine.is_waiting_input();
    if waiting && !k.was_waiting {
        k.open = true;
    }
    k.was_waiting = waiting;
}

/// Draws the keys into `rect` and types the pressed one.
pub fn keys_ui(app: &mut App, k: &mut KeypadState, ui: &mut Ui, rect: Rect, colors: &Colors) {
    let rows = rows(k.fn_keys);
    let unit = (rect.width() - GAP * (UNITS - 1.0)) / UNITS;
    let key_h = (rect.height() - GAP * 4.0) / 5.0;
    let mut pressed = None;
    for (r, row) in rows.iter().enumerate() {
        let mut x = rect.left();
        let y = rect.top() + r as f32 * (key_h + GAP);
        for &(key, w) in row {
            let kw = unit * w + GAP * (w - 1.0).max(0.0);
            let kr = Rect::from_min_size(pos2(x, y), vec2(kw, key_h));
            x += kw + GAP;
            let id = ui.scope_id().with(("key", r, kr.left() as i32));
            let resp = ui.interact(kr, id, Sense::click());
            let label = label(k, key);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("key {label}")));
            let active = match key {
                Key::Shift => k.shift,
                Key::Ctrl => k.ctrl,
                Key::Fn => k.fn_keys,
                _ => false,
            };
            let special = !matches!(key, Key::Ch(..) | Key::Space);
            let base = if active {
                colors.accent
            } else if special {
                colors.special
            } else {
                colors.key
            };
            let fill = if resp.is_pointer_button_down_on() {
                colors.accent.gamma_multiply(0.7)
            } else if resp.hovered() {
                base.gamma_multiply(1.25)
            } else {
                base
            };
            let p = ui.painter();
            p.rect(kr, 6.0, fill, Stroke::new(1.0, colors.border), StrokeKind::Inside);
            if !paint_symbol(p, key, kr.center(), key_h.min(kw) * 0.42, colors.text) {
                let size = if label.chars().count() > 2 { 15.0 } else { 19.0 };
                p.text(kr.center(), Align2::CENTER_CENTER, &label, FontId::proportional(size), colors.text);
            }
            if resp.clicked() {
                pressed = Some(key);
            }
        }
    }
    if let Some(key) = pressed {
        press(app, k, key);
    }
}

/// Arrows, Backspace, Shift and Hide are drawn (the bundled fonts have no such symbols).
fn paint_symbol(p: &egui::Painter, key: Key, c: egui::Pos2, s: f32, color: Color32) -> bool {
    let st = Stroke::new(2.0, color);
    let arrow = |dir: egui::Vec2| {
        let n = vec2(-dir.y, dir.x);
        let tip = c + dir * s * 0.5;
        p.line_segment([c - dir * s * 0.5, tip], st);
        p.line_segment([tip, tip - dir * s * 0.35 + n * s * 0.3], st);
        p.line_segment([tip, tip - dir * s * 0.35 - n * s * 0.3], st);
    };
    match key {
        Key::Ext(keys::UP, _) => arrow(vec2(0.0, -1.0)),
        Key::Ext(keys::DOWN, _) => arrow(vec2(0.0, 1.0)),
        Key::Ext(keys::LEFT, _) => arrow(vec2(-1.0, 0.0)),
        Key::Ext(keys::RIGHT, _) => arrow(vec2(1.0, 0.0)),
        Key::Back => {
            let (w, h) = (s * 0.8, s * 0.45);
            let pts = vec![
                c + vec2(-w, 0.0),
                c + vec2(-w * 0.45, -h),
                c + vec2(w, -h),
                c + vec2(w, h),
                c + vec2(-w * 0.45, h),
            ];
            p.add(egui::Shape::closed_line(pts, st));
            let x = c + vec2(w * 0.25, 0.0);
            let d = h * 0.45;
            p.line_segment([x + vec2(-d, -d), x + vec2(d, d)], st);
            p.line_segment([x + vec2(-d, d), x + vec2(d, -d)], st);
        }
        Key::Shift => {
            let (w, h) = (s * 0.55, s * 0.6);
            let pts = vec![
                c + vec2(0.0, -h),
                c + vec2(w, 0.0),
                c + vec2(w * 0.45, 0.0),
                c + vec2(w * 0.45, h),
                c + vec2(-w * 0.45, h),
                c + vec2(-w * 0.45, 0.0),
                c + vec2(-w, 0.0),
            ];
            p.add(egui::Shape::closed_line(pts, st));
        }
        Key::Hide => {
            p.add(egui::Shape::line(
                vec![c + vec2(-s * 0.4, -s * 0.15), c + vec2(0.0, s * 0.25), c + vec2(s * 0.4, -s * 0.15)],
                st,
            ));
        }
        _ => return false,
    }
    true
}

/// Colours of the keys (from the tablet theme).
pub struct Colors {
    pub key: Color32,
    pub special: Color32,
    pub accent: Color32,
    pub border: Color32,
    pub text: Color32,
}

fn label(s: &KeypadState, key: Key) -> String {
    match key {
        Key::Ch(c, sc) => {
            let shifted = s.shift;
            if c.is_ascii_lowercase() {
                if shifted { sc.to_string() } else { c.to_ascii_uppercase().to_string() }
            } else if shifted {
                sc.to_string()
            } else {
                c.to_string()
            }
        }
        Key::Esc => "Esc".into(),
        Key::Tab => "Tab".into(),
        Key::Enter => "Enter".into(),
        Key::Back => "Backspace".into(),
        Key::Space => "space".into(),
        Key::Shift => "Shift".into(),
        Key::Ctrl => "Ctrl".into(),
        Key::Fn => if s.fn_keys { "123" } else { "Fn" }.into(),
        Key::Hide => "Hide".into(),
        Key::F(n) => format!("F{}", n + 1),
        Key::Ext(_, name) => name.into(),
    }
}

/// Types `key` into the emulated machine (scan code + ASCII, as the BIOS stores them).
fn press(app: &mut App, s: &mut KeypadState, key: Key) {
    let m = &mut app.session.machine;
    match key {
        Key::Shift => {
            s.shift = !s.shift;
            return;
        }
        Key::Ctrl => {
            s.ctrl = !s.ctrl;
            return;
        }
        Key::Fn => {
            s.fn_keys = !s.fn_keys;
            return;
        }
        Key::Hide => {
            s.open = false;
            return;
        }
        Key::Ch(c, sc) => {
            let c = if s.shift { sc } else { c };
            if s.ctrl && c.is_ascii_alphabetic() {
                // Ctrl+letter: ASCII 1..26.
                m.push_char(c.to_ascii_lowercase() as u8 - b'a' + 1);
            } else {
                m.push_char(char_to_cp437(c));
            }
        }
        Key::Esc => m.push_key(0x01, 27),
        Key::Tab => m.push_key(0x0F, 9),
        Key::Enter => m.push_key(0x1C, 13),
        Key::Back => m.push_key(0x0E, 8),
        Key::Space => m.push_key(0x39, b' '),
        Key::F(n) => m.push_key(keys::F1 + n, 0),
        Key::Ext(scan, _) => m.push_key(scan, 0),
    }
    s.shift = false;
    s.ctrl = false;
}

/// The on-screen keyboard with a large emulator screen beside it (the output of the typed keys).
pub(crate) fn keyboard_sheet(app: &mut App, k: &mut KeypadState, ui: &mut Ui) {
    let width = ui.available_width();
    // The keys are at least 44 points wide (Apple's minimum touch target). In portrait the screen
    // goes above the keys.
    let wide = width >= 1100.0;
    let keys_w = if wide { (15.0 * 46.0f32).max(width * 0.48).min(width - 400.0) } else { width };
    let keys_h = height(keys_w);
    let screen_h = if wide { keys_h } else { 190.0 };
    let height = if wide { keys_h } else { screen_h + SHEET_GAP + keys_h };
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let (screen, keys) = if wide { split_x(rect, width - keys_w - SHEET_GAP) } else { split_y(rect, screen_h) };
    let mut s =
        ui.new_child(UiBuilder::new().id_salt("keys_screen").max_rect(screen).layout(Layout::top_down(Align::Center)));
    let hint = if app.session.machine.is_waiting_input() {
        RichText::new("the program is waiting for input").color(pal::AMBER)
    } else if app.run != RunMode::Paused {
        RichText::new("running").color(pal::TEAL)
    } else {
        RichText::new("keys are buffered until the program reads them").color(pal::WEAK)
    };
    let avail = screen.size() - vec2(0.0, 26.0);
    rustemu_ui::screen::screen_view(app, &mut s, rustemu_ui::screen::Fit::Cursor(avail, 18.0));
    s.label(hint);
    let colors =
        Colors { key: pal::BUTTON, special: pal::HEADER, accent: pal::BLUE, border: pal::BORDER, text: pal::TEXT };
    let mut area = ui.new_child(UiBuilder::new().id_salt("keys").max_rect(keys));
    keys_ui(app, k, &mut area, keys, &colors);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_equally_wide() {
        for fn_keys in [false, true] {
            for (i, row) in rows(fn_keys).iter().enumerate() {
                let w: f32 = row.iter().map(|(_, w)| w).sum();
                assert!((w - UNITS).abs() < 1e-3, "fn={fn_keys} row {i}: {w}");
            }
        }
    }

    #[test]
    fn keys_reach_the_bios_buffer() {
        let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
        let mut k = KeypadState::default();
        let keys = |app: &App| app.session.machine.sys.keys.iter().copied().collect::<Vec<u16>>();
        press(&mut app, &mut k, Key::Ch('a', 'A'));
        press(&mut app, &mut k, Key::Shift);
        press(&mut app, &mut k, Key::Ch('1', '!'));
        press(&mut app, &mut k, Key::Ch('b', 'B'));
        press(&mut app, &mut k, Key::Ctrl);
        press(&mut app, &mut k, Key::Ch('c', 'C'));
        press(&mut app, &mut k, Key::Enter);
        press(&mut app, &mut k, Key::F(0));
        press(&mut app, &mut k, Key::Ext(keys::UP, "Up"));
        assert_eq!(keys(&app), [0x1E61, 0x0221, 0x3062, 0x2E03, 0x1C0D, 0x3B00, 0x4800]);
    }
}
