//! Tool windows and dialogs shared by the desktop and tablet front ends: new file, options, virtual
//! drive, command line, unsaved changes, notices, ASCII codes, base converter, about.

use egui::{RichText, Ui};

use crate::app::{App, PendingAction, ThemeChoice};

/// Tool windows and dialogs of both layouts (desktop and tablet).
pub fn shared_windows(app: &mut App, ctx: &egui::Context) {
    let ctx = ctx.clone();
    crate::debug::debug_windows(app, &ctx);
    crate::debugger::debugger_windows(app, &ctx);
    crate::devices_ui::devices_window(app, &ctx);
    crate::drive_ui::drive_window(app, &ctx);
    tools(app, &ctx);
    crate::find::goto_dialog(app, &ctx);
    crate::console::console_window(app, &ctx);
    crate::library_ui::library_window(app, &ctx);
    crate::reference_ui::reference_window(&mut app.reference, &ctx);
    crate::calc::calculator_window(&mut app.calculator, &ctx);
    new_dialog(app, &ctx);
    options_window(app, &ctx);
    dialogs(app, &ctx);
}

/// "choose code template" (File -> New), like Emu8086.
fn new_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut choice) = app.new_dialog else { return };
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
            ui.checkbox(&mut app.new_nasm, "use NASM syntax (bits 16, section, times, %macro)").on_hover_text(
                "COM, BIN and boot sector templates in NASM syntax; EXE needs a linker and is not available",
            );
            if app.new_nasm && choice == 1 {
                choice = 0;
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_sized([120.0, 28.0], egui::Button::new(RichText::new("OK").strong())).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    match choice {
                        0..=4 if app.new_nasm => app.new_from_nasm_template(choice),
                        0..=3 => app.new_from_template(choice),
                        4 => app.new_from_template(usize::MAX),
                        _ => {
                            app.windows.emulator = true;
                            app.windows.screen = true;
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
    app.new_dialog = if close { None } else { Some(choice) };
}

fn options_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.options_open;
    egui::Window::new("options")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(egui::pos2(760.0, 110.0))
        .show(ctx, |ui| {
            egui::Grid::new("options").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                if app.can_switch_layout {
                    ui.label("layout");
                    ui.horizontal(|ui| {
                        let mut tablet = app.tablet_layout;
                        ui.radio_value(&mut tablet, false, "desktop (Emu8086)");
                        ui.radio_value(&mut tablet, true, "tablet (touch)");
                        if tablet != app.tablet_layout {
                            app.switch_layout = true;
                        }
                    });
                    ui.end_row();
                }
                // The tablet layout has a theme of its own.
                if app.fixed_theme.is_none() {
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
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Auto, "auto").on_hover_text(
                        "NASM when the source uses bits 16, section, times, resb, %define, byte [..]...",
                    );
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Masm, "Emu8086 (MASM)");
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Nasm, "NASM");
                });
                ui.end_row();
                ui.label("editor font size");
                ui.add(egui::Slider::new(&mut app.settings.font_size, 10.0..=24.0).text("pt"));
                ui.end_row();
                ui.label("emulator screen");
                ui.add(egui::Slider::new(&mut app.screen.font_size, 10.0..=22.0).text("zoom"));
                ui.end_row();
                ui.label("step delay");
                ui.add(egui::Slider::new(&mut app.settings.step_delay_ms, 0..=1000).suffix(" ms").text("run speed"));
                ui.end_row();
                ui.label("debugging");
                ui.vertical(|ui| {
                    ui.checkbox(&mut app.show_macro_code, "step into macros (show emu8086.inc code)");
                    ui.checkbox(&mut app.follow_source, "source window follows execution");
                    if ui.checkbox(&mut app.settings.timer_irq, "timer IRQ (INT 08h / 1Ch)").changed() {
                        app.session.machine.timer_irq = app.settings.timer_irq;
                    }
                });
                ui.end_row();
            });
        });
    app.options_open = open;
}

