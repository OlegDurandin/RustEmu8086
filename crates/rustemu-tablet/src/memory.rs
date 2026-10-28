//! Memory views: the hex editor and the compact memory card, byte inspector, segments, stack and
//! the memory map.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use rustemu_core::cpu::{self};
use rustemu_core::font::cp437_to_char;
use rustemu_core::linear;
use rustemu_ui::app::{App, RunMode};

use crate::theme::pal;
use crate::widgets::{HEAD_H, Icon, ROW_H, card, field_style, hex_box, small_icon, value_box};

/// Segment of the memory views.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemSeg {
    #[default]
    Ds,
    Cs,
    Es,
    Ss,
    Video,
    /// Chosen by address (go to, memory map).
    Fixed(u16),
}

impl MemSeg {
    fn name(self) -> String {
        match self {
            MemSeg::Ds => "DS".into(),
            MemSeg::Cs => "CS".into(),
            MemSeg::Es => "ES".into(),
            MemSeg::Ss => "SS".into(),
            MemSeg::Video => "video".into(),
            MemSeg::Fixed(s) => format!("{s:04X}"),
        }
    }

    pub(crate) fn value(self, app: &App) -> u16 {
        let c = &app.session.machine.cpu;
        match self {
            MemSeg::Ds => c.sregs[cpu::DS],
            MemSeg::Cs => c.sregs[cpu::CS],
            MemSeg::Es => c.sregs[cpu::ES],
            MemSeg::Ss => c.sregs[cpu::SS],
            MemSeg::Video if rustemu_core::video::Video::is_text(&app.session.machine.sys.mem) => 0xB800,
            MemSeg::Video => 0xA000,
            MemSeg::Fixed(s) => s,
        }
    }

    /// Offset the views start at when the segment is chosen.
    fn start(self, app: &App) -> u16 {
        let c = &app.session.machine.cpu;
        match self {
            MemSeg::Cs => c.ip & !0xF,
            MemSeg::Ss => c.regs[cpu::SP] & !0xF,
            _ => 0,
        }
    }
}

/// State of the memory views (hex editor, memory card, inspector, map).
pub struct MemoryView {
    pub(crate) seg: MemSeg,
    /// First offset of the memory views (relative to the segment).
    pub(crate) mem_off: u16,
    /// Selected byte of the hex editor (linear address).
    pub(crate) selected: Option<u32>,
    /// High nibble typed into the selected byte.
    nibble: Option<u8>,
    /// Typing goes to the ASCII column of the hex editor.
    ascii_input: bool,
    bytes_per_row: u16,
    goto_text: String,
    find_text: String,
    /// The hex editor scrolls to `mem_off` in the next frame.
    scroll_pending: bool,
}

impl Default for MemoryView {
    fn default() -> Self {
        MemoryView {
            seg: MemSeg::Ds,
            mem_off: 0,
            selected: None,
            nibble: None,
            ascii_input: false,
            bytes_per_row: 16,
            goto_text: String::new(),
            find_text: String::new(),
            scroll_pending: true,
        }
    }
}

/// Segment chooser of the memory views (`rtl`: in a right-to-left header).
fn segment_menu(app: &mut App, m: &mut MemoryView, ui: &mut Ui, rtl: bool) {
    let seg = m.seg;
    let text = format!("{}:{:04X}", seg.name(), m.mem_off);
    let mut label = None;
    if !rtl {
        label = Some(ui.label(RichText::new("Segment:").color(pal::TEXT)).id);
    }
    let r = ui.scope(|ui| {
        field_style(ui, None);
        egui::ComboBox::from_id_salt(("t_seg", ui.scope_id())).selected_text(text).width(130.0).show_ui(ui, |ui| {
            for s in [MemSeg::Ds, MemSeg::Cs, MemSeg::Es, MemSeg::Ss, MemSeg::Video] {
                let label = match s {
                    MemSeg::Cs => "CS:IP".to_string(),
                    MemSeg::Ss => "SS:SP".to_string(),
                    MemSeg::Video => format!("video ({:04X})", s.value(app)),
                    _ => format!("{}:0000", s.name()),
                };
                if ui.selectable_label(m.seg == s, label).clicked() {
                    show_segment(app, m, s);
                }
            }
        })
    });
    if rtl && ui.available_width() > 80.0 {
        label = Some(ui.label(RichText::new("segment").color(pal::WEAK)).id);
    } else if rtl {
        r.inner.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "segment"));
    }
    if let Some(id) = label {
        r.inner.response.labelled_by(id);
    }
}

