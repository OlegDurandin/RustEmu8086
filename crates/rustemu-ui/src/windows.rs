//! Tool windows and dialogs shared by the desktop and tablet front ends: new file, options, virtual
//! drive, command line, unsaved changes, notices, ASCII codes, base converter, about, the Examples
//! menu and "more examples...".

use egui::{RichText, Ui};

use crate::app::{App, PendingAction, ThemeChoice};

/// Tool windows and dialogs of both layouts (desktop and tablet).
pub fn shared_windows(app: &mut App, ctx: &egui::Context) {
    let ctx = ctx.clone();
    crate::debug::debug_windows(app, &ctx);
    crate::debugger::debugger_windows(app, &ctx);
    crate::devices_ui::devices_window(app, &ctx);
    crate::drive_ui::drive_window(app, &ctx);
    crate::programs::programs_window(app, &ctx);
    tools(app, &ctx);
    crate::find::goto_dialog(app, &ctx);
    crate::console::console_window(app, &ctx);
    crate::library_ui::library_window(app, &ctx);
    if !app.panels() {
        crate::reference_ui::reference_window(&mut app.reference, &ctx);
    }
    crate::calc::calculator_window(&mut app.calculator, &ctx);
    new_dialog(app, &ctx);
    options_window(app, &ctx);
    dialogs(app, &ctx);
    crate::programs::save_name_dialog(app, &ctx);
    crate::share::share_dialog(app, &ctx);
    more_examples_window(app, &ctx);
}

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
        app.more_examples = Some(emulate);
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
fn more_examples_window(app: &mut App, ctx: &egui::Context) {
    let Some(emulate) = app.more_examples else { return };
    let mut open = true;
    let mut chosen = None;
    egui::Window::new("more examples")
        .open(&mut open)
        .default_pos(egui::pos2(160.0, 100.0))
        .default_size([620.0, 480.0])
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.examples_filter)
                    .hint_text("filter: name or description")
                    .desired_width(f32::INFINITY),
            );
            let filter = app.examples_filter.to_lowercase();
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
        app.more_examples = None;
        open_example(app, i, emulate);
    } else if !open {
        app.more_examples = None;
    }
}

const AUTO_SYNTAX: &str = "Detected from the source (NASM for bits 16, section, times, resb, %define, \
     byte [..]...); a program that does not assemble that way is tried in the other syntax";

/// The Options menu: the most used settings and the options window with all of them.
pub fn options_menu(app: &mut App, ui: &mut Ui) {
    use crate::app::SyntaxChoice;
    use crate::workspace::Layout;
    if !app.tablet_layout {
        ui.label(RichText::new("windows").weak());
        ui.radio_value(&mut app.settings.layout, Layout::Classic, "classic windows (Emu8086)");
        ui.radio_value(&mut app.settings.layout, Layout::Panels, "panels");
        ui.separator();
    }
    ui.label(RichText::new("assembler syntax").weak());
    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Auto, "auto").on_hover_text(AUTO_SYNTAX);
    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Masm, "Emu8086 (MASM)");
    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Nasm, "NASM");
    // The tablet layout has a theme of its own.
    if app.fixed_theme.is_none() {
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
        app.options_open = true;
        ui.close();
    }
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
                if !app.tablet_layout {
                    ui.label("windows");
                    crate::workspace::layout_options(app, ui);
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
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Auto, "auto").on_hover_text(AUTO_SYNTAX);
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Masm, "Emu8086 (MASM)");
                    ui.radio_value(&mut app.settings.syntax, SyntaxChoice::Nasm, "NASM");
                });
                ui.end_row();
                ui.label("editor font size");
                ui.add(egui::Slider::new(&mut app.settings.font_size, 10.0..=24.0).text("pt"));
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
                // The browser's drive C: is in memory.
                if !app.browser {
                    ui.label("drive C:");
                    ui.vertical(|ui| {
                        let own = app.drive_dir.display().to_string();
                        ui.radio_value(&mut app.settings.drive_c_source_folder, false, "a folder of its own")
                            .on_hover_text(own);
                        ui.radio_value(&mut app.settings.drive_c_source_folder, true, "the folder of the program")
                            .on_hover_text("Programs can change and delete the files next to the source");
                        ui.weak("Takes effect when a program is loaded.");
                        if ui.button("show drive C:").clicked() {
                            open_drive_folder(app);
                        }
                    });
                    ui.end_row();
                }
            });
        });
    app.options_open = open;
}

/// Opens the folder DOS programs see as drive C: (of the loaded program, otherwise the default).
fn open_drive_folder(app: &App) {
    std::fs::create_dir_all(app.host_drive()).ok();
    crate::platform::open_folder(app.host_drive());
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
    } else if !app.browser {
        let dir = app.host_drive().display().to_string();
        if ui.button("show drive C: folder").on_hover_text(dir).clicked() {
            open_drive_folder(app);
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
                    if ui.button("Save").clicked() {
                        if app.save() {
                            app.confirm_discard = None;
                            finish(app, ctx, action.clone());
                        } else if let Some(d) = &mut app.save_name {
                            // A new program in the browser: named first, then the action.
                            d.then = Some(action.clone());
                            app.confirm_discard = None;
                        }
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

/// The table of characters (code page 437).
fn ascii_ui(ui: &mut Ui) {
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
}

fn tools(app: &mut App, ctx: &egui::Context) {
    // A floating window in both layouts, as in Emu8086: it stays open while you work with the emulator.
    let mut open = app.windows.ascii;
    egui::Window::new("ASCII codes")
        .open(&mut open)
        .default_pos(egui::pos2(160.0, 100.0))
        .default_width(520.0)
        .show(ctx, ascii_ui);
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
    egui::Window::new("About")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, about_ui);
    app.windows.about = open;
}

/// A list item whose wrapped lines stay to the right of the bullet.
pub fn bullet(ui: &mut Ui, text: &str) {
    ui.horizontal_top(|ui| {
        ui.label("•");
        ui.add(egui::Label::new(text).wrap());
    });
}

/// Where this build runs.
fn platform_name() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web version"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

fn about_ui(ui: &mut Ui) {
    ui.set_max_width(440.0);
    ui.heading("RustEmu8086");
    ui.label(format!("version {} · {}", env!("CARGO_PKG_VERSION"), platform_name()));
    ui.add_space(6.0);
    ui.label("8086 assembler, emulator and debugger, compatible with Emu8086 programs.");
    ui.add_space(4.0);
    for line in [
        "Emu8086 (MASM) and NASM syntax, COM / EXE / BIN / boot sector output",
        "source-level debugger: breakpoints with conditions, watch, call stack, step back",
        "BIOS and DOS services, a virtual drive, floppies and the Emu8086 virtual devices",
        "the desktop layout of Emu8086 and a touch layout for tablets",
        "the CPU is checked against 2.9 million tests recorded on a real 8088",
    ] {
        bullet(ui, line);
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for (name, url) in [
            ("source code (GitHub)", crate::app::REPO_URL),
            ("web version", crate::app::WEB_URL),
            ("releases", crate::app::RELEASES_URL),
        ] {
            if ui.link(name).on_hover_text(url).clicked() {
                crate::platform::open_url(url);
            }
        }
    });
    ui.add_space(8.0);
    ui.label("MIT License · © 2026 Oleg Durandin");
    ui.weak(
        "Written in Rust with egui. RustEmu8086 is an independent program, not affiliated with the authors of Emu8086.",
    );
}