/// Virtual drive menu (main Emulator menu and the emulator window), as in Emu8086. There is one
/// drive, FLOPPY_0, kept in memory; images are loaded and saved explicitly.
pub fn virtual_drive_menu(app: &mut App, ui: &mut Ui) {
    use rustemu_core::machine::{FLOPPY_SIZE, MAX_FLOPPIES};
    ui.set_min_width(330.0);
    if app.session.machine.dos_memory_drive().is_some() {
        if ui.button("drive C: files…").on_hover_text("Files DOS programs read and write (INT 21h)").clicked() {
            app.windows.drive = true;
            ui.close();
        }
        ui.separator();
    }
    let drives = app.session.machine.floppies.len();
    ui.menu_button("boot from virtual floppy", |ui| {
        for d in 0..drives {
            if ui.button(format!("FLOPPY_{d}")).clicked() {
                match app.session.boot_from_floppy(d) {
                    Ok(()) => app.after_binary_load(),
                    Err(e) => app.notice("Virtual drive", &e),
                }
            }
        }
    });
    ui.menu_button("write 512 bytes at 0000:7c00 to boot sector", |ui| {
        for d in 0..drives {
            if ui.button(format!("FLOPPY_{d}")).clicked() {
                match app.session.write_boot_sector(d) {
                    Ok(()) => app.status = format!("Boot sector of FLOPPY_{d} written"),
                    Err(e) => app.notice("Virtual drive", &e),
                }
            }
        }
    });
    if ui.button("write '.bin' file to floppy…").clicked() {
        app.bin_to_floppy = Some(crate::app::BinToFloppy { path: String::new(), drive: 0, cyl: 0, head: 0, sector: 2 });
    }
    ui.separator();
    if ui
        .add_enabled(drives < MAX_FLOPPIES, egui::Button::new("create new floppy drive"))
        .on_hover_text("Adds FLOPPY_1..FLOPPY_3 with an empty 1.44 MB disk (INT 13h DL = 1..3, INT 11h counts them)")
        .on_disabled_hover_text("Emu8086 supports up to four floppy drives")
        .clicked()
        && let Some(d) = app.session.machine.add_floppy()
    {
        app.status = format!("FLOPPY_{d} created: an empty 1.44 MB disk, INT 13h drive DL = {d}");
    }
    ui.separator();
    ui.menu_button("load floppy image", |ui| {
        for d in 0..drives {
            if ui.button(format!("into FLOPPY_{d}…")).clicked()
                && let Some(p) = crate::platform::pick_file(&[], None)
            {
                match std::fs::read(&p) {
                    Ok(mut b) => {
                        b.resize(b.len().max(FLOPPY_SIZE), 0);
                        app.session.machine.floppies[d] = b;
                        app.status = format!("FLOPPY_{d} loaded from {}", p.display());
                    }
                    Err(e) => app.notice("Virtual drive", &e.to_string()),
                }
            }
        }
    });
    ui.menu_button("save floppy image", |ui| {
        for d in 0..drives {
            if ui.button(format!("FLOPPY_{d}…")).clicked()
                && let Some(p) = crate::platform::save_file(&[], None, &format!("FLOPPY_{d}"))
                && let Err(e) = std::fs::write(&p, &app.session.machine.floppies[d])
            {
                app.notice("Virtual drive", &e.to_string());
            }
        }
    });
}

/// "write '.bin' file to floppy": file, then cylinder / head / sector to start at.
fn bin_to_floppy_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.bin_to_floppy.take() else { return };
    let mut keep = true;
    let mut write = false;
    egui::Window::new("write '.bin' file to floppy")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut d.path).hint_text("file.bin").desired_width(300.0));
                if ui.button("browse…").clicked()
                    && let Some(p) = crate::platform::pick_file(&[("binary", &["bin", "com", "img"])], None)
                {
                    d.path = p.display().to_string();
                }
            });
            ui.horizontal(|ui| {
                let drives = app.session.machine.floppies.len();
                egui::ComboBox::from_label("drive").selected_text(format!("FLOPPY_{}", d.drive)).show_ui(ui, |ui| {
                    for i in 0..drives {
                        ui.selectable_value(&mut d.drive, i, format!("FLOPPY_{i}"));
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut d.cyl).range(0..=79).prefix("cylinder "));
                ui.add(egui::DragValue::new(&mut d.head).range(0..=1).prefix("head "));
                ui.add(egui::DragValue::new(&mut d.sector).range(1..=18).prefix("sector "));
            });
            ui.weak("1.44 MB disk: 80 cylinders, 2 heads, 18 sectors of 512 bytes. Sector 1 of cylinder 0, head 0 is the boot sector.");
            ui.horizontal(|ui| {
                write = ui.add_enabled(!d.path.trim().is_empty(), egui::Button::new("write")).clicked();
                if ui.button("cancel").clicked() {
                    keep = false;
                }
            });
        });
    if write {
        match std::fs::read(d.path.trim()) {
            Ok(data) => match app.session.write_floppy(d.drive, d.cyl, d.head, d.sector, &data) {
                Ok(n) => {
                    app.status = format!("{} bytes ({n} sectors) written to FLOPPY_{}", data.len(), d.drive);
                    keep = false;
                }
                Err(e) => app.notice("Virtual drive", &e),
            },
            Err(e) => app.notice("Virtual drive", &format!("{}: {e}", d.path.trim())),
        }
    }
    if keep {
        app.bin_to_floppy = Some(d);
    }
}

