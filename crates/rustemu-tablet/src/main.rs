//! The tablet layout in a desktop window, for development (the web version uses it on tablets).

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    rustemu_tablet::run(rustemu_ui::native_options())
}

/// The browser runs the `rustemu-web` crate instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
