//! RustEmu8086 for the desktop: 8086 assembler, emulator and debugger (Emu8086 compatible).
//! The Emu8086-style windows (`ui.rs`, `emulator.rs`) around the shared GUI of `rustemu_ui`; used
//! by the macOS executable (`main.rs`) and by the web version on large screens.

mod emulator;
mod ui;
#[cfg(test)]
mod ui_tests;

use rustemu_ui::app::App;

/// The application with the desktop layout.
pub struct DesktopApp(pub App);

impl eframe::App for DesktopApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.0.persist(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let app = &mut self.0;
        let ctx = ui.ctx().clone();
        app.shortcuts(&ctx);
        app.tick(&ctx);
        ui::main_ui(app, ui);
        app.confirm_close(&ctx);
    }
}