fn show_segment(app: &mut App, m: &mut MemoryView, s: MemSeg) {
    m.seg = s;
    m.mem_off = s.start(app);
    m.scroll_pending = true;
}

/// Shows linear address `a` in the memory views (keeps the segment when it covers `a`).
pub(crate) fn show_address(app: &mut App, m: &mut MemoryView, a: u32) {
    let base = (m.seg.value(app) as u32) << 4;
    if a < base || a - base > 0xFFFF {
        m.seg = MemSeg::Fixed(((a >> 4) & 0xF000) as u16);
    }
    let base = (m.seg.value(app) as u32) << 4;
    m.mem_off = (a - base) as u16 & !0xF;
    m.selected = Some(a);
    m.nibble = None;
    m.scroll_pending = true;
}

/// Column positions of a hex row.
struct HexCols {
    bpr: u16,
    /// `seg:off` addresses (hex editor) instead of offsets (compact view).
    long_addr: bool,
    addr_w: f32,
    cell_w: f32,
    ascii_x: f32,
    char_w: f32,
}

fn hex_cols(ui: &Ui, font: &FontId, bpr: u16, long_addr: bool) -> HexCols {
    let char_w = ui.fonts_mut(|f| f.glyph_width(font, '0'));
    let addr_w = char_w * if long_addr { 10.0 } else { 5.0 } + 10.0;
    let cell_w = char_w * 2.0 + 8.0;
    HexCols { bpr, long_addr, addr_w, cell_w, ascii_x: addr_w + cell_w * bpr as f32 + 10.0, char_w }
}

fn hex_header(ui: &mut Ui, font: &FontId, cols: &HexCols, bpr: u16) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let p = ui.painter();
    let y = r.center().y;
    let title = if cols.addr_w > cols.char_w * 8.0 { "Address" } else { "Addr" };
    p.text(pos2(r.left(), y), Align2::LEFT_CENTER, title, font.clone(), pal::LINK);
    for i in 0..bpr {
        let x = r.left() + cols.addr_w + i as f32 * cols.cell_w + cols.cell_w / 2.0;
        p.text(pos2(x, y), Align2::CENTER_CENTER, format!("{i:02X}"), font.clone(), pal::WEAK);
    }
    p.text(pos2(r.left() + cols.ascii_x, y), Align2::LEFT_CENTER, "ASCII", font.clone(), pal::LINK);
    p.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], Stroke::new(1.0, pal::BORDER));
}

/// One row of the hex views; returns the clicked byte.
fn hex_row(app: &App, m: &MemoryView, ui: &mut Ui, font: &FontId, cols: &HexCols, seg: u16, off: u16) -> Option<u32> {
    let (bpr, long_addr) = (cols.bpr, cols.long_addr);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(r) {
        return None;
    }
    let p = ui.painter();
    let y = r.center().y;
    let mem = &app.session.machine.sys.mem;
    let base = linear(seg, off);
    let selected = m.selected;
    let row_has_sel = selected.is_some_and(|s| s.wrapping_sub(base) < bpr as u32);
    if row_has_sel && long_addr {
        p.rect_filled(r, 2.0, pal::AMBER_BG);
    }
    let addr = if long_addr { format!("{seg:04X}:{off:04X}") } else { format!("{off:04X}:") };
    p.text(pos2(r.left(), y), Align2::LEFT_CENTER, addr, font.clone(), if long_addr { pal::LINK } else { pal::WEAK });
    let pc = app.session.machine.cpu.pc();
    for i in 0..bpr {
        let a = linear(seg, off.wrapping_add(i));
        let v = mem.read8(a);
        let cell = Rect::from_center_size(
            pos2(r.left() + cols.addr_w + (i as f32 + 0.5) * cols.cell_w, y),
            vec2(cols.cell_w - 4.0, ROW_H - 4.0),
        );
        if selected == Some(a) {
            p.rect_stroke(
                cell,
                3.0,
                Stroke::new(1.5, if long_addr { pal::AMBER } else { pal::LINK }),
                StrokeKind::Inside,
            );
        }
        let color = if a == pc { pal::AMBER } else { pal::TEXT };
        p.text(cell.center(), Align2::CENTER_CENTER, format!("{v:02X}"), font.clone(), color);
        let cx = r.left() + cols.ascii_x + i as f32 * cols.char_w;
        if selected == Some(a) {
            p.rect_filled(
                Rect::from_min_size(pos2(cx, y - 9.0), vec2(cols.char_w, 18.0)),
                1.0,
                pal::AMBER.gamma_multiply(0.6),
            );
        }
        let ch = if (32..127).contains(&v) {
            v as char
        } else if v >= 128 {
            cp437_to_char(v)
        } else {
            '.'
        };
        p.text(pos2(cx, y), Align2::LEFT_CENTER, ch.to_string(), font.clone(), pal::TEXT);
    }
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let x = pos.x - r.left();
        let i = if x >= cols.ascii_x {
            ((x - cols.ascii_x) / cols.char_w).floor()
        } else {
            ((x - cols.addr_w) / cols.cell_w).floor()
        };
        if (0.0..bpr as f32).contains(&i) {
            return Some(linear(seg, off.wrapping_add(i as u16)));
        }
    }
    None
}

