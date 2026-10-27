//! Main window layout: menu, toolbar, editor, messages, status bar, dialogs and tools.

use egui::{RichText, Ui};

use crate::app::{App, PendingAction, RunMode, ThemeChoice};
use crate::examples::EXAMPLES;

pub fn main_ui(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    // Set again by the screen window when it is shown and focused (read by next frame's shortcuts).
    app.screen.focused = false;
    egui::Panel::top("menu").show(ui, |ui| menu_bar(app, ui));
    egui::Panel::top("toolbar").show(ui, |ui| toolbar(app, ui));
    egui::Panel::bottom("status").show(ui, |ui| status_bar(app, ui));
    egui::Panel::bottom("messages").resizable(true).default_size(110.0).show(ui, |ui| messages(app, ui));
    if app.find.open {
        egui::Panel::top("findbar").show(ui, |ui| crate::find::find_bar(app, ui));
    }
    egui::CentralPanel::default().show(ui, |ui| crate::editor::editor(app, ui));

    crate::emulator::emulator_window(app, &ctx);
    crate::screen::screen_window(app, &ctx);
    crate::emulator::source_window(app, &ctx);
    crate::debug::debug_windows(app, &ctx);
    crate::debugger::debugger_windows(app, &ctx);
    crate::devices_ui::devices_window(app, &ctx);
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
                ui.label("theme");
                ui.horizontal(|ui| {
                    let mut changed = false;
                    for (t, name) in
                        [(ThemeChoice::Light, "light"), (ThemeChoice::Dark, "dark"), (ThemeChoice::System, "system")]
                    {
                        changed |= ui.radio_value(&mut app.settings.theme, t, name).changed();
                    }
                    if changed {
                        app.apply_theme(ui.ctx());
                    }
                });
                ui.end_row();
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

fn menu_bar(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("New…                 ⌘N").clicked() {
                app.guarded(PendingAction::New);
            }
            if ui.button("Open…                ⌘O").clicked() {
                app.guarded(PendingAction::Open(None));
            }
            ui.menu_button("Examples", |ui| examples_menu(app, ui));
            ui.menu_button("Recent files", |ui| {
                if app.settings.recent.is_empty() {
                    ui.weak("(empty)");
                }
                for p in app.settings.recent.clone() {
                    if ui.button(p.display().to_string()).clicked() {
                        app.guarded(PendingAction::Open(Some(p)));
                    }
                }
            });
            ui.separator();
            if ui.button("Save                 ⌘S").clicked() {
                app.save();
            }
            if ui.button("Save as…             ⇧⌘S").clicked() {
                app.save_as();
            }
            if ui.button("Save compiled file   ⌘B").clicked() {
                app.compile();
            }
            ui.separator();
            if ui.button("Quit                 ⌘Q").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Edit", |ui| {
            use crate::editcmd as ed;
            if ui.button("Undo                 ⌘Z").clicked() {
                app.pending_undo = Some(true);
            }
            if ui.button("Redo                 ⇧⌘Z").clicked() {
                app.pending_undo = Some(false);
            }
            ui.separator();
            if ui.button("Cut                  ⌘X").clicked() {
                ed::cut(app, &ctx);
            }
            if ui.button("Copy                 ⌘C").clicked() {
                ed::copy(app, &ctx);
            }
            if ui.button("Paste                ⌘V").clicked() {
                ed::paste(app);
            }
            if ui.button("Select all           ⌘A").clicked() {
                ed::select_all(app);
            }
            ui.separator();
            if ui.button("Find…                ⌘F").clicked() {
                app.find.open = true;
                app.find.focus = true;
            }
            if ui.button("Find next            F3").clicked() {
                crate::find::find_next(app, false);
            }
            if ui.button("Find previous        ⇧F3").clicked() {
                crate::find::find_next(app, true);
            }
            if ui.button("Replace…             ⌃H").clicked() {
                app.find.open = true;
                app.find.focus = true;
            }
            if ui.button("Go to line…          ⌃G").clicked() {
                app.find.goto_open = true;
            }
            ui.separator();
            if ui.button("Indent").clicked() {
                ed::indent(app);
            }
            if ui.button("Outdent").clicked() {
                ed::outdent(app);
            }
            if ui.button("Comment block        ⌃Q").clicked() {
                ed::comment(app);
            }
            if ui.button("Uncomment block      ⌃W").clicked() {
                ed::uncomment(app);
            }
            if ui.button("Lowercase            ⌃L").clicked() {
                ed::lowercase(app);
            }
            if ui.button("Uppercase            ⌃U").clicked() {
                ed::uppercase(app);
            }
        });
        ui.menu_button("Bookmarks", |ui| {
            use crate::editcmd as ed;
            if ui.button("Toggle bookmark      ⌃F2").clicked() {
                ed::toggle_bookmark(app);
            }
            if ui.button("Next bookmark        F2").clicked() {
                ed::next_bookmark(app);
            }
            if ui.button("Previous bookmark    ⇧F2").clicked() {
                ed::prev_bookmark(app);
            }
            if ui.button("Clear all bookmarks").clicked() {
                app.bookmarks.clear();
            }
            if !app.bookmarks.is_empty() {
                ui.separator();
                for l in app.bookmarks.clone() {
                    let text = app.text.lines().nth(l as usize - 1).unwrap_or("").trim().to_string();
                    if ui.button(format!("{l}: {}", text.chars().take(40).collect::<String>())).clicked() {
                        app.goto_line = Some(l);
                    }
                }
            }
        });
        ui.menu_button("Assembler", |ui| {
            if ui.button("Compile                     ⌘B").clicked() {
                app.compile();
            }
            if ui.button("Compile and emulate         F9").clicked() {
                app.emulate();
            }
            ui.separator();
            if ui.button("Listing").clicked() {
                app.windows.listing = true;
            }
            if ui.button("Symbol table").clicked() {
                app.windows.symbols = true;
            }
        });
        ui.menu_button("Emulator", |ui| emulator_menu(app, ui));
        ui.menu_button("Math", |ui| {
            if ui.button("Multi base calculator").clicked() {
                app.calculator.open = true;
            }
            if ui.button("Base convertor").clicked() {
                app.windows.converter = true;
            }
        });
        if ui.button("ASCII Codes").clicked() {
            app.windows.ascii = !app.windows.ascii;
        }
        ui.menu_button("Help", |ui| {
            use crate::reference_ui::Tab;
            for (tab, label) in [
                (Tab::Instructions, "Instruction set (F1)"),
                (Tab::Interrupts, "BIOS and DOS interrupts"),
                (Tab::Directives, "Assembler directives"),
                (Tab::Quick, "Quick reference"),
            ] {
                if ui.button(label).clicked() {
                    app.reference.show_tab(tab);
                }
            }
            if ui.button("emu8086.inc library").clicked() {
                app.library.open = true;
            }
            ui.separator();
            if ui.button("About RustEmu8086").clicked() {
                app.windows.about = true;
            }
        });
    });
}

