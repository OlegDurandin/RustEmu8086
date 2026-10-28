//! Painted widgets of the tablet layout: icons, buttons, value boxes, cards and layout helpers.

use crate::theme::pal;
use egui::{
    Align, Align2, Color32, CornerRadius, DragValue, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind,
    TextStyle, Ui, UiBuilder, pos2, vec2,
};

pub(crate) const GAP: f32 = 10.0;
pub(crate) const HEAD_H: f32 = 40.0;
pub(crate) const ROW_H: f32 = 26.0;

// ---- widgets -----------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(crate) enum Icon {
    Play,
    Pause,
    Step,
    Reset,
    Chip,
    File,
    Trash,
    Plus,
    Up,
    Down,
    Menu,
    Keyboard,
    Share,
}

pub(crate) fn paint_icon(p: &egui::Painter, icon: Icon, c: egui::Pos2, s: f32, color: Color32) {
    let st = Stroke::new(2.0, color);
    let tri = |pts: Vec<egui::Pos2>| egui::Shape::convex_polygon(pts, color, Stroke::NONE);
    match icon {
        Icon::Play => {
            p.add(tri(vec![c + vec2(-s * 0.35, -s * 0.45), c + vec2(s * 0.45, 0.0), c + vec2(-s * 0.35, s * 0.45)]));
        }
        Icon::Pause => {
            for dx in [-0.25, 0.25] {
                p.rect_filled(Rect::from_center_size(c + vec2(dx * s, 0.0), vec2(s * 0.22, s * 0.85)), 1.0, color);
            }
        }
        Icon::Step => {
            p.add(tri(vec![c + vec2(-s * 0.4, -s * 0.42), c + vec2(s * 0.2, 0.0), c + vec2(-s * 0.4, s * 0.42)]));
            p.rect_filled(Rect::from_center_size(c + vec2(s * 0.33, 0.0), vec2(s * 0.16, s * 0.84)), 1.0, color);
        }
        Icon::Reset => {
            let r = s * 0.38;
            let pts: Vec<egui::Pos2> = (0..=24)
                .map(|i| {
                    let a = -0.9 + i as f32 / 24.0 * 5.2;
                    c + vec2(a.cos() * r, a.sin() * r)
                })
                .collect();
            let tip = pts[0];
            p.add(egui::Shape::line(pts, st));
            p.add(tri(vec![
                tip + vec2(-s * 0.22, -s * 0.12),
                tip + vec2(s * 0.12, -s * 0.18),
                tip + vec2(0.0, s * 0.16),
            ]));
        }
        Icon::Chip => {
            let body = Rect::from_center_size(c, vec2(s * 0.62, s * 0.62));
            p.rect_stroke(body, 3.0, st, StrokeKind::Middle);
            p.rect_stroke(body.shrink(s * 0.16), 1.0, Stroke::new(1.5, color), StrokeKind::Middle);
            for i in 0..4 {
                let t = -0.21 + i as f32 * 0.14;
                for (a, b) in [
                    (vec2(t, -0.31), vec2(t, -0.45)),
                    (vec2(t, 0.31), vec2(t, 0.45)),
                    (vec2(-0.31, t), vec2(-0.45, t)),
                    (vec2(0.31, t), vec2(0.45, t)),
                ] {
                    p.line_segment([c + a * s, c + b * s], Stroke::new(1.5, color));
                }
            }
        }
        Icon::File => {
            let r = Rect::from_center_size(c, vec2(s * 0.6, s * 0.78));
            let fold = s * 0.2;
            p.add(egui::Shape::closed_line(
                vec![
                    r.left_top(),
                    r.right_top() - vec2(fold, 0.0),
                    r.right_top() + vec2(0.0, fold),
                    r.right_bottom(),
                    r.left_bottom(),
                ],
                st,
            ));
            for i in 0..3 {
                let y = r.top() + s * (0.32 + i as f32 * 0.14);
                p.line_segment([pos2(r.left() + s * 0.12, y), pos2(r.right() - s * 0.12, y)], Stroke::new(1.5, color));
            }
        }
        Icon::Trash => {
            let r = Rect::from_center_size(c + vec2(0.0, s * 0.08), vec2(s * 0.5, s * 0.6));
            p.rect_stroke(r, 2.0, st, StrokeKind::Middle);
            p.line_segment([c + vec2(-s * 0.36, -s * 0.28), c + vec2(s * 0.36, -s * 0.28)], st);
            p.line_segment([c + vec2(-s * 0.1, -s * 0.38), c + vec2(s * 0.1, -s * 0.38)], st);
        }
        Icon::Plus => {
            p.line_segment([c + vec2(-s * 0.35, 0.0), c + vec2(s * 0.35, 0.0)], st);
            p.line_segment([c + vec2(0.0, -s * 0.35), c + vec2(0.0, s * 0.35)], st);
        }
        Icon::Up | Icon::Down => {
            let k = if matches!(icon, Icon::Up) { 1.0 } else { -1.0 };
            p.add(egui::Shape::line(
                vec![c + vec2(-s * 0.3, s * 0.15 * k), c + vec2(0.0, -s * 0.15 * k), c + vec2(s * 0.3, s * 0.15 * k)],
                st,
            ));
        }
        Icon::Menu => {
            for dy in [-0.28, 0.0, 0.28] {
                p.circle_filled(c + vec2(0.0, dy * s), s * 0.08, color);
            }
        }
        Icon::Share => {
            // An open box with an arrow out of it.
            let (w, top, bottom) = (s * 0.3, -s * 0.05, s * 0.4);
            p.add(egui::Shape::line(
                vec![
                    c + vec2(-w * 0.45, top),
                    c + vec2(-w, top),
                    c + vec2(-w, bottom),
                    c + vec2(w, bottom),
                    c + vec2(w, top),
                    c + vec2(w * 0.45, top),
                ],
                Stroke::new(1.5, color),
            ));
            p.line_segment([c + vec2(0.0, s * 0.18), c + vec2(0.0, -s * 0.42)], Stroke::new(1.5, color));
            p.add(egui::Shape::line(
                vec![c + vec2(-s * 0.15, -s * 0.27), c + vec2(0.0, -s * 0.42), c + vec2(s * 0.15, -s * 0.27)],
                Stroke::new(1.5, color),
            ));
        }
        Icon::Keyboard => {
            let body = Rect::from_center_size(c, vec2(s * 0.9, s * 0.6));
            p.rect_stroke(body, 3.0, Stroke::new(1.5, color), StrokeKind::Middle);
            for row in 0..2 {
                for col in 0..4 {
                    let k =
                        pos2(body.left() + s * (0.17 + col as f32 * 0.19), body.top() + s * (0.15 + row as f32 * 0.15));
                    p.rect_filled(Rect::from_center_size(k, vec2(s * 0.09, s * 0.07)), 0.5, color);
                }
            }
            p.line_segment(
                [
                    pos2(body.left() + s * 0.25, body.bottom() - s * 0.13),
                    pos2(body.right() - s * 0.25, body.bottom() - s * 0.13),
                ],
                Stroke::new(1.5, color),
            );
        }
    }
}

