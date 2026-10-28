//! RustEmu8086 for the desktop; the application is in the library (`lib.rs`).

/// `RustEmu8086 file.asm` opens the file (also used by `open -a RustEmu8086 --args file.asm`).
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let file = std::env::args().nth(1).map(std::path::PathBuf::from).filter(|p| p.exists());
    eframe::run_native(
        "RustEmu8086",
        rustemu_ui::native_options(),
        Box::new(move |cc| {
            let mut app = rustemu_ui::app::App::new(cc);
            if let Some(p) = file {
                app.open_path(&p);
            }
            Ok(Box::new(rustemu_desktop::DesktopApp(app)))
        }),
    )
}

/// The browser runs the `rustemu-web` crate instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
