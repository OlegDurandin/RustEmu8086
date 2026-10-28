//! Main window of the desktop: menu, toolbar, editor, messages and status bar (Emu8086 style).

use egui::{RichText, Ui};

use rustemu_ui::app::{App, PendingAction, RunMode};
use rustemu_ui::examples::EXAMPLES;

pub fn main_ui(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    // Set again by the screen window when it is shown and focused (read by next frame's shortcuts).
    app.screen.focused = false;
    egui::Panel::top("menu").show(ui, |ui| menu_bar(app, ui));
    egui::Panel::top("toolbar").show(ui, |ui| toolbar(app, ui));
    egui::Panel::bottom("status").show(ui, |ui| status_bar(app, ui));
    egui::Panel::bottom("messages").resizable(true).default_size(110.0).show(ui, |ui| messages(app, ui));
    if app.find.open {
        egui::Panel::top("findbar").show(ui, |ui| rustemu_ui::find::find_bar(app, ui));
    }
    egui::CentralPanel::default().show(ui, |ui| rustemu_ui::editor::editor(app, ui));

    crate::emulator::emulator_window(app, &ctx);
    rustemu_ui::screen::screen_window(app, &ctx);
    crate::emulator::source_window(app, &ctx);
    rustemu_ui::windows::shared_windows(app, &ctx);
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
            use rustemu_ui::editcmd as ed;
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
                rustemu_ui::find::find_next(app, false);
            }
            if ui.button("Find previous        ⇧F3").clicked() {
                rustemu_ui::find::find_next(app, true);
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
            use rustemu_ui::editcmd as ed;
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
            use rustemu_ui::reference_ui::Tab;
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
    ui.menu_button("Virtual drive", |ui| rustemu_ui::windows::virtual_drive_menu(app, ui));
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
            rustemu_ui::colors::error(ui)
        } else if app.run != RunMode::Paused {
            rustemu_ui::colors::ok(ui)
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
            let color = if m.error { rustemu_ui::colors::error(ui) } else { ui.visuals().text_color() };
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