/// A button with an icon and an optional label; `fill` for the main action.
pub(crate) fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    fill: Option<Color32>,
    enabled: bool,
) -> egui::Response {
    let font = FontId::proportional(17.0);
    let text_w = if label.is_empty() {
        0.0
    } else {
        ui.fonts_mut(|f| f.layout_no_wrap(label.into(), font.clone(), pal::TEXT).size().x)
    };
    let h = 44.0;
    let w = if label.is_empty() { h } else { 22.0 + 22.0 + 10.0 + text_w + 22.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), if enabled { Sense::click() } else { Sense::hover() });
    let base = fill.unwrap_or(pal::BUTTON);
    let bg = if !enabled {
        base.gamma_multiply(0.5)
    } else if resp.is_pointer_button_down_on() {
        base.gamma_multiply(0.75)
    } else if resp.hovered() {
        base.gamma_multiply(1.2)
    } else {
        base
    };
    let fg = if enabled { Color32::WHITE } else { pal::WEAK };
    let name = if label.is_empty() { format!("{icon:?}") } else { label.to_string() };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
    let p = ui.painter();
    p.rect_filled(rect, 6.0, bg);
    if label.is_empty() {
        paint_icon(p, icon, rect.center(), 22.0, fg);
    } else {
        paint_icon(p, icon, pos2(rect.left() + 33.0, rect.center().y), 22.0, fg);
        p.text(pos2(rect.left() + 54.0, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
    }
    resp
}

/// A small icon button in a card header.
pub(crate) fn small_icon(ui: &mut Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 30.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    let p = ui.painter();
    p.rect(
        rect,
        4.0,
        if resp.hovered() { pal::BORDER } else { pal::BUTTON },
        Stroke::new(1.0, pal::BORDER),
        StrokeKind::Inside,
    );
    paint_icon(p, icon, rect.center(), 18.0, pal::TEXT);
    resp.on_hover_text(tip)
}

pub(crate) fn badge(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, pal::BORDER))
        .fill(pal::FIELD)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| ui.label(RichText::new(text).monospace().color(pal::TEXT)));
}

/// An on/off switch.
pub(crate) fn toggle_switch(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let p = ui.painter();
    let bg = if *on { pal::BLUE } else { pal::BUTTON };
    p.rect(rect, 12.0, bg, Stroke::new(1.0, pal::BORDER), StrokeKind::Inside);
    let x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), t);
    p.circle_filled(pos2(x, rect.center().y), 9.0, Color32::WHITE);
    resp
}

