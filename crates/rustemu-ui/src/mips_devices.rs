//! MARS's tools for memory-mapped devices in the MIPS workspace: the Keyboard and Display MMIO
//! Simulator ([`rustemu_mips::mmio`]) and the Bitmap Display (words of memory as pixels).

use egui::{Color32, ColorImage, FontId, RichText, Sense, TextureHandle, TextureOptions, Ui, vec2};
use serde::{Deserialize, Serialize};

use rustemu_mips::mmio::{DELAYS, DelayTechnique, LAB_OFFSET, RECEIVER_DATA_OFFSET, TRANSMITTER_DATA_OFFSET};

use crate::app::App;

/// Settings of the two tools (saved).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DeviceSettings {
    /// Transmitter delay, in instructions (one of [`DELAYS`]).
    pub delay: u32,
    /// The display shows a character when its delay ends (MARS's "DAD").
    pub display_after_delay: bool,
    /// How the delay of each character is chosen: 0 fixed, 1 uniform, 2 "normal" (MARS's list).
    pub technique: usize,
    /// Bitmap Display: width and height of a unit (one word of memory), in pixels.
    pub unit: [u32; 2],
    /// Bitmap Display: width and height of the display, in pixels.
    pub size: [u32; 2],
    /// Bitmap Display: address of the upper left unit.
    pub base: u32,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        DeviceSettings {
            delay: rustemu_mips::mmio::DEFAULT_DELAY,
            display_after_delay: true,
            technique: 0,
            unit: [1, 1],
            size: [512, 256],
            base: rustemu_mips::memory::DATA_BASE,
        }
    }
}

/// State of the tools between frames.
#[derive(Default)]
pub struct DevicesState {
    /// What was typed on the keyboard (MARS echoes it in its keyboard area).
    pub typed: String,
    /// The keyboard area takes the keys at the next frame.
    pub focus_request: bool,
    /// Keys typed and not given to the program yet: the next one goes when the program has read
    /// the previous one (several can arrive in one frame; the device holds one).
    pub queue: std::collections::VecDeque<u8>,
    /// The bitmap's texture and the pixels it holds.
    bitmap: Option<(TextureHandle, Vec<Color32>, [usize; 2])>,
}

/// The transmitter delay techniques, in the order of MARS's list.
const TECHNIQUES: [DelayTechnique; 3] = [DelayTechnique::Fixed, DelayTechnique::Uniform, DelayTechnique::Normal];

/// Unit sizes the Bitmap Display offers, in pixels.
const UNITS: [u32; 6] = [1, 2, 4, 8, 16, 32];
/// Display sizes the Bitmap Display offers, in pixels.
const SIZES: [u32; 5] = [64, 128, 256, 512, 1024];
/// Base addresses the Bitmap Display offers, as MARS names them.
const BASE_NAMES: [&str; 5] = ["global data", "$gp", "static data", "heap", "memory map"];

/// The addresses of [`BASE_NAMES`] in memory configuration `l` (MARS takes them from its memory).
fn bases(l: &rustemu_mips::Layout) -> [u32; 5] {
    [l.data_segment_base, l.gp, l.data_base, l.heap_base, l.mmio_base]
}

/// The display's base in configuration `l`. A choice of the list is stored as its address in
/// the default configuration, so it follows the configuration; another address is used as it is.
fn base_in(stored: u32, l: &rustemu_mips::Layout) -> u32 {
    match bases(&rustemu_mips::settings::DEFAULT).iter().position(|&b| b == stored) {
        Some(i) => bases(l)[i],
        None => stored,
    }
}

impl App {
    /// Gives the MIPS machine the device settings (they may change while it runs).
    pub(crate) fn apply_mips_devices(&mut self) {
        let s = &self.settings.mips_devices;
        let (delay, dad) = (s.delay, s.display_after_delay);
        let technique = TECHNIQUES[s.technique.min(TECHNIQUES.len() - 1)];
        if let Some(m) = self.session.mips_mut() {
            m.mmio.delay = delay;
            m.mmio.display_after_delay = dad;
            m.mmio.technique = technique;
            if m.mem.peek32(rustemu_mips::mmio::RECEIVER_CONTROL) & 1 == 0
                && let Some(k) = self.devices.queue.pop_front()
            {
                m.mmio_key(k);
            }
        }
    }
}

