//! RustEmu8086 - 8086 assembler, emulator and debugger (Emu8086 compatible).

mod app;
mod calc;
mod colors;
mod console;
mod debug;
mod devices_ui;
mod editcmd;
mod editor;
mod emulator;
mod examples;
mod find;
mod fonts;
mod help;
mod highlight;
mod library_ui;
mod reference;
mod reference_ui;
mod screen;
mod ui;
#[cfg(test)]
mod ui_tests;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustEmu8086")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    // `RustEmu8086 file.asm` opens the file (also used by `open -a RustEmu8086 --args file.asm`).
    let file = std::env::args().nth(1).map(std::path::PathBuf::from).filter(|p| p.exists());
    eframe::run_native(
        "RustEmu8086",
        options,
        Box::new(move |cc| {
            let mut app = app::App::new(cc);
            if let Some(p) = file {
                app.open_path(&p);
            }
            Ok(Box::new(app))
        }),
    )
}