/// Compact memory view of the Editor tab.
pub(crate) fn memory_card(app: &mut App, m: &mut MemoryView, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "MEMORY");
    b.spacing_mut().item_spacing.y = 0.0;
    let rows = (((b.available_height() - ROW_H) / ROW_H).floor() as u16).max(1);
    let font = FontId::monospace(14.0);
    let bpr = [16u16, 8]
        .into_iter()
        .find(|&n| {
            let c = hex_cols(&b, &font, n, false);
            c.ascii_x + n as f32 * c.char_w + 4.0 <= b.available_width()
        })
        .unwrap_or(8);
    if small_icon(&mut head, Icon::Down, "next rows").clicked() {
        m.mem_off = m.mem_off.wrapping_add(bpr * rows);
    }
    if small_icon(&mut head, Icon::Up, "previous rows").clicked() {
        m.mem_off = m.mem_off.wrapping_sub(bpr * rows);
    }
    segment_menu(app, m, &mut head, true);
    let cols = hex_cols(&b, &font, bpr, false);
    hex_header(&mut b, &font, &cols, bpr);
    let seg = m.seg.value(app);
    let mut clicked = None;
    for row in 0..rows {
        let off = m.mem_off.wrapping_add(row * bpr);
        clicked = clicked.or(hex_row(app, m, &mut b, &font, &cols, seg, off));
    }
    if let Some(a) = clicked {
        m.selected = Some(a);
        m.nibble = None;
    }
}

// ---- memory tab --------------------------------------------------------------------------------

pub(crate) fn hex_editor_card(app: &mut App, m: &mut MemoryView, ui: &mut Ui, rect: Rect) {
    let (_, mut b) = card(ui, rect, "HEX EDITOR");
    let font = FontId::monospace(14.5);
    b.horizontal_wrapped(|ui| {
        segment_menu(app, m, ui, false);
        ui.label(RichText::new("Go to address:").color(pal::TEXT));
        let r = ui.scope(|ui| {
            field_style(ui, None);
            ui.add_sized([110.0, 30.0], egui::TextEdit::singleline(&mut m.goto_text).hint_text("0100"))
        });
        if r.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            go_to(app, m);
        }
        let r = ui.scope(|ui| {
            field_style(ui, None);
            ui.add_sized([180.0, 30.0], egui::TextEdit::singleline(&mut m.find_text).hint_text("Find hex or ASCII…"))
        });
        if r.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            find_next(app, m);
        }
        for (ascii, name) in [(false, "HEX"), (true, "ASCII")] {
            if ui.selectable_label(m.ascii_input == ascii, name).on_hover_text("typing edits this column").clicked() {
                m.ascii_input = ascii;
                m.nibble = None;
            }
        }
        ui.scope(|ui| {
            field_style(ui, None);
            egui::ComboBox::new("t_bpr", "bytes / row").selected_text(m.bytes_per_row.to_string()).width(60.0).show_ui(
                ui,
                |ui| {
                    for n in [8u16, 16, 32] {
                        if ui.selectable_label(m.bytes_per_row == n, n.to_string()).clicked() {
                            m.bytes_per_row = n;
                            m.scroll_pending = true;
                        }
                    }
                },
            );
        });
    });
    b.add_space(6.0);
    let fits = |n: u16| {
        let c = hex_cols(&b, &font, n, true);
        c.ascii_x + n as f32 * c.char_w + 4.0 <= b.available_width()
    };
    let bpr = [m.bytes_per_row, 16, 8].into_iter().find(|&n| fits(n)).unwrap_or(8);
    let cols = hex_cols(&b, &font, bpr, true);
    b.spacing_mut().item_spacing.y = 0.0;
    hex_header(&mut b, &font, &cols, bpr);
    let seg = m.seg.value(app);
    let total = 0x10000 / bpr as usize;
    let mut sa = egui::ScrollArea::vertical().id_salt("t_hex").auto_shrink([false, false]);
    if m.scroll_pending {
        m.scroll_pending = false;
        let step = ROW_H + b.spacing().item_spacing.y;
        sa = sa.vertical_scroll_offset((m.mem_off / bpr) as f32 * step);
    }
    let mut clicked = None;
    let out = sa.show_rows(&mut b, ROW_H, total, |ui, range| {
        for row in range {
            let off = (row as u32 * bpr as u32) as u16;
            clicked = clicked.or(hex_row(app, m, ui, &font, &cols, seg, off));
        }
    });
    let step = ROW_H + b.spacing().item_spacing.y;
    m.mem_off = ((out.state.clamped_offset().y / step) as u32 * bpr as u32).min(0xFFFF) as u16;
    if let Some(a) = clicked {
        m.selected = Some(a);
        m.nibble = None;
    }
    hex_typing(app, m, &b, bpr);
}