/// The Keyboard and Display MMIO Simulator: the display, its options, the keyboard.
pub fn keyboard_display_ui(app: &mut App, ui: &mut Ui) {
    if app.session.mips().is_none() {
        ui.weak("no MIPS machine");
        return;
    }
    let font = FontId::monospace(13.0);
    let mmio = crate::mips_ui::layout(app).mmio_base;
    // From the bottom: the keyboard and the options keep their height, the display takes the rest.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        keyboard_area(app, ui, &font);
        ui.label(
            RichText::new(format!(
                "KEYBOARD: Characters typed here are stored to Receiver Data 0x{:08x}",
                mmio + RECEIVER_DATA_OFFSET
            ))
            .small(),
        );
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("Reset")
                .on_hover_text("Clear the display and the keyboard; the transmitter is ready again")
                .clicked()
                && let Some(m) = app.session.mips_mut()
            {
                m.mmio.reset(&mut m.mem);
                // The device changed outside the program's steps.
                m.clear_history();
                app.devices.typed.clear();
                app.devices.queue.clear();
            }
            if app.session.mips().is_some_and(|m| m.mmio.is_transmitting()) {
                ui.weak("transmitting…");
            }
            let queued = app.devices.queue.len();
            if queued > 0 {
                ui.weak(format!("{queued} key(s) wait for the program to read the previous one"));
            }
        });
        ui.horizontal_wrapped(|ui| {
            let s = &mut app.settings.mips_devices;
            let mut index = DELAYS.iter().position(|&d| d >= s.delay).unwrap_or(DELAYS.len() - 1);
            ui.label("Delay length:");
            let r = ui
                .add(
                    egui::Slider::new(&mut index, 0..=DELAYS.len() - 1)
                        .show_value(false)
                        .custom_formatter(|v, _| DELAYS[v as usize].to_string()),
                )
                .on_hover_text("Parameter for simulated delay length (MIPS instruction execution count)");
            if r.changed() {
                s.delay = DELAYS[index];
            }
            ui.label(format!("{} instruction executions", DELAYS[index]));
            ui.checkbox(&mut s.display_after_delay, "DAD")
                .on_hover_text("Display After Delay: if checked, transmitter data not displayed until after delay");
        });
        ui.horizontal(|ui| {
            let s = &mut app.settings.mips_devices;
            let current = TECHNIQUES[s.technique.min(TECHNIQUES.len() - 1)];
            let l = ui.label("Delay:");
            egui::ComboBox::from_id_salt("mmio_technique")
                .selected_text(current.title())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (i, t) in TECHNIQUES.iter().enumerate() {
                        ui.selectable_value(&mut s.technique, i, t.title());
                    }
                })
                .response
                .labelled_by(l.id)
                .on_hover_text("Technique for determining simulated transmitter device processing delay");
        });
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| display_area(app, ui, &font));
    });
    app.apply_mips_devices();
}

/// The display: its title line and text, in all the height there is.
fn display_area(app: &mut App, ui: &mut Ui, font: &FontId) {
    let Some(m) = app.session.mips_mut() else { return };
    let (char_w, row_h) = ui.fonts_mut(|f| (f.glyph_width(font, 'm'), f.row_height(font)));
    let title_h = ui.text_style_height(&egui::TextStyle::Small) + ui.spacing().item_spacing.y;
    let display_h = (ui.available_height() - title_h - 12.0).max(40.0);
    // MARS sizes its terminal from the window: one less than fits in each direction.
    let area = (
        ((ui.available_width() - 12.0) / char_w).floor().max(2.0) as u32 - 1,
        (display_h / row_h).floor().max(2.0) as u32 - 1,
    );
    m.mmio.terminal_size = area;
    ui.label(
        RichText::new(format!(
            "DISPLAY: Store to Transmitter Data 0x{:08x}, cursor {}, area {} x {}",
            m.mem.layout.mmio_base + TRANSMITTER_DATA_OFFSET,
            m.mmio.cursor_text(),
            area.0,
            area.1
        ))
        .small(),
    );
    let text = display_text(m.mmio.text());
    let terminal = m.mmio.is_terminal();
    egui::Frame::canvas(ui.style()).inner_margin(4.0).show(ui, |ui| {
        egui::ScrollArea::both()
            .id_salt("mmio_display")
            .stick_to_bottom(!terminal)
            .auto_shrink([false, false])
            .max_height(display_h)
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text).font(font.clone())).extend().selectable(true));
            });
    });
}

/// The display's bytes as text: Latin-1, control characters other than new lines as spaces.
fn display_text(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b == b'\n' || b >= 0x20 && b != 0x7F { b as char } else { ' ' }).collect()
}

