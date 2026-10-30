//! Helpers for the UI tests of the front ends (feature `testing`): they drive the app with
//! egui_kittest and render snapshots to `target/ui-snapshots/*.png` for visual inspection.

use egui_kittest::Harness;

use crate::app::{App, Settings};

macro_rules! program {
    ($dir:literal, $name:literal) => {
        ($name, include_str!(concat!("../../../", $dir, "/", $name)))
    };
}

/// The test programs (`tests/programs`, ours; the examples are Emu8086's) and the NASM examples.
pub const PROGRAMS: &[(&str, &str)] = &[
    program!("tests/programs", "hello.asm"),
    program!("tests/programs", "hello_exe.asm"),
    program!("tests/programs", "echo.asm"),
    program!("tests/programs", "calc.asm"),
    program!("tests/programs", "factorial.asm"),
    program!("tests/programs", "bubble_sort.asm"),
    program!("tests/programs", "keyboard.asm"),
    program!("tests/programs", "colors.asm"),
    program!("tests/programs", "graphics.asm"),
    program!("tests/programs", "traffic_lights.asm"),
    program!("tests/programs", "stepper_motor.asm"),
    program!("tests/programs", "led_display.asm"),
    program!("tests/programs", "thermometer.asm"),
    program!("tests/programs", "printer.asm"),
    program!("tests/programs", "robot.asm"),
    program!("examples", "hello_nasm.asm"),
    program!("examples", "boot_nasm.asm"),
];

/// An app with test program `i` ([`PROGRAMS`]) open as an example and default settings (no last
/// file).
pub fn app_with_example(i: usize) -> App {
    let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
    let (name, src) = PROGRAMS[i];
    app.load_example_text(name, src);
    app
}

/// Index of the test program `name` in [`PROGRAMS`].
pub fn example(name: &str) -> usize {
    PROGRAMS.iter().position(|e| e.0 == name).unwrap()
}

/// Renders the harness to `target/ui-snapshots/<name>.png`.
pub fn save<State>(h: &mut Harness<'_, State>, name: &str) {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    // egui_kittest 0.37 panics while lazily creating its renderer when WGPU
    // has no adapter, instead of returning the documented `Err`. UI logic is
    // still testable in such headless environments; only the PNG is skipped.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render())) {
        Ok(Ok(img)) => img.save(dir.join(format!("{name}.png"))).unwrap(),
        Ok(Err(e)) => eprintln!("render skipped: {e}"),
        Err(panic) => {
            let message =
                panic.downcast_ref::<String>().map(String::as_str).or_else(|| panic.downcast_ref::<&str>().copied());
            if message.is_some_and(|m| m.contains("No adapter found")) {
                eprintln!("render skipped: WGPU renderer is unavailable");
            } else {
                std::panic::resume_unwind(panic);
            }
        }
    }
}