/// Typing into the selected byte (hex digits or characters) and moving with the arrow keys.
fn hex_typing(app: &mut App, m: &mut MemoryView, ui: &Ui, bpr: u16) {
    let Some(sel) = m.selected else { return };
    if ui.ctx().memory(|m| m.focused().is_some()) || app.run != RunMode::Paused {
        return;
    }
    let events = ui.input(|i| i.events.clone());
    let mut a = sel;
    for ev in events {
        match ev {
            egui::Event::Text(t) => {
                for ch in t.chars() {
                    let mem = &mut app.session.machine.sys.mem;
                    if m.ascii_input {
                        mem.write8(a, rustemu_core::font::char_to_cp437(ch));
                        a = (a + 1) & 0xFFFFF;
                    } else if let Some(d) = ch.to_digit(16) {
                        match m.nibble.take() {
                            None => {
                                m.nibble = Some(d as u8);
                                mem.write8(a, (d as u8) << 4 | (mem.read8(a) & 0xF));
                            }
                            Some(h) => {
                                mem.write8(a, h << 4 | d as u8);
                                a = (a + 1) & 0xFFFFF;
                            }
                        }
                    }
                }
            }
            egui::Event::Key { key, pressed: true, .. } => {
                let d: i64 = match key {
                    egui::Key::ArrowLeft => -1,
                    egui::Key::ArrowRight => 1,
                    egui::Key::ArrowUp => -(bpr as i64),
                    egui::Key::ArrowDown => bpr as i64,
                    _ => 0,
                };
                if d != 0 {
                    a = (a as i64 + d).rem_euclid(0x100000) as u32;
                    m.nibble = None;
                }
            }
            _ => {}
        }
    }
    if a != sel {
        m.selected = Some(a);
    }
}

fn go_to(app: &mut App, m: &mut MemoryView) {
    let text = m.goto_text.trim().to_string();
    let parsed = if text.contains(':') {
        rustemu_ui::debug::parse_addr(app, &text)
    } else {
        // An offset in the current segment.
        u32::from_str_radix(text.trim_end_matches(['h', 'H']), 16)
            .ok()
            .filter(|v| *v <= 0xFFFF)
            .map(|v| linear(m.seg.value(app), v as u16))
            .or_else(|| rustemu_ui::debug::parse_addr(app, &text))
    };
    match parsed {
        Some(a) => show_address(app, m, a),
        None => app.status = format!("bad address '{text}'"),
    }
}