/// The keyboard area: while it has the focus, typed characters go to the receiver.
fn keyboard_area(app: &mut App, ui: &mut Ui, font: &FontId) {
    let size = vec2(ui.available_width(), 72.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() || std::mem::take(&mut app.devices.focus_request) {
        resp.request_focus();
    }
    let focused = resp.has_focus();
    if focused {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
            )
        });
        let keys = ui.input(|i| {
            let mut keys = Vec::new();
            for e in &i.events {
                match e {
                    egui::Event::Text(t) => keys.extend(t.chars().map(|c| (c, latin1(c)))),
                    egui::Event::Key { key, pressed: true, modifiers, .. } if !modifiers.command => {
                        let k = match key {
                            egui::Key::Enter => Some('\n'),
                            egui::Key::Backspace => Some('\u{8}'),
                            egui::Key::Tab => Some('\t'),
                            egui::Key::Escape => Some('\u{1b}'),
                            _ => None,
                        };
                        keys.extend(k.map(|c| (c, c as u8)));
                    }
                    _ => {}
                }
            }
            keys
        });
        for (c, b) in keys {
            if app.devices.queue.len() < 256 {
                app.devices.queue.push_back(b);
            }
            match c {
                '\u{8}' => {
                    app.devices.typed.pop();
                }
                '\u{1b}' => {}
                c => app.devices.typed.push(c),
            }
        }
    }
    let visuals = ui.visuals();
    let stroke = if focused { visuals.selection.stroke } else { visuals.widgets.inactive.bg_stroke };
    ui.painter().rect(rect, 2.0, visuals.extreme_bg_color, stroke, egui::StrokeKind::Inside);
    let typed = &app.devices.typed;
    // The end of what was typed, as much as fits.
    let lines: Vec<&str> = typed.split('\n').collect();
    let fit = ((rect.height() - 8.0) / ui.fonts_mut(|f| f.row_height(font))).max(1.0) as usize;
    let shown = lines[lines.len().saturating_sub(fit)..].join("\n");
    let (text, color) = if typed.is_empty() && !focused {
        ("click here and type: each key goes to the program".to_string(), visuals.weak_text_color())
    } else {
        (format!("{shown}{}", if focused { "▏" } else { "" }), visuals.text_color())
    };
    ui.painter().with_clip_rect(rect).text(
        rect.left_top() + vec2(4.0, 4.0),
        egui::Align2::LEFT_TOP,
        text,
        font.clone(),
        color,
    );
}

/// The byte MARS stores for a typed character (the low byte of its code).
fn latin1(c: char) -> u8 {
    c as u32 as u8
}

