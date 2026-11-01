//! The Examples menu, as in Emu8086 (plus NASM), and the "more examples..." window.

use egui::{RichText, Ui};

use crate::app::{App, PendingAction};

/// The Examples menu, as in Emu8086 (plus NASM): from the emulator (`emulate`) the example is
/// assembled and loaded at once.
pub fn examples_menu(app: &mut App, ui: &mut Ui, emulate: bool) {
    for &(caption, file) in crate::examples::MENU {
        if file == "hello_nasm.asm" {
            ui.separator();
        }
        if ui.button(caption).on_hover_text(file).clicked() {
            if let Some(i) = crate::examples::index(file) {
                open_example(app, i, emulate);
            }
            ui.close();
        }
    }
    ui.separator();
    if ui.button("more examples...").clicked() {
        app.dialogs.more_examples = Some(emulate);
        ui.close();
    }
}

fn open_example(app: &mut App, i: usize, emulate: bool) {
    if emulate {
        app.emulate_example(i);
    } else {
        app.guarded(PendingAction::Example(i));
    }
}

/// "more examples...": every example file (Emu8086 opens its examples folder).
pub(crate) fn more_examples_window(app: &mut App, ctx: &egui::Context) {
    let Some(emulate) = app.dialogs.more_examples else { return };
    let mut open = true;
    let mut chosen = None;
    egui::Window::new("more examples")
        .open(&mut open)
        .default_pos(egui::pos2(160.0, 100.0))
        .default_size([620.0, 480.0])
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.dialogs.examples_filter)
                    .hint_text("filter: name or description")
                    .desired_width(f32::INFINITY),
            );
            let filter = app.dialogs.examples_filter.to_lowercase();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for (i, (name, src)) in crate::examples::EXAMPLES.iter().enumerate() {
                    // The name and what the example does (whole in the tooltip); the filter looks
                    // at both.
                    let about = crate::examples::description(name, src);
                    if !name.to_lowercase().contains(&filter) && !about.to_lowercase().contains(&filter) {
                        continue;
                    }
                    let r = ui
                        .horizontal(|ui| {
                            let name = ui.add(
                                egui::Label::new(RichText::new(*name).monospace().strong()).sense(egui::Sense::click()),
                            );
                            let text = ui.add(
                                egui::Label::new(RichText::new(&about).weak()).truncate().sense(egui::Sense::click()),
                            );
                            name.union(text)
                        })
                        .inner
                        .on_hover_text(&about);
                    if r.clicked() {
                        chosen = Some(i);
                    }
                }
            });
        });
    if let Some(i) = chosen {
        app.dialogs.more_examples = None;
        open_example(app, i, emulate);
    } else if !open {
        app.dialogs.more_examples = None;
    }
}