/// Finds the next occurrence of the hex bytes (`B8 05 00`) or the text after the selection.
fn find_next(app: &mut App, m: &mut MemoryView) {
    let t = m.find_text.trim().to_string();
    let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    let pattern: Vec<u8> =
        if !compact.is_empty() && compact.len().is_multiple_of(2) && compact.chars().all(|c| c.is_ascii_hexdigit()) {
            (0..compact.len()).step_by(2).map(|i| u8::from_str_radix(&compact[i..i + 2], 16).unwrap()).collect()
        } else {
            t.chars().map(rustemu_core::font::char_to_cp437).collect()
        };
    if pattern.is_empty() {
        return;
    }
    let mem = app.session.machine.sys.mem.raw();
    let size = mem.len();
    let from = m.selected.map_or(0, |s| s as usize + 1);
    let hit = (0..size)
        .map(|i| (from + i) % size)
        .find(|&i| pattern.iter().enumerate().all(|(k, b)| mem[(i + k) % size] == *b));
    match hit {
        Some(a) => {
            show_address(app, m, a as u32);
            app.status = format!("found at {a:05X}h");
        }
        None => app.status = format!("'{t}' not found"),
    }
}

pub(crate) fn inspector_card(app: &mut App, m: &mut MemoryView, ui: &mut Ui, rect: Rect) {
    let (_, mut b) = card(ui, rect, "BYTE INSPECTOR");
    let Some(a) = m.selected else {
        b.label(RichText::new("Tap a byte in the hex editor.").color(pal::WEAK));
        return;
    };
    let seg = m.seg.value(app);
    let off = a.wrapping_sub((seg as u32) << 4);
    let addr = if off <= 0xFFFF { format!("{seg:04X}:{off:04X}") } else { format!("{a:05X}h") };
    let mem = &mut app.session.machine.sys.mem;
    let v = mem.read8(a);
    let w = mem.read16(a);
    let field_w = (b.available_width() * 0.55).max(110.0);
    egui::Grid::new("t_insp").num_columns(2).spacing([10.0, 8.0]).show(&mut b, |ui| {
        let row = |ui: &mut Ui, k: &str| ui.label(RichText::new(k).monospace().color(pal::TEXT)).id;
        row(ui, "Address:");
        value_box(ui, &addr, None, field_w);
        ui.end_row();
        let label = row(ui, "Hex:");
        let mut x = v as u16;
        if hex_box(ui, &mut x, 2, Some(pal::AMBER), field_w, label).changed() {
            mem.write8(a, x as u8);
        }
        ui.end_row();
        row(ui, "Decimal:");
        let dec = if v >= 0x80 { format!("{v}  ({})", v as i8) } else { v.to_string() };
        value_box(ui, &dec, None, field_w);
        ui.end_row();
        row(ui, "Binary:");
        value_box(ui, &format!("{v:08b}"), None, field_w);
        ui.end_row();
        row(ui, "Word (LE):");
        value_box(ui, &format!("0x{w:04X}"), None, field_w);
        ui.end_row();
        row(ui, "Char:");
        value_box(ui, &format!("'{}'", cp437_to_char(v)), None, field_w);
        ui.end_row();
    });
}

pub(crate) fn segments_card(app: &mut App, m: &mut MemoryView, ui: &mut Ui, rect: Rect) {
    let (_, mut b) = card(ui, rect, "SEGMENTS");
    let prev = app.prev_cpu.sregs;
    let field_w = (b.available_width() * 0.55).max(90.0);
    let mut show = None;
    egui::Grid::new("t_segs").num_columns(2).spacing([10.0, 8.0]).show(&mut b, |ui| {
        for (i, name, seg) in [
            (cpu::CS, "CS", MemSeg::Cs),
            (cpu::DS, "DS", MemSeg::Ds),
            (cpu::ES, "ES", MemSeg::Es),
            (cpu::SS, "SS", MemSeg::Ss),
        ] {
            let r = ui.add(egui::Label::new(RichText::new(name).monospace().color(pal::LINK)).sense(Sense::click()));
            let label = r.id;
            if r.on_hover_text("show in the hex editor").clicked() {
                show = Some(seg);
            }
            let v = &mut app.session.machine.cpu.sregs[i];
            let accent = (*v != prev[i]).then_some(pal::AMBER);
            hex_box(ui, v, 4, accent, field_w, label);
            ui.end_row();
        }
    });
    if let Some(s) = show {
        show_segment(app, m, s);
    }
}