/// The Bitmap Display: its settings and the units of memory as pixels.
pub fn bitmap_ui(app: &mut App, ui: &mut Ui) {
    let layout = crate::mips_ui::layout(app);
    let s = &mut app.settings.mips_devices;
    egui::Grid::new("bitmap_settings").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let choice = |ui: &mut Ui, label: &str, hover: &str, value: &mut u32, choices: &[u32]| {
            let l = ui.label(label).on_hover_text(hover);
            egui::ComboBox::from_id_salt(label)
                .selected_text(value.to_string())
                .show_ui(ui, |ui| {
                    for &c in choices {
                        ui.selectable_value(value, c, c.to_string());
                    }
                })
                .response
                .labelled_by(l.id);
            ui.end_row();
        };
        choice(
            ui,
            "Unit Width in Pixels",
            "Width in pixels of rectangle representing memory word",
            &mut s.unit[0],
            &UNITS,
        );
        choice(
            ui,
            "Unit Height in Pixels",
            "Height in pixels of rectangle representing memory word",
            &mut s.unit[1],
            &UNITS,
        );
        choice(ui, "Display Width in Pixels", "Total width in pixels of display area", &mut s.size[0], &SIZES);
        choice(ui, "Display Height in Pixels", "Total height in pixels of display area", &mut s.size[1], &SIZES);
        let l = ui.label("Base address for display").on_hover_text("Base address for display area (upper left corner)");
        let defaults = bases(&rustemu_mips::settings::DEFAULT);
        let here = bases(layout);
        let name = |b: u32| match defaults.iter().position(|&d| d == b) {
            Some(i) => format!("0x{:08x} ({})", here[i], BASE_NAMES[i]),
            None => format!("0x{b:08x}"),
        };
        egui::ComboBox::from_id_salt("bitmap_base")
            .selected_text(name(s.base))
            .show_ui(ui, |ui| {
                for a in defaults {
                    ui.selectable_value(&mut s.base, a, name(a));
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
    });
    let s = app.settings.mips_devices.clone();
    let base = base_in(s.base, layout);
    let cols = (s.size[0] / s.unit[0].max(1)).max(1) as usize;
    let rows = (s.size[1] / s.unit[1].max(1)).max(1) as usize;
    ui.weak(format!(
        "{cols} x {rows} units: words 0x{:08x}..0x{:08x}, colour 0x00RRGGBB",
        base,
        base.wrapping_add((4 * (cols * rows - 1)) as u32)
    ));
    let pixels = match app.session.mips() {
        Some(m) => bitmap_pixels(&m.mem, base, cols, rows),
        None => vec![Color32::BLACK; cols * rows],
    };
    let state = &mut app.devices.bitmap;
    let fresh = !matches!(state, Some((_, p, size)) if *size == [cols, rows] && *p == pixels);
    if fresh {
        let image = ColorImage::new([cols, rows], pixels.clone());
        match state {
            Some((t, p, size)) => {
                t.set(image, TextureOptions::NEAREST);
                *p = pixels;
                *size = [cols, rows];
            }
            None => {
                *state = Some((
                    ui.ctx().load_texture("bitmap_display", image, TextureOptions::NEAREST),
                    pixels,
                    [cols, rows],
                ))
            }
        }
    }
    let Some((texture, ..)) = state else { return };
    // The display's size in points, smaller when the window is.
    let want = vec2(s.size[0] as f32, s.size[1] as f32);
    let scale = (ui.available_width() / want.x).clamp(0.1, 1.0);
    ui.add(egui::Image::new((texture.id(), want * scale)).sense(Sense::hover())).on_hover_text("Bitmap display area");
}

/// The segments of a digit as MARS draws them in its 60 x 80 pixel cell: a..g as hexagons (points).
const SEGMENTS: [[(f32, f32); 6]; 7] = [
    [(9.0, 8.0), (12.0, 5.0), (36.0, 5.0), (39.0, 8.0), (36.0, 11.0), (12.0, 11.0)],
    [(40.0, 9.0), (43.0, 12.0), (43.0, 36.0), (40.0, 39.0), (37.0, 36.0), (37.0, 12.0)],
    [(40.0, 41.0), (43.0, 44.0), (43.0, 68.0), (40.0, 71.0), (37.0, 68.0), (37.0, 44.0)],
    [(9.0, 72.0), (12.0, 69.0), (36.0, 69.0), (39.0, 72.0), (36.0, 75.0), (12.0, 75.0)],
    [(8.0, 41.0), (11.0, 44.0), (11.0, 68.0), (8.0, 71.0), (5.0, 68.0), (5.0, 44.0)],
    [(8.0, 9.0), (11.0, 12.0), (11.0, 36.0), (8.0, 39.0), (5.0, 36.0), (5.0, 12.0)],
    [(9.0, 40.0), (12.0, 37.0), (36.0, 37.0), (39.0, 40.0), (36.0, 43.0), (12.0, 43.0)],
];

/// The Digital Lab Sim: two seven-segment displays and the hexadecimal keyboard.
pub fn digital_lab_ui(app: &mut App, ui: &mut Ui) {
    let Some(m) = app.session.mips() else {
        ui.weak("no MIPS machine");
        return;
    };
    let (segments, pressed, counter) = (m.lab.segments(), m.lab.pressed(), m.lab.counter());
    let scale = 1.2;
    let lit = Color32::from_rgb(230, 30, 30);
    let off = if ui.visuals().dark_mode { Color32::from_gray(60) } else { Color32::from_gray(205) };
    let mut click = None;
    ui.horizontal_top(|ui| {
        for (i, &bits) in segments.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(60.0, 80.0) * scale, Sense::hover());
            let at = |(x, y): (f32, f32)| rect.min + vec2(x, y) * scale;
            let painter = ui.painter();
            for (s, points) in SEGMENTS.iter().enumerate() {
                let color = if bits >> s & 1 == 1 { lit } else { off };
                painter.add(egui::Shape::convex_polygon(
                    points.iter().map(|&p| at(p)).collect(),
                    color,
                    egui::Stroke::NONE,
                ));
            }
            painter.circle_filled(at((53.0, 72.0)), 4.0 * scale, if bits & 0x80 != 0 { lit } else { off });
            ui.add_space(4.0);
            if i == 1 {
                ui.add_space(16.0);
            }
        }
        egui::Grid::new("hex_keyboard").spacing([4.0, 4.0]).show(ui, |ui| {
            for key in 0..16u8 {
                let mut b = egui::Button::new(RichText::new(format!("{key:x}")).monospace().size(16.0))
                    .min_size(vec2(40.0, 36.0));
                if pressed == Some(key) {
                    b = b.fill(Color32::from_rgb(60, 180, 75));
                }
                if ui.add(b).on_hover_text(format!("key {key:x}: row {}, column {}", key / 4, key % 4)).clicked() {
                    click = Some(key);
                }
                if key % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    });
    if let Some(k) = click
        && let Some(m) = app.session.mips_mut()
    {
        m.lab_click(k);
    }
    ui.horizontal_wrapped(|ui| {
        ui.weak(match pressed {
            Some(k) => format!("key {k:x} pressed (a click releases it)"),
            None => "no key pressed".into(),
        });
        ui.weak(match counter {
            Some(n) => format!("· counter on: interrupt after {} more", n + 1),
            None => "· counter off".into(),
        });
        if ui.button("Reset").on_hover_text("Displays dark, no key pressed, counter off").clicked()
            && let Some(m) = app.session.mips_mut()
        {
            m.lab.reset();
            m.clear_history();
        }
    });
    let b = crate::mips_ui::layout(app).mmio_base + LAB_OFFSET;
    ui.label(
        RichText::new(format!(
            "0x{b:08x} / 0x{:08x}: segments of the right / left display (bit 0 = a … bit 6 = g, bit 7 = point) · \
             0x{:08x}: row to scan (bits 0..3), bit 7 = keyboard interrupt · 0x{:08x}: the key found (row | column << 4) · \
             0x{:08x}: non-zero = counter interrupt every 30 instructions",
            b + 1,
            b + 2,
            b + 4,
            b + 3
        ))
        .small()
        .weak(),
    );
}

/// Which devices the code reaches: (keyboard and display, Digital Lab Sim), with memory-mapped
/// I/O at `mmio_base`. At 0xffff0000 an address of the segment is built with `lui $at, 0xffff`
/// and the offset is in the next instruction (`ori`, a load or a store); in a compact
/// configuration (MMIO at 0x7f00) the address is an instruction's 16-bit immediate. Offsets
/// 0x10..0x14 are the Digital Lab Sim's.
pub fn devices_used(words: impl Iterator<Item = u32>, mmio_base: u32) -> (bool, bool) {
    let (mut keyboard, mut lab) = (false, false);
    let mut note = |offset: u32| {
        if (0x10..=0x14).contains(&offset) { lab = true } else { keyboard = true }
    };
    let mut after_lui = false;
    for w in words {
        let imm = w & 0xFFFF;
        if mmio_base >> 16 == 0 {
            // An I-type instruction (not a branch) with the address as its immediate.
            let op = w >> 26;
            if op >= 0x08 && (mmio_base..mmio_base + 0x18).contains(&imm) {
                note(imm - mmio_base);
            }
            continue;
        }
        if after_lui {
            note(imm);
        }
        after_lui = w & 0xFFE0_FFFF == (0x3C00_0000 | mmio_base >> 16);
    }
    (keyboard, lab)
}

/// The colours of `cols` x `rows` words from `base`, row by row (past the end of the address
/// space: black).
fn bitmap_pixels(mem: &rustemu_mips::Memory, base: u32, cols: usize, rows: usize) -> Vec<Color32> {
    (0..cols * rows)
        .map(|i| {
            let Some(a) = base.checked_add(4 * i as u32) else { return Color32::BLACK };
            let w = mem.peek32(a);
            Color32::from_rgb((w >> 16) as u8, (w >> 8) as u8, w as u8)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_rgb_pixels() {
        let mut mem = rustemu_mips::Memory::new();
        mem.load(0x1001_0004, &0x00ff_8000u32.to_le_bytes());
        let p = bitmap_pixels(&mem, 0x1001_0000, 2, 1);
        assert_eq!(p, vec![Color32::BLACK, Color32::from_rgb(255, 128, 0)]);
        assert_eq!(bitmap_pixels(&mem, 0xffff_fffc, 2, 1).len(), 2, "the end of memory");
    }

    #[test]
    fn display_text_is_latin1() {
        assert_eq!(display_text(b"a\x07\n\xe9"), "a \né");
    }
}