/// Field look for value boxes; `accent` marks a changed (amber) or set (teal) value.
pub(crate) fn field_style(ui: &mut Ui, accent: Option<Color32>) {
    let v = ui.visuals_mut();
    let fill = accent.map_or(pal::FIELD, |c| c.gamma_multiply(0.16));
    let stroke = Stroke::new(1.0, accent.unwrap_or(pal::BORDER));
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.weak_bg_fill = fill;
        w.bg_fill = fill;
        w.bg_stroke = stroke;
        w.fg_stroke.color = accent.unwrap_or(pal::TEXT);
        w.corner_radius = CornerRadius::same(4);
        w.expansion = 0.0;
    }
    ui.style_mut().override_text_style = Some(TextStyle::Monospace);
}

/// A hexadecimal value box (`digits` 2 or 4) named by the label `label`.
pub(crate) fn hex_box(
    ui: &mut Ui,
    v: &mut u16,
    digits: usize,
    accent: Option<Color32>,
    width: f32,
    label: egui::Id,
) -> egui::Response {
    ui.scope(|ui| {
        field_style(ui, accent);
        let max = if digits == 2 { 0xFF } else { 0xFFFF };
        ui.add_sized([width, 28.0], DragValue::new(v).hexadecimal(digits, false, true).range(0..=max).speed(0.0))
            .labelled_by(label)
    })
    .inner
}

/// A read-only value box.
pub(crate) fn value_box(ui: &mut Ui, text: &str, accent: Option<Color32>, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let fill = accent.map_or(pal::FIELD, |c| c.gamma_multiply(0.16));
    ui.painter().rect(rect, 4.0, fill, Stroke::new(1.0, accent.unwrap_or(pal::BORDER)), StrokeKind::Inside);
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::monospace(14.5),
        accent.unwrap_or(pal::TEXT),
    );
}

/// Draws a card in `rect` and returns the user interfaces of its title bar (right to left, after
/// the title) and of its body.
pub(crate) fn card(ui: &mut Ui, rect: Rect, title: &str) -> (Ui, Ui) {
    let p = ui.painter();
    p.rect(rect, 6.0, pal::CARD, Stroke::new(1.0, pal::BORDER), StrokeKind::Inside);
    let head = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + HEAD_H));
    p.rect_filled(head.shrink(1.0), CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, pal::HEADER);
    p.line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, pal::BORDER));
    p.text(
        pos2(head.left() + 14.0, head.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(16.0),
        pal::TEXT,
    );
    let title_w = ui.fonts_mut(|f| f.layout_no_wrap(title.into(), FontId::proportional(16.0), pal::TEXT).size().x);
    let head_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("card_head", title))
            .max_rect(Rect::from_min_max(
                pos2(head.left() + title_w + 28.0, head.top() + 4.0),
                pos2(head.right() - 6.0, head.bottom() - 4.0),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    body_ui(ui, Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max), title, head_ui)
}

/// A card without a title bar.
pub(crate) fn plain_card(ui: &mut Ui, rect: Rect, salt: &str) -> Ui {
    ui.painter().rect(rect, 6.0, pal::CARD, Stroke::new(1.0, pal::BORDER), StrokeKind::Inside);
    let inner = rect.shrink(6.0);
    let mut b = ui
        .new_child(UiBuilder::new().id_salt(("card_body", salt)).max_rect(inner).layout(Layout::top_down(Align::Min)));
    b.set_clip_rect(inner.intersect(ui.clip_rect()));
    b
}

fn body_ui(ui: &mut Ui, rect: Rect, salt: &str, head: Ui) -> (Ui, Ui) {
    let inner = rect.shrink2(vec2(12.0, 8.0));
    let mut b = ui
        .new_child(UiBuilder::new().id_salt(("card_body", salt)).max_rect(inner).layout(Layout::top_down(Align::Min)));
    b.set_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
    (head, b)
}

/// Splits `r` at `x` (left part `x` wide) with a gap.
pub(crate) fn split_x(r: Rect, x: f32) -> (Rect, Rect) {
    (
        Rect::from_min_max(r.min, pos2(r.left() + x, r.bottom())),
        Rect::from_min_max(pos2(r.left() + x + GAP, r.top()), r.max),
    )
}

pub(crate) fn split_y(r: Rect, y: f32) -> (Rect, Rect) {
    (
        Rect::from_min_max(r.min, pos2(r.right(), r.top() + y)),
        Rect::from_min_max(pos2(r.left(), r.top() + y + GAP), r.max),
    )
}

pub(crate) fn table_header(ui: &mut Ui, cols: &[(f32, &str)]) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    for (x, t) in cols {
        ui.painter().text(
            pos2(r.left() + x, r.center().y),
            Align2::LEFT_CENTER,
            *t,
            FontId::proportional(14.0),
            pal::WEAK,
        );
    }
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, pal::BORDER));
}
