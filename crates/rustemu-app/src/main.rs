//! RustEmu8086 - 8086 assembler, emulator and debugger (Emu8086 compatible).
//! The application lives in the library (`lib.rs`); this is the desktop / iPadOS executable.

fn main() -> eframe::Result {
    rustemu_app::run_desktop()
}
