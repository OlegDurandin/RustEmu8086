//! RustEmu8086 for the desktop: 8086 assembler, emulator and debugger (Emu8086 compatible).
//! The Emu8086-style windows (`ui.rs`, `emulator.rs`) around the shared GUI of `rustemu_ui`.

mod emulator;
mod ui;
#[cfg(test)]
mod ui_tests;

use rustemu_ui::app::App;

struct DesktopApp(App);

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

/// `RustEmu8086 file.asm` opens the file (also used by `open -a RustEmu8086 --args file.asm`).
fn main() -> eframe::Result {
    let file = std::env::args().nth(1).map(std::path::PathBuf::from).filter(|p| p.exists());
    eframe::run_native(
        "RustEmu8086",
        rustemu_ui::native_options(),
        Box::new(move |cc| {
            let mut app = App::new(cc);
            if let Some(p) = file {
                app.open_path(&p);
            }
            Ok(Box::new(DesktopApp(app)))
        }),
    )
}
