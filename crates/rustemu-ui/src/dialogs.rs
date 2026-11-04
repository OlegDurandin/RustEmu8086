//! Dialogs of both front ends: choose code template (New), options, command line parameters,
//! unsaved changes and notices. Unlike the tools, they are not remembered between runs.

use egui::{RichText, Ui};

use crate::app::{App, PendingAction, ThemeChoice};
use crate::tools::Tool;

/// The dialogs that are open.
pub fn dialogs(app: &mut App, ctx: &egui::Context) {
    crate::find::goto_dialog(app, ctx);
    new_dialog(app, ctx);
    options_window(app, ctx);
    crate::drive_ui::bin_to_floppy_dialog(app, ctx);
    small_dialogs(app, ctx);
    crate::programs::save_name_dialog(app, ctx);
    crate::share::share_dialog(app, ctx);
    crate::examples_ui::more_examples_window(app, ctx);
}

const AUTO_SYNTAX: &str = "Detected from the source (NASM for bits 16, section, times, resb, %define, \
     byte [..]...); a program that does not assemble that way is tried in the other syntax";

/// The Options menu: the most used settings and the options window with all of them.
pub fn options_menu(app: &mut App, ui: &mut Ui) {
    use crate::app::SyntaxChoice;
    use crate::workspace::Layout;
    if !app.front.tablet_layout {
        ui.label(RichText::new("windows").weak());
        ui.radio_value(&mut app.settings.layout, Layout::Classic, "classic windows (Emu8086)");
        ui.radio_value(&mut app.settings.layout, Layout::Panels, "panels");
        ui.separator();
    }
    // The MIPS assembler has one syntax, MARS's, and MARS's settings.
    if app.arch() == rustemu_session::Arch::I8086 {
        ui.label(RichText::new("assembler syntax").weak());
        ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Auto, "auto").on_hover_text(AUTO_SYNTAX);
        ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Masm, "Emu8086 (MASM)");
        ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Nasm, "NASM");
    } else {
        egui::containers::menu::SubMenuButton::new("MARS settings")
            .config(crate::mars_settings::menu_config())
            .ui(ui, |ui| crate::mars_settings::settings_ui(app, ui));
    }
    // The tablet layout has a theme of its own.
    if app.front.fixed_theme.is_none() {
        ui.separator();
        ui.label(RichText::new("theme").weak());
        let mut changed = false;
        for (t, name) in [(ThemeChoice::Light, "light"), (ThemeChoice::Dark, "dark"), (ThemeChoice::System, "system")] {
            changed |= ui.radio_value(&mut app.settings.theme, t, name).changed();
        }
        if changed {
            app.apply_theme(ui.ctx());
        }
    }
    ui.separator();
    if ui.button("all options…").clicked() {
        app.dialogs.options = true;
        ui.close();
    }
}

/// "choose code template" (File -> New), like Emu8086.
fn new_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut choice) = app.dialogs.new_file else { return };
    let mut close = false;
    egui::Window::new("choose code template")
        .collapsible(false)
        .resizable(false)
        .min_width(640.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.spacing_mut().item_spacing.y = 10.0;
            const OPTIONS: [&str; 4] = [
                "COM template - simple and tiny executable file format, pure machine code",
                "EXE template - advanced executable file. header: relocation, checksum",
                "BIN template - pure binary file, allows all sorts of customizations (advanced)",
                "BOOT template - for creating floppy disk boot records (very advanced)",
            ];
            for (i, text) in OPTIONS.iter().enumerate() {
                ui.radio_value(&mut choice, i, *text);
            }
            ui.horizontal(|ui| {
                ui.add_space(120.0);
                ui.radio_value(&mut choice, 4, "empty workspace");
                ui.add_space(40.0);
                ui.radio_value(&mut choice, 5, "the emulator");
            });
            ui.checkbox(&mut app.dialogs.new_nasm, "use NASM syntax (bits 16, section, times, %macro)").on_hover_text(
                "COM, BIN and boot sector templates in NASM syntax; EXE needs a linker and is not available",
            );
            if app.dialogs.new_nasm && choice == 1 {
                choice = 0;
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_sized([120.0, 28.0], egui::Button::new(RichText::new("OK").strong())).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    match choice {
                        0..=4 if app.dialogs.new_nasm => app.new_from_nasm_template(choice),
                        0..=3 => app.new_from_template(choice),
                        4 => app.new_from_template(usize::MAX),
                        _ => {
                            app.windows[Tool::Emulator] = true;
                            app.windows[Tool::Screen] = true;
                        }
                    }
                    close = true;
                }
                if ui.add_sized([120.0, 28.0], egui::Button::new("Cancel")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    close = true;
                }
            });
        });
    app.dialogs.new_file = if close { None } else { Some(choice) };
}