fn dialogs(app: &mut App, ctx: &egui::Context) {
    bin_to_floppy_dialog(app, ctx);
    if let Some(mut text) = app.cmdline_dialog.take() {
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
                        app.session.machine.set_command_line(&t);
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if keep {
            app.cmdline_dialog = Some(text);
        }
    }
    if let Some(action) = app.confirm_discard.clone() {
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Save changes to {}?", app.file_name()));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() && app.save() {
                        app.confirm_discard = None;
                        finish(app, ctx, action.clone());
                    }
                    if ui.button("Discard").clicked() {
                        app.confirm_discard = None;
                        app.saved_text = app.text.clone();
                        finish(app, ctx, action.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        app.confirm_discard = None;
                    }
                });
            });
    }
    if let Some((title, text)) = app.notice.clone() {
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(text);
                if ui.button("OK").clicked() {
                    app.notice = None;
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

fn tools(app: &mut App, ctx: &egui::Context) {
    let mut open = app.windows.ascii;
    egui::Window::new("ASCII codes").open(&mut open).default_width(520.0).show(ctx, |ui| {
        egui::Grid::new("ascii").num_columns(16).spacing([4.0, 2.0]).show(ui, |ui| {
            for row in 0..16u8 {
                for col in 0..16u8 {
                    let b = row * 16 + col;
                    let c = rustemu_core::font::cp437_to_char(b);
                    ui.label(RichText::new(c.to_string()).monospace().size(16.0))
                        .on_hover_text(format!("{b} = {b:02X}h = {b:08b}b"));
                }
                ui.end_row();
            }
        });
        ui.weak("hover a character to see its code (code page 437)");
    });
    app.windows.ascii = open;

    let mut open = app.windows.converter;
    egui::Window::new("base converter").open(&mut open).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("value:");
            ui.add(egui::TextEdit::singleline(&mut app.converter_text).hint_text("number"));
        });
        ui.weak("examples: 255, 0FFh, 0x1F, 1010b, 17o, -5, 'A'");
        let t = app.converter_text.trim();
        let v = if t.len() == 3 && t.starts_with('\'') && t.ends_with('\'') {
            Some(t.as_bytes()[1] as i64)
        } else if let Some(n) = t.strip_prefix('-') {
            rustemu_asm::lexer::parse_number(n).map(|v| -v)
        } else {
            rustemu_asm::lexer::parse_number(t)
        };
        match v {
            Some(v) => {
                let u16v = v as u16;
                egui::Grid::new("conv").num_columns(2).show(ui, |ui| {
                    let row = |ui: &mut Ui, k: &str, s: String| {
                        ui.label(k);
                        ui.label(RichText::new(s).monospace());
                        ui.end_row();
                    };
                    row(ui, "decimal (unsigned 16)", u16v.to_string());
                    row(ui, "decimal (signed 16)", (u16v as i16).to_string());
                    row(ui, "decimal (signed 8)", (u16v as u8 as i8).to_string());
                    row(ui, "hex", format!("{u16v:04X}h"));
                    row(ui, "binary", format!("{:08b} {:08b}b", u16v >> 8, u16v & 0xFF));
                    row(ui, "octal", format!("{u16v:o}o"));
                    let c = rustemu_core::font::cp437_to_char(u16v as u8);
                    row(ui, "char (low byte)", c.to_string());
                });
            }
            None if t.is_empty() => {}
            None => {
                ui.colored_label(crate::colors::error(ui), "not a number");
            }
        }
    });
    app.windows.converter = open;

    let mut open = app.windows.about;
    egui::Window::new("About").open(&mut open).collapsible(false).show(ctx, |ui| {
        ui.heading("RustEmu8086");
        ui.label(format!("version {}", env!("CARGO_PKG_VERSION")));
        ui.label("8086 assembler, emulator and debugger for macOS,");
        ui.label("compatible with Emu8086 programs.");
    });
    app.windows.about = open;
}
