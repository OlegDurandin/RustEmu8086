//! RustEmu8086 for the desktop; the application is in the library (`lib.rs`).

#[cfg(target_os = "macos")]
mod macos_open;

/// `RustEmu8086 file.asm` opens the file (also used by `open -a RustEmu8086 --args file.asm`).
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    let finder = macos_open::FinderEvents::install();
    let file = std::env::args().nth(1).map(std::path::PathBuf::from).filter(|p| p.exists());
    eframe::run_native(
        "RustEmu8086",
        rustemu_ui::native_options(),
        Box::new(move |cc| {
            let mut app = rustemu_ui::app::App::new(cc);
            if let Some(p) = file {
                app.open_path(&p);
            }
            let app = rustemu_desktop::DesktopApp(app);
            #[cfg(target_os = "macos")]
            {
                finder.set_context(cc.egui_ctx.clone());
                Ok(Box::new(macos_open::FinderApp(app, finder)))
            }
            #[cfg(not(target_os = "macos"))]
            Ok(Box::new(app))
        }),
    )
}

/// The browser runs the `rustemu-web` crate instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