pub(crate) fn stack_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    if rect.height() < HEAD_H + 40.0 {
        return;
    }
    let c = &app.session.machine.cpu;
    let (ss, sp) = (c.sregs[cpu::SS], c.regs[cpu::SP]);
    let (_, mut b) = card(ui, rect, &format!("STACK (SS:{sp:04X})"));
    let field_w = (b.available_width() * 0.55).max(90.0);
    let rows = (((b.available_height() + 8.0) / 36.0).floor() as u16).max(1);
    egui::Grid::new("t_stack").num_columns(2).spacing([10.0, 8.0]).show(&mut b, |ui| {
        for i in 0..rows {
            let off = sp.wrapping_add(i * 2);
            let v = app.session.machine.sys.mem.read16(linear(ss, off));
            let name = if i == 0 { "[SP]:".to_string() } else { format!("SP + {:02X}:", i * 2) };
            ui.label(RichText::new(name).monospace().color(pal::TEXT));
            value_box(ui, &format!("{v:04X}"), (i == 0).then_some(pal::AMBER), field_w);
            ui.end_row();
        }
    });
}

/// The 1 MB address space with the program, its segments, video memory and the BIOS.
pub(crate) fn memory_map_card(app: &mut App, m: &mut MemoryView, ui: &mut Ui, rect: Rect) {
    let (_, mut b) = card(ui, rect, "MEMORY MAP  (1 MB PHYSICAL ADDRESS SPACE)");
    let width = b.available_width();
    let (bar, resp) = b.allocate_exact_size(vec2(width, 20.0), Sense::click());
    let (labels, _) = b.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let p = b.painter();
    p.rect(bar, 3.0, pal::FIELD, Stroke::new(1.0, pal::BORDER), StrokeKind::Inside);
    let x = |a: u32| bar.left() + a as f32 / 0x100000 as f32 * bar.width();
    let c = &app.session.machine.cpu;
    let mut regions: Vec<(u32, u32, &str, Color32)> = vec![
        (0x00000, 0x00500, "IVT/BDA", Color32::from_rgb(0x55, 0x66, 0x88)),
        (0xA0000, 0xC0000, "Video", Color32::from_rgb(0x8A, 0x5C, 0xD6)),
        (0xF0000, 0x100000, "BIOS", Color32::from_rgb(0x55, 0x66, 0x88)),
    ];
    if let Some(l) = &app.session.machine.loaded {
        regions.push((l.image_base, l.image_base + l.image_len.max(16), "Code", pal::BLUE));
    }
    let ds = (c.sregs[cpu::DS] as u32) << 4;
    if c.sregs[cpu::DS] != c.sregs[cpu::CS] {
        regions.push((ds, (ds + 0x10000).min(0x100000), "Data", pal::TEAL));
    }
    let ss = (c.sregs[cpu::SS] as u32) << 4;
    regions.push((
        ss + c.regs[cpu::SP] as u32,
        (ss + 0x10000).min(0x100000),
        "Stack",
        Color32::from_rgb(0xD9, 0x8E, 0x1F),
    ));
    regions.sort_by_key(|r| r.0);
    let mut label_x = labels.left();
    for (s, e, name, color) in &regions {
        let r = Rect::from_min_max(pos2(x(*s), bar.top() + 2.0), pos2(x(*e).max(x(*s) + 4.0), bar.bottom() - 2.0));
        p.rect_filled(r, 2.0, *color);
        let text = format!("{name} {s:05X}h");
        let tw = text.len() as f32 * 7.2;
        let cx = (r.center().x - tw / 2.0).max(label_x).min(labels.right() - tw);
        p.text(pos2(cx, labels.top() + 2.0), Align2::LEFT_TOP, name.to_string(), FontId::proportional(13.0), *color);
        p.text(
            pos2(cx, labels.top() + 16.0),
            Align2::LEFT_TOP,
            format!("{s:05X}h"),
            FontId::monospace(11.5),
            pal::WEAK,
        );
        label_x = cx + tw + 8.0;
    }
    // The part shown in the hex editor.
    let shown = linear(m.seg.value(app), m.mem_off);
    p.rect_stroke(
        Rect::from_min_size(pos2(x(shown) - 1.0, bar.top()), vec2(3.0, bar.height())),
        0.0,
        Stroke::new(2.0, Color32::WHITE),
        StrokeKind::Middle,
    );
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let a = (((pos.x - bar.left()) / bar.width()).clamp(0.0, 1.0) * 0xFFFFF as f32) as u32 & !0xF;
        show_address(app, m, a);
    }
    resp.on_hover_text("tap: show this address in the hex editor");
}
