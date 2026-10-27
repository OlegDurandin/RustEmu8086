//! RustEmu8086 - 8086 assembler, emulator and debugger (Emu8086 compatible).
//!
//! The application is a library so that every platform can start it its own way: `main.rs` on
//! macOS / Windows / Linux and iPadOS, [`android_main`] on Android (built as a `cdylib`).
//! OS services (file dialogs, clipboard, URLs) are in [`platform`].

mod app;
mod calc;
mod colors;
mod console;
mod debug;
mod debugger;
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
pub mod platform;
mod reference;
mod reference_ui;
mod screen;
mod ui;
#[cfg(test)]
mod ui_tests;

/// Window options shared by all platforms (on tablets the window is the whole screen).
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustEmu8086")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    }
}

/// Runs the application; `file` is opened at start.
pub fn run(options: eframe::NativeOptions, file: Option<std::path::PathBuf>) -> eframe::Result {
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

/// Desktop and iPadOS entry point: `RustEmu8086 file.asm` opens the file (also used by
/// `open -a RustEmu8086 --args file.asm`).
pub fn run_desktop() -> eframe::Result {
    let file = std::env::args().nth(1).map(std::path::PathBuf::from).filter(|p| p.exists());
    run(native_options(), file)
}

/// Android entry point, called by the NativeActivity glue (the library is built as a `cdylib`).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let options = eframe::NativeOptions { android_app: Some(app), ..native_options() };
    if let Err(e) = run(options, None) {
        eprintln!("RustEmu8086: {e}");
    }
}