fn options_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.dialogs.options;
    egui::Window::new("options")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(egui::pos2(760.0, 110.0))
        .show(ctx, |ui| {
            egui::Grid::new("options").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                if app.front.can_switch_layout {
                    ui.label("layout");
                    ui.horizontal(|ui| {
                        let mut tablet = app.front.tablet_layout;
                        ui.radio_value(&mut tablet, false, "desktop (Emu8086)");
                        ui.radio_value(&mut tablet, true, "tablet (touch)");
                        if tablet != app.front.tablet_layout {
                            app.front.switch_layout = true;
                        }
                    });
                    ui.end_row();
                }
                if !app.front.tablet_layout {
                    ui.label("windows");
                    crate::workspace::layout_options(app, ui);
                    ui.end_row();
                }
                // The tablet layout has a theme of its own.
                if app.front.fixed_theme.is_none() {
                    ui.label("theme");
                    ui.horizontal(|ui| {
                        let mut changed = false;
                        for (t, name) in [
                            (ThemeChoice::Light, "light"),
                            (ThemeChoice::Dark, "dark"),
                            (ThemeChoice::System, "system"),
                        ] {
                            changed |= ui.radio_value(&mut app.settings.theme, t, name).changed();
                        }
                        if changed {
                            app.apply_theme(ui.ctx());
                        }
                    });
                    ui.end_row();
                }
                ui.label("assembler syntax");
                ui.horizontal(|ui| {
                    use crate::app::SyntaxChoice;
                    if app.arch() != rustemu_session::Arch::I8086 {
                        ui.label("MARS (MIPS)");
                        return;
                    }
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Auto, "auto").on_hover_text(AUTO_SYNTAX);
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Masm, "Emu8086 (MASM)");
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Nasm, "NASM");
                });
                ui.end_row();
                if app.arch() != rustemu_session::Arch::I8086 {
                    ui.label("MARS settings");
                    ui.vertical(|ui| crate::mars_settings::settings_ui(app, ui));
                    ui.end_row();
                }
                ui.label("editor font size");
                ui.add(egui::Slider::new(&mut app.settings.font_size, 10.0..=24.0).text("pt"));
                ui.end_row();
                ui.label("step delay");
                ui.add(egui::Slider::new(&mut app.settings.step_delay_ms, 0..=1000).suffix(" ms").text("run speed"));
                ui.end_row();
                ui.label("debugging");
                ui.vertical(|ui| {
                    ui.checkbox(&mut app.settings.show_macro_code, "step into macros (show emu8086.inc code)");
                    ui.checkbox(&mut app.settings.follow_source, "source window follows execution");
                    if ui.checkbox(&mut app.settings.timer_irq, "timer IRQ (INT 08h / 1Ch)").changed()
                        && let Some(m) = app.session.x86_mut()
                    {
                        m.timer_irq = app.settings.timer_irq;
                    }
                });
                ui.end_row();
                // The browser's drive C: is in memory.
                if !app.front.browser {
                    ui.label("drive C:");
                    ui.vertical(|ui| {
                        let own = app.drive_dir.display().to_string();
                        ui.radio_value(&mut app.settings.drive_c_source_folder, false, "a folder of its own")
                            .on_hover_text(own);
                        ui.radio_value(&mut app.settings.drive_c_source_folder, true, "the folder of the program")
                            .on_hover_text("Programs can change and delete the files next to the source");
                        ui.weak("Takes effect when a program is loaded.");
                        if ui.button("show drive C:").clicked() {
                            crate::drive_ui::open_drive_folder(app);
                        }
                    });
                    ui.end_row();
                }
            });
        });
    app.dialogs.options = open;
}

/// Command line parameters, unsaved changes, notices.
fn small_dialogs(app: &mut App, ctx: &egui::Context) {
    if let Some(mut text) = app.dialogs.cmdline.take() {
        let mut keep = true;
        egui::Window::new("Command line parameters")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Parameters passed to the program (PSP:80h), e.g.  p1 p2 p3");
                let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("parameters").desired_width(320.0));
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        app.session.command_line = text.trim().to_string();
                        let t = app.session.command_line.clone();
                        if let Some(m) = app.session.x86_mut() {
                            m.set_command_line(&t);
                        }
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if keep {
            app.dialogs.cmdline = Some(text);
        }
    }
    if let Some(action) = app.dialogs.confirm_discard.clone() {
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Save changes to {}?", app.file_name()));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if app.save() {
                            app.dialogs.confirm_discard = None;
                            finish(app, ctx, action.clone());
                        } else if let Some(d) = &mut app.dialogs.save_name {
                            // A new program in the browser: named first, then the action.
                            d.then = Some(action.clone());
                            app.dialogs.confirm_discard = None;
                        }
                    }
                    if ui.button("Discard").clicked() {
                        app.dialogs.confirm_discard = None;
                        app.doc.saved_text = app.doc.text.clone();
                        finish(app, ctx, action.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        app.dialogs.confirm_discard = None;
                        app.dialogs.open_in = None;
                    }
                });
            });
    }
    if let Some(open_as) = app.dialogs.open_as.take() {
        let mut keep = true;
        egui::Window::new("Which processor?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "This program looks like {} code, and the {} workspace is open.\nOpen it as:",
                    open_as.detected.title(),
                    app.arch().title()
                ));
                ui.horizontal(|ui| {
                    for arch in [open_as.detected, app.arch()] {
                        if ui.button(arch.title()).clicked() {
                            keep = false;
                            app.dialogs.open_in = Some(arch);
                            app.perform(open_as.action.clone());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if keep && app.dialogs.open_as.is_none() {
            app.dialogs.open_as = Some(open_as);
        }
    }
    if let Some((title, text)) = app.dialogs.notice.clone() {
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(text);
                if ui.button("OK").clicked() {
                    app.dialogs.notice = None;
                }
            });
    }
}

fn finish(app: &mut App, ctx: &egui::Context, action: PendingAction) {
    match action {
        PendingAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        a => app.perform(a),
    }
}
