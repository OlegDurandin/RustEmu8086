//! RustEmu8086 for iPadOS (and the Mac, for development): the tablet layout.

fn main() -> eframe::Result {
    rustemu_tablet::run(rustemu_ui::native_options())
}