fn examples_menu(app: &mut App, ui: &mut Ui) {
    for (i, (name, _)) in EXAMPLES.iter().enumerate() {
        if ui.button(*name).clicked() {
            app.guarded(PendingAction::Example(i));
        }
    }
}

fn emulator_menu(app: &mut App, ui: &mut Ui) {
    let loaded = app.loaded();
    if ui.button("Show emulator").clicked() {
        app.windows.emulator = true;
    }
    if ui.add_enabled(loaded, egui::Button::new("Run / Stop          F5")).clicked() {
        if app.run == RunMode::Paused { app.start_run() } else { app.stop() }
    }
    if ui.add_enabled(loaded, egui::Button::new("Single step         F8")).clicked() {
        app.single_step();
    }
    if ui.add_enabled(loaded, egui::Button::new("Step over           F10")).clicked() {
        app.step_over();
    }
    if ui.add_enabled(loaded, egui::Button::new("Step out            ⇧F11")).clicked() {
        app.step_out();
    }
    if ui.add_enabled(loaded, egui::Button::new("Step back           ⇧F8")).clicked() {
        app.step_back();
    }
    if ui.button("Run to cursor       F4").clicked() {
        app.run_to_cursor();
    }
    if ui.button("Toggle breakpoint   ⌘F8").clicked() {
        app.toggle_breakpoint_at_cursor();
    }
    if ui.button("Breakpoints…").clicked() {
        app.windows.breakpoints = true;
    }
    if ui.button("Stop on condition…").clicked() {
        app.windows.stop_condition = true;
    }
    if ui.add_enabled(loaded, egui::Button::new("Reload")).clicked() {
        app.reload();
    }
    ui.separator();
    if ui.button("Load executable…").clicked() {
        app.open_dialog();
    }
    if ui.button("Set command line parameters…").clicked() {
        app.cmdline_dialog = Some(app.session.command_line.clone());
    }
    ui.menu_button("Virtual drive", |ui| virtual_drive_menu(app, ui));
    ui.menu_button("Virtual devices", |ui| {
        crate::emulator::virtual_devices_menu(app, ui);
    });
    ui.menu_button("Windows", |ui| {
        let w = &mut app.windows;
        ui.checkbox(&mut w.emulator, "Emulator");
        ui.checkbox(&mut w.screen, "Screen");
        ui.checkbox(&mut w.source, "Source");
        ui.checkbox(&mut w.memory, "Memory");
        ui.checkbox(&mut w.stack, "Stack");
        ui.checkbox(&mut w.flags, "Flags");
        ui.checkbox(&mut w.vars, "Variables");
        ui.checkbox(&mut w.symbols, "Symbol table");
        ui.checkbox(&mut w.listing, "Listing");
        ui.checkbox(&mut w.log, "Log");
        ui.checkbox(&mut w.devices, "Virtual devices");
        ui.checkbox(&mut w.console, "Debug console");
        ui.checkbox(&mut w.breakpoints, "Breakpoints");
        ui.checkbox(&mut w.watch, "Watch");
        ui.checkbox(&mut w.call_stack, "Call stack");
        ui.checkbox(&mut w.alu, "ALU");
    });
    if ui.button("Clear all breakpoints").clicked() {
        app.bp_lines.clear();
        app.bp_addr.clear();
        app.session.machine.breakpoints.clear();
    }
}

