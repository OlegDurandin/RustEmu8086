//! Main window layout: menu, toolbar, editor, messages, status bar, dialogs and tools.

use egui::{RichText, Ui};

use crate::app::{App, PendingAction, RunMode, ThemeChoice};
use crate::examples::EXAMPLES;

pub fn main_ui(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
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
    crate::devices_ui::devices_window(app, &ctx);
    tools(app, &ctx);
    crate::find::goto_dialog(app, &ctx);
    crate::console::console_window(app, &ctx);
    crate::library_ui::library_window(app, &ctx);
    dialogs(app, &ctx);
}

fn menu_bar(app: &mut App, ui: &mut Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("New            ⌘N").clicked() {
                app.guarded(PendingAction::New);
            }
            if ui.button("Open…          ⌘O").clicked() {
                app.guarded(PendingAction::Open(None));
            }
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
            if ui.button("Save           ⌘S").clicked() {
                app.save();
            }
            if ui.button("Save as…       ⇧⌘S").clicked() {
                app.save_as();
            }
            ui.separator();
            if ui.button("Find / replace ⌘F").clicked() {
                app.find.open = true;
                app.find.focus = true;
            }
            if ui.button("Go to line…    ⌘L").clicked() {
                app.find.goto_open = true;
            }
            if ui.button("Toggle comment ⌘/").clicked() {
                crate::find::toggle_comment(app);
            }
            ui.separator();
            if ui.button("Quit           ⌘Q").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Examples", |ui| {
            for (i, (name, _)) in EXAMPLES.iter().enumerate() {
                if ui.button(*name).clicked() {
                    app.guarded(PendingAction::Example(i));
                }
            }
        });
        ui.menu_button("Assembler", |ui| {
            if ui.button("Compile                 ⌘B").clicked() {
                app.compile();
            }
            if ui.button("Compile and emulate   F9").clicked() {
                app.emulate();
            }
        });
        ui.menu_button("Emulator", |ui| {
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
            if ui.add_enabled(loaded, egui::Button::new("Reload")).clicked() {
                app.reload();
            }
            ui.separator();
            if ui.button("Load executable…").clicked() {
                app.open_dialog();
            }
            if ui.button("Clear all breakpoints").clicked() {
                app.bp_lines.clear();
                app.session.machine.breakpoints.clear();
            }
        });
        ui.menu_button("View", |ui| {
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
            ui.separator();
            ui.label("Theme");
            let mut changed = false;
            for (t, name) in
                [(ThemeChoice::Light, "Light"), (ThemeChoice::Dark, "Dark"), (ThemeChoice::System, "System")]
            {
                changed |= ui.radio_value(&mut app.settings.theme, t, name).changed();
            }
            if changed {
                app.apply_theme(ui.ctx());
            }
            ui.add(egui::Slider::new(&mut app.settings.font_size, 10.0..=24.0).text("editor font"));
        });
        ui.menu_button("Tools", |ui| {
            ui.checkbox(&mut app.windows.ascii, "ASCII codes");
            ui.checkbox(&mut app.windows.converter, "Base converter");
        });
        ui.menu_button("Help", |ui| {
            ui.checkbox(&mut app.windows.help, "Quick reference");
            ui.checkbox(&mut app.library.open, "emu8086.inc library");
            ui.checkbox(&mut app.windows.about, "About RustEmu8086");
        });
    });
}

fn toolbar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.button("🗋 new").clicked() {
            app.guarded(PendingAction::New);
        }
        if ui.button("📂 open").clicked() {
            app.guarded(PendingAction::Open(None));
        }
        ui.menu_button("📚 examples", |ui| {
            for (i, (name, _)) in EXAMPLES.iter().enumerate() {
                if ui.button(*name).clicked() {
                    app.guarded(PendingAction::Example(i));
                }
            }
        });
        if ui.button("💾 save").clicked() {
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
        if ui.button("🔢 convertor").clicked() {
            app.windows.converter = !app.windows.converter;
        }
        if ui.button("🔤 ascii").clicked() {
            app.windows.ascii = !app.windows.ascii;
        }
        if ui.button("📖 emu8086.inc").on_hover_text("Macros and procedures of the include library").clicked() {
            app.library.open = !app.library.open;
        }
        if ui.button("❓ help").clicked() {
            app.windows.help = !app.windows.help;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let dark = ui.visuals().dark_mode;
            let (icon, tip) =
                if dark { ("☀", "Switch to the light theme") } else { ("🌙", "Switch to the dark theme") };
            if ui.button(icon).on_hover_text(tip).clicked() {
                app.settings.theme = if dark { ThemeChoice::Light } else { ThemeChoice::Dark };
                app.apply_theme(ui.ctx());
            }
        });
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

fn dialogs(app: &mut App, ctx: &egui::Context) {
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

    let mut open = app.windows.help;
    egui::Window::new("quick reference").open(&mut open).default_size([520.0, 480.0]).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(RichText::new(crate::help::HELP).monospace().size(12.5));
        });
    });
    app.windows.help = open;

    let mut open = app.windows.about;
    egui::Window::new("About").open(&mut open).collapsible(false).show(ctx, |ui| {
        ui.heading("RustEmu8086");
        ui.label(format!("version {}", env!("CARGO_PKG_VERSION")));
        ui.label("8086 assembler, emulator and debugger for macOS,");
        ui.label("compatible with Emu8086 programs.");
    });
    app.windows.about = open;
}