fn toolbar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.button("🗋 new").on_hover_text("New file from a template (⌘N)").clicked() {
            app.guarded(PendingAction::New);
        }
        if ui.button("📂 open").on_hover_text("⌘O").clicked() {
            app.guarded(PendingAction::Open(None));
        }
        ui.menu_button("📚 examples", |ui| examples_menu(app, ui));
        if ui.button("💾 save").on_hover_text("⌘S").clicked() {
            app.save();
        }
        ui.separator();
        if ui.button("⚙ compile").on_hover_text("Assemble and write the executable (⌘B)").clicked() {
            app.compile();
        }
        if ui
            .button(RichText::new("▶ emulate").strong())
            .on_hover_text("Assemble and load into the emulator (F9)")
            .clicked()
        {
            app.emulate();
        }
        ui.separator();
        if ui.button("🔣 calculator").on_hover_text("Multi base calculator").clicked() {
            app.calculator.open = !app.calculator.open;
        }
        if ui.button("🔢 convertor").on_hover_text("Base convertor").clicked() {
            app.windows.converter = !app.windows.converter;
        }
        if ui.button("🔧 options").clicked() {
            app.options_open = !app.options_open;
        }
        if ui.button("❓ help").on_hover_text("Instructions, interrupts, directives (F1)").clicked() {
            app.reference.open = !app.reference.open;
        }
        if ui.button("ℹ about").clicked() {
            app.windows.about = !app.windows.about;
        }
    });
}

fn status_bar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let dirty = if app.dirty() { " •" } else { "" };
        ui.label(RichText::new(format!("{}{dirty}", app.file_name())).strong());
        ui.separator();
        ui.label(format!("line {}", app.cursor_line));
        ui.separator();
        let color = if app.status.starts_with("Error") || app.status.contains("failed") {
            crate::colors::error(ui)
        } else if app.run != RunMode::Paused {
            crate::colors::ok(ui)
        } else {
            ui.visuals().text_color()
        };
        ui.colored_label(color, &app.status);
    });
}

fn messages(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        if app.messages.is_empty() {
            ui.weak("Press “emulate” (F9) to assemble and run the program, or “compile” (⌘B) to build an executable.");
        }
        let mut jump = None;
        for m in &app.messages {
            let color = if m.error { crate::colors::error(ui) } else { ui.visuals().text_color() };
            let r =
                ui.add(egui::Label::new(RichText::new(&m.text).monospace().color(color)).sense(egui::Sense::click()));
            if let Some((_, line)) = &m.loc {
                if r.clicked() {
                    jump = Some(*line);
                }
                r.on_hover_text("click to go to the line");
            }
        }
        if jump.is_some() {
            app.goto_line = jump;
        }
    });
}

/// Virtual drive menu (main Emulator menu and the emulator window), as in Emu8086. There is one
/// drive, FLOPPY_0, kept in memory; images are loaded and saved explicitly.
pub fn virtual_drive_menu(app: &mut App, ui: &mut Ui) {
    use rustemu_core::machine::{FLOPPY_SIZE, MAX_FLOPPIES};
    ui.set_min_width(330.0);
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
